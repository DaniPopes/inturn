use std::num::NonZeroU32;

/// Trait for types that can be used as symbols in an `Interner`.
pub trait InternerSymbol: Sized + Copy + std::hash::Hash + Eq {
    /// Tries to create a new `Symbol` from a `usize`.
    fn try_from_usize(id: usize) -> Option<Self>;

    /// Creates a new `Symbol` from a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if `id` is an invalid index for `Self`.
    #[inline]
    #[track_caller]
    fn from_usize(id: usize) -> Self {
        Self::try_from_usize(id).expect("invalid index")
    }

    /// Converts the `Symbol` to a `usize`.
    fn to_usize(self) -> usize;
}

/// Default unique identifier for a string in an `Interner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(NonZeroU32);

impl Symbol {
    /// Creates a new `Symbol` from a `u32`.
    ///
    /// # Panics
    ///
    /// Panics if `id` is `u32::MAX`.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self::try_new(id).unwrap()
    }

    /// Tries to create a new `Symbol` from a `u32`.
    #[inline]
    pub fn try_new(id: u32) -> Option<Self> {
        if id < u32::MAX {
            // SAFETY: `id` is less than `u32::MAX`.
            Some(unsafe { Self::new_unchecked(id) })
        } else {
            None
        }
    }

    /// Creates a new `Symbol` from a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if `id` is greater than or equal to `u32::MAX`.
    #[inline]
    #[track_caller]
    pub fn from_usize(id: usize) -> Self {
        <Self as InternerSymbol>::from_usize(id)
    }

    /// Tries to create a new `Symbol` from a `usize`.
    #[inline]
    pub fn try_from_usize(id: usize) -> Option<Self> {
        if id < u32::MAX as usize {
            // SAFETY: `id` is less than `u32::MAX`.
            Some(unsafe { Self::new_unchecked(id as u32) })
        } else {
            None
        }
    }

    /// Creates a new `Symbol` from a `u32` without checking if it is valid.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `id` is less than `u32::MAX`.
    #[inline]
    pub unsafe fn new_unchecked(id: u32) -> Self {
        Self(unsafe { NonZeroU32::new_unchecked(id.unchecked_add(1)) })
    }

    /// Returns the `u32` value of the `Symbol`.
    #[inline]
    pub fn get(self) -> u32 {
        // SAFETY: `NonZeroU32` is guaranteed to be non-zero, so we can safely
        // subtract 1 to get the original id.
        unsafe { self.0.get().unchecked_sub(1) }
    }

    /// Returns the `usize` value of the `Symbol`.
    #[inline]
    pub fn to_usize(self) -> usize {
        self.get() as usize
    }
}

impl InternerSymbol for Symbol {
    #[inline]
    fn try_from_usize(id: usize) -> Option<Self> {
        Self::try_from_usize(id)
    }

    #[inline]
    fn to_usize(self) -> usize {
        self.to_usize()
    }
}
