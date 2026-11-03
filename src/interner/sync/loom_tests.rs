//! Loom models of concurrent interning.
//!
//! Run with `RUSTFLAGS="--cfg loom" cargo test --lib loom_tests`.

use super::Interner;
use crate::Symbol;
use loom::{sync::Arc, thread};
use std::hash::{BuildHasherDefault, Hasher};

#[test]
fn same_string_gets_one_symbol() {
    loom::model(|| {
        let interner = Arc::new(Interner::new());

        let other = Arc::clone(&interner);
        let handle = thread::spawn(move || other.intern("hello"));
        let sym = interner.intern("hello");

        assert_eq!(handle.join().unwrap(), sym);
        assert_eq!(interner.resolve(sym), "hello");
        assert_eq!(interner.len(), 1);
    });
}

/// Hashes a string to its first byte, so strings get distinct tags. `a`, `b` and `d` also share a
/// shard.
#[derive(Default)]
struct FirstByte(u64);

impl Hasher for FirstByte {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0 = bytes[0].into();
    }
}

#[test]
fn growth_races_with_lookups() {
    loom::model(|| {
        let interner = Arc::new(Interner::<Symbol, _>::with_hasher(
            BuildHasherDefault::<FirstByte>::default(),
        ));

        let other = Arc::clone(&interner);
        let handle = thread::spawn(move || ["a", "b", "d"].map(|s| other.intern(s)));
        let syms = ["d", "a", "b"].map(|s| interner.intern(s));
        let other_syms = handle.join().unwrap();

        assert_eq!(syms, [other_syms[2], other_syms[0], other_syms[1]]);
        for (s, sym) in ["a", "b", "d"].into_iter().zip(other_syms) {
            assert_eq!(interner.resolve(sym), s);
            assert_eq!(interner.intern(s), sym);
        }
        assert_eq!(interner.len(), 3);
    });
}
