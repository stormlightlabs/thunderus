//! Thin adapter around the official Rust MCP SDK.
//!
//! This module owns the boundary between `thndrs`' synchronous runtime and the
//! async `rmcp` client. It deliberately converts SDK objects into local tool
//! definitions and outputs instead of exposing `rmcp` types through the rest of
//! the app.

use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::TokioChildProcess;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::runtime::{Builder, Runtime};

use super::config::{McpServerConfig, McpTransport};
use crate::tools::{ToolDefinition, ToolOutput};

const MAX_STDERR_BYTES: usize = 8 * 1024;
const SUPPORTED_PROTOCOL_VERSION: &str = "2025-06-18";

/// Errors surfaced by the MCP SDK adapter.
#[derive(Debug, Error)]
pub enum McpSdkError {
    #[error("unsupported MCP transport for SDK adapter: {0:?}")]
    UnsupportedTransport(McpTransport),
    #[error("failed to build MCP runtime: {0}")]
    RuntimeBuild(std::io::Error),
    #[error("failed to create MCP stdio transport: {0}")]
    TransportCreate(String),
    #[error("failed to initialize MCP server: {0}")]
    Initialize(String),
    #[error("failed to list MCP tools: {0}")]
    ListTools(String),
    #[error("failed to call MCP tool `{tool}`: {message}")]
    CallTool { tool: String, message: String },
    #[error("MCP {operation} timed out after {timeout_secs}s{stderr}")]
    Timeout {
        operation: &'static str,
        timeout_secs: u64,
        stderr: String,
    },
    #[error("MCP tool arguments must be a JSON object")]
    ArgumentsNotObject,
    #[error("MCP client is closed")]
    Closed,
}

/// Metadata from a successful MCP server initialize handshake.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpServerInfo {
    /// Negotiated MCP protocol version.
    pub protocol_version: String,
    /// Server implementation name.
    pub name: String,
    /// Server implementation version.
    pub version: String,
    /// Optional server instructions.
    pub instructions: Option<String>,
    /// Non-fatal diagnostics from initialization.
    pub diagnostics: Vec<String>,
}

/// An MCP tool converted into the local registry definition shape.
#[derive(Clone, Debug)]
pub struct McpToolDefinition {
    /// Provider-visible namespaced tool definition.
    pub definition: ToolDefinition,
    /// Configured MCP server name.
    pub server_name: String,
    /// Original MCP tool name reported by the server.
    pub original_tool_name: String,
}

#[derive(Debug, Default)]
struct BoundedStderr {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Connected SDK client for one stdio MCP server.
pub struct McpSdkClient {
    client: Option<RunningService<RoleClient, ()>>,
    runtime: Runtime,
    server_info: McpServerInfo,
    stderr: Arc<Mutex<BoundedStderr>>,
    timeout_secs: u64,
}

impl McpSdkClient {
    /// Connect to a configured stdio MCP server and run the initialize handshake.
    pub fn connect_stdio(server: &McpServerConfig) -> Result<Self, McpSdkError> {
        if server.transport != McpTransport::Stdio {
            return Err(McpSdkError::UnsupportedTransport(server.transport));
        }

        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(McpSdkError::RuntimeBuild)?;
        let stderr = Arc::new(Mutex::new(BoundedStderr::default()));
        let client = match runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(server.timeout_secs),
                connect_stdio_async(server, Arc::clone(&stderr)),
            )
            .await
        }) {
            Ok(result) => result?,
            Err(_) => {
                return Err(McpSdkError::Timeout {
                    operation: "initialize",
                    timeout_secs: server.timeout_secs,
                    stderr: stderr_error_suffix(&stderr),
                });
            }
        };
        let server_info = client
            .peer_info()
            .as_deref()
            .map(server_info_from_sdk)
            .unwrap_or_else(|| McpServerInfo {
                protocol_version: "unknown".to_string(),
                name: "unknown".to_string(),
                version: "unknown".to_string(),
                instructions: None,
                diagnostics: vec!["mcp initialize completed without server info".to_string()],
            });

        Ok(Self { client: Some(client), runtime, server_info, stderr, timeout_secs: server.timeout_secs })
    }

    /// Server information captured during initialize.
    pub fn server_info(&self) -> &McpServerInfo {
        &self.server_info
    }

    /// Current bounded stderr text captured from the child process.
    pub fn stderr_diagnostics(&self) -> Option<String> {
        stderr_text(&self.stderr)
    }

    /// List tools and convert them to local registry definitions.
    pub fn list_tool_definitions(&self, server_name: &str) -> Result<Vec<McpToolDefinition>, McpSdkError> {
        let client = self.client.as_ref().ok_or(McpSdkError::Closed)?;
        let tools = match self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(self.timeout_secs), client.list_all_tools()).await
        }) {
            Ok(result) => result.map_err(|err| McpSdkError::ListTools(err.to_string()))?,
            Err(_) => {
                return Err(McpSdkError::Timeout {
                    operation: "tools/list",
                    timeout_secs: self.timeout_secs,
                    stderr: stderr_error_suffix(&self.stderr),
                });
            }
        };
        Ok(tools
            .iter()
            .map(|tool| sdk_tool_to_definition(server_name, tool))
            .collect())
    }

    /// Call an MCP tool and convert the result into unified local tool output.
    pub fn call_tool(
        &self, namespaced_tool_name: &str, original_tool_name: &str, arguments: serde_json::Value,
    ) -> Result<ToolOutput, McpSdkError> {
        let client = self.client.as_ref().ok_or(McpSdkError::Closed)?;
        let arguments = json_object_arguments(arguments)?;
        let result = match self.runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(self.timeout_secs),
                client.call_tool(CallToolRequestParams::new(original_tool_name.to_string()).with_arguments(arguments)),
            )
            .await
        }) {
            Ok(result) => result.map_err(|err| McpSdkError::CallTool {
                tool: original_tool_name.to_string(),
                message: err.to_string(),
            })?,
            Err(_) => {
                return Err(McpSdkError::Timeout {
                    operation: "tools/call",
                    timeout_secs: self.timeout_secs,
                    stderr: stderr_error_suffix(&self.stderr),
                });
            }
        };
        Ok(sdk_call_result_to_output(namespaced_tool_name, &result))
    }

    /// Close the SDK service and child process.
    pub fn close(mut self) -> Result<(), McpSdkError> {
        self.close_inner()
    }

    fn close_inner(&mut self) -> Result<(), McpSdkError> {
        if let Some(client) = self.client.take() {
            self.runtime
                .block_on(client.cancel())
                .map_err(|err| McpSdkError::CallTool { tool: "shutdown".to_string(), message: err.to_string() })?;
        }
        Ok(())
    }
}

impl Drop for McpSdkClient {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}

async fn connect_stdio_async(
    server: &McpServerConfig, stderr_capture: Arc<Mutex<BoundedStderr>>,
) -> Result<RunningService<RoleClient, ()>, McpSdkError> {
    let mut command = Command::new(&server.command);
    command.args(&server.args);
    command.envs(&server.env);

    let (transport, stderr) = TokioChildProcess::builder(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| McpSdkError::TransportCreate(err.to_string()))?;
    if let Some(stderr) = stderr {
        tokio::spawn(capture_stderr(stderr, stderr_capture));
    }
    ().serve(transport)
        .await
        .map_err(|err| McpSdkError::Initialize(err.to_string()))
}

fn server_info_from_sdk(info: &rmcp::model::ServerInfo) -> McpServerInfo {
    let protocol_version = info.protocol_version.to_string();
    let diagnostics = if protocol_version == SUPPORTED_PROTOCOL_VERSION {
        Vec::new()
    } else {
        vec![format!(
            "mcp server negotiated protocol version {protocol_version}; expected {SUPPORTED_PROTOCOL_VERSION}"
        )]
    };

    McpServerInfo {
        protocol_version,
        name: info.server_info.name.clone(),
        version: info.server_info.version.clone(),
        instructions: info.instructions.clone(),
        diagnostics,
    }
}

/// Build the provider-visible namespaced MCP tool name.
pub fn namespaced_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("mcp__{server_name}__{tool_name}")
}

/// Convert one SDK tool definition to the local registry definition shape.
pub fn sdk_tool_to_definition(server_name: &str, tool: &Tool) -> McpToolDefinition {
    let original_tool_name = tool.name.to_string();
    let namespaced_name = namespaced_tool_name(server_name, &original_tool_name);
    let description = tool
        .description
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| tool.title.clone())
        .unwrap_or_else(|| format!("MCP tool `{original_tool_name}` from server `{server_name}`."));
    let input_schema = serde_json::Value::Object(tool.input_schema.as_ref().clone());

    McpToolDefinition {
        definition: ToolDefinition::new(namespaced_name, description, input_schema),
        server_name: server_name.to_string(),
        original_tool_name,
    }
}

/// Convert an SDK tool-call result into the local execution output shape.
pub fn sdk_call_result_to_output(tool_name: &str, result: &CallToolResult) -> ToolOutput {
    let lines = call_result_lines(result);
    if result.is_error.unwrap_or(false) {
        let error = if lines.is_empty() { "MCP tool returned an error".to_string() } else { lines.join("\n") };
        ToolOutput::failed(tool_name, error)
    } else {
        ToolOutput::ok(tool_name, lines)
    }
}

/// Convert SDK/protocol errors into stable failed tool output.
pub fn sdk_error_to_output(tool_name: &str, operation: &str, error: &McpSdkError) -> ToolOutput {
    ToolOutput::failed(tool_name, format!("mcp {operation} failed: {error}"))
}

fn call_result_lines(result: &CallToolResult) -> Vec<String> {
    let mut lines = Vec::new();
    for block in &result.content {
        match block {
            ContentBlock::Text(text) => lines.push(text.text.clone()),
            other => lines.push(serde_json::to_string(other).unwrap_or_else(|_| format!("{other:?}"))),
        }
    }
    if let Some(value) = &result.structured_content {
        lines.push(format!("structured_content: {value}"));
    }
    lines
}

fn json_object_arguments(arguments: serde_json::Value) -> Result<rmcp::model::JsonObject, McpSdkError> {
    match arguments {
        serde_json::Value::Object(object) => Ok(object),
        serde_json::Value::Null => Ok(serde_json::Map::new()),
        _ => Err(McpSdkError::ArgumentsNotObject),
    }
}

async fn capture_stderr(mut stderr: tokio::process::ChildStderr, capture: Arc<Mutex<BoundedStderr>>) {
    let mut chunk = [0_u8; 1024];
    loop {
        match stderr.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => append_stderr(&capture, &chunk[..n]),
        }
    }
}

fn append_stderr(capture: &Arc<Mutex<BoundedStderr>>, chunk: &[u8]) {
    let mut capture = capture.lock().expect("stderr capture lock poisoned");
    capture.bytes.extend_from_slice(chunk);
    if capture.bytes.len() > MAX_STDERR_BYTES {
        let overflow = capture.bytes.len() - MAX_STDERR_BYTES;
        capture.bytes.drain(0..overflow);
        capture.truncated = true;
    }
}

fn stderr_text(capture: &Arc<Mutex<BoundedStderr>>) -> Option<String> {
    let capture = capture.lock().expect("stderr capture lock poisoned");
    let text = String::from_utf8_lossy(&capture.bytes).trim().to_string();
    if text.is_empty() {
        None
    } else if capture.truncated {
        Some(format!("[stderr truncated]\n{text}"))
    } else {
        Some(text)
    }
}

fn stderr_error_suffix(capture: &Arc<Mutex<BoundedStderr>>) -> String {
    stderr_text(capture)
        .map(|text| format!("; stderr: {text}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use rmcp::model::{CallToolResult, ContentBlock, Tool};
    use serde_json::json;

    use super::*;

    #[test]
    fn converts_sdk_tool_to_namespaced_definition() {
        let tool = Tool::new(
            "echo",
            "Echo input",
            serde_json::Map::from_iter([("type".to_string(), serde_json::Value::String("object".to_string()))]),
        );

        let converted = sdk_tool_to_definition("docs", &tool);

        assert_eq!(converted.definition.name, "mcp__docs__echo");
        assert_eq!(converted.definition.description, "Echo input");
        assert_eq!(converted.definition.input_schema["type"], "object");
        assert_eq!(converted.server_name, "docs");
        assert_eq!(converted.original_tool_name, "echo");
    }

    #[test]
    fn converts_successful_call_result_to_tool_output() {
        let result = CallToolResult::success(vec![ContentBlock::text("ok")]);

        let output = sdk_call_result_to_output("mcp__docs__echo", &result);

        assert_eq!(output, ToolOutput::ok("mcp__docs__echo", vec!["ok".to_string()]));
    }

    #[test]
    fn converts_error_call_result_to_failed_tool_output() {
        let result = CallToolResult::error(vec![ContentBlock::text("bad input")]);

        let output = sdk_call_result_to_output("mcp__docs__echo", &result);

        assert_eq!(output, ToolOutput::failed("mcp__docs__echo", "bad input".to_string()));
    }

    #[test]
    fn rejects_non_object_tool_arguments() {
        let err = json_object_arguments(json!("bad")).expect_err("non-object rejected");

        assert!(matches!(err, McpSdkError::ArgumentsNotObject));
    }

    #[test]
    fn sdk_error_becomes_stable_failed_output() {
        let error = McpSdkError::ArgumentsNotObject;

        let output = sdk_error_to_output("mcp__docs__echo", "call", &error);

        assert_eq!(
            output,
            ToolOutput::failed(
                "mcp__docs__echo",
                "mcp call failed: MCP tool arguments must be a JSON object".to_string()
            )
        );
    }

    #[test]
    fn stdio_client_initializes_lists_and_calls_fake_server() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::Normal);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            ..McpServerConfig::default()
        };

        let client = McpSdkClient::connect_stdio(&server).expect("connect fake MCP server");
        assert_eq!(client.server_info().name, "fake");
        assert_eq!(client.server_info().protocol_version, SUPPORTED_PROTOCOL_VERSION);
        assert!(client.server_info().diagnostics.is_empty());

        let tools = client.list_tool_definitions("docs").expect("list tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].definition.name, "mcp__docs__echo");

        let output = client
            .call_tool("mcp__docs__echo", "echo", json!({ "text": "ok" }))
            .expect("call echo");
        assert_eq!(output, ToolOutput::ok("mcp__docs__echo", vec!["ok".to_string()]));
        client.close().expect("close client");
    }

    #[test]
    fn malformed_server_message_becomes_initialize_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::MalformedInitialize);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            ..McpServerConfig::default()
        };

        let err = match McpSdkClient::connect_stdio(&server) {
            Ok(_) => panic!("malformed server should fail"),
            Err(err) => err,
        };

        assert!(matches!(err, McpSdkError::Initialize(_)));
    }

    #[test]
    fn startup_timeout_includes_stderr_diagnostics() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::SlowInitialize);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            timeout_secs: 1,
            ..McpServerConfig::default()
        };

        let err = match McpSdkClient::connect_stdio(&server) {
            Ok(_) => panic!("slow server should time out"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            McpSdkError::Timeout { operation: "initialize", timeout_secs: 1, .. }
        ));
        assert!(err.to_string().contains("starting slowly"));
    }

    #[test]
    fn per_call_timeout_returns_stable_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::SlowCall);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            timeout_secs: 1,
            ..McpServerConfig::default()
        };
        let client = McpSdkClient::connect_stdio(&server).expect("connect fake MCP server");

        let err = client
            .call_tool("mcp__docs__echo", "echo", json!({ "text": "ok" }))
            .expect_err("slow call should time out");

        assert!(matches!(
            err,
            McpSdkError::Timeout { operation: "tools/call", timeout_secs: 1, .. }
        ));
        assert!(err.to_string().contains("call is slow"));
    }

    #[test]
    fn server_process_exit_becomes_list_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::ExitAfterInitialize);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            timeout_secs: 1,
            ..McpServerConfig::default()
        };
        let err = match McpSdkClient::connect_stdio(&server) {
            Ok(client) => client
                .list_tool_definitions("docs")
                .expect_err("exited server should not list tools"),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            McpSdkError::Initialize(_)
                | McpSdkError::ListTools(_)
                | McpSdkError::Timeout { operation: "tools/list", .. }
        ));
    }

    #[derive(Clone, Copy)]
    enum FakeServerMode {
        Normal,
        MalformedInitialize,
        SlowInitialize,
        SlowCall,
        ExitAfterInitialize,
    }

    fn write_fake_server(dir: &Path, mode: FakeServerMode) -> std::path::PathBuf {
        let path = dir.join("fake_mcp.py");
        let malformed = python_bool(matches!(mode, FakeServerMode::MalformedInitialize));
        let slow_initialize = python_bool(matches!(mode, FakeServerMode::SlowInitialize));
        let slow_call = python_bool(matches!(mode, FakeServerMode::SlowCall));
        let exit_after_initialize = python_bool(matches!(mode, FakeServerMode::ExitAfterInitialize));
        fs::write(
            &path,
            format!(
                r#"#!/usr/bin/env python3
import json
import sys
import time

malformed_initialize = {malformed}
slow_initialize = {slow_initialize}
slow_call = {slow_call}
exit_after_initialize = {exit_after_initialize}

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        if malformed_initialize:
            print("not-json", flush=True)
            sys.exit(0)
        if slow_initialize:
            print("starting slowly", file=sys.stderr, flush=True)
            time.sleep(2)
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
                    "name": "echo",
                    "description": "Echo text",
                    "inputSchema": {{"type": "object", "properties": {{"text": {{"type": "string"}}}}}}
                }}]
            }}
        }}), flush=True)
    elif method == "tools/call":
        args = msg.get("params", {{}}).get("arguments", {{}})
        if slow_call:
            print("call is slow", file=sys.stderr, flush=True)
            time.sleep(2)
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
