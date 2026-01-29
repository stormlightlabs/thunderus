//! Extism host functions for WASM plugins.
//!
//! This module implements the host functions that WASM plugins can call
//! to interact with the system. These functions are registered with Extism
//! and provide permission-checked access to filesystem, logging, and storage.
//!
//! Note: The current implementation uses a simplified approach. Future versions
//! should use Extism's user_data feature to pass plugin-specific context to
//! host functions for proper per-plugin isolation.

use crate::host_api::{HostApiError, HostContext};
use extism::{Function, PTR, UserData, host_fn};

fn map_host_error(err: HostApiError) -> extism::Error {
    extism::Error::msg(err.to_string())
}

host_fn!(thunderus_log(user_data: HostContext; level: String, msg: String) -> () {
    let ctx = user_data.get()?;
    let ctx = ctx.lock().unwrap();
    let level_normalized = level.to_lowercase();
    match level_normalized.as_str() {
        "error" => tracing::error!(target: "plugin", plugin = %ctx.plugin_name, "{}", msg),
        "warn" | "warning" => tracing::warn!(target: "plugin", plugin = %ctx.plugin_name, "{}", msg),
        "debug" => tracing::debug!(target: "plugin", plugin = %ctx.plugin_name, "{}", msg),
        "trace" => tracing::trace!(target: "plugin", plugin = %ctx.plugin_name, "{}", msg),
        _ => tracing::info!(target: "plugin", plugin = %ctx.plugin_name, "{}", msg),
    }
    Ok(())
});

host_fn!(thunderus_read_file(user_data: HostContext; path: String) -> Vec<u8> {
    let ctx = user_data.get()?;
    let ctx = ctx.lock().unwrap();
    let resolved = ctx.check_read_permission(&path).map_err(map_host_error)?;
    std::fs::read(&resolved)
        .map_err(|e| extism::Error::msg(format!("Failed to read file: {e}")))
});

host_fn!(thunderus_write_file(user_data: HostContext; path: String, data: Vec<u8>) -> () {
    let ctx = user_data.get()?;
    let ctx = ctx.lock().unwrap();
    let resolved = ctx.check_write_permission(&path).map_err(map_host_error)?;
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| extism::Error::msg(format!("Failed to create directory: {e}")))?;
    }
    std::fs::write(&resolved, data)
        .map_err(|e| extism::Error::msg(format!("Failed to write file: {e}")))?;
    Ok(())
});

host_fn!(thunderus_kv_get(user_data: HostContext; key: String) -> String {
    let ctx = user_data.get()?;
    let ctx = ctx.lock().unwrap();
    let store = ctx.kv_store.read().unwrap();
    Ok(store.get(&key).unwrap_or_default())
});

host_fn!(thunderus_kv_set(user_data: HostContext; key: String, value: String) -> () {
    let ctx = user_data.get()?;
    let ctx = ctx.lock().unwrap();
    let mut store = ctx.kv_store.write().unwrap();
    store.set(key, value).map_err(map_host_error)?;
    Ok(())
});

/// Get all host functions as Extism Function objects.
///
/// This function converts the module-level host functions into
/// Function objects that can be registered with Extism plugins.
pub fn get_host_functions(context: HostContext) -> Vec<Function> {
    let user_data = UserData::new(context);
    vec![
        Function::new("thunderus_log", [PTR, PTR], [], user_data.clone(), thunderus_log),
        Function::new(
            "thunderus_read_file",
            [PTR],
            [PTR],
            user_data.clone(),
            thunderus_read_file,
        ),
        Function::new(
            "thunderus_write_file",
            [PTR, PTR],
            [],
            user_data.clone(),
            thunderus_write_file,
        ),
        Function::new("thunderus_kv_get", [PTR], [PTR], user_data.clone(), thunderus_kv_get),
        Function::new("thunderus_kv_set", [PTR, PTR], [], user_data, thunderus_kv_set),
    ]
}

/// Host functions container for WASM plugins.
#[derive(Clone, Debug)]
pub struct HostFunctions;

impl HostFunctions {
    /// Get all host functions for Extism registration.
    pub fn all(context: HostContext) -> Vec<Function> {
        get_host_functions(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_functions_creation() {
        let ctx = HostContext::new(
            "test-plugin".to_string(),
            std::path::PathBuf::from("."),
            crate::types::SkillPermissions::default(),
        );
        let fns = HostFunctions::all(ctx);
        assert!(!fns.is_empty());
    }
}
