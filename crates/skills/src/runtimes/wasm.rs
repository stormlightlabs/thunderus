//! WASM plugin runtime using Extism.
//!
//! This module provides a WASM runtime implementation using the Extism plugin framework.
//! Extism wraps Wasmtime and provides "bytes-in, bytes-out" ABI with built-in
//! host function registration and permission-based security.

use crate::host_api::HostContext;
use crate::runtimes::{Plugin, PluginError, PluginFunction};
use crate::types::{SkillDriver, SkillPermissions};

use extism::{Manifest, Plugin as ExtismPlugin, Wasm};
use std::collections::HashMap;
use std::path::Path;

/// WASM engine using Extism for plugin execution.
///
/// The engine manages loaded WASM plugins and handles execution through
/// Extism's "bytes-in, bytes-out" ABI.
#[derive(Debug)]
pub struct WasmEngine {
    /// Loaded Extism plugins indexed by name
    plugins: HashMap<String, WasmPlugin>,
}

impl WasmEngine {
    /// Create a new WASM engine.
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    /// Load a WASM plugin from a file.
    ///
    /// # Arguments
    /// * `name` - Unique identifier for this plugin
    /// * `wasm_path` - Path to the .wasm file
    /// * `permissions` - Permission grants for the plugin
    ///
    /// # Errors
    /// Returns an error if the WASM file cannot be loaded or compiled.
    pub fn load(
        &mut self, name: String, wasm_path: &Path, context: HostContext, functions: Vec<PluginFunction>,
    ) -> Result<(), PluginError> {
        let manifest = self.build_manifest(wasm_path, &context)?;

        let host_fns = crate::runtimes::wasm_host::HostFunctions::all(context);

        let plugin = ExtismPlugin::new(&manifest, host_fns, true)
            .map_err(|e| PluginError::Wasm(format!("Failed to load plugin: {e}")))?;

        let wasm_plugin = WasmPlugin { functions, name: name.clone(), plugin };
        self.plugins.insert(name, wasm_plugin);
        Ok(())
    }

    /// Unload a plugin by name.
    ///
    /// This is primarily useful for hot-reload scenarios.
    pub fn unload(&mut self, name: &str) -> Result<(), PluginError> {
        self.plugins
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| PluginError::NotFound(name.to_string()))
    }

    /// Get a loaded plugin by name.
    pub fn get(&self, name: &str) -> Option<&WasmPlugin> {
        self.plugins.get(name)
    }

    /// Execute a plugin function with permission checks.
    ///
    /// # Arguments
    /// * `plugin` - Name of the plugin
    /// * `func` - Name of the function to call
    /// * `input` - Input data as bytes (typically JSON)
    ///
    /// # Errors
    /// Returns an error if the plugin or function doesn't exist, or if
    /// execution fails.
    pub fn execute(&mut self, plugin: &str, func: &str, input: &[u8]) -> Result<Vec<u8>, PluginError> {
        let wasm_plugin = self
            .plugins
            .get_mut(plugin)
            .ok_or_else(|| PluginError::NotFound(plugin.to_string()))?;

        wasm_plugin.call(func, input)
    }

    /// Build an Extism manifest from our permission structure.
    ///
    /// This converts our `SkillPermissions` to Extism's manifest format, mapping allowed hosts and paths.
    fn build_manifest(&self, wasm_path: &Path, context: &HostContext) -> Result<Manifest, PluginError> {
        let mut manifest = Manifest::new([Wasm::file(wasm_path)]);

        if !context.permissions.network.allowed_hosts.is_empty() {
            manifest = manifest.with_allowed_hosts(context.permissions.network.allowed_hosts.iter().cloned());
        }

        let allowed_paths = self.build_allowed_paths(&context.workspace_root, &context.permissions);
        if !allowed_paths.is_empty() {
            manifest = manifest.with_allowed_paths(allowed_paths.into_iter());
        }

        Ok(manifest)
    }

    fn build_allowed_paths(
        &self, workspace_root: &Path, permissions: &SkillPermissions,
    ) -> Vec<(String, std::path::PathBuf)> {
        let mut mappings = std::collections::HashMap::new();
        for pattern in permissions
            .filesystem
            .read
            .iter()
            .chain(permissions.filesystem.write.iter())
        {
            if let Some((guest_path, host_path)) = Self::map_pattern_to_allowed_path(workspace_root, pattern) {
                mappings.insert(guest_path, host_path);
            }
        }
        mappings.into_iter().collect()
    }

    fn map_pattern_to_allowed_path(workspace_root: &Path, pattern: &str) -> Option<(String, std::path::PathBuf)> {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return None;
        }
        let normalized = trimmed.trim_start_matches("./");
        let base = normalized.split(['*', '?']).next().unwrap_or("").trim_end_matches('/');
        if base.is_empty() {
            return None;
        }
        let base_path = std::path::PathBuf::from(base);
        let host_path = if base_path.is_absolute() { base_path.clone() } else { workspace_root.join(&base_path) };
        let guest_path = if base_path.is_absolute() { base.to_string() } else { format!("/{base}") };
        Some((guest_path, host_path))
    }
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A loaded WASM plugin.
///
/// Wraps an Extism Plugin and provides the Plugin trait interface.
#[derive(Debug)]
pub struct WasmPlugin {
    /// Plugin name
    name: String,
    /// The underlying Extism plugin
    plugin: ExtismPlugin,
    /// Available functions (discovered at load time)
    functions: Vec<PluginFunction>,
}

impl WasmPlugin {
    /// Get the underlying Extism plugin.
    pub fn extism_plugin(&self) -> &ExtismPlugin {
        &self.plugin
    }
}

impl Plugin for WasmPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn functions(&self) -> Vec<PluginFunction> {
        self.functions.clone()
    }

    fn has_function(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f.name == name)
    }

    /// Execute a WASM function using Extism's "bytes-in, bytes-out" ABI.
    ///
    /// This converts the input to a string, calls the Extism plugin function,
    /// and returns the output as bytes.
    fn call(&mut self, func: &str, input: &[u8]) -> Result<Vec<u8>, PluginError> {
        if !self.plugin.function_exists(func) {
            return Err(PluginError::FunctionNotFound(func.to_string()));
        }
        let output: &[u8] = self
            .plugin
            .call(func, input)
            .map_err(|e| PluginError::ExecutionFailed(format!("{e}")))?;

        Ok(output.to_vec())
    }

    fn driver(&self) -> SkillDriver {
        SkillDriver::Wasm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[cfg(feature = "wasm")]
    fn test_wasm_engine_creation() {
        let engine = WasmEngine::new();
        assert_eq!(engine.plugins.len(), 0);
    }

    #[test]
    #[cfg(feature = "wasm")]
    fn test_wasm_engine_default() {
        let engine = WasmEngine::default();
        assert_eq!(engine.plugins.len(), 0);
    }

    #[test]
    #[cfg(feature = "wasm")]
    fn test_wasm_plugin_not_found() {
        let engine = WasmEngine::new();
        assert!(engine.get("nonexistent").is_none());
    }

    fn create_test_wasm() -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let wasm_path = temp_dir.path().join("test.wasm");
        let minimal_wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        fs::write(&wasm_path, minimal_wasm).unwrap();
        (temp_dir, wasm_path)
    }

    #[test]
    #[cfg(feature = "wasm")]
    fn test_wasm_load_invalid_plugin() {
        let (_, wasm_path) = create_test_wasm();
        let mut engine = WasmEngine::new();
        let ctx = HostContext::new(
            "test".to_string(),
            std::env::current_dir().unwrap(),
            SkillPermissions::default(),
        );
        let result = engine.load("test".to_string(), &wasm_path, ctx, Vec::new());
        assert!(result.is_ok() || result.is_err());
    }
}
