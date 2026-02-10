use super::bytes::{NoHasherBuilder, cvt, cvt_mut};
use crate::{InternerSymbol, Symbol};
use boxcar::Vec as LFVec;
use dashmap::DashMap;
use hashbrown::hash_table;
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
};

type Map<T, S> = DashMap<MapKey<T>, S, NoHasherBuilder>;
type MapKey<T> = (u64, &'static T);
type RawMapKey<T, S> = (MapKey<T>, S);

/// Copy type interner.
///
/// This interns values of a [`Copy`] type `T`, storing them directly without indirection.
///
/// Values are hashed using `T`'s [`Hash`] implementation and compared using `T`'s [`Eq`]
/// implementation.
///
/// See the [crate-level docs][crate] for more details.
pub struct CopyInterner<T: 'static, S = Symbol, H = RandomState> {
    map: Map<T, S>,
    hash_builder: H,
    values: LFVec<T>,
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

impl<T: Copy + Hash + Eq, S: InternerSymbol, H: BuildHasher> CopyInterner<T, S, H> {
    /// Creates a new `CopyInterner` with the given custom hasher.
    #[inline]
    pub fn with_hasher(hash_builder: H) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates a new `CopyInterner` with the given capacity and custom hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: H) -> Self {
        Self {
            map: Map::with_capacity_and_hasher(capacity, Default::default()),
            hash_builder,
            values: LFVec::with_capacity(capacity),
        }
    }

    /// Returns the number of unique values in the interner.
    #[inline]
    pub fn len(&self) -> usize {
        self.values.count()
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
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (S, &T)> + Clone + '_ {
        self.all_symbols().map(|s| (s, self.resolve(s)))
    }

    /// Returns an iterator over all symbols in the interner.
    #[inline]
    pub fn all_symbols(&self) -> impl ExactSizeIterator<Item = S> + Send + Sync + Clone {
        (0..self.len()).map(S::from_usize)
    }

    /// Interns a value, returning its unique `Symbol`.
    ///
    /// Stores the value internally if it is not already interned.
    pub fn intern(&self, value: &T) -> S {
        let hash = self.hash_builder.hash_one(value);
        let shard_idx = self.map.determine_shard(hash as usize);
        let shard = &*self.map.shards()[shard_idx];

        let eq = mk_eq(value);
        if let Some((_, v)) = cvt(&shard.read()).find(hash, eq) {
            return *v.get();
        }

        get_or_insert(&self.values, *value, hash, cvt_mut(&mut shard.write()), eq)
    }

    /// Interns a value, returning its unique `Symbol`.
    ///
    /// Stores the value internally if it is not already interned.
    ///
    /// By taking `&mut self`, this never acquires any locks.
    pub fn intern_mut(&mut self, value: &T) -> S {
        let hash = self.hash_builder.hash_one(value);
        let shard_idx = self.map.determine_shard(hash as usize);
        let shard = &mut *self.map.shards_mut()[shard_idx];

        get_or_insert(&self.values, *value, hash, cvt_mut(shard.get_mut()), mk_eq(value))
    }

    /// Interns multiple values.
    ///
    /// Stores the values internally if they are not already interned.
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
    /// Stores the values internally if they are not already interned.
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

    /// Maps a `Symbol` to its value. This is a cheap, lock-free operation.
    ///
    /// # Panics
    ///
    /// Panics if `Symbol` is out of bounds of this `CopyInterner`. You should only use `Symbol`s
    /// created by this `CopyInterner`.
    #[inline]
    #[must_use]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn resolve(&self, sym: S) -> &T {
        if cfg!(debug_assertions) {
            self.values.get(sym.to_usize()).expect("symbol out of bounds")
        } else {
            unsafe { self.values.get_unchecked(sym.to_usize()) }
        }
    }
}

#[inline]
fn mk_eq<'a, T: Copy + Eq, S>(value: &'a T) -> impl Fn(&RawMapKey<T, S>) -> bool + Copy + 'a {
    move |((_, v), _): &RawMapKey<T, S>| *value == **v
}

#[inline]
fn hasher<T, S>(((hash, _), _): &RawMapKey<T, S>) -> u64 {
    *hash
}

#[inline]
fn get_or_insert<T: Copy, S: InternerSymbol>(
    values: &LFVec<T>,
    value: T,
    hash: u64,
    shard: &mut hash_table::HashTable<RawMapKey<T, dashmap::SharedValue<S>>>,
    eq: impl Fn(&RawMapKey<T, dashmap::SharedValue<S>>) -> bool + Copy,
) -> S {
    match shard.entry(hash, eq, hasher) {
        hash_table::Entry::Occupied(e) => *e.get().1.get(),
        hash_table::Entry::Vacant(e) => {
            let i = values.push(value);
            let new_sym = S::from_usize(i);
            // SAFETY: `boxcar::Vec` has stable addresses (bucket-based, never reallocates).
            // The vec outlives all references; same justification as `BytesInterner::alloc`.
            let static_ref =
                unsafe { std::mem::transmute::<&T, &'static T>(values.get_unchecked(i)) };
            e.insert(((hash, static_ref), dashmap::SharedValue::new(new_sym)));
            new_sym
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn _assert_send_sync<T: Send + Sync>() {}
    const _: () = {
        _assert_send_sync::<CopyInterner<u64>>();
        _assert_send_sync::<CopyInterner<[u8; 64]>>();
    };

    macro_rules! basic {
        ($intern:ident, $T:ty, $make:expr) => {
            #[allow(unused_mut)]
            let mut interner = CopyInterner::<$T>::new();

            let a: $T = $make(1);
            let b: $T = $make(2);
            let sym_a = interner.$intern(&a);
            assert_eq!(sym_a.get(), 0);
            assert_eq!(*interner.resolve(sym_a), a);
            assert_eq!(interner.len(), 1);

            let sym_b = interner.$intern(&b);
            assert_eq!(sym_b.get(), 1);
            assert_eq!(*interner.resolve(sym_b), b);
            assert_eq!(interner.len(), 2);

            let sym_a2 = interner.$intern(&a);
            assert_eq!(sym_a, sym_a2);
            let sym_a3 = interner.$intern(&a);
            assert_eq!(sym_a, sym_a3);

            let sym_b2 = interner.$intern(&b);
            assert_eq!(sym_b, sym_b2);

            assert_eq!(interner.len(), 2);
        };
    }

    #[test]
    fn basic_u64() {
        basic!(intern, u64, |i: u64| i);
    }
    #[test]
    fn basic_u64_mut() {
        basic!(intern_mut, u64, |i: u64| i);
    }
    #[test]
    fn basic_big() {
        basic!(intern, [u8; 64], |i: u64| {
            let mut v = [0u8; 64];
            v[..8].copy_from_slice(&i.to_le_bytes());
            v
        });
    }
    #[test]
    fn basic_big_mut() {
        basic!(intern_mut, [u8; 64], |i: u64| {
            let mut v = [0u8; 64];
            v[..8].copy_from_slice(&i.to_le_bytes());
            v
        });
    }

    #[test]
    fn mt_u64() {
        let interner = CopyInterner::<u64>::new();
        let symbols_per_thread: u64 = if cfg!(miri) { 5 } else { 5000 };
        let n_threads = if cfg!(miri) {
            2
        } else {
            std::thread::available_parallelism().map_or(4, usize::from)
        };

        std::thread::scope(|scope| {
            let intern_many = |salt: u64| {
                let intern_one = |i: u64| {
                    let value = salt * symbols_per_thread + i;
                    let sym = interner.intern(&value);
                    assert_eq!(*interner.resolve(sym), value);
                };
                for i in 0..symbols_per_thread {
                    intern_one(i);
                    intern_one(i);
                }
            };
            for i in 0..n_threads {
                scope.spawn(move || intern_many(i as u64));
            }
        });

        assert_eq!(interner.len(), n_threads * symbols_per_thread as usize);
    }

    #[test]
    fn mt_big() {
        let interner = CopyInterner::<[u8; 64]>::new();
        let symbols_per_thread: usize = if cfg!(miri) { 5 } else { 5000 };
        let n_threads = if cfg!(miri) {
            2
        } else {
            std::thread::available_parallelism().map_or(4, usize::from)
        };

        std::thread::scope(|scope| {
            let intern_many = |salt: usize| {
                let intern_one = |i: usize| {
                    let n = (salt * symbols_per_thread + i) as u64;
                    let mut value = [0u8; 64];
                    value[..8].copy_from_slice(&n.to_le_bytes());
                    let sym = interner.intern(&value);
                    assert_eq!(*interner.resolve(sym), value);
                };
                for i in 0..symbols_per_thread {
                    intern_one(i);
                    intern_one(i);
                }
            };
            for i in 0..n_threads {
                scope.spawn(move || intern_many(i));
            }
        });

        assert_eq!(interner.len(), n_threads * symbols_per_thread);
    }

    /// A type where Hash and Eq only consider the `key` field, ignoring `ignored`.
    /// This means two values with the same `key` but different `ignored` are equal
    /// and hash the same, even though their byte representations differ.
    #[derive(Clone, Copy, Debug)]
    struct HashEqKey {
        key: u32,
        ignored: u32,
    }

    impl std::hash::Hash for HashEqKey {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.key.hash(state);
        }
    }

    impl PartialEq for HashEqKey {
        fn eq(&self, other: &Self) -> bool {
            self.key == other.key
        }
    }

    impl Eq for HashEqKey {}

    #[test]
    fn uses_t_eq() {
        let interner = CopyInterner::<HashEqKey>::new();

        let a = HashEqKey { key: 1, ignored: 100 };
        let b = HashEqKey { key: 1, ignored: 200 };
        assert_ne!(a.ignored, b.ignored);

        let sym_a = interner.intern(&a);
        let sym_b = interner.intern(&b);
        assert_eq!(sym_a, sym_b);
        assert_eq!(interner.len(), 1);

        let resolved = *interner.resolve(sym_a);
        assert_eq!(resolved.key, 1);
        assert_eq!(resolved.ignored, 100);
    }

    #[test]
    fn uses_t_hash() {
        let interner = CopyInterner::<HashEqKey>::new();

        let a = HashEqKey { key: 1, ignored: 100 };
        let b = HashEqKey { key: 2, ignored: 100 };

        let sym_a = interner.intern(&a);
        let sym_b = interner.intern(&b);
        assert_ne!(sym_a, sym_b);
        assert_eq!(interner.len(), 2);

        assert_eq!(interner.resolve(sym_a).key, 1);
        assert_eq!(interner.resolve(sym_b).key, 2);
    }
}
