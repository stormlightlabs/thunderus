#![doc = include_str!("../README.md")]

mod support;

pub mod context;

/// Memory contracts, with file-backed storage and lexical recall behind the
/// optional `memory` feature.
pub mod memory;
pub mod session;
