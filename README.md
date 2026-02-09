# inturn

[![github](https://img.shields.io/badge/github-danipopes/inturn-8da0cb?style=for-the-badge&labelColor=555555&logo=github)](https://github.com/danipopes/inturn)
[![crates.io](https://img.shields.io/crates/v/inturn.svg?style=for-the-badge&color=fc8d62&logo=rust)](https://crates.io/crates/inturn)
[![docs.rs](https://img.shields.io/badge/docs.rs-inturn-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs)](https://docs.rs/inturn)
[![build status](https://img.shields.io/github/actions/workflow/status/danipopes/inturn/ci.yml?branch=master&style=for-the-badge)](https://github.com/danipopes/inturn/actions?query=branch%3Amaster)

Efficient, performant, thread-safe bytes/string interning.

This crate was designed to have a lock-free mapping of symbols back to their original string.

It currently uses `dashmap` for deduplicating strings,
and a lock-free stack to map the string index (symbol) back to the string bytes.

It supports interning any `&str`/`&[u8]` by allocating it internally in an efficient arena when encountered for the first time,
or `&'static str`/`&'static [u8]` without allocation.

A `*_mut` variant of each API is provided which side-step any locks,
for e.g. initializing the interner with a static set of strings to pre-intern.

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
