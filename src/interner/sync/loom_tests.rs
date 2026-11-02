//! Loom models of concurrent interning.
//!
//! Run with `RUSTFLAGS="--cfg loom" cargo test --lib loom_tests`.

use super::Interner;
use loom::{sync::Arc, thread};

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
