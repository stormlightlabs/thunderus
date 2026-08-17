//! Unified agent loop for the built-in provider routes.
//!
//! The loop runs on a background thread and sends [`AgentEvent`]s through a
//! channel. The TUI drains them with `try_recv`, keeping the UI responsive.
//!
//! ## Lifecycle
//!
//! 1. [`RunHandle::spawn`] starts a thread with a run configuration and cancellation token.
//! 2. The run emits `Started`, then streams reasoning/assistant deltas and
//!    tool-use requests.
//! 3. Each tool-use request is dispatched via [`tools::dispatch_full`] and the
//!    result is emitted as a `ToolFinished` event appended to the transcript.
//! 4. Provider tool results are fed back into the next turn using the provider's
//!    native continuation format.
//! 5. Cancellation is cooperative: the loop checks the shared [`CancelToken`]
//!    between events, lines, and tool executions. When cancelled, it emits
//!    [`AgentEvent::Cancelled`] and stops.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
#[cfg(test)]
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use ureq::http::Response;

use crate::app::{AgentEvent, ToolStatus};
use crate::cli::{ReasoningEffort, ReasoningSummary};
use crate::providers::{
    ProviderContentBlock, ProviderContinuation, ProviderError, ProviderMessage, ProviderTurn, StreamFormat,
    StreamingProvider, StreamingRequest,
};
use crate::providers::{anthropic, codex, openai, opencode};
use crate::tools::{self, AgentRunConfig, ToolOutput, ToolUseRequest, WriteResult, shell::ProcessResult};
use thndrs_agent::CancelToken;
use thndrs_agent::{ModelProjectionMessage, ProviderRequestAccounting, ProviderUsageComponents, ProviderUsageRule};

const PROVIDER_RETRY_POLICY: RetryPolicy = RetryPolicy::new(4, Duration::from_millis(2500));
const FAILED_TOOL_INPUT_PROJECTION_METHOD: &str = "failed_tool_input_omission";
const FAILED_TOOL_INPUT_PROJECTION_VERSION: &str = "failed-tool-input-omission-v1";
const FAILED_TOOL_INPUT_MIN_BYTES: usize = 4 * 1024;

/// Which provider drives this agent run.
///
/// The live app uses ChatGPT Codex or OpenCode. The fake provider is kept for
/// deterministic offline smoke tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    /// Deterministic fake provider, i.e. no network, scripted events.
    Fake,
    /// Unsupported or unrecognized provider route.
    Unsupported,
    /// OpenCode Go provider.
    OpenCodeGo,
    /// OpenCode Zen provider.
    OpenCodeZen,
    /// ChatGPT subscription-backed Codex provider.
    ChatGptCodex,
}

impl ProviderKind {
    pub fn for_model(model: &str) -> Self {
        if model.starts_with("fake-agent") {
            ProviderKind::Fake
        } else if opencode::is_go_model_id(model) {
            ProviderKind::OpenCodeGo
        } else if opencode::is_zen_model_id(model) {
            ProviderKind::OpenCodeZen
        } else if codex::is_model_id(model) {
            ProviderKind::ChatGptCodex
        } else {
            ProviderKind::Unsupported
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Fake => "fake",
            ProviderKind::Unsupported => "unsupported",
            ProviderKind::OpenCodeGo => "opencode-go",
            ProviderKind::OpenCodeZen => "opencode-zen",
            ProviderKind::ChatGptCodex => "chatgpt-codex",
        }
    }
}

#[derive(Debug)]
pub(super) enum ProviderAttemptError {
    Request(ProviderError),
    Stream(String),
}

impl ProviderAttemptError {
    pub(super) fn message<P>(&self) -> String
    where
        P: StreamingProvider,
    {
        match self {
            ProviderAttemptError::Request(err) => P::request_error_message(err),
            ProviderAttemptError::Stream(msg) => msg.clone(),
        }
    }

    pub(super) fn is_retryable<P>(&self) -> bool
    where
        P: StreamingProvider,
    {
        match self {
            ProviderAttemptError::Request(err) => P::is_retryable_request_error(err),
            ProviderAttemptError::Stream(msg) => is_retryable_stream_error(msg),
        }
    }
}

#[derive(Debug)]
pub(super) enum MetadataLoaded<T> {
    Abort,
    Loaded(T),
    Unavailable,
}

pub use thndrs_agent::{RetryPolicy, ToolPermissionDecision};

type ToolExecutionResult = Option<(ToolOutput, Option<WriteResult>, Option<ProcessResult>)>;

/// Application-local permission policy supplied to the provider-neutral run.
pub type ToolPermissionHook = thndrs_agent::ToolPermissionHook<AgentRunConfig>;

/// Application-local execution override supplied to the provider-neutral run.
pub type ToolExecutionHook = thndrs_agent::ToolExecutionHook<AgentRunConfig, ToolExecutionResult>;

#[derive(Clone, Debug)]
pub(super) struct ToolUseBuilder {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) initial_input: serde_json::Value,
    pub(super) input_json: String,
}

impl ToolUseBuilder {
    pub(super) fn finish(self) -> Option<ToolUseRequest> {
        let input = if self.input_json.trim().is_empty() {
            self.initial_input
        } else {
            serde_json::from_str(&self.input_json).unwrap_or(serde_json::Value::Null)
        };
        let arguments = if input.is_null() {
            String::from("{}")
        } else {
            serde_json::to_string(&input).unwrap_or_else(|_| String::from("{}"))
        };
        Some(ToolUseRequest::new(self.name, arguments, self.id))
    }
}

#[derive(Clone, Debug)]
pub(super) struct ChatToolCallBuilder {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) arguments_json: String,
}

impl ChatToolCallBuilder {
    pub(super) fn finish(self) -> Option<ToolUseRequest> {
        if self.name.is_empty() {
            None
        } else {
            Some(ToolUseRequest::new(
                self.name,
                if self.arguments_json.trim().is_empty() { "{}".to_string() } else { self.arguments_json },
                self.id,
            ))
        }
    }
}

#[path = "agent/execution.rs"]
mod execution;
#[path = "agent/projection.rs"]
mod projection;
#[path = "agent/run.rs"]
mod run;
#[path = "agent/streaming.rs"]
mod streaming;
#[cfg(test)]
#[path = "agent/tests.rs"]
mod tests;

pub(super) use execution::{
    ProviderTurnRequest, append_steering_messages, approve_tool_request, dispatch_tool_request,
    is_retryable_stream_error, load_provider_metadata, request_provider_turn_with_retries,
    stopped_without_expected_write,
};
pub(super) use projection::{model_tool_result, project_failed_tool_input, prompt_expects_workspace_write};
pub use run::RunHandle;
pub(crate) use streaming::stream_provider_response;
#[cfg(test)]
pub(crate) use streaming::{
    AnthropicStreamState, collect_chatgpt_codex_event, collect_openai_chat_event, extract_tool_use_start,
};
/// Send an event, respecting cancellation.
///
/// A cancellation notification itself must still cross the channel after the
/// token has been set. Otherwise the UI can remain in its stopping state while
/// the worker exits.
fn send(tx: &Sender<AgentEvent>, event: AgentEvent, cancel: &CancelToken) -> Option<()> {
    if cancel.is_cancelled() && !matches!(event, AgentEvent::Cancelled) {
        let _ = tx.send(AgentEvent::Cancelled);
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

fn fake_tool_id(config: &AgentRunConfig, label: &str) -> String {
    config
        .accounting_turn_id
        .as_ref()
        .map_or_else(|| label.to_string(), |turn_id| format!("{turn_id}-{label}"))
}
