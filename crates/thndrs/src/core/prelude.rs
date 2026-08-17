//! Shared headless `thndrs` harness surface.
//!
//! This module is the internal `thndrs-core` boundary. The TUI binary and ACP
//! server binary should depend on this surface instead of depending on each
//! other.

pub use crate::app::{AgentEvent, ToolStatus};
pub use crate::harness::{HarnessHandle, HarnessTurn};
pub use crate::providers::ProviderMessage;
pub use crate::tools::AgentRunConfig;
