//! Provider-neutral coding-agent loop and contracts.
//!
//! Application adapters own provider selection, tool policy, terminal I/O,
//! and ACP transport. This crate owns shared run contracts such as cooperative
//! cancellation.

pub mod cancel;
pub mod contracts;

pub use cancel::CancelToken;
pub use contracts::{RetryPolicy, ToolPermissionDecision, ToolStatus};
