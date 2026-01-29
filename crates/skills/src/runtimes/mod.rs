//! Runtime plugin support for Skills.
//!
//! This module provides plugin runtime support for different execution drivers:
//! - WASM via Extism (behind `wasm` feature)
//! - Lua via mlua (behind `lua` feature)

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "wasm")]
pub mod wasm_host;

#[cfg(feature = "lua")]
pub mod lua;

/// Re-exports for runtime plugins.
#[cfg(feature = "wasm")]
pub use wasm::{WasmEngine, WasmPlugin};

#[cfg(feature = "lua")]
pub use lua::{LuaEngine, LuaPlugin};

/// Plugin trait for unified runtime interface.
///
/// All runtime plugins (WASM, Lua) implement this trait to provide
/// a consistent interface for loading and executing plugin functions.
pub trait Plugin: Send + Sync {
    /// Unique identifier for this plugin
    fn name(&self) -> &str;

    /// List available function exports
    fn functions(&self) -> Vec<PluginFunction>;

    /// Check if a function exists
    fn has_function(&self, name: &str) -> bool;

    /// Execute a function with JSON input, returns JSON output
    ///
    /// Takes `&mut self` because some runtimes (like Extism) require mutable
    /// access for internal state management during execution.
    fn call(&mut self, func: &str, input: &[u8]) -> Result<Vec<u8>>;

    /// Get the driver type
    fn driver(&self) -> crate::types::SkillDriver;
}

pub use crate::types::PluginFunction;

/// Errors that can occur during plugin operations.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("WASM error: {0}")]
    Wasm(String),

    #[cfg(feature = "wasm")]
    #[error("Extism error: {0}")]
    Extism(#[from] extism::Error),
}

/// Result type for plugin operations.
pub type Result<T> = std::result::Result<T, PluginError>;
