mod bytes;
pub use bytes::BytesInterner;

mod str;
pub use self::str::Interner;

#[cfg(all(test, loom))]
mod loom_tests;
