# inturn

[![github](https://img.shields.io/badge/github-danipopes/inturn-8da0cb?style=for-the-badge&labelColor=555555&logo=github)](https://github.com/danipopes/inturn)
[![crates.io](https://img.shields.io/crates/v/inturn.svg?style=for-the-badge&color=fc8d62&logo=rust)](https://crates.io/crates/inturn)
[![docs.rs](https://img.shields.io/badge/docs.rs-inturn-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs)](https://docs.rs/inturn)
[![build status](https://img.shields.io/github/actions/workflow/status/danipopes/inturn/ci.yml?branch=master&style=for-the-badge)](https://github.com/danipopes/inturn/actions?query=branch%3Amaster)

Efficient, performant, thread-safe interning for strings, bytes, and `Copy` types.

This crate provides lock-free symbol resolution (mapping symbols back to values) and concurrent
deduplication via `dashmap`.

## Interners

- **`Interner`** — `str` interning. Thin wrapper around `BytesInterner`.
- **`BytesInterner`** — `[u8]` interning. The core implementation.
- **`CopyInterner<T>`** — Generic interning for any `T: Copy + Hash + Eq`.

`Interner` and `BytesInterner` support `&'static` variants that avoid allocation. All interners
support `*_mut` variants that side-step locks for single-threaded initialization.

## Examples

Basic `str` interning (the same API is available with `BytesInterner` for `[u8]`):

```rust
use inturn::Interner;

let interner = Interner::new();
let hello = interner.intern("hello");
assert_eq!(hello.get(), 0);
assert_eq!(interner.resolve(hello), "hello");

let world = interner.intern("world");
assert_eq!(world.get(), 1);
assert_eq!(interner.resolve(world), "world");

let hello2 = interner.intern("hello");
assert_eq!(hello, hello2);

assert_eq!(interner.len(), 2);
```
