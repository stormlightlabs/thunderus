//! MCP client manager and namespaced tool routing.
//!
//! The manager keeps MCP protocol and process handling behind the adapter while
//! exposing the same local tool definition and output shapes used by built-in
//! tools.

use std::collections::BTreeMap;

use super::adapter::{McpSdkClient, McpSdkError, McpToolDefinition, sdk_error_to_output};
use super::config::{McpConfig, McpServerConfig};
use crate::tools::{MAX_LINE_LEN, MAX_OUTPUT_BYTES, ToolDefinition, ToolOutput, ToolUseRequest, shell};

const MAX_MCP_OUTPUT_LINES: usize = 100;

/// Errors returned while building an MCP client or manager.
#[derive(Debug, thiserror::Error)]
pub enum McpManagerError {
    /// The server initialized, but listing tools failed.
    #[error("mcp server `{server}` failed to list tools: {source}")]
    ListTools { server: String, source: McpSdkError },
    /// The server could not be initialized.
    #[error("mcp server `{server}` failed to initialize: {source}")]
    Initialize { server: String, source: McpSdkError },
}

/// Connected MCP client for one configured server.
pub struct McpClient {
    name: String,
    sdk: McpSdkClient,
    tools: Vec<McpToolDefinition>,
    tool_routes: BTreeMap<String, String>,
}

impl McpClient {
    /// Initialize a server and cache its tool definitions.
    pub fn connect(name: impl Into<String>, config: &McpServerConfig) -> Result<Self, McpManagerError> {
        let name = name.into();
        let sdk = McpSdkClient::connect(config)
            .map_err(|source| McpManagerError::Initialize { server: name.clone(), source })?;
        let tools = sdk
            .list_tool_definitions(&name)
            .map_err(|source| McpManagerError::ListTools { server: name.clone(), source })?;
        let tool_routes = tools
            .iter()
            .map(|tool| (tool.definition.name.to_string(), tool.original_tool_name.clone()))
            .collect();

        Ok(Self { name, sdk, tools, tool_routes })
    }

    /// Configured server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Cached tool definitions for this server.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.definition.clone()).collect()
    }

    /// Current startup and stderr diagnostics for this server.
    pub fn diagnostics(&self) -> Vec<String> {
        let mut diagnostics = self.sdk.server_info().diagnostics.clone();
        if let Some(stderr) = self.sdk.stderr_diagnostics() {
            diagnostics.push(format!("mcp server `{}` stderr: {stderr}", self.name));
        }
        diagnostics
    }

    /// Return the original MCP tool name for a namespaced provider-visible name.
    pub fn original_tool_name(&self, namespaced_tool_name: &str) -> Option<&str> {
        self.tool_routes.get(namespaced_tool_name).map(String::as_str)
    }

    /// Call a namespaced MCP tool through this client.
    pub fn call_tool(&self, request: &ToolUseRequest) -> ToolOutput {
        let Some(original_name) = self.original_tool_name(&request.name) else {
            return ToolOutput::failed(&request.name, format!("unknown MCP tool: {}", request.name));
        };
        let arguments = match serde_json::from_str::<serde_json::Value>(&request.arguments) {
            Ok(arguments) => arguments,
            Err(err) => return ToolOutput::failed(&request.name, format!("invalid tool arguments: {err}")),
        };

        self.sdk
            .call_tool(&request.name, original_name, arguments)
            .map(sanitize_mcp_output)
            .unwrap_or_else(|err| sdk_error_to_output(&request.name, "call", &err))
    }
}

/// Manager for configured MCP servers.
pub struct McpManager {
    clients: BTreeMap<String, McpClient>,
    tool_routes: BTreeMap<String, String>,
    diagnostics: Vec<String>,
}

impl std::fmt::Debug for McpManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpManager")
            .field("clients", &self.clients.keys().collect::<Vec<_>>())
            .field("tool_routes", &self.tool_routes)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl McpManager {
    /// Initialize all enabled servers with bounded startup and cache their tools.
    pub fn from_config(config: &McpConfig) -> Self {
        let mut clients = BTreeMap::new();
        let mut tool_routes = BTreeMap::new();
        let mut diagnostics = Vec::new();

        for (name, server) in &config.servers {
            if !server.enabled {
                diagnostics.push(format!("mcp server `{name}` disabled"));
                continue;
            }

            match McpClient::connect(name.clone(), server) {
                Ok(client) => {
                    for tool in &client.tools {
                        tool_routes.insert(tool.definition.name.to_string(), name.clone());
                    }
                    diagnostics.extend(client.diagnostics());
                    clients.insert(name.clone(), client);
                }
                Err(err) => diagnostics.push(err.to_string()),
            }
        }

        Self { clients, tool_routes, diagnostics }
    }

    /// Cached MCP tool definitions for provider prompt assembly.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.clients.values().flat_map(McpClient::tool_definitions).collect()
    }

    /// Startup and non-fatal diagnostics recorded while initializing servers.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Append loader diagnostics to this manager.
    pub fn extend_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = String>) {
        self.diagnostics.extend(diagnostics);
    }

    /// Route a namespaced tool request to the correct MCP client.
    pub fn call_tool(&self, request: &ToolUseRequest) -> ToolOutput {
        let Some(server_name) = self.tool_routes.get(&request.name) else {
            return ToolOutput::failed(&request.name, format!("unknown MCP tool: {}", request.name));
        };
        let Some(client) = self.clients.get(server_name) else {
            return ToolOutput::failed(&request.name, format!("MCP server `{server_name}` is not available"));
        };

        client.call_tool(request)
    }
}

fn sanitize_mcp_output(mut output: ToolOutput) -> ToolOutput {
    let mut remaining_bytes = MAX_OUTPUT_BYTES;
    let mut sanitized = Vec::new();
    let mut truncated = false;

    for line in output.output {
        if sanitized.len() >= MAX_MCP_OUTPUT_LINES {
            truncated = true;
            break;
        }

        let redacted = shell::redact_secrets(&line);
        let mut capped: String = redacted.chars().take(MAX_LINE_LEN).collect();
        if capped.len() < redacted.len() {
            capped.push_str("...");
            truncated = true;
        }

        let bytes = capped.len();
        if bytes > remaining_bytes {
            let allowed = remaining_bytes.min(capped.len());
            capped = capped.chars().take(allowed).collect();
            capped.push_str("...");
            sanitized.push(capped);
            truncated = true;
            break;
        }

        remaining_bytes = remaining_bytes.saturating_sub(bytes);
        sanitized.push(capped);
    }

    if truncated {
        sanitized.push("[mcp output truncated]".to_string());
    }

    if let Some(error) = output.error {
        output.error = Some(shell::redact_secrets(&error));
    }
    output.output = sanitized;
    output
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;

    use super::*;
    use crate::app::ToolStatus;
    use crate::mcp::config::McpServerConfig;

    #[test]
    fn manager_routes_duplicate_tool_names_by_namespace() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), "echo", 0, false);
        let mut config = McpConfig::default();
        config.servers.insert("alpha".to_string(), server_config(&script));
        config.servers.insert("beta".to_string(), server_config(&script));

        let manager = McpManager::from_config(&config);
        let names: Vec<String> = manager
            .tool_definitions()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();

        assert_eq!(names, vec!["mcp__alpha__echo", "mcp__beta__echo"]);

        let output = manager.call_tool(&ToolUseRequest::new(
            "mcp__beta__echo".to_string(),
            json!({ "text": "from beta" }).to_string(),
            "toolu_1".to_string(),
        ));

        assert_eq!(output, ToolOutput::ok("mcp__beta__echo", vec!["from beta".to_string()]));
    }

    #[test]
    fn manager_records_failed_server_diagnostics() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), "echo", 2, false);
        let mut config = McpConfig::default();
        config.servers.insert(
            "slow".to_string(),
            McpServerConfig { timeout_secs: 1, ..server_config(&script) },
        );

        let manager = McpManager::from_config(&config);

        assert!(manager.tool_definitions().is_empty());
        assert!(
            manager
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.contains("timed out after 1s"))
        );
    }

    #[test]
    fn manager_returns_stable_failure_for_unknown_tool() {
        let manager = McpManager { clients: BTreeMap::new(), tool_routes: BTreeMap::new(), diagnostics: Vec::new() };

        let output = manager.call_tool(&ToolUseRequest::new(
            "mcp__missing__echo".to_string(),
            "{}".to_string(),
            "toolu_1".to_string(),
        ));

        assert_eq!(output.status, ToolStatus::Failed);
        assert_eq!(output.error.as_deref(), Some("unknown MCP tool: mcp__missing__echo"));
    }

    #[test]
    fn mcp_output_is_redacted_and_capped() {
        let output = sanitize_mcp_output(ToolOutput::ok(
            "mcp__docs__secret",
            vec![format!("api_key=sk-{}", "a".repeat(80)), "x".repeat(MAX_LINE_LEN + 20)],
        ));

        assert!(output.output[0].contains("[REDACTED]"));
        assert!(output.output[1].ends_with("..."));
        assert!(output.output.iter().any(|line| line == "[mcp output truncated]"));
    }

    #[test]
    fn provider_catalog_includes_cached_mcp_stdio_tools() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), "echo", 0, false);
        let mut config = McpConfig::default();
        config.servers.insert("docs".to_string(), server_config(&script));

        let manager = McpManager::from_config(&config);
        let definitions = crate::tools::runtime_tool_definitions(Some(&manager));
        let schemas = crate::tools::tool_catalog_schemas(&definitions);
        assert!(
            schemas
                .as_array()
                .expect("schemas")
                .iter()
                .filter_map(|schema| schema["name"].as_str())
                .any(|name| name == "mcp__docs__echo")
        );

        assert_eq!(
            schemas
                .as_array()
                .expect("schemas")
                .iter()
                .find(|schema| schema["name"] == "mcp__docs__echo")
                .expect("mcp schema")["input_schema"]["properties"]["text"]["type"],
            "string"
        );
    }

    fn server_config(script: &Path) -> McpServerConfig {
        McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            timeout_secs: 5,
            ..McpServerConfig::default()
        }
    }

    fn write_fake_server(
        dir: &Path, tool_name: &str, initialize_sleep_secs: u64, exit_after_initialize: bool,
    ) -> std::path::PathBuf {
        let path = dir.join(format!("fake_{tool_name}.py"));
        let exit_after_initialize = python_bool(exit_after_initialize);
        fs::write(
            &path,
            format!(
                r#"#!/usr/bin/env python3
import json
import sys
import time

tool_name = {tool_name:?}
initialize_sleep_secs = {initialize_sleep_secs}
exit_after_initialize = {exit_after_initialize}

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        if initialize_sleep_secs:
            time.sleep(initialize_sleep_secs)
        print(json.dumps({{
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {{
                "protocolVersion": "2025-06-18",
                "capabilities": {{"tools": {{}}}},
                "serverInfo": {{"name": "fake", "version": "0.1.0"}}
            }}
        }}), flush=True)
        if exit_after_initialize:
            sys.exit(0)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        print(json.dumps({{
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {{
                "tools": [{{
                    "name": tool_name,
                    "description": "Echo text",
                    "inputSchema": {{"type": "object", "properties": {{"text": {{"type": "string"}}}}}}
                }}]
            }}
        }}), flush=True)
    elif method == "tools/call":
        args = msg.get("params", {{}}).get("arguments", {{}})
        print(json.dumps({{
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {{"content": [{{"type": "text", "text": args.get("text", "")}}], "isError": False}}
        }}), flush=True)
"#
            ),
        )
        .expect("write fake server");
        path
    }

    fn python_bool(value: bool) -> &'static str {
        if value { "True" } else { "False" }
    }
}
