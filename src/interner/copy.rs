use crate::{BytesInterner, InternerSymbol, Symbol};
use std::{collections::hash_map::RandomState, hash::BuildHasher, mem};

/// Copy type interner.
///
/// This is a thin wrapper around [`BytesInterner`] that interns values of a [`Copy`] type `T`.
///
/// `MIN_ALIGN` controls the minimum alignment of all allocations made by the internal arena and
/// should be set to `align_of::<T>()` for optimal performance. See [`BytesInterner`] for more
/// details.
///
/// See the [crate-level docs][crate] for more details.
pub struct CopyInterner<T, S = Symbol, H = RandomState, const MIN_ALIGN: usize = 1> {
    inner: BytesInterner<S, H, MIN_ALIGN>,
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

impl<T: Copy, S: InternerSymbol, H: BuildHasher, const MIN_ALIGN: usize>
    CopyInterner<T, S, H, MIN_ALIGN>
{
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
        self.inner.intern(as_bytes(value))
    }

    /// Interns a value, returning its unique `Symbol`.
    ///
    /// Allocates the value internally if it is not already interned.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, value: &T) -> S {
        self.inner.intern_mut(as_bytes(value))
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
        // SAFETY: The bytes are a valid representation of `T`. When `MIN_ALIGN >= align_of::<T>()`,
        // the pointer is guaranteed to be aligned; otherwise we use an unaligned read.
        unsafe {
            if MIN_ALIGN >= mem::align_of::<T>() {
                std::ptr::read(bytes.as_ptr().cast::<T>())
            } else {
                std::ptr::read_unaligned(bytes.as_ptr().cast::<T>())
            }
        }
    }
}
