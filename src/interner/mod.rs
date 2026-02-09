mod bytes;
pub use bytes::BytesInterner;

mod copy;
pub use self::copy::CopyInterner;

mod str;
pub use self::str::Interner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Symbol;

    const fn _assert_send_sync<T: Send + Sync>() {}
    const _: () = {
        _assert_send_sync::<Interner>();
        _assert_send_sync::<BytesInterner>();
        _assert_send_sync::<CopyInterner<u64>>();
    };

    macro_rules! basic {
        ($intern:ident) => {
            #[allow(unused_mut)]
            let mut interner = Interner::new();
            assert!(interner.inner.map.is_empty());

            let hello = interner.$intern("hello");
            assert_eq!(hello.get(), 0);
            assert_eq!(interner.resolve(hello), "hello");
            assert_eq!(interner.len(), 1);

            let world = interner.$intern("world");
            assert_eq!(world.get(), 1);
            assert_eq!(interner.resolve(world), "world");
            assert_eq!(interner.len(), 2);

            let hello2 = interner.$intern("hello");
            assert_eq!(hello, hello2);
            let hello3 = interner.$intern("hello");
            assert_eq!(hello, hello3);

            let world2 = interner.$intern("world");
            assert_eq!(world, world2);

            assert_eq!(interner.len(), 2);

            #[allow(unused_mut)]
            let mut interner2 = Interner::new();
            let prefill = &["hello", "world"];
            for &s in prefill {
                interner2.$intern(s);
            }
            assert_eq!(interner2.resolve(hello), "hello");
            assert_eq!(interner2.resolve(world), "world");
            assert_eq!(interner2.$intern("hello"), hello);
            assert_eq!(interner2.$intern("world"), world);
            assert_eq!(interner2.len(), 2);
        };
    }

    #[test]
    fn basic() {
        basic!(intern);
    }
    #[test]
    fn basic_mut() {
        basic!(intern_mut);
    }
    #[test]
    fn basic_static() {
        basic!(intern_static);
    }
    #[test]
    fn basic_mut_static() {
        basic!(intern_mut_static);
    }

    #[test]
    fn mt() {
        let interner = Interner::new();
        let symbols_per_thread = if cfg!(miri) { 5 } else { 5000 };
        let n_threads = if cfg!(miri) {
            2
        } else {
            std::thread::available_parallelism().map_or(4, usize::from)
        };

        std::thread::scope(|scope| {
            let intern_many = |salt: usize| {
                let intern_one = |i: usize| {
                    let s = format!("hello {salt} {i}");
                    let sym = interner.intern(&s);
                    assert_eq!(interner.resolve(sym), s);
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

    #[test]
    fn hash_collision() {
        #[derive(Default)]
        struct MyBadHasher;
        impl std::hash::Hasher for MyBadHasher {
            fn finish(&self) -> u64 {
                4 // Chosen by fair dice roll.
            }
            fn write(&mut self, _bytes: &[u8]) {}
        }

        let interner = Interner::<Symbol, _>::with_hasher(std::hash::BuildHasherDefault::<
            MyBadHasher,
        >::default());
        let hello = interner.intern("hello");
        let world = interner.intern("world");
        assert_eq!(hello.get(), 0);
        assert_eq!(world.get(), 1);
        assert_eq!(interner.resolve(hello), "hello");
        assert_eq!(interner.resolve(world), "world");
        assert_eq!(interner.len(), 2);
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
    fn copy_uses_t_eq() {
        let interner = CopyInterner::<HashEqKey>::new();

        let a = HashEqKey { key: 1, ignored: 100 };
        let b = HashEqKey { key: 1, ignored: 200 };
        assert_ne!(a.ignored, b.ignored);

        let sym_a = interner.intern(&a);
        let sym_b = interner.intern(&b);
        // T::Eq considers them equal, so they must get the same symbol.
        assert_eq!(sym_a, sym_b);
        assert_eq!(interner.len(), 1);

        // The resolved value should be the first one interned.
        let resolved = interner.resolve(sym_a);
        assert_eq!(resolved.key, 1);
        assert_eq!(resolved.ignored, 100);
    }

    #[test]
    fn copy_uses_t_hash() {
        let interner = CopyInterner::<HashEqKey>::new();

        let a = HashEqKey { key: 1, ignored: 100 };
        let b = HashEqKey { key: 2, ignored: 100 };

        let sym_a = interner.intern(&a);
        let sym_b = interner.intern(&b);
        // Different keys, so T::Hash produces different hashes and they are distinct.
        assert_ne!(sym_a, sym_b);
        assert_eq!(interner.len(), 2);

        assert_eq!(interner.resolve(sym_a).key, 1);
        assert_eq!(interner.resolve(sym_b).key, 2);
    }
}
