#[rustfmt::skip]
macro_rules! impl_interner_api {
    (
        $ty:ident,
        value = $value:ty,
        mut_note = $mut_note:literal,
        cheap_op = $cheap_op:literal
    ) => {
        impl Default for $ty {
            #[inline]
            fn default() -> Self {
                Self::new()
            }
        }

        impl $ty<Symbol, RandomState> {
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

        impl<S: InternerSymbol, H: BuildHasher> $ty<S, H> {
            /// Creates a new `Interner` with the given custom hasher.
            #[inline]
            pub fn with_hasher(hash_builder: H) -> Self {
                Self::with_capacity_and_hasher(0, hash_builder)
            }

            /// Creates a new `Interner` with the given capacitiy and custom hasher.
            #[inline]
            pub fn with_capacity_and_hasher(capacity: usize, hash_builder: H) -> Self {
                Self::with_capacity_and_hasher_impl(capacity, hash_builder)
            }

            /// Returns the number of unique strings in the interner.
            #[inline]
            pub fn len(&self) -> usize {
                self.len_impl()
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
            pub fn iter(&self) -> impl ExactSizeIterator<Item = (S, &$value)> + Clone {
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
            #[doc = concat!("If `s` is `&'static ", stringify!($value), "`, prefer using")]
            /// [`intern_static`](Self::intern_static), as it will not allocate the string on the
            /// heap.
            #[inline]
            pub fn intern(&self, s: &$value) -> S {
                self.intern_impl(s)
            }

            /// Interns a string, returning its unique `Symbol`.
            ///
            /// Allocates the string internally if it is not already interned.
            #[doc = concat!("If `s` is `&'static ", stringify!($value), "`, prefer using")]
            /// [`intern_mut_static`](Self::intern_mut_static), as it will not allocate the string
            /// on the heap.
            #[doc = concat!("By taking `&mut self`, ", $mut_note)]
            #[inline]
            pub fn intern_mut(&mut self, s: &$value) -> S {
                self.intern_mut_impl(s)
            }

            /// Interns a static string, returning its unique `Symbol`.
            ///
            /// The input must be `'static`, which means we can avoid allocating the string.
            ///
            /// For non-`'static` inputs that outlive this interner, see
            /// [`intern_static_unchecked`](Self::intern_static_unchecked).
            #[inline]
            pub fn intern_static(&self, s: &'static $value) -> S {
                self.intern_static_impl(s)
            }

            /// Interns a string without allocating, returning its unique `Symbol`.
            ///
            /// This is the unchecked version of [`intern_static`](Self::intern_static) for inputs
            /// that are not typed as `'static`.
            ///
            /// # Safety
            ///
            /// The caller must ensure that `s` remains valid and unchanged until this interner is
            /// dropped.
            #[inline]
            pub unsafe fn intern_static_unchecked(&self, s: &$value) -> S {
                // SAFETY: This method has the same safety requirements as the helper.
                unsafe { self.intern_static_unchecked_impl(s) }
            }

            /// Interns a static string, returning its unique `Symbol`.
            ///
            /// The input must be `'static`, which means we can avoid allocating the string.
            ///
            /// For non-`'static` inputs that outlive this interner, see
            /// [`intern_mut_static_unchecked`](Self::intern_mut_static_unchecked).
            #[doc = concat!("By taking `&mut self`, ", $mut_note)]
            #[inline]
            pub fn intern_mut_static(&mut self, s: &'static $value) -> S {
                self.intern_mut_static_impl(s)
            }

            /// Interns a string without allocating, returning its unique `Symbol`.
            ///
            /// This is the unchecked version of [`intern_mut_static`](Self::intern_mut_static) for
            /// inputs that are not typed as `'static`.
            #[doc = concat!("By taking `&mut self`, ", $mut_note)]
            ///
            /// # Safety
            ///
            /// The caller must ensure that `s` remains valid and unchanged until this interner is
            /// dropped.
            #[inline]
            pub unsafe fn intern_mut_static_unchecked(&mut self, s: &$value) -> S {
                // SAFETY: This method has the same safety requirements as the helper.
                unsafe { self.intern_mut_static_unchecked_impl(s) }
            }

            /// Interns multiple strings.
            ///
            /// Allocates the strings internally if they are not already interned.
            #[doc = concat!("If the strings are `&'static ", stringify!($value), "`, prefer using")]
            /// [`intern_many_static`](Self::intern_many_static), as it will not allocate the
            /// strings on the heap.
            #[inline]
            pub fn intern_many<'a>(&self, strings: impl IntoIterator<Item = &'a $value>) {
                for s in strings {
                    self.intern_impl(s);
                }
            }

            /// Interns multiple strings.
            ///
            /// Allocates the strings internally if they are not already interned.
            #[doc = concat!("If the strings are `&'static ", stringify!($value), "`, prefer using")]
            /// [`intern_many_mut_static`](Self::intern_many_mut_static), as it will not allocate
            /// the strings on the heap.
            #[doc = concat!("By taking `&mut self`, ", $mut_note)]
            #[inline]
            pub fn intern_many_mut<'a>(&mut self, strings: impl IntoIterator<Item = &'a $value>) {
                for s in strings {
                    self.intern_mut_impl(s);
                }
            }

            /// Interns multiple static strings.
            ///
            /// The inputs must be `'static`, which means we can avoid allocating the strings.
            #[inline]
            pub fn intern_many_static(&self, strings: impl IntoIterator<Item = &'static $value>) {
                for s in strings {
                    self.intern_static_impl(s);
                }
            }

            /// Interns multiple strings without allocating.
            ///
            /// This is the unchecked version of [`intern_many_static`](Self::intern_many_static)
            /// for inputs that are not typed as `'static`.
            ///
            /// # Safety
            ///
            /// The caller must ensure that all inputs remain valid and unchanged until this
            /// interner is dropped.
            #[inline]
            pub unsafe fn intern_many_static_unchecked<'a>(
                &self,
                strings: impl IntoIterator<Item = &'a $value>,
            ) {
                for s in strings {
                    // SAFETY: This method has the same safety requirements as the helper.
                    unsafe { self.intern_static_unchecked_impl(s) };
                }
            }

            /// Interns multiple static strings.
            ///
            /// The inputs must be `'static`, which means we can avoid allocating the strings.
            #[doc = concat!("By taking `&mut self`, ", $mut_note)]
            #[inline]
            pub fn intern_many_mut_static(
                &mut self,
                strings: impl IntoIterator<Item = &'static $value>,
            ) {
                for s in strings {
                    self.intern_mut_static_impl(s);
                }
            }

            /// Interns multiple strings without allocating.
            ///
            /// This is the unchecked version of
            /// [`intern_many_mut_static`](Self::intern_many_mut_static) for inputs that are not
            /// typed as `'static`.
            #[doc = concat!("By taking `&mut self`, ", $mut_note)]
            ///
            /// # Safety
            ///
            /// The caller must ensure that all inputs remain valid and unchanged until this
            /// interner is dropped.
            #[inline]
            pub unsafe fn intern_many_mut_static_unchecked<'a>(
                &mut self,
                strings: impl IntoIterator<Item = &'a $value>,
            ) {
                for s in strings {
                    // SAFETY: This method has the same safety requirements as the helper.
                    unsafe { self.intern_mut_static_unchecked_impl(s) };
                }
            }

            #[doc = concat!("Maps a `Symbol` to its string. This is a ", $cheap_op, ".")]
            ///
            /// # Panics
            ///
            /// Panics if `Symbol` is out of bounds of this `Interner`. You should only use
            /// `Symbol`s created by this `Interner`.
            #[inline]
            #[must_use]
            #[cfg_attr(debug_assertions, track_caller)]
            pub fn resolve(&self, sym: S) -> &$value {
                self.resolve_impl(sym)
            }

            #[doc = concat!("Tries to map a `Symbol` to its string. This is a ", $cheap_op, ".")]
            ///
            /// Returns `None` if `Symbol` is out of bounds of this `Interner`.
            #[inline]
            #[must_use]
            pub fn try_resolve(&self, sym: S) -> Option<&$value> {
                self.try_resolve_impl(sym)
            }
        }
    };
}

pub mod sync;
pub mod unsync;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Symbol;

    const fn _assert_send<T: Send>() {}
    const fn _assert_send_sync<T: Send + Sync>() {}
    const _: () = {
        _assert_send_sync::<sync::Interner>();
        _assert_send_sync::<sync::BytesInterner>();
        _assert_send::<unsync::Interner>();
        _assert_send::<unsync::BytesInterner>();
    };

    macro_rules! basic {
        ($ty:ty, $intern:ident) => {
            #[allow(unused_mut)]
            let mut interner = <$ty>::new();
            assert!(interner.is_empty());

            let hello = interner.$intern("hello");
            assert_eq!(hello.get(), 0);
            assert_eq!(interner.try_resolve(hello), Some("hello"));
            assert_eq!(interner.resolve(hello), "hello");
            assert_eq!(interner.len(), 1);

            let world = interner.$intern("world");
            assert_eq!(world.get(), 1);
            assert_eq!(interner.try_resolve(world), Some("world"));
            assert_eq!(interner.resolve(world), "world");
            assert_eq!(interner.len(), 2);
            assert_eq!(interner.try_resolve(Symbol::new(2)), None);

            let hello2 = interner.$intern("hello");
            assert_eq!(hello, hello2);
            let hello3 = interner.$intern("hello");
            assert_eq!(hello, hello3);

            let world2 = interner.$intern("world");
            assert_eq!(world, world2);

            assert_eq!(interner.len(), 2);

            #[allow(unused_mut)]
            let mut interner2 = <$ty>::new();
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

    macro_rules! basic_unchecked {
        ($ty:ty, $intern:ident) => {
            #[allow(unused_mut)]
            let mut interner = <$ty>::new();
            assert!(interner.is_empty());

            // SAFETY: String literals are valid for the lifetime of the interner.
            let hello = unsafe { interner.$intern("hello") };
            assert_eq!(hello.get(), 0);
            assert_eq!(interner.try_resolve(hello), Some("hello"));
            assert_eq!(interner.resolve(hello), "hello");
            assert_eq!(interner.len(), 1);

            // SAFETY: String literals are valid for the lifetime of the interner.
            let world = unsafe { interner.$intern("world") };
            assert_eq!(world.get(), 1);
            assert_eq!(interner.try_resolve(world), Some("world"));
            assert_eq!(interner.resolve(world), "world");
            assert_eq!(interner.len(), 2);
            assert_eq!(interner.try_resolve(Symbol::new(2)), None);

            // SAFETY: String literals are valid for the lifetime of the interner.
            let hello2 = unsafe { interner.$intern("hello") };
            assert_eq!(hello, hello2);
        };
    }

    macro_rules! basic_many_unchecked {
        ($ty:ty, $intern:ident) => {
            #[allow(unused_mut)]
            let mut interner = <$ty>::new();
            assert!(interner.is_empty());

            // SAFETY: String literals are valid for the lifetime of the interner.
            unsafe { interner.$intern(["hello", "world", "hello"]) };
            assert_eq!(interner.len(), 2);

            let hello = interner.intern("hello");
            let world = interner.intern("world");
            assert_eq!(hello.get(), 0);
            assert_eq!(world.get(), 1);
            assert_eq!(interner.try_resolve(hello), Some("hello"));
            assert_eq!(interner.try_resolve(world), Some("world"));
            assert_eq!(interner.resolve(hello), "hello");
            assert_eq!(interner.resolve(world), "world");
            assert_eq!(interner.try_resolve(Symbol::new(2)), None);
        };
    }

    #[test]
    fn basic() {
        basic!(sync::Interner, intern);
    }
    #[test]
    fn basic_mut() {
        basic!(sync::Interner, intern_mut);
    }
    #[test]
    fn basic_static() {
        basic!(sync::Interner, intern_static);
    }
    #[test]
    fn basic_mut_static() {
        basic!(sync::Interner, intern_mut_static);
    }
    #[test]
    fn basic_static_unchecked() {
        basic_unchecked!(sync::Interner, intern_static_unchecked);
    }
    #[test]
    fn basic_mut_static_unchecked() {
        basic_unchecked!(sync::Interner, intern_mut_static_unchecked);
    }
    #[test]
    fn basic_many_static_unchecked() {
        basic_many_unchecked!(sync::Interner, intern_many_static_unchecked);
    }
    #[test]
    fn basic_many_mut_static_unchecked() {
        basic_many_unchecked!(sync::Interner, intern_many_mut_static_unchecked);
    }
    #[test]
    fn unsync_basic() {
        basic!(unsync::Interner, intern);
    }
    #[test]
    fn unsync_basic_mut() {
        basic!(unsync::Interner, intern_mut);
    }
    #[test]
    fn unsync_basic_static() {
        basic!(unsync::Interner, intern_static);
    }
    #[test]
    fn unsync_basic_mut_static() {
        basic!(unsync::Interner, intern_mut_static);
    }
    #[test]
    fn unsync_basic_static_unchecked() {
        basic_unchecked!(unsync::Interner, intern_static_unchecked);
    }
    #[test]
    fn unsync_basic_mut_static_unchecked() {
        basic_unchecked!(unsync::Interner, intern_mut_static_unchecked);
    }
    #[test]
    fn unsync_basic_many_static_unchecked() {
        basic_many_unchecked!(unsync::Interner, intern_many_static_unchecked);
    }
    #[test]
    fn unsync_basic_many_mut_static_unchecked() {
        basic_many_unchecked!(unsync::Interner, intern_many_mut_static_unchecked);
    }

    #[test]
    fn bytes_try_resolve() {
        let interner = sync::BytesInterner::new();
        let hello = interner.intern(b"hello");
        assert_eq!(interner.try_resolve(hello), Some(&b"hello"[..]));
        assert_eq!(interner.try_resolve(Symbol::new(1)), None);

        let interner = unsync::BytesInterner::new();
        let hello = interner.intern(b"hello");
        assert_eq!(interner.try_resolve(hello), Some(&b"hello"[..]));
        assert_eq!(interner.try_resolve(Symbol::new(1)), None);
    }

    #[test]
    fn mt() {
        let interner = sync::Interner::new();
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

        let interner = sync::Interner::<Symbol, _>::with_hasher(std::hash::BuildHasherDefault::<
            MyBadHasher,
        >::default());
        let hello = interner.intern("hello");
        let world = interner.intern("world");
        assert_eq!(hello.get(), 0);
        assert_eq!(world.get(), 1);
        assert_eq!(interner.resolve(hello), "hello");
        assert_eq!(interner.resolve(world), "world");
        assert_eq!(interner.len(), 2);

        let interner = unsync::Interner::<Symbol, _>::with_hasher(std::hash::BuildHasherDefault::<
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
