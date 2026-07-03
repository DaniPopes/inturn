use super::BytesInterner;
use crate::{InternerSymbol, Symbol};
use std::{collections::hash_map::RandomState, hash::BuildHasher};

/// String interner.
///
/// This is a thin wrapper around [`BytesInterner`] that uses `str` instead of `[u8]`.
///
/// See the [crate-level docs][crate] for more details.
pub struct Interner<S = Symbol, H = RandomState> {
    pub(crate) inner: BytesInterner<S, H>,
}

impl Default for Interner {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Interner<Symbol, RandomState> {
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

impl<S: InternerSymbol, H: BuildHasher> Interner<S, H> {
    /// Creates a new `Interner` with the given custom hasher.
    #[inline]
    pub fn with_hasher(hash_builder: H) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates a new `Interner` with the given capacitiy and custom hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: H) -> Self {
        Self { inner: BytesInterner::with_capacity_and_hasher(capacity, hash_builder) }
    }

    /// Returns the number of unique strings in the interner.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
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
    /// If `s` is `&'static str`, prefer using
    /// [`intern_static`](Self::intern_static), as it will not allocate the string on the heap.
    pub fn intern(&self, s: &str) -> S {
        self.inner.intern(s.as_bytes())
    }

    /// Interns a string, returning its unique `Symbol`.
    ///
    /// Allocates the string internally if it is not already interned.
    ///
    /// If `s` is `&'static str`, prefer using
    /// [`intern_mut_static`](Self::intern_mut_static), as it will not allocate the string on the
    /// heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, s: &str) -> S {
        self.inner.intern_mut(s.as_bytes())
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// The input must be `'static`, which means we can avoid allocating the string.
    ///
    /// For non-`'static` inputs that outlive this interner, see
    /// [`intern_static_unchecked`](Self::intern_static_unchecked).
    pub fn intern_static(&self, s: &'static str) -> S {
        self.inner.intern_static(s.as_bytes())
    }

    /// Interns a string without allocating, returning its unique `Symbol`.
    ///
    /// This is the unchecked version of [`intern_static`](Self::intern_static) for inputs that are
    /// not typed as `'static`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `s` remains valid and unchanged until this interner is dropped.
    pub unsafe fn intern_static_unchecked(&self, s: &str) -> S {
        // SAFETY: The caller upholds the same lifetime requirement for `s`.
        unsafe { self.inner.intern_static_unchecked(s.as_bytes()) }
    }

    /// Interns a static string, returning its unique `Symbol`.
    ///
    /// The input must be `'static`, which means we can avoid allocating the string.
    ///
    /// For non-`'static` inputs that outlive this interner, see
    /// [`intern_mut_static_unchecked`](Self::intern_mut_static_unchecked).
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut_static(&mut self, s: &'static str) -> S {
        self.inner.intern_mut_static(s.as_bytes())
    }

    /// Interns a string without allocating, returning its unique `Symbol`.
    ///
    /// This is the unchecked version of [`intern_mut_static`](Self::intern_mut_static) for inputs
    /// that are not typed as `'static`.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `s` remains valid and unchanged until this interner is dropped.
    pub unsafe fn intern_mut_static_unchecked(&mut self, s: &str) -> S {
        // SAFETY: The caller upholds the same lifetime requirement for `s`.
        unsafe { self.inner.intern_mut_static_unchecked(s.as_bytes()) }
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings are `&'static str`, prefer using
    /// [`intern_many_static`](Self::intern_many_static), as it will not allocate the strings on the
    /// heap.
    pub fn intern_many<'a>(&self, strings: impl IntoIterator<Item = &'a str>) {
        self.inner.intern_many(strings.into_iter().map(str::as_bytes));
    }

    /// Interns multiple strings.
    ///
    /// Allocates the strings internally if they are not already interned.
    ///
    /// If the strings are `&'static str`, prefer using
    /// [`intern_many_mut_static`](Self::intern_many_mut_static), as it will not allocate the
    /// strings on the heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut<'a>(&mut self, strings: impl IntoIterator<Item = &'a str>) {
        self.inner.intern_many_mut(strings.into_iter().map(str::as_bytes));
    }

    /// Interns multiple static strings.
    ///
    /// The inputs must be `'static`, which means we can avoid allocating the strings.
    pub fn intern_many_static(&self, strings: impl IntoIterator<Item = &'static str>) {
        self.inner.intern_many_static(strings.into_iter().map(str::as_bytes));
    }

    /// Interns multiple strings without allocating.
    ///
    /// This is the unchecked version of [`intern_many_static`](Self::intern_many_static) for inputs
    /// that are not typed as `'static`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all inputs remain valid and unchanged until this interner is
    /// dropped.
    pub unsafe fn intern_many_static_unchecked<'a>(
        &self,
        strings: impl IntoIterator<Item = &'a str>,
    ) {
        // SAFETY: The caller upholds the same lifetime requirement for all inputs.
        unsafe { self.inner.intern_many_static_unchecked(strings.into_iter().map(str::as_bytes)) }
    }

    /// Interns multiple static strings.
    ///
    /// The inputs must be `'static`, which means we can avoid allocating the strings.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut_static(&mut self, strings: impl IntoIterator<Item = &'static str>) {
        self.inner.intern_many_mut_static(strings.into_iter().map(str::as_bytes));
    }

    /// Interns multiple strings without allocating.
    ///
    /// This is the unchecked version of [`intern_many_mut_static`](Self::intern_many_mut_static)
    /// for inputs that are not typed as `'static`.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all inputs remain valid and unchanged until this interner is
    /// dropped.
    pub unsafe fn intern_many_mut_static_unchecked<'a>(
        &mut self,
        strings: impl IntoIterator<Item = &'a str>,
    ) {
        // SAFETY: The caller upholds the same lifetime requirement for all inputs.
        unsafe {
            self.inner.intern_many_mut_static_unchecked(strings.into_iter().map(str::as_bytes));
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
    pub fn resolve(&self, sym: S) -> &str {
        // SAFETY: Only `str`s are interned.
        unsafe { std::str::from_utf8_unchecked(self.inner.resolve(sym)) }
    }
}
