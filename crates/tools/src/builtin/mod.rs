//! Built-in tools for the Thunderus harness
//!
//! This module provides a collection of commonly-used tools that can be
//! registered with the tool registry and invoked by agents.

mod echo;
mod edit;
mod glob;
mod grep;
mod multiedit;
mod noop;
mod patch;
mod read;
mod shell;
mod write;

pub use echo::EchoTool;
pub use edit::EditTool;
pub use glob::{GlobSortOrder, GlobTool};
pub use grep::{GrepOutputMode, GrepTool};
pub use multiedit::{MultiEditOperation, MultiEditTool};
pub use noop::NoopTool;
pub use patch::PatchTool;
pub use read::ReadTool;
pub use shell::ShellTool;
pub use write::WriteTool;

#[cfg(test)]
pub mod test_helpers;
