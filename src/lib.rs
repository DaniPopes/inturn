#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod interner;
pub use interner::{BytesInterner, Interner, unsync};

mod symbol;
pub use symbol::{InternerSymbol, Symbol};
