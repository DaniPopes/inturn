use crate::{InternerSymbol, Symbol};
use bumpalo::Bump;
use hashbrown::hash_table;
use std::{cell::UnsafeCell, collections::hash_map::RandomState, hash::BuildHasher};

/// `[u8] -> Symbol` interner.
/// The hash is also stored to avoid double hashing.
pub(crate) type Map<S> = hash_table::HashTable<RawMapKey<S>>;
pub(crate) type MapKey = (u64, &'static [u8]);
pub(crate) type RawMapKey<S> = (MapKey, S);

/// Non-thread-safe byte string interner.
///
/// See the [crate-level docs][crate] for more details.
pub struct BytesInterner<S = Symbol, H = RandomState> {
    pub(crate) map: UnsafeCell<Map<S>>,
    hash_builder: H,
    strs: UnsafeCell<Vec<&'static [u8]>>,
    arena: Bump,
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
        let map = Map::with_capacity(capacity);
        let strs = Vec::with_capacity(capacity);
        Self {
            map: UnsafeCell::new(map),
            hash_builder,
            strs: UnsafeCell::new(strs),
            arena: Bump::new(),
        }
    }

    /// Returns the number of unique strings in the interner.
    #[inline]
    pub fn len(&self) -> usize {
        // SAFETY: This type is not `Sync`, and this method only reads the vector length.
        unsafe { (*self.strs.get()).len() }
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
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (S, &[u8])> + Clone {
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
    /// If `s` is `&'static [u8]`, prefer using
    /// [`intern_static`](Self::intern_static), as it will not allocate the string on the heap.
    pub fn intern(&self, s: &[u8]) -> S {
        self.do_intern(s, alloc)
    }

    /// Interns a string, returning its unique `Symbol`.
    ///
    /// Allocates the string internally if it is not already interned.
    ///
    /// If `s` is `&'static [u8]`, prefer using
    /// [`intern_mut_static`](Self::intern_mut_static), as it will not allocate the string on the
    /// heap.
    ///
    /// By taking `&mut self`, this never uses shared interior mutability.
    pub fn intern_mut(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, alloc)
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// The input must be `'static`, which means we can avoid allocating the string.
    ///
    /// For non-`'static` inputs that outlive this interner, see
    /// [`intern_static_unchecked`](Self::intern_static_unchecked).
    pub fn intern_static(&self, s: &'static [u8]) -> S {
        self.do_intern(s, no_alloc)
    }

    /// Interns a string without allocating, returning its unique `Symbol`.
    ///
    /// This is the unchecked version of [`intern_static`](Self::intern_static) for inputs that are
    /// not typed as `'static`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `s` remains valid and immutable until this interner is dropped.
    pub unsafe fn intern_static_unchecked(&self, s: &[u8]) -> S {
        self.do_intern(s, no_alloc_unchecked)
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// The input must be `'static`, which means we can avoid allocating the string.
    ///
    /// For non-`'static` inputs that outlive this interner, see
    /// [`intern_mut_static_unchecked`](Self::intern_mut_static_unchecked).
    ///
    /// By taking `&mut self`, this never uses shared interior mutability.
    pub fn intern_mut_static(&mut self, s: &'static [u8]) -> S {
        self.do_intern_mut(s, no_alloc)
    }

    /// Interns a string without allocating, returning its unique `Symbol`.
    ///
    /// This is the unchecked version of [`intern_mut_static`](Self::intern_mut_static) for inputs
    /// that are not typed as `'static`.
    ///
    /// By taking `&mut self`, this never uses shared interior mutability.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `s` remains valid and immutable until this interner is dropped.
    pub unsafe fn intern_mut_static_unchecked(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, no_alloc_unchecked)
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings are `&'static [u8]`, prefer using
    /// [`intern_many_static`](Self::intern_many_static), as it will not allocate the strings on the
    /// heap.
    pub fn intern_many<'a>(&self, strings: impl IntoIterator<Item = &'a [u8]>) {
        for s in strings {
            self.intern(s);
        }
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings are `&'static [u8]`, prefer using
    /// [`intern_many_mut_static`](Self::intern_many_mut_static), as it will not allocate the
    /// strings on the heap.
    ///
    /// By taking `&mut self`, this never uses shared interior mutability.
    pub fn intern_many_mut<'a>(&mut self, strings: impl IntoIterator<Item = &'a [u8]>) {
        for s in strings {
            self.intern_mut(s);
        }
    }

    /// Interns multiple static strings.
    ///
    /// The inputs must be `'static`, which means we can avoid allocating the strings.
    pub fn intern_many_static(&self, strings: impl IntoIterator<Item = &'static [u8]>) {
        for s in strings {
            self.intern_static(s);
        }
    }

    /// Interns multiple strings without allocating.
    ///
    /// This is the unchecked version of [`intern_many_static`](Self::intern_many_static) for inputs
    /// that are not typed as `'static`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all inputs remain valid and immutable until this interner is
    /// dropped.
    pub unsafe fn intern_many_static_unchecked<'a>(
        &self,
        strings: impl IntoIterator<Item = &'a [u8]>,
    ) {
        for s in strings {
            self.do_intern(s, no_alloc_unchecked);
        }
    }

    /// Interns multiple static strings.
    ///
    /// The inputs must be `'static`, which means we can avoid allocating the strings.
    ///
    /// By taking `&mut self`, this never uses shared interior mutability.
    pub fn intern_many_mut_static(&mut self, strings: impl IntoIterator<Item = &'static [u8]>) {
        for s in strings {
            self.intern_mut_static(s);
        }
    }

    /// Interns multiple strings without allocating.
    ///
    /// This is the unchecked version of [`intern_many_mut_static`](Self::intern_many_mut_static)
    /// for inputs that are not typed as `'static`.
    ///
    /// By taking `&mut self`, this never uses shared interior mutability.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all inputs remain valid and immutable until this interner is
    /// dropped.
    pub unsafe fn intern_many_mut_static_unchecked<'a>(
        &mut self,
        strings: impl IntoIterator<Item = &'a [u8]>,
    ) {
        for s in strings {
            self.do_intern_mut(s, no_alloc_unchecked);
        }
    }

    /// Maps a `Symbol` to its string. This is a cheap operation.
    ///
    /// # Panics
    ///
    /// Panics if `Symbol` is out of bounds of this `Interner`. You should only use `Symbol`s
    /// created by this `Interner`.
    #[inline]
    #[must_use]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn resolve(&self, sym: S) -> &[u8] {
        // SAFETY: This type is not `Sync`, and interned slices outlive the vector slot.
        let strs = unsafe { &*self.strs.get() };
        if cfg!(debug_assertions) {
            strs.get(sym.to_usize()).copied().expect("symbol out of bounds")
        } else {
            unsafe { strs.get_unchecked(sym.to_usize()) }
        }
    }

    #[inline]
    fn do_intern<'a>(
        &self,
        s: &'a [u8],
        alloc: impl FnOnce(&Bump, &'a [u8]) -> &'static [u8],
    ) -> S {
        let hash = self.hash(s);
        // SAFETY: This type is not `Sync`, so shared access cannot race across threads.
        let (map, strs) = unsafe { (&mut *self.map.get(), &mut *self.strs.get()) };
        get_or_insert(strs, &self.arena, s, hash, map, alloc)
    }

    #[inline]
    fn do_intern_mut<'a>(
        &mut self,
        s: &'a [u8],
        alloc: impl FnOnce(&Bump, &'a [u8]) -> &'static [u8],
    ) -> S {
        let hash = self.hash(s);
        get_or_insert(self.strs.get_mut(), &self.arena, s, hash, self.map.get_mut(), alloc)
    }

    #[inline]
    fn hash(&self, s: &[u8]) -> u64 {
        // We don't use `self.hash_builder.hash_one(s)` because we want to avoid hashing the length.
        use std::hash::Hasher;
        let mut h = self.hash_builder.build_hasher();
        h.write(s);
        h.finish()
    }
}

#[inline]
fn get_or_insert<'a, S: InternerSymbol>(
    strs: &mut Vec<&'static [u8]>,
    arena: &Bump,
    s: &'a [u8],
    hash: u64,
    map: &mut Map<S>,
    alloc: impl FnOnce(&Bump, &'a [u8]) -> &'static [u8],
) -> S {
    match map.entry(hash, mk_eq(s), hasher) {
        hash_table::Entry::Occupied(e) => e.get().1,
        hash_table::Entry::Vacant(e) => {
            let s = alloc(arena, s);
            let i = strs.len();
            let new_sym = S::from_usize(i);
            strs.push(s);
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
fn mk_eq<S>(s: &[u8]) -> impl Fn(&RawMapKey<S>) -> bool + Copy + '_ {
    move |((_, ss), _): &RawMapKey<S>| s == *ss
}

#[inline]
fn alloc(arena: &Bump, s: &[u8]) -> &'static [u8] {
    // SAFETY: Extends the lifetime of `&Bump` to `'static`. This is never exposed so it's ok.
    unsafe { std::mem::transmute::<&[u8], &'static [u8]>(arena.alloc_slice_copy(s)) }
}

#[inline]
fn no_alloc(_: &Bump, s: &'static [u8]) -> &'static [u8] {
    s
}

#[inline]
fn no_alloc_unchecked(_: &Bump, s: &[u8]) -> &'static [u8] {
    // SAFETY: Callers guarantee that `s` remains valid and immutable until the interner is dropped.
    unsafe { std::mem::transmute::<&[u8], &'static [u8]>(s) }
}
