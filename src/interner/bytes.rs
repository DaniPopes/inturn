use crate::{InternerSymbol, Symbol};
use boxcar::Vec as LFVec;
use bumpalo::Bump;
use papaya::HashMap;
use std::{collections::hash_map::RandomState, hash::BuildHasher};
use thread_local::ThreadLocal;

// TODO: Use a lock-free arena.
type Arena = ThreadLocal<Bump>;

/// Byte string interner.
///
/// See the [crate-level docs][crate] for more details.
pub struct BytesInterner<S = Symbol, H = RandomState> {
    pub(crate) map: HashMap<&'static [u8], S, H>,
    strs: LFVec<&'static [u8]>,
    arena: Arena,
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
        let map = HashMap::with_capacity_and_hasher(capacity, hash_builder);
        let strs = LFVec::with_capacity(capacity);
        Self { map, strs, arena: Default::default() }
    }

    /// Returns the number of unique strings in the interner.
    #[inline]
    pub fn len(&self) -> usize {
        self.strs.count()
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
    /// If `s` outlives `self`, like `&'static [u8]`, prefer using
    /// [`intern_static`](Self::intern_static), as it will not allocate the string on the heap.
    pub fn intern(&self, s: &[u8]) -> S {
        self.do_intern(s, alloc)
    }

    /// Interns a string, returning its unique `Symbol`.
    ///
    /// Allocates the string internally if it is not already interned.
    ///
    /// If `s` outlives `self`, like `&'static [u8]`, prefer using
    /// [`intern_mut_static`](Self::intern_mut_static), as it will not allocate the string on the
    /// heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, alloc)
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// Note that this only requires that `s` outlives `self`, which means we can avoid allocating
    /// the string.
    pub fn intern_static<'a, 'b: 'a>(&'a self, s: &'b [u8]) -> S {
        self.do_intern(s, no_alloc)
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// Note that this only requires that `s` outlives `self`, which means we can avoid allocating
    /// the string.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut_static<'a, 'b: 'a>(&'a mut self, s: &'b [u8]) -> S {
        self.do_intern_mut(s, no_alloc)
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings outlive `self`, like `&'static [u8]`, prefer using
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
    /// If the strings outlive `self`, like `&'static [u8]`, prefer using
    /// [`intern_many_mut_static`](Self::intern_many_mut_static), as it will not allocate the
    /// strings on the heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut<'a>(&mut self, strings: impl IntoIterator<Item = &'a [u8]>) {
        for s in strings {
            self.intern_mut(s);
        }
    }

    /// Interns multiple static strings.
    ///
    /// Note that this only requires that the strings outlive `self`, which means we can avoid
    /// allocating the strings.
    pub fn intern_many_static<'a, 'b: 'a>(&'a self, strings: impl IntoIterator<Item = &'b [u8]>) {
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
        strings: impl IntoIterator<Item = &'b [u8]>,
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
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn resolve(&self, sym: S) -> &[u8] {
        if cfg!(debug_assertions) {
            self.strs.get(sym.to_usize()).expect("symbol out of bounds")
        } else {
            unsafe { self.strs.get_unchecked(sym.to_usize()) }
        }
    }

    #[inline]
    fn do_intern(&self, s: &[u8], alloc: impl FnOnce(&Arena, &[u8]) -> &'static [u8]) -> S {
        let map = self.map.pin();
        if let Some(v) = map.get(s) {
            return *v;
        }
        let s = alloc(&self.arena, s);
        *map.get_or_insert_with(s, || S::from_usize(self.strs.push(s)))
    }

    #[inline]
    fn do_intern_mut(&mut self, s: &[u8], alloc: impl FnOnce(&Arena, &[u8]) -> &'static [u8]) -> S {
        self.do_intern(s, alloc)
    }
}

#[inline]
fn alloc(arena: &Arena, s: &[u8]) -> &'static [u8] {
    // SAFETY: extends the lifetime of `&Arena` to `'static`. This is never exposed so it's ok.
    unsafe {
        std::mem::transmute::<&[u8], &'static [u8]>(arena.get_or_default().alloc_slice_copy(s))
    }
}

#[inline]
fn no_alloc(_: &Arena, s: &[u8]) -> &'static [u8] {
    // SAFETY: `s` outlives `arena`, so we don't need to allocate it. See above.
    unsafe { std::mem::transmute::<&[u8], &'static [u8]>(s) }
}
