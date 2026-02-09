use crate::{BytesInterner, InternerSymbol, Symbol};
use std::{collections::hash_map::RandomState, hash::BuildHasher, mem};

use super::bytes::Arena;

/// Copy type interner.
///
/// This is a thin wrapper around [`BytesInterner`] that interns values of a [`Copy`] type `T`.
///
/// Allocated values are aligned to `align_of::<T>()`.
///
/// See the [crate-level docs][crate] for more details.
pub struct CopyInterner<T, S = Symbol, H = RandomState> {
    inner: BytesInterner<S, H>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Copy> Default for CopyInterner<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy> CopyInterner<T, Symbol, RandomState> {
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

impl<T: Copy, S: InternerSymbol, H: BuildHasher> CopyInterner<T, S, H> {
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
    pub fn intern(&self, value: &T) -> S {
        self.inner.do_intern(as_bytes(value), alloc_aligned::<T>)
    }

    /// Interns a value, returning its unique `Symbol`.
    ///
    /// Allocates the value internally if it is not already interned.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, value: &T) -> S {
        self.inner.do_intern_mut(as_bytes(value), alloc_aligned::<T>)
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
        let bytes = self.inner.resolve(sym);
        debug_assert_eq!(bytes.len(), mem::size_of::<T>());
        debug_assert_eq!(bytes.as_ptr() as usize % mem::align_of::<T>(), 0);
        // SAFETY: The bytes are a valid representation of `T`, allocated with proper alignment.
        unsafe { std::ptr::read(bytes.as_ptr().cast::<T>()) }
    }
}
