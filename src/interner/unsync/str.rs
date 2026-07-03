use super::BytesInterner;
use crate::{InternerSymbol, Symbol};
use std::{collections::hash_map::RandomState, hash::BuildHasher};

/// Non-thread-safe string interner.
///
/// This is a thin wrapper around [`BytesInterner`] that uses `str` instead of `[u8]`.
///
/// See the [crate-level docs][crate] for more details.
pub struct Interner<S = Symbol, H = RandomState> {
    pub(crate) inner: BytesInterner<S, H>,
}

impl_interner_api!(
    Interner,
    value = str,
    mut_note = "this never uses shared interior mutability.",
    cheap_op = "cheap operation"
);

impl<S: InternerSymbol, H: BuildHasher> Interner<S, H> {
    fn with_capacity_and_hasher_impl(capacity: usize, hash_builder: H) -> Self {
        Self { inner: BytesInterner::with_capacity_and_hasher(capacity, hash_builder) }
    }

    #[inline]
    fn len_impl(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    fn intern_impl(&self, s: &str) -> S {
        self.inner.intern(s.as_bytes())
    }

    #[inline]
    fn intern_mut_impl(&mut self, s: &str) -> S {
        self.inner.intern_mut(s.as_bytes())
    }

    #[inline]
    fn intern_static_impl(&self, s: &'static str) -> S {
        self.inner.intern_static(s.as_bytes())
    }

    #[inline]
    unsafe fn intern_static_unchecked_impl(&self, s: &str) -> S {
        // SAFETY: The caller upholds the same lifetime requirement for `s`.
        unsafe { self.inner.intern_static_unchecked(s.as_bytes()) }
    }

    #[inline]
    fn intern_mut_static_impl(&mut self, s: &'static str) -> S {
        self.inner.intern_mut_static(s.as_bytes())
    }

    #[inline]
    unsafe fn intern_mut_static_unchecked_impl(&mut self, s: &str) -> S {
        // SAFETY: The caller upholds the same lifetime requirement for `s`.
        unsafe { self.inner.intern_mut_static_unchecked(s.as_bytes()) }
    }

    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    fn resolve_impl(&self, sym: S) -> &str {
        // SAFETY: Only `str`s are interned.
        unsafe { std::str::from_utf8_unchecked(self.inner.resolve(sym)) }
    }

    #[inline]
    fn try_resolve_impl(&self, sym: S) -> Option<&str> {
        self.inner.try_resolve(sym).map(|s| {
            // SAFETY: Only `str`s are interned.
            unsafe { std::str::from_utf8_unchecked(s) }
        })
    }
}
