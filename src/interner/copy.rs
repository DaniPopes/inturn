use super::bytes::{Arena, RawMapKey};
use crate::{BytesInterner, InternerSymbol, Symbol};
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    mem,
};

/// Copy type interner.
///
/// This is a thin wrapper around [`BytesInterner`] that interns values of a [`Copy`] type `T`.
///
/// Values are hashed using `T`'s [`Hash`] implementation and compared using `T`'s [`Eq`]
/// implementation. Allocated values are aligned to `align_of::<T>()`.
///
/// See the [crate-level docs][crate] for more details.
pub struct CopyInterner<T, S = Symbol, H = RandomState> {
    inner: BytesInterner<S, H>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Copy + Hash + Eq> Default for CopyInterner<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Hash + Eq> CopyInterner<T, Symbol, RandomState> {
    /// Creates a new, empty `CopyInterner` with the default symbol and hasher.
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new `CopyInterner` with the given capacity and default symbol and hasher.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, Default::default())
    }
}

fn as_bytes<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value as *const T as *const u8, mem::size_of::<T>()) }
}

unsafe fn from_bytes<T: Copy>(bytes: &[u8]) -> T {
    debug_assert_eq!(bytes.len(), mem::size_of::<T>());
    debug_assert_eq!(bytes.as_ptr() as usize % mem::align_of::<T>(), 0);
    unsafe { std::ptr::read(bytes.as_ptr().cast::<T>()) }
}

fn mk_eq<T: Copy + Eq, S>(value: &T) -> impl Fn(&RawMapKey<S>) -> bool + Copy + '_ {
    move |((_, ss), _): &RawMapKey<S>| *value == unsafe { from_bytes::<T>(ss) }
}

fn alloc_aligned<T>(arena: &Arena, s: &[u8]) -> &'static [u8] {
    let bump = arena.get_or_default();
    let layout =
        std::alloc::Layout::from_size_align(s.len(), mem::align_of::<T>()).expect("invalid layout");
    // SAFETY: Layout is valid, and we initialize the allocated memory immediately.
    unsafe {
        let ptr = bump.alloc_layout(layout).as_ptr();
        std::ptr::copy_nonoverlapping(s.as_ptr(), ptr, s.len());
        // SAFETY: Extends the lifetime. The arena outlives all references; same justification as
        // `BytesInterner::alloc`.
        std::mem::transmute::<&[u8], &'static [u8]>(std::slice::from_raw_parts(ptr, s.len()))
    }
}

fn no_alloc(_: &Arena, s: &[u8]) -> &'static [u8] {
    // SAFETY: `s` outlives the arena, so we don't need to allocate it.
    unsafe { std::mem::transmute::<&[u8], &'static [u8]>(s) }
}

impl<T: Copy + Hash + Eq, S: InternerSymbol, H: BuildHasher> CopyInterner<T, S, H> {
    /// Creates a new `CopyInterner` with the given custom hasher.
    #[inline]
    pub fn with_hasher(hash_builder: H) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates a new `CopyInterner` with the given capacity and custom hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: H) -> Self {
        Self {
            inner: BytesInterner::with_capacity_and_hasher(capacity, hash_builder),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns the number of unique values in the interner.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the interner is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the interned values and their corresponding `Symbol`s.
    ///
    /// Does not guarantee that it includes symbols added after the iterator was created.
    #[inline]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (S, T)> + Clone + '_ {
        self.all_symbols().map(|s| (s, self.resolve(s)))
    }

    /// Returns an iterator over all symbols in the interner.
    #[inline]
    pub fn all_symbols(&self) -> impl ExactSizeIterator<Item = S> + Send + Sync + Clone {
        (0..self.len()).map(S::from_usize)
    }

    /// Interns a value, returning its unique `Symbol`.
    ///
    /// Allocates the value internally if it is not already interned.
    ///
    /// If `value` outlives `self`, prefer using [`intern_static`](Self::intern_static), as it will
    /// not allocate on the heap.
    pub fn intern(&self, value: &T) -> S {
        let hash = self.inner.hash_one(value);
        self.inner.do_intern_prehashed(hash, as_bytes(value), mk_eq(value), alloc_aligned::<T>)
    }

    /// Interns a value, returning its unique `Symbol`.
    ///
    /// Allocates the value internally if it is not already interned.
    ///
    /// If `value` outlives `self`, prefer using [`intern_mut_static`](Self::intern_mut_static), as
    /// it will not allocate on the heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, value: &T) -> S {
        let hash = self.inner.hash_one(value);
        self.inner.do_intern_mut_prehashed(hash, as_bytes(value), mk_eq(value), alloc_aligned::<T>)
    }

    /// Interns a static value, returning its unique `Symbol`.
    ///
    /// Note that this only requires that `value` outlives `self`, which means we can avoid
    /// allocating.
    pub fn intern_static<'a, 'b: 'a>(&'a self, value: &'b T) -> S {
        let hash = self.inner.hash_one(value);
        self.inner.do_intern_prehashed(hash, as_bytes(value), mk_eq(value), no_alloc)
    }

    /// Interns a static value, returning its unique `Symbol`.
    ///
    /// Note that this only requires that `value` outlives `self`, which means we can avoid
    /// allocating.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut_static<'a, 'b: 'a>(&'a mut self, value: &'b T) -> S {
        let hash = self.inner.hash_one(value);
        self.inner.do_intern_mut_prehashed(hash, as_bytes(value), mk_eq(value), no_alloc)
    }

    /// Interns multiple values.
    ///
    /// Allocates the values internally if they are not already interned.
    ///
    /// If the values outlive `self`, prefer using [`intern_many_static`](Self::intern_many_static),
    /// as it will not allocate on the heap.
    pub fn intern_many<'a>(&self, values: impl IntoIterator<Item = &'a T>)
    where
        T: 'a,
    {
        for v in values {
            self.intern(v);
        }
    }

    /// Interns multiple values.
    ///
    /// Allocates the values internally if they are not already interned.
    ///
    /// If the values outlive `self`, prefer using
    /// [`intern_many_mut_static`](Self::intern_many_mut_static), as it will not allocate on the
    /// heap.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut<'a>(&mut self, values: impl IntoIterator<Item = &'a T>)
    where
        T: 'a,
    {
        for v in values {
            self.intern_mut(v);
        }
    }

    /// Interns multiple static values.
    ///
    /// Note that this only requires that the values outlive `self`, which means we can avoid
    /// allocating.
    pub fn intern_many_static<'a, 'b: 'a>(&'a self, values: impl IntoIterator<Item = &'b T>)
    where
        T: 'b,
    {
        for v in values {
            self.intern_static(v);
        }
    }

    /// Interns multiple static values.
    ///
    /// Note that this only requires that the values outlive `self`, which means we can avoid
    /// allocating.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_many_mut_static<'a, 'b: 'a>(&'a mut self, values: impl IntoIterator<Item = &'b T>)
    where
        T: 'b,
    {
        for v in values {
            self.intern_mut_static(v);
        }
    }

    /// Maps a `Symbol` to its value. This is a cheap, lock-free operation.
    ///
    /// # Panics
    ///
    /// Panics if `Symbol` is out of bounds of this `CopyInterner`. You should only use `Symbol`s
    /// created by this `CopyInterner`.
    #[inline]
    #[must_use]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn resolve(&self, sym: S) -> T {
        // SAFETY: The bytes are a valid representation of `T`, allocated with proper alignment.
        unsafe { from_bytes(self.inner.resolve(sym)) }
    }
}
