use crate::{lock_free_stack::LFStack, InternerSymbol, Symbol};
use bumpalo::Bump;
use dashmap::DashMap;
use hashbrown::hash_table;
use std::{collections::hash_map::RandomState, hash::BuildHasher};
use thread_local::ThreadLocal;

/// `str -> Symbol` interner.
/// The hash is also stored to avoid double hashing.
///
/// This uses `NoHasher` because we want to store the `hash_builder`
/// outside of the lock, and to avoid hashing twice on insertion.
pub(crate) type Map<S> = DashMap<MapKey, S, NoHasherBuilder>;
pub(crate) type MapKey = (u64, &'static str);
pub(crate) type RawMapKey<S> = (MapKey, S);

// TODO: Use a lock-free arena.
type Arena = ThreadLocal<Bump>;

/// String interner.
///
/// See the [module docs][self] for more details.
pub struct BytesInterner<S = Symbol, H = RandomState> {
    pub(crate) map: Map<S>,
    strs: LFStack<&'static str>,
    arena: Box<Arena>,
    hash_builder: H,
}

impl Default for BytesInterner {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl BytesInterner<Symbol, RandomState> {
    /// Creates a new, empty `Interner` with the default symbol and hasher.
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new `Interner` with the given capacity and default symbol and hasher.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, Default::default())
    }
}

impl<S: InternerSymbol, H: BuildHasher> BytesInterner<S, H> {
    /// Creates a new `Interner` with the given custom hasher.
    #[inline]
    pub fn with_hasher(hash_builder: H) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates a new `Interner` with the given capacitiy and custom hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: H) -> Self {
        let map = Map::with_capacity_and_hasher(capacity, Default::default());
        let strs = LFStack::with_capacity(capacity);
        Self { map, strs, arena: Default::default(), hash_builder }
    }

    /// Returns the number of unique strings in the interner.
    #[inline]
    pub fn len(&self) -> usize {
        self.strs.len()
    }

    /// Returns `true` if the interner is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the interned strings and their corresponding `Symbol`s.
    ///
    /// Does not guarantee that it includes symbols added after the iterator was created.
    #[inline]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (S, &str)> + Clone {
        self.all_symbols().map(|s| (s, self.resolve(s)))
    }

    /// Returns an iterator over all symbols in the interner.
    #[inline]
    pub fn all_symbols(&self) -> impl ExactSizeIterator<Item = S> + Send + Sync + Clone {
        (0..self.len()).map(S::from_usize)
    }

    /// Interns a string, returning its unique `Symbol`.
    ///
    /// Allocates the string internally if it is not already interned.
    ///
    /// If `s` outlives `self`, like `&'static str`, prefer using
    /// [`intern_static`](Self::intern_static), as it will not allocate the string on the heap.
    pub fn intern(&self, s: &str) -> S {
        self.do_intern(s, alloc)
    }

    /// Interns a string, returning its unique `Symbol`.
    ///
    /// Allocates the string internally if it is not already interned.
    ///
    /// If `s` outlives `self`, like `&'static str`, prefer using
    /// [`intern_mut_static`](Self::intern_mut_static), as it will not allocate the string on the
    /// heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, s: &str) -> S {
        self.do_intern_mut(s, alloc)
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// Note that this only requires that `s` outlives `self`, which means we can avoid allocating
    /// the string.
    pub fn intern_static<'a, 'b: 'a>(&'a self, s: &'b str) -> S {
        self.do_intern(s, no_alloc)
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// Note that this only requires that `s` outlives `self`, which means we can avoid allocating
    /// the string.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut_static<'a, 'b: 'a>(&'a mut self, s: &'b str) -> S {
        self.do_intern_mut(s, no_alloc)
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings outlive `self`, like `&'static str`, prefer using
    /// [`intern_many_static`](Self::intern_many_static), as it will not allocate the strings on the
    /// heap.
    pub fn intern_many<'a>(&self, strings: impl IntoIterator<Item = &'a str>) {
        for s in strings {
            self.intern(s);
        }
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings outlive `self`, like `&'static str`, prefer using
    /// [`intern_many_mut_static`](Self::intern_many_mut_static), as it will not allocate the
    /// strings on the heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut<'a>(&mut self, strings: impl IntoIterator<Item = &'a str>) {
        for s in strings {
            self.intern_mut(s);
        }
    }

    /// Interns multiple static strings.
    ///
    /// Note that this only requires that the strings outlive `self`, which means we can avoid
    /// allocating the strings.
    pub fn intern_many_static<'a, 'b: 'a>(&'a self, strings: impl IntoIterator<Item = &'b str>) {
        for s in strings {
            self.intern_static(s);
        }
    }

    /// Interns multiple static strings.
    ///
    /// Note that this only requires that the strings outlive `self`, which means we can avoid
    /// allocating the strings.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut_static<'a, 'b: 'a>(
        &'a mut self,
        strings: impl IntoIterator<Item = &'b str>,
    ) {
        for s in strings {
            self.intern_mut_static(s);
        }
    }

    /// Maps a `Symbol` to its string. This is a cheap, lock-free operation.
    ///
    /// # Panics
    ///
    /// Panics if `Symbol` is out of bounds of this `Interner`. You should only use `Symbol`s
    /// created by this `Interner`.
    #[inline]
    #[must_use]
    pub fn resolve(&self, sym: S) -> &str {
        unsafe { self.strs.get_unchecked(sym.to_usize()) }
    }

    #[inline]
    fn do_intern(&self, s: &str, alloc: impl FnOnce(&Arena, &str) -> &'static str) -> S {
        let hash = self.hash(s);
        let shard_idx = self.map.determine_shard(hash as usize);
        let shard = &*self.map.shards()[shard_idx];

        if let Some((_, sym)) = shard.read().find(hash, mk_eq(s)) {
            return *sym;
        }

        insert(&self.strs, &self.arena, s, hash, &mut shard.write(), alloc)
    }

    #[inline]
    fn do_intern_mut(&mut self, s: &str, alloc: impl FnOnce(&Arena, &str) -> &'static str) -> S {
        let hash = self.hash(s);
        let shard_idx = self.map.determine_shard(hash as usize);
        let shard = &mut *self.map.shards_mut()[shard_idx];

        insert(&self.strs, &self.arena, s, hash, shard.get_mut(), alloc)
    }

    #[inline]
    fn hash(&self, s: &str) -> u64 {
        self.hash_builder.hash_one(s)
    }
}

pub(crate) type NoHasherBuilder = std::hash::BuildHasherDefault<NoHasher>;

pub(crate) enum NoHasher {}
impl Default for NoHasher {
    #[inline]
    fn default() -> Self {
        unreachable!()
    }
}
impl std::hash::Hasher for NoHasher {
    #[inline]
    fn finish(&self) -> u64 {
        match *self {}
    }
    #[inline]
    fn write(&mut self, _bytes: &[u8]) {
        match *self {}
    }
}

#[inline]
fn insert<S: InternerSymbol>(
    strs: &LFStack<&'static str>,
    arena: &Arena,
    s: &str,
    hash: u64,
    shard: &mut hash_table::HashTable<RawMapKey<S>>,
    alloc: impl FnOnce(&Arena, &str) -> &'static str,
) -> S {
    match shard.entry(hash, mk_eq(s), hasher) {
        hash_table::Entry::Occupied(e) => {
            // TODO: cold path
            e.get().1
        }
        hash_table::Entry::Vacant(e) => {
            let s = alloc(arena, s);
            let i = strs.push(s);
            let new_sym = S::from_usize(i);
            e.insert(((hash, s), new_sym));
            new_sym
        }
    }
}

#[inline]
fn hasher<S>(((hash, _), _): &RawMapKey<S>) -> u64 {
    *hash
}

#[inline]
fn mk_eq<S>(s: &str) -> impl Fn(&RawMapKey<S>) -> bool + Copy + '_ {
    move |((_, ss), _): &RawMapKey<S>| s == *ss
}

#[inline]
fn alloc(arena: &Arena, s: &str) -> &'static str {
    // SAFETY: extends the lifetime of `&Arena` to `'static`. This is never exposed so it's ok.
    unsafe { std::mem::transmute::<&str, &'static str>(arena.get_or_default().alloc_str(s)) }
}

#[inline]
fn no_alloc(_: &Arena, s: &str) -> &'static str {
    // SAFETY: `s` outlives `arena`, so we don't need to allocate it. See above.
    unsafe { std::mem::transmute::<&str, &'static str>(s) }
}
