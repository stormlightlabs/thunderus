//! Context, memory, prompt-context, and session contracts for coding agents.
//!
//! The library owns project-instruction discovery, typed context selection,
//! and optional file-backed memory. Application adapters choose when those
//! capabilities are used and where their user interfaces are rendered.

mod support;

pub mod context;

/// Memory contracts, with file-backed storage and lexical recall behind the
/// optional `memory` feature.
pub mod memory;
