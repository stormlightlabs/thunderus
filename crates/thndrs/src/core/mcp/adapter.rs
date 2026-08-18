//! Thin adapter around the official Rust MCP SDK.
//!
//! This module owns the boundary between `thndrs`' synchronous runtime and the
//! async `rmcp` client. It deliberately converts SDK objects into local tool
//! definitions and outputs instead of exposing `rmcp` types through the rest of
//! the app.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use http::{HeaderName, HeaderValue};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ReadResourceRequestParams, ResourceContents, Tool,
};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::runtime::{Builder, Runtime};

use super::config::{McpServerConfig, McpTransport};
use crate::tools::{ToolDefinition, ToolOutput, shell};
use thndrs_agent::CancelToken;

const MAX_STDERR_BYTES: usize = 8 * 1024;
const MAX_RESOURCE_ITEMS: usize = 100;
const MAX_RESOURCE_CONTENT_ITEMS: usize = 8;
const MAX_RESOURCE_BYTES: usize = 128 * 1024;
const SUPPORTED_PROTOCOL_VERSION: &str = "2025-06-18";

/// Errors surfaced by the MCP SDK adapter.
#[derive(Debug, Error)]
pub enum McpSdkError {
    #[error("failed to build MCP runtime: {0}")]
    RuntimeBuild(std::io::Error),
    #[error("failed to create MCP transport: {0}")]
    TransportCreate(String),
    #[error("failed to initialize MCP server: {0}")]
    Initialize(String),
    #[error("failed to list MCP tools: {0}")]
    ListTools(String),
    #[error("failed to list MCP resources: {0}")]
    ListResources(String),
    #[error("failed to read MCP resource: {0}")]
    ReadResource(String),
    #[error("MCP server did not advertise `{capability}` capability")]
    CapabilityNotAdvertised { capability: &'static str },
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
    #[error("MCP operation cancelled")]
    Cancelled,
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
    /// Whether the server advertised the resources capability.
    pub resources_available: bool,
    /// Whether the server advertised the tools capability.
    pub tools_available: bool,
    /// Non-fatal diagnostics from initialization.
    pub diagnostics: Vec<String>,
}

/// Compact metadata for a resource advertised by an MCP server.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpResourceMetadata {
    /// MCP URI used to fetch the resource.
    pub uri: String,
    /// Server-provided resource name.
    pub name: String,
    /// Server-provided media type, when known.
    pub mime_type: Option<String>,
    /// Server-provided content size, when known.
    pub size: Option<u64>,
}

/// Serialization-safe content returned from an MCP resource read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpResourceContent {
    /// URI returned by the server for this content item.
    pub uri: String,
    /// Server-provided media type, when known.
    pub mime_type: Option<String>,
    /// `text` or `opaque_binary`.
    pub kind: &'static str,
    /// Text or base64 data, when it fits within the configured byte limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Size of the serialized source data before capping.
    pub byte_count: usize,
    /// Whether the source data was omitted because it exceeded a limit.
    pub truncated: bool,
}

/// Result returned from an MCP resource read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpResourceRead {
    /// Content items returned by the server, capped at a small fixed count.
    pub contents: Vec<McpResourceContent>,
    /// Whether items or data were omitted to enforce limits.
    pub truncated: bool,
}

/// An MCP tool converted into the local registry definition shape.
#[derive(Clone, Debug)]
pub struct McpToolDefinition {
    /// Provider-visible namespaced tool definition.
    pub definition: ToolDefinition,
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
    /// Connect to a configured MCP server and run the initialize handshake.
    pub fn connect(server: &McpServerConfig) -> Result<Self, McpSdkError> {
        match server.transport {
            McpTransport::Stdio => Self::connect_stdio(server),
            McpTransport::StreamableHttp => Self::connect_streamable_http(server),
        }
    }

    /// Connect to a configured stdio MCP server and run the initialize handshake.
    pub fn connect_stdio(server: &McpServerConfig) -> Result<Self, McpSdkError> {
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
                resources_available: false,
                tools_available: false,
                diagnostics: vec!["mcp initialize completed without server info".to_string()],
            });

        Ok(Self { client: Some(client), runtime, server_info, stderr, timeout_secs: server.timeout_secs })
    }

    /// Connect to a configured Streamable HTTP MCP server and run initialize.
    pub fn connect_streamable_http(server: &McpServerConfig) -> Result<Self, McpSdkError> {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(McpSdkError::RuntimeBuild)?;
        let stderr = Arc::new(Mutex::new(BoundedStderr::default()));
        let client = match runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(server.timeout_secs),
                connect_streamable_http_async(server),
            )
            .await
        }) {
            Ok(result) => result?,
            Err(_) => {
                return Err(McpSdkError::Timeout {
                    operation: "initialize",
                    timeout_secs: server.timeout_secs,
                    stderr: String::new(),
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
                resources_available: false,
                tools_available: false,
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
        if !self.server_info.tools_available {
            return Ok(Vec::new());
        }
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

    /// List  metadata for resources advertised by this server.
    pub fn list_resources(&self) -> Result<Vec<McpResourceMetadata>, McpSdkError> {
        if !self.server_info.resources_available {
            return Ok(Vec::new());
        }
        let client = self.client.as_ref().ok_or(McpSdkError::Closed)?;
        let resources = match self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(self.timeout_secs), client.list_all_resources()).await
        }) {
            Ok(result) => result.map_err(|err| McpSdkError::ListResources(shell::redact_secrets(&err.to_string())))?,
            Err(_) => {
                return Err(McpSdkError::Timeout {
                    operation: "resources/list",
                    timeout_secs: self.timeout_secs,
                    stderr: stderr_error_suffix(&self.stderr),
                });
            }
        };
        Ok(resources
            .into_iter()
            .take(MAX_RESOURCE_ITEMS)
            .map(|resource| McpResourceMetadata {
                uri: resource.uri,
                name: resource.name,
                mime_type: resource.mime_type,
                size: resource.size,
            })
            .collect())
    }

    /// Read one explicitly requested resource with serialization.
    pub fn read_resource(&self, uri: &str) -> Result<McpResourceRead, McpSdkError> {
        self.read_resource_with_cancel(uri, &CancelToken::new())
    }

    /// Read one explicitly requested resource while honoring cooperative cancellation.
    pub fn read_resource_with_cancel(&self, uri: &str, cancel: &CancelToken) -> Result<McpResourceRead, McpSdkError> {
        if !self.server_info.resources_available {
            return Err(McpSdkError::CapabilityNotAdvertised { capability: "resources" });
        }
        let client = self.client.as_ref().ok_or(McpSdkError::Closed)?;
        let cancel = cancel.clone();
        let result = match self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(self.timeout_secs), async {
                tokio::select! {
                    result = client.read_resource(ReadResourceRequestParams::new(uri)) => {
                        result.map_err(|err| McpSdkError::ReadResource(shell::redact_secrets(&err.to_string())))
                    }
                    () = wait_for_cancellation(cancel) => Err(McpSdkError::Cancelled),
                }
            })
            .await
        }) {
            Ok(result) => result?,
            Err(_) => {
                return Err(McpSdkError::Timeout {
                    operation: "resources/read",
                    timeout_secs: self.timeout_secs,
                    stderr: stderr_error_suffix(&self.stderr),
                });
            }
        };
        Ok(bound_resource_contents(result.contents))
    }

    /// Call an MCP tool and convert the result into unified local tool output.
    pub fn call_tool(
        &self, namespaced_tool_name: &str, original_tool_name: &str, arguments: serde_json::Value,
    ) -> Result<ToolOutput, McpSdkError> {
        self.call_tool_with_cancel(namespaced_tool_name, original_tool_name, arguments, &CancelToken::new())
    }

    /// Call an MCP tool while honoring cooperative cancellation.
    pub fn call_tool_with_cancel(
        &self, namespaced_tool_name: &str, original_tool_name: &str, arguments: serde_json::Value, cancel: &CancelToken,
    ) -> Result<ToolOutput, McpSdkError> {
        let client = self.client.as_ref().ok_or(McpSdkError::Closed)?;
        let arguments = json_object_arguments(arguments)?;
        let cancel = cancel.clone();
        let result = match self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(self.timeout_secs), async {
                tokio::select! {
                    result = client.call_tool(CallToolRequestParams::new(original_tool_name.to_string()).with_arguments(arguments)) => {
                        result.map_err(|err| McpSdkError::CallTool {
                            tool: original_tool_name.to_string(),
                            message: shell::redact_secrets(&err.to_string()),
                        })
                    }
                    () = wait_for_cancellation(cancel) => Err(McpSdkError::Cancelled),
                }
            })
            .await
        }) {
            Ok(result) => result?,
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

async fn connect_streamable_http_async(
    server: &McpServerConfig,
) -> Result<RunningService<RoleClient, ()>, McpSdkError> {
    let url = server
        .url
        .as_deref()
        .ok_or_else(|| McpSdkError::TransportCreate("missing streamable_http url".to_string()))?;
    let transport_config =
        StreamableHttpClientTransportConfig::with_uri(url).custom_headers(http_headers(&server.headers)?);
    let transport = StreamableHttpClientTransport::from_config(transport_config);
    ().serve(transport)
        .await
        .map_err(|err| McpSdkError::Initialize(err.to_string()))
}

fn http_headers(
    headers: &std::collections::BTreeMap<String, String>,
) -> Result<HashMap<HeaderName, HeaderValue>, McpSdkError> {
    headers
        .iter()
        .map(|(name, value)| {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|err| McpSdkError::TransportCreate(format!("invalid HTTP header name `{name}`: {err}")))?;
            let value = HeaderValue::from_str(value).map_err(|err| {
                McpSdkError::TransportCreate(format!("invalid HTTP header value for `{name}`: {err}"))
            })?;
            Ok((name, value))
        })
        .collect()
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
        resources_available: info.capabilities.resources.is_some(),
        tools_available: info.capabilities.tools.is_some(),
        diagnostics,
    }
}

/// Transcript presentation for a namespaced MCP tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpToolPresentation {
    /// A tool whose original name identifies web search.
    Search,
    /// A tool whose original name identifies web retrieval.
    Fetch,
    /// Any other MCP tool.
    Generic,
}

/// Build the provider-visible namespaced MCP tool name.
pub fn namespaced_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("mcp__{server_name}__{tool_name}")
}

/// Build the compact namespace used to identify MCP resources.
pub fn namespaced_resource_name(server_name: &str) -> String {
    format!("mcp__{server_name}__resource")
}

/// Build the provider-visible tool name for explicit MCP resource reads.
pub fn namespaced_resource_read_name(server_name: &str) -> String {
    format!("mcp__{server_name}__resource_read")
}

/// Build the tool definition used for one explicit MCP resource read.
pub fn resource_read_tool_definition(server_name: &str) -> ToolDefinition {
    ToolDefinition::new(
        namespaced_resource_read_name(server_name),
        format!("Read one explicitly requested MCP resource from server `{server_name}`."),
        serde_json::json!({
            "type": "object",
            "properties": { "uri": { "type": "string", "description": "The MCP resource URI to read." } },
            "required": ["uri"],
            "additionalProperties": false,
        }),
    )
}

/// Classify a namespaced MCP tool from its server-reported original name.
///
/// The namespace preserves the original name after its second `__`, so this
/// remains independent of any particular MCP server package or configuration.
pub fn tool_presentation(namespaced_tool_name: &str) -> McpToolPresentation {
    let Some((_, original_name)) = namespaced_tool_name
        .strip_prefix("mcp__")
        .and_then(|name| name.split_once("__"))
    else {
        return McpToolPresentation::Generic;
    };

    match original_name.to_ascii_lowercase().as_str() {
        "search" | "web_search" | "search_web" | "websearch" => McpToolPresentation::Search,
        "fetch" | "web_fetch" | "fetch_url" | "read_url" => McpToolPresentation::Fetch,
        _ => McpToolPresentation::Generic,
    }
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

fn bound_resource_contents(contents: Vec<ResourceContents>) -> McpResourceRead {
    let mut remaining_bytes = MAX_RESOURCE_BYTES;
    let mut bounded = Vec::new();
    let mut truncated = false;

    for content in contents {
        if bounded.len() >= MAX_RESOURCE_CONTENT_ITEMS {
            truncated = true;
            break;
        }
        let (uri, mime_type, kind, source) = match content {
            ResourceContents::TextResourceContents { uri, mime_type, text, .. } => {
                (uri, mime_type, "text", shell::redact_secrets(&text))
            }
            ResourceContents::BlobResourceContents { uri, mime_type, blob, .. } => {
                (uri, mime_type, "opaque_binary", blob)
            }
            _ => continue,
        };
        let byte_count = source.len();
        let item_truncated = byte_count > remaining_bytes;
        if item_truncated {
            truncated = true;
        } else {
            remaining_bytes -= byte_count;
        }
        bounded.push(McpResourceContent {
            uri,
            mime_type,
            kind,
            data: (!item_truncated).then_some(source),
            byte_count,
            truncated: item_truncated,
        });
    }

    McpResourceRead { contents: bounded, truncated }
}

fn json_object_arguments(arguments: serde_json::Value) -> Result<rmcp::model::JsonObject, McpSdkError> {
    match arguments {
        serde_json::Value::Object(object) => Ok(object),
        serde_json::Value::Null => Ok(serde_json::Map::new()),
        _ => Err(McpSdkError::ArgumentsNotObject),
    }
}

async fn wait_for_cancellation(cancel: CancelToken) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(20)).await;
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
    let text = shell::redact_secrets(&String::from_utf8_lossy(&capture.bytes));
    let text = text.trim().to_string();
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::Path;
    use std::thread;

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
        assert_eq!(converted.original_tool_name, "echo");
    }

    #[test]
    fn namespaced_web_tools_keep_search_and_fetch_presentation() {
        assert_eq!(
            tool_presentation("mcp__research__web_search"),
            McpToolPresentation::Search
        );
        assert_eq!(
            tool_presentation("mcp__research__web_fetch"),
            McpToolPresentation::Fetch
        );
        assert_eq!(
            tool_presentation("mcp__research__database_query"),
            McpToolPresentation::Generic
        );
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
    fn resource_contents_preserve_media_type_and_bound_serialization() {
        let result = bound_resource_contents(vec![
            ResourceContents::TextResourceContents {
                uri: "memo://secret".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: "api_key=super-secret-value".to_string(),
                meta: None,
            },
            ResourceContents::BlobResourceContents {
                uri: "memo://binary".to_string(),
                mime_type: Some("application/octet-stream".to_string()),
                blob: "AAEC".to_string(),
                meta: None,
            },
            ResourceContents::TextResourceContents {
                uri: "memo://large".to_string(),
                mime_type: None,
                text: "x".repeat(MAX_RESOURCE_BYTES),
                meta: None,
            },
        ]);

        assert_eq!(result.contents[0].kind, "text");
        assert_eq!(result.contents[0].mime_type.as_deref(), Some("text/plain"));
        assert_eq!(result.contents[0].data.as_deref(), Some("api_key=[REDACTED]"));
        assert_eq!(result.contents[1].kind, "opaque_binary");
        assert_eq!(result.contents[1].data.as_deref(), Some("AAEC"));
        assert!(result.contents[2].truncated);
        assert!(result.truncated);
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
    }

    #[test]
    fn stdio_client_lists_and_reads_advertised_resources() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::Normal);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            ..McpServerConfig::default()
        };

        let client = McpSdkClient::connect_stdio(&server).expect("connect fake MCP server");
        let resources = client.list_resources().expect("list resources");
        assert_eq!(resources[0].uri, "memo://status");
        assert_eq!(resources[0].mime_type.as_deref(), Some("text/plain"));

        let resource = client.read_resource("memo://status").expect("read resource");
        assert_eq!(resource.contents[0].kind, "text");
        assert_eq!(resource.contents[0].data.as_deref(), Some("ready"));
    }

    #[test]
    fn mismatched_protocol_version_is_a_diagnostic_not_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::OldProtocolVersion);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            ..McpServerConfig::default()
        };

        let client = McpSdkClient::connect_stdio(&server).expect("connect fake MCP server");

        assert_eq!(client.server_info().protocol_version, "2024-11-05");
        assert_eq!(
            client.server_info().diagnostics,
            vec!["mcp server negotiated protocol version 2024-11-05; expected 2025-06-18"]
        );
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
    fn startup_timeout_truncates_bounded_stderr_diagnostics() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::NoisySlowInitialize);
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
        let text = err.to_string();

        assert!(text.contains("[stderr truncated]"));
        assert!(
            text.len() < MAX_STDERR_BYTES + 512,
            "stderr diagnostic should stay bounded"
        );
        assert!(!text.contains("noise-0"), "oldest stderr bytes should be dropped");
    }

    #[test]
    fn streamable_http_client_initializes_lists_and_calls_json_fixture() {
        let fixture = HttpMcpFixture::spawn(HttpResponseMode::Json);
        let server = McpServerConfig {
            transport: McpTransport::StreamableHttp,
            url: Some(fixture.url),
            timeout_secs: 5,
            ..McpServerConfig::default()
        };

        let client = McpSdkClient::connect_streamable_http(&server).expect("connect HTTP MCP server");
        assert_eq!(client.server_info().name, "fake-http");

        let tools = client.list_tool_definitions("web").expect("list tools");
        assert_eq!(tools[0].definition.name, "mcp__web__echo");

        let output = client
            .call_tool("mcp__web__echo", "echo", json!({ "text": "hello http" }))
            .expect("call echo");
        assert_eq!(output, ToolOutput::ok("mcp__web__echo", vec!["hello http".to_string()]));
    }

    #[test]
    fn streamable_http_client_accepts_sse_fixture_responses() {
        let fixture = HttpMcpFixture::spawn(HttpResponseMode::Sse);
        let server = McpServerConfig {
            transport: McpTransport::StreamableHttp,
            url: Some(fixture.url),
            timeout_secs: 5,
            ..McpServerConfig::default()
        };

        let client = McpSdkClient::connect_streamable_http(&server).expect("connect SSE MCP server");
        let tools = client.list_tool_definitions("web").expect("list tools");

        assert_eq!(tools[0].definition.name, "mcp__web__echo");
    }

    #[test]
    fn streamable_http_headers_are_validated() {
        let mut server = McpServerConfig {
            transport: McpTransport::StreamableHttp,
            url: Some("http://127.0.0.1:1/mcp".to_string()),
            timeout_secs: 1,
            ..McpServerConfig::default()
        };
        server
            .headers
            .insert("bad header".to_string(), "secret-token-value".to_string());

        let err = match McpSdkClient::connect_streamable_http(&server) {
            Ok(_) => panic!("invalid header should fail before connect"),
            Err(err) => err,
        };

        assert!(matches!(err, McpSdkError::TransportCreate(_)));
        assert!(!err.to_string().contains("secret-token-value"));
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
    fn cancelled_tool_call_returns_cancelled_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = write_fake_server(temp.path(), FakeServerMode::SlowCall);
        let server = McpServerConfig {
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            timeout_secs: 5,
            ..McpServerConfig::default()
        };
        let client = McpSdkClient::connect_stdio(&server).expect("connect fake MCP server");
        let cancel = CancelToken::new();
        cancel.cancel();

        let error = client
            .call_tool_with_cancel("mcp__docs__echo", "echo", json!({ "text": "ok" }), &cancel)
            .expect_err("cancelled call should fail");

        assert!(matches!(error, McpSdkError::Cancelled));
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
        OldProtocolVersion,
        SlowInitialize,
        NoisySlowInitialize,
        SlowCall,
        ExitAfterInitialize,
    }

    fn write_fake_server(dir: &Path, mode: FakeServerMode) -> std::path::PathBuf {
        let path = dir.join("fake_mcp.py");
        let malformed = python_bool(matches!(mode, FakeServerMode::MalformedInitialize));
        let old_protocol = python_bool(matches!(mode, FakeServerMode::OldProtocolVersion));
        let slow_initialize = python_bool(matches!(mode, FakeServerMode::SlowInitialize));
        let noisy_slow_initialize = python_bool(matches!(mode, FakeServerMode::NoisySlowInitialize));
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
old_protocol = {old_protocol}
slow_initialize = {slow_initialize}
noisy_slow_initialize = {noisy_slow_initialize}
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
        if noisy_slow_initialize:
            for i in range(900):
                print(f"noise-{{i}}-" + ("x" * 80), file=sys.stderr, flush=True)
            time.sleep(2)
        print(json.dumps({{
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {{
                "protocolVersion": "2024-11-05" if old_protocol else "2025-06-18",
                "capabilities": {{"tools": {{}}, "resources": {{}}}},
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
    elif method == "resources/list":
        print(json.dumps({{
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {{"resources": [{{"uri": "memo://status", "name": "status", "mimeType": "text/plain", "size": 5}}]}}
        }}), flush=True)
    elif method == "resources/read":
        print(json.dumps({{
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {{"contents": [{{"uri": msg["params"]["uri"], "mimeType": "text/plain", "text": "ready"}}]}}
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

    struct HttpMcpFixture {
        url: String,
    }

    #[derive(Clone, Copy)]
    enum HttpResponseMode {
        Json,
        Sse,
    }

    impl HttpMcpFixture {
        fn spawn(mode: HttpResponseMode) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
            let addr = listener.local_addr().expect("fixture addr");
            thread::spawn(move || {
                for stream in listener.incoming().take(8).flatten() {
                    handle_http_mcp_request(stream, mode);
                }
            });
            Self { url: format!("http://{addr}/mcp") }
        }
    }

    fn handle_http_mcp_request(mut stream: TcpStream, mode: HttpResponseMode) {
        let body = read_http_body(&mut stream);
        let message: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
        let Some(id) = message.get("id").cloned() else {
            write_http_response(&mut stream, mode, None);
            return;
        };
        let method = message
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "fake-http", "version": "0.1.0"}
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [{
                        "name": "echo",
                        "description": "Echo text",
                        "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}
                    }]
                }
            }),
            "tools/call" => {
                let text = message["params"]["arguments"]["text"].as_str().unwrap_or_default();
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {"content": [{"type": "text", "text": text}], "isError": false}
                })
            }
            _ => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "not found"}}),
        };
        write_http_response(&mut stream, mode, Some(response));
    }

    fn read_http_body(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let n = stream.read(&mut chunk).expect("read request");
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
            if let Some((header_len, content_len)) = http_request_lengths(&buffer)
                && buffer.len() >= header_len + content_len
            {
                return String::from_utf8_lossy(&buffer[header_len..header_len + content_len]).to_string();
            }
        }
        String::new()
    }

    fn http_request_lengths(buffer: &[u8]) -> Option<(usize, usize)> {
        let headers_end = buffer.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
        let headers = String::from_utf8_lossy(&buffer[..headers_end]);
        let content_len = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .or_else(|| line.strip_prefix("content-length:"))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        Some((headers_end, content_len))
    }

    fn write_http_response(stream: &mut TcpStream, mode: HttpResponseMode, response: Option<serde_json::Value>) {
        let Some(response) = response else {
            stream
                .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n")
                .expect("write accepted");
            return;
        };
        let body = match mode {
            HttpResponseMode::Json => response.to_string(),
            HttpResponseMode::Sse => format!("event: message\ndata: {response}\n\n"),
        };
        let content_type = match mode {
            HttpResponseMode::Json => "application/json",
            HttpResponseMode::Sse => "text/event-stream",
        };
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("write response");
    }
}
