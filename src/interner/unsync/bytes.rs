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

impl_interner_api!(
    BytesInterner,
    value = [u8],
    mut_note = "this never uses shared interior mutability.",
    cheap_op = "cheap operation"
);

impl<S: InternerSymbol, H: BuildHasher> BytesInterner<S, H> {
    #[doc(hidden)]
    fn with_capacity_and_hasher_impl(capacity: usize, hash_builder: H) -> Self {
        let map = Map::with_capacity(capacity);
        let strs = Vec::with_capacity(capacity);
        Self {
            map: UnsafeCell::new(map),
            hash_builder,
            strs: UnsafeCell::new(strs),
            arena: Bump::new(),
        }
    }

    #[doc(hidden)]
    #[inline]
    fn len_impl(&self) -> usize {
        // SAFETY: This type is not `Sync`, and this method only reads the vector length.
        unsafe { (*self.strs.get()).len() }
    }

    #[doc(hidden)]
    #[inline]
    fn intern_impl(&self, s: &[u8]) -> S {
        self.do_intern(s, alloc)
    }

    #[doc(hidden)]
    #[inline]
    fn intern_mut_impl(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, alloc)
    }

    #[doc(hidden)]
    #[inline]
    fn intern_static_impl(&self, s: &'static [u8]) -> S {
        self.do_intern(s, no_alloc)
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn intern_static_unchecked_impl(&self, s: &[u8]) -> S {
        self.do_intern(s, no_alloc_unchecked)
    }

    #[doc(hidden)]
    #[inline]
    fn intern_mut_static_impl(&mut self, s: &'static [u8]) -> S {
        self.do_intern_mut(s, no_alloc)
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn intern_mut_static_unchecked_impl(&mut self, s: &[u8]) -> S {
        self.do_intern_mut(s, no_alloc_unchecked)
    }

    #[doc(hidden)]
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    fn resolve_impl(&self, sym: S) -> &[u8] {
        // SAFETY: This type is not `Sync`, and interned slices outlive the vector slot.
        let strs = unsafe { &*self.strs.get() };
        if cfg!(debug_assertions) {
            strs.get(sym.to_usize()).copied().expect("symbol out of bounds")
        } else {
            unsafe { strs.get_unchecked(sym.to_usize()) }
        }
    }

    #[doc(hidden)]
    #[inline]
    fn try_resolve_impl(&self, sym: S) -> Option<&[u8]> {
        // SAFETY: This type is not `Sync`, and interned slices outlive the vector slot.
        let strs = unsafe { &*self.strs.get() };
        strs.get(sym.to_usize()).copied()
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
