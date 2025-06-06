mod bytes;
pub use bytes::BytesInterner;

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
}
