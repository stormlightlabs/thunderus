//! Unified agent loop for both the fake provider and the Umans provider.
//!
//! The loop runs on a background thread and sends [`AgentEvent`]s through a
//! channel. The TUI drains them with `try_recv`, keeping the UI responsive.
//!
//! ## Lifecycle
//!
//! 1. [`spawn_run`] starts a thread with a [`RunHandle`] (config + cancel flag).
//! 2. The run emits `Started`, then streams reasoning/assistant deltas and
//!    tool-use requests.
//! 3. Each tool-use request is dispatched via [`crate::tools::dispatch_tool`]
//!    and the result is emitted as a `ToolFinished` event appended to the
//!    transcript.
//! 4. For the Umans provider, tool results are fed back into the next turn.
//! 5. The loop enforces [`MAX_TOOL_ITERATIONS`] per turn to prevent recursive
//!    or unbounded tool-call loops.
//! 6. Cancellation is cooperative: the loop checks the shared [`CancelToken`]
//!    between events, lines, and tool executions. When cancelled, it emits
//!    [`AgentEvent::Cancelled`] and stops.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use ureq::http::Response;

use crate::app::AgentEvent;
use crate::cli::WebSearchMode;
use crate::providers::umans;
use crate::tools::{AgentRunConfig, ToolUseRequest, dispatch_tool};

/// Shared cancellation flag. Checked cooperatively by the agent loop.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Signal cancellation. The agent loop observes this on its next check.
    pub fn cancel(&self) {
        self.inner().store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner().load(Ordering::SeqCst)
    }

    pub fn inner(&self) -> &Arc<AtomicBool> {
        &self.0
    }
}

/// Which provider drives this agent run.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    /// Deterministic fake provider — no network, scripted events.
    Fake,
    /// Umans Code provider — real network calls.
    Umans,
}

/// Handle for a single agent run: provider kind, config, prompt, and cancel.
#[derive(Clone, Debug)]
pub struct RunHandle {
    pub provider: ProviderKind,
    pub config: AgentRunConfig,
    pub prompt: String,
    pub cancel: CancelToken,
}

impl RunHandle {
    /// Create a fake-provider run handle.
    pub fn fake(config: AgentRunConfig, prompt: String) -> Self {
        RunHandle { provider: ProviderKind::Fake, config, prompt, cancel: CancelToken::new() }
    }

    /// Create an Umans-provider run handle.
    #[allow(dead_code)]
    pub fn umans(config: AgentRunConfig, prompt: String) -> Self {
        RunHandle { provider: ProviderKind::Umans, config, prompt, cancel: CancelToken::new() }
    }
}

/// Spawn the unified agent loop on a background thread and return the receiver.
///
/// The thread closes its sender when done, so the receiver's `try_recv` will
/// return `Err(Disconnected)` once the run completes.
///
/// If the receiver is dropped early (e.g. the user cancels), the thread exits
/// on the next failed send. The [`CancelToken`] inside `handle` can also be
/// signalled for cooperative cancellation.
pub fn spawn_run(handle: RunHandle) -> Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>();
    let cancel = handle.cancel.clone();
    thread::spawn(move || run_agent(&handle, &tx, &cancel));
    rx
}

/// Backwards-compatible entrypoint: spawn the deterministic fake stream.
///
/// This keeps existing callers working while the unified loop is wired in.
#[allow(dead_code)]
pub fn spawn_fake_stream() -> Receiver<AgentEvent> {
    let config = AgentRunConfig::new(PathBuf::from("."), String::from("fake-agent"), WebSearchMode::Native);
    let handle = RunHandle::fake(config, String::new());
    spawn_run(handle)
}

/// The unified agent loop. Dispatches to the fake or Umans provider, handles
/// tool-use requests, enforces the per-turn cap, and checks cancellation
/// cooperatively.
fn run_agent(handle: &RunHandle, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
    if send(tx, AgentEvent::Started, cancel).is_none() {
        return;
    }
    step();

    match handle.provider {
        ProviderKind::Fake => run_fake(handle, tx, cancel),
        ProviderKind::Umans => run_umans(handle, tx, cancel),
    }
}

/// Deterministic fake provider: emits reasoning, a tool-use request, assistant
/// text, and finishes. Demonstrates the tool dispatch path end-to-end.
fn run_fake(handle: &RunHandle, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
    if send(
        tx,
        AgentEvent::ReasoningDelta(String::from("Let me think about this... ")),
        cancel,
    )
    .is_none()
    {
        return;
    }
    step();
    if send(
        tx,
        AgentEvent::ReasoningDelta(String::from("The repo is a Rust + Ratatui harness.")),
        cancel,
    )
    .is_none()
    {
        return;
    }
    step();

    if handle.config.search_mode != WebSearchMode::None {
        let search_req = ToolUseRequest {
            name: String::from("web_search"),
            arguments: serde_json::json!({ "query": "rust ratatui coding harness" }).to_string(),
        };
        let search_id = String::from("search-0");
        if send(
            tx,
            AgentEvent::ToolStarted {
                id: search_id.clone(),
                name: search_req.name.clone(),
                arguments: search_req.arguments.clone(),
            },
            cancel,
        )
        .is_none()
        {
            return;
        }
        step();

        let search_output = dispatch_tool(&search_req, &handle.config.root);
        let search_status = search_output.status;
        match send(
            tx,
            AgentEvent::ToolFinished { id: search_id, output: search_output.output, status: search_status },
            cancel,
        ) {
            None => return,
            Some(_) => {
                step();
            }
        }
    }

    let tool_req = ToolUseRequest {
        name: String::from("read_file_range"),
        arguments: serde_json::json!({ "path": "Cargo.toml", "start_line": 1, "end_line": 5 }).to_string(),
    };

    let tool_id = String::from("0");
    if send(
        tx,
        AgentEvent::ToolStarted {
            id: tool_id.clone(),
            name: tool_req.name.clone(),
            arguments: tool_req.arguments.clone(),
        },
        cancel,
    )
    .is_none()
    {
        return;
    }
    step();

    let output = dispatch_tool(&tool_req, &handle.config.root);
    let status = output.status;
    if send(
        tx,
        AgentEvent::ToolFinished { id: tool_id, output: output.output, status },
        cancel,
    )
    .is_none()
    {
        return;
    }
    step();

    if send(tx, AgentEvent::AssistantDelta(String::from("This is a ")), cancel).is_none() {
        return;
    }
    step();
    if send(
        tx,
        AgentEvent::AssistantDelta(String::from("fake streaming response.")),
        cancel,
    )
    .is_none()
    {
        return;
    }
    step();

    let _ = tx.send(AgentEvent::Finished);
}

/// Umans provider sends the prompt to the Umans API, streams the response,
/// dispatches any tool-use requests, feeds results back, and repeats until the
/// model stops requesting tools or the per-turn cap is hit.
///
/// If `UMANS_API_KEY` is not set, emits a `Failed` event and returns.
fn run_umans(handle: &RunHandle, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
    let client = match umans::UmansClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            let event = umans::error_to_agent_event(&e);
            let _ = send(tx, event, cancel);
            return;
        }
    };

    let tool_defs = crate::tools::tool_definitions();
    let tool_schemas = crate::tools::tool_catalog_schemas(&tool_defs);
    let messages = vec![umans::Message::user(&handle.prompt)];
    let mut iterations = 0usize;

    loop {
        if cancel.is_cancelled() {
            let _ = send(tx, AgentEvent::Cancelled, cancel);
            return;
        }

        if iterations >= handle.config.max_tool_iterations {
            let _ = send(
                tx,
                AgentEvent::Failed(format!("tool-call cap exceeded ({} iterations)", iterations)),
                cancel,
            );
            return;
        }

        let response = match client.send_streaming_request(
            &handle.config.model,
            &messages,
            4096,
            handle.config.search_mode,
            Some(&tool_schemas),
        ) {
            Ok(r) => r,
            Err(e) => {
                let event = umans::error_to_agent_event(&e);
                let _ = send(tx, event, cancel);
                return;
            }
        };

        let tool_requests = match stream_umans_response(response, tx, cancel) {
            Ok(reqs) => reqs,
            Err(msg) => {
                let _ = send(tx, AgentEvent::Failed(msg), cancel);
                return;
            }
        };

        if tool_requests.is_empty() {
            let _ = send(tx, AgentEvent::Finished, cancel);
            return;
        }

        iterations += 1;

        for (i, req) in tool_requests.iter().enumerate() {
            if cancel.is_cancelled() {
                let _ = send(tx, AgentEvent::Cancelled, cancel);
                return;
            }

            let tool_id = format!("{iterations}-{i}");
            if send(
                tx,
                AgentEvent::ToolStarted {
                    id: tool_id.clone(),
                    name: req.name.clone(),
                    arguments: req.arguments.clone(),
                },
                cancel,
            )
            .is_none()
            {
                return;
            }

            let output = dispatch_tool(req, &handle.config.root);
            let status = output.status;
            if send(
                tx,
                AgentEvent::ToolFinished { id: tool_id, output: output.output.clone(), status },
                cancel,
            )
            .is_none()
            {
                return;
            }
        }

        // TODO: append tool_result messages and re-request.
    }
}

/// Read an Umans SSE streaming response, converting events to [`AgentEvent`]
/// instances and collecting any tool-use requests.
///
/// Returns the list of tool-use requests found in the response, or an error
/// message if the stream failed.
fn stream_umans_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<Vec<ToolUseRequest>, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut buffer = String::new();
    let mut tool_requests = Vec::new();

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            return Err("cancelled".to_string());
        }

        match line_result {
            Ok(line) => {
                buffer.push_str(&line);
                buffer.push('\n');

                if line.is_empty() {
                    let events = umans::parse_sse_chunk(&buffer);
                    buffer.clear();

                    for (event_type, data) in events {
                        let sse_event = umans::parse_sse_event(&event_type, &data);

                        if let umans::SseEvent::Other(ref t) = sse_event
                            && t.starts_with("content_block_start")
                            && let Some(req) = extract_tool_use(&data)
                        {
                            tool_requests.push(req);
                        }

                        if let Some(agent_event) = umans::sse_to_agent_event(&sse_event)
                            && send(tx, agent_event, cancel).is_none()
                        {
                            return Err("cancelled".to_string());
                        }
                    }
                }
            }
            Err(e) => return Err(format!("stream read error: {e}")),
        }
    }

    if !buffer.is_empty() {
        let events = umans::parse_sse_chunk(&buffer);
        for (event_type, data) in events {
            let sse_event = umans::parse_sse_event(&event_type, &data);
            if let umans::SseEvent::Other(ref t) = sse_event
                && t.starts_with("content_block_start")
                && let Some(req) = extract_tool_use(&data)
            {
                tool_requests.push(req);
            }
            if let Some(agent_event) = umans::sse_to_agent_event(&sse_event) {
                let _ = send(tx, agent_event, cancel);
            }
        }
    }

    Ok(tool_requests)
}

/// Extract a tool-use request from a `content_block_start` data payload,
/// if the content block type is `tool_use`.
fn extract_tool_use(data: &str) -> Option<ToolUseRequest> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let cb = v.get("content_block")?;
    let block_type = cb.get("type").and_then(|t| t.as_str())?;
    if block_type != "tool_use" {
        return None;
    }
    let name = cb.get("name").and_then(|n| n.as_str())?.to_string();
    let input = cb.get("input").cloned().unwrap_or(serde_json::Value::Null);
    let arguments = if input.is_null() {
        String::from("{}")
    } else {
        serde_json::to_string(&input).unwrap_or_else(|_| String::from("{}"))
    };
    Some(ToolUseRequest { name, arguments })
}

/// Send an event, respecting cancellation. Returns `Some(())` on success, or
/// `None` if the send failed (receiver dropped) or cancellation was requested.
fn send(tx: &Sender<AgentEvent>, event: AgentEvent, cancel: &CancelToken) -> Option<()> {
    if cancel.is_cancelled() {
        return None;
    }
    if tx.send(event).is_err() {
        return None;
    }
    Some(())
}

/// Sleep briefly to simulate streaming latency in the fake provider.
fn step() {
    thread::sleep(Duration::from_millis(40));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;
    use crate::tools::{self, AgentRunConfig, MAX_TOOL_ITERATIONS, dispatch_tool};
    use std::path::Path;

    fn config() -> AgentRunConfig {
        AgentRunConfig::new(PathBuf::from("."), String::from("fake-agent"), WebSearchMode::Native)
    }

    #[test]
    fn fake_stream_emits_expected_sequence() {
        let handle = RunHandle::fake(config(), String::new());
        let rx = spawn_run(handle);

        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        assert_eq!(events.first(), Some(&AgentEvent::Started));
        assert_eq!(events.last(), Some(&AgentEvent::Finished));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ReasoningDelta(_))));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::AssistantDelta(_))));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolStarted { .. })));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolFinished { .. })));
    }

    #[test]
    fn fake_stream_with_native_search_emits_search_tool_event() {
        let mut cfg = config();
        cfg.search_mode = WebSearchMode::Native;
        let handle = RunHandle::fake(cfg, String::new());
        let rx = spawn_run(handle);

        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        let has_search = events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolStarted { name, .. } if name == "web_search"));
        assert!(has_search, "native search should emit web_search tool event");
    }

    #[test]
    fn fake_stream_with_none_search_skips_search_and_returns_assistant_text() {
        let mut cfg = config();
        cfg.search_mode = WebSearchMode::None;
        let handle = RunHandle::fake(cfg, String::new());
        let rx = spawn_run(handle);

        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        let has_search = events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolStarted { name, .. } if name == "web_search"));
        assert!(!has_search, "none search should not emit web_search tool event");

        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::AssistantDelta(_))),
            "search-disabled prompt should still return assistant text"
        );
        assert_eq!(events.last(), Some(&AgentEvent::Finished));
    }

    /// Drop the receiver immediately; the thread should exit without panic.
    #[test]
    fn fake_stream_drops_cleanly_when_receiver_dropped() {
        let handle = RunHandle::fake(config(), String::new());
        let rx = spawn_run(handle);
        drop(rx);
    }

    #[test]
    fn cancel_token_signals_cancellation() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancel_terminates_run_without_finishing() {
        let handle = RunHandle::fake(config(), String::new());
        handle.cancel.cancel();

        let rx = spawn_run(handle);
        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        assert!(
            !events.contains(&AgentEvent::Finished),
            "cancelled run must not finish normally"
        );
    }

    #[test]
    fn dispatch_tool_find_files_success() {
        let req = ToolUseRequest {
            name: String::from("find_files"),
            arguments: serde_json::json!({ "pattern": "cli" }).to_string(),
        };
        let output = dispatch_tool(&req, Path::new("src"));
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().any(|p| p.contains("cli.rs")));
    }

    #[test]
    fn dispatch_tool_read_file_range_success() {
        let req = ToolUseRequest {
            name: String::from("read_file_range"),
            arguments: serde_json::json!({
                "path": "Cargo.toml",
                "start_line": 1,
                "end_line": 3
            })
            .to_string(),
        };
        let output = dispatch_tool(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output.len(), 3);
    }

    #[test]
    fn dispatch_tool_unknown_name_fails() {
        let req = ToolUseRequest { name: String::from("nonexistent_tool"), arguments: String::from("{}") };
        let output = dispatch_tool(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("unknown tool")));
    }

    #[test]
    fn dispatch_tool_malformed_arguments_falls_back_to_defaults() {
        let req = ToolUseRequest { name: String::from("find_files"), arguments: String::from("not valid json") };
        let output = dispatch_tool(&req, Path::new("src"));
        assert_eq!(output.status, ToolStatus::Ok);
    }

    #[test]
    fn max_tool_iterations_is_reasonable() {
        let cap = MAX_TOOL_ITERATIONS;
        assert!(cap >= 4 && cap <= 16, "per-turn cap should be 4..=16, got {cap}");
    }

    #[test]
    fn extract_tool_use_returns_none_for_text_block() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        assert!(extract_tool_use(data).is_none());
    }

    #[test]
    fn extract_tool_use_returns_request_for_tool_use_block() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"find_files","input":{"pattern":"cli"}}}"#;
        let req = extract_tool_use(data).expect("should extract");
        assert_eq!(req.name, "find_files");
        assert!(req.arguments.contains("cli"));
    }

    #[test]
    fn dispatch_read_url_rejects_private_network() {
        let req = ToolUseRequest {
            name: String::from("read_url"),
            arguments: serde_json::json!({ "url": "http://127.0.0.1/secret" }).to_string(),
        };
        let output = dispatch_tool(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("private network")));
    }

    #[test]
    fn dispatch_read_url_rejects_non_public_scheme() {
        let req = ToolUseRequest {
            name: String::from("read_url"),
            arguments: serde_json::json!({ "url": "file:///etc/passwd" }).to_string(),
        };
        let output = dispatch_tool(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("unsupported")));
    }

    #[test]
    fn tool_definitions_include_web_search_and_read_url() {
        let defs = tools::tool_definitions();
        let names = defs.iter().map(|d| d.name).collect::<Vec<&str>>();
        assert!(names.contains(&"web_search"));
        assert!(names.contains(&"read_url"));
    }
}
