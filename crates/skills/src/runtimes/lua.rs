//! Lua plugin runtime using mlua.
//!
//! This module provides a Lua runtime implementation with sandboxing and
//! permission-checked host API injection.

use crate::host_api::{HostApiError, HostContext};
use crate::runtimes::{Plugin, PluginError, PluginFunction};
use crate::types::SkillDriver;

use mlua::{Function as LuaFunction, Lua, LuaOptions, StdLib, String as LuaString, Value as LuaValue};
use std::collections::HashMap;
use std::path::Path;

/// Lua engine using mlua for plugin execution.
pub struct LuaEngine {
    /// Loaded Lua plugins indexed by name
    plugins: HashMap<String, LuaPlugin>,
}

impl LuaEngine {
    /// Create a new Lua engine.
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    /// Load a Lua plugin from a file.
    pub fn load(
        &mut self, name: String, script_path: &Path, context: HostContext, functions: Vec<PluginFunction>,
    ) -> Result<(), PluginError> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )
        .map_err(map_lua_error)?;

        self.sandbox_lua(&lua).map_err(map_lua_error)?;
        self.inject_host_api(&lua, context.clone()).map_err(map_lua_error)?;

        let script = std::fs::read_to_string(script_path)
            .map_err(|e| PluginError::ExecutionFailed(format!("Failed to read Lua script: {e}")))?;
        lua.load(&script)
            .exec()
            .map_err(|e| PluginError::ExecutionFailed(format!("Failed to execute Lua script: {e}")))?;

        let plugin = LuaPlugin { name: name.clone(), lua, functions, _context: context };
        self.plugins.insert(name, plugin);
        Ok(())
    }

    /// Unload a plugin by name.
    pub fn unload(&mut self, name: &str) -> Result<(), PluginError> {
        self.plugins
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| PluginError::NotFound(name.to_string()))
    }

    /// Get a loaded plugin by name.
    pub fn get(&self, name: &str) -> Option<&LuaPlugin> {
        self.plugins.get(name)
    }

    /// Execute a plugin function.
    pub fn execute(&mut self, plugin: &str, func: &str, input: &[u8]) -> Result<Vec<u8>, PluginError> {
        let lua_plugin = self
            .plugins
            .get_mut(plugin)
            .ok_or_else(|| PluginError::NotFound(plugin.to_string()))?;

        lua_plugin.call(func, input)
    }

    fn sandbox_lua(&self, lua: &Lua) -> mlua::Result<()> {
        let globals = lua.globals();
        for key in ["dofile", "loadfile", "load", "require", "os", "io", "package", "debug"] {
            globals.set(key, LuaValue::Nil)?;
        }
        Ok(())
    }

    fn inject_host_api(&self, lua: &Lua, context: HostContext) -> mlua::Result<()> {
        let thunderus = lua.create_table()?;

        let log_ctx = context.clone();
        thunderus.set(
            "log",
            lua.create_function(move |_, (level, msg): (String, String)| {
                let level_normalized = level.to_lowercase();
                match level_normalized.as_str() {
                    "error" => tracing::error!(target: "plugin", plugin = %log_ctx.plugin_name, "{}", msg),
                    "warn" | "warning" => tracing::warn!(target: "plugin", plugin = %log_ctx.plugin_name, "{}", msg),
                    "debug" => tracing::debug!(target: "plugin", plugin = %log_ctx.plugin_name, "{}", msg),
                    "trace" => tracing::trace!(target: "plugin", plugin = %log_ctx.plugin_name, "{}", msg),
                    _ => tracing::info!(target: "plugin", plugin = %log_ctx.plugin_name, "{}", msg),
                }
                Ok(())
            })?,
        )?;

        let read_ctx = context.clone();
        thunderus.set(
            "read_file",
            lua.create_function(move |lua, path: String| {
                let resolved = read_ctx.check_read_permission(&path).map_err(map_host_error)?;
                match std::fs::read(&resolved) {
                    Ok(bytes) => Ok(Some(lua.create_string(&bytes)?)),
                    Err(_) => Ok(None),
                }
            })?,
        )?;

        let write_ctx = context.clone();
        thunderus.set(
            "write_file",
            lua.create_function(move |_, (path, data): (String, LuaString)| {
                let resolved = write_ctx.check_write_permission(&path).map_err(map_host_error)?;
                if let Some(parent) = resolved.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| map_host_error(e.into()))?;
                }
                std::fs::write(&resolved, data.as_bytes()).map_err(|e| map_host_error(e.into()))?;
                Ok(())
            })?,
        )?;

        let kv_get_ctx = context.clone();
        thunderus.set(
            "kv_get",
            lua.create_function(move |_, key: String| {
                let store = kv_get_ctx.kv_store.read().unwrap();
                Ok(store.get(&key))
            })?,
        )?;

        let kv_set_ctx = context.clone();
        thunderus.set(
            "kv_set",
            lua.create_function(move |_, (key, value): (String, String)| {
                let mut store = kv_set_ctx.kv_store.write().unwrap();
                store.set(key, value).map_err(map_host_error)?;
                Ok(())
            })?,
        )?;

        lua.globals().set("thunderus", thunderus)?;
        Ok(())
    }
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A loaded Lua plugin.
pub struct LuaPlugin {
    name: String,
    lua: Lua,
    functions: Vec<PluginFunction>,
    #[allow(dead_code)]
    _context: HostContext,
}

impl Plugin for LuaPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn functions(&self) -> Vec<PluginFunction> {
        self.functions.clone()
    }

    fn has_function(&self, name: &str) -> bool {
        let globals = self.lua.globals();
        matches!(globals.get::<LuaValue>(name), Ok(LuaValue::Function(_)))
    }

    fn call(&mut self, func: &str, input: &[u8]) -> Result<Vec<u8>, PluginError> {
        let globals = self.lua.globals();
        let function: LuaFunction = globals
            .get(func)
            .map_err(|_| PluginError::FunctionNotFound(func.to_string()))?;
        let input_str = std::str::from_utf8(input)
            .map_err(|e| PluginError::InvalidInput(format!("Input must be valid UTF-8: {e}")))?;
        let output: LuaString = function
            .call::<LuaString>(input_str.to_string())
            .map_err(|e| PluginError::ExecutionFailed(format!("Lua call failed: {e}")))?;
        Ok(output.as_bytes().to_vec())
    }

    fn driver(&self) -> SkillDriver {
        SkillDriver::Lua
    }
}

fn map_lua_error(err: mlua::Error) -> PluginError {
    PluginError::ExecutionFailed(err.to_string())
}

fn map_host_error(err: HostApiError) -> mlua::Error {
    mlua::Error::RuntimeError(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SkillPermissions;
    use tempfile::TempDir;

    #[test]
    #[cfg(feature = "lua")]
    fn test_lua_engine_creation() {
        let engine = LuaEngine::new();
        assert_eq!(engine.plugins.len(), 0);
    }

    #[test]
    #[cfg(feature = "lua")]
    fn test_lua_load_and_execute() {
        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("plugin.lua");
        std::fs::write(
            &script_path,
            r#"
function run(input)
    return input .. "_ok"
end
"#,
        )
        .unwrap();

        let mut engine = LuaEngine::new();
        let ctx = HostContext::new(
            "lua-test".to_string(),
            temp_dir.path().to_path_buf(),
            SkillPermissions::default(),
        );
        engine
            .load("lua-test".to_string(), &script_path, ctx, Vec::new())
            .unwrap();
        let output = engine.execute("lua-test", "run", b"ping").unwrap();
        assert_eq!(String::from_utf8_lossy(&output), "ping_ok");
    }
}
