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
//! 5. The loop enforces bounded tool-budget continuations to prevent recursive
//!    or unbounded tool-call loops while still allowing longer useful runs.
//! 6. Cancellation is cooperative: the loop checks the shared [`CancelToken`]
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

use crate::WebSearchMode;
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
enum ProviderAttemptError {
    Request(ProviderError),
    Stream(String),
}

impl ProviderAttemptError {
    fn message<P>(&self) -> String
    where
        P: StreamingProvider,
    {
        match self {
            ProviderAttemptError::Request(err) => P::request_error_message(err),
            ProviderAttemptError::Stream(msg) => msg.clone(),
        }
    }

    fn is_retryable<P>(&self) -> bool
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
enum MetadataLoaded<T> {
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
struct ToolUseBuilder {
    id: String,
    name: String,
    initial_input: serde_json::Value,
    input_json: String,
}

impl ToolUseBuilder {
    fn finish(self) -> Option<ToolUseRequest> {
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
struct ChatToolCallBuilder {
    id: String,
    name: String,
    arguments_json: String,
}

impl ChatToolCallBuilder {
    fn finish(self) -> Option<ToolUseRequest> {
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

/// Handle for a single agent run: provider kind, config, prompt, and cancel.
#[derive(Debug)]
pub struct RunHandle {
    pub provider: ProviderKind,
    pub config: AgentRunConfig,
    pub prompt: String,
    pub messages: Vec<ProviderMessage>,
    pub expects_write: bool,
    pub steering: Option<Receiver<String>>,
    pub cancel: CancelToken,
    pub permission_hook: Option<ToolPermissionHook>,
    pub execution_hook: Option<ToolExecutionHook>,
}

impl RunHandle {
    /// Spawn the unified agent loop on an owned background run.
    ///
    /// The thread closes its sender when done, so the run's `try_recv` will
    /// return `Err(Disconnected)` once the run completes.
    ///
    /// Dropping the run requests cooperative cancellation, disconnects event
    /// delivery, and joins the worker.
    pub fn spawn(self) -> thndrs_agent::AgentRun<AgentEvent> {
        let cancel = self.cancel.clone();
        tracing::info!(provider = ?self.provider, "starting agent thread");
        thndrs_agent::AgentRun::spawn(cancel, move |sender, cancel| self.run_agent(&sender, &cancel))
    }

    /// Create a fake-provider run handle.
    pub fn fake(config: AgentRunConfig, prompt: String) -> Self {
        RunHandle {
            provider: ProviderKind::Fake,
            config,
            prompt,
            messages: Vec::new(),
            expects_write: false,
            steering: None,
            cancel: CancelToken::new(),
            permission_hook: None,
            execution_hook: None,
        }
    }

    /// Create a provider run handle with a steering-message receiver.
    pub fn provider_with_steering(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
    ) -> Self {
        let provider = ProviderKind::for_model(&config.model);
        RunHandle {
            provider,
            config,
            prompt: String::new(),
            messages,
            expects_write,
            steering: Some(steering),
            cancel: CancelToken::new(),
            permission_hook: None,
            execution_hook: None,
        }
    }

    /// Attach a permission hook for sensitive tool calls.
    pub fn with_permission_hook(mut self, hook: ToolPermissionHook) -> Self {
        self.permission_hook = Some(hook);
        self
    }

    /// Attach an execution hook for front-end-specific tool handling.
    pub fn with_execution_hook(mut self, hook: ToolExecutionHook) -> Self {
        self.execution_hook = Some(hook);
        self
    }

    /// The unified agent loop. Dispatches to a built-in provider, handles
    /// tool-use requests, enforces the per-turn cap, and checks cancellation
    /// cooperatively.
    fn run_agent(&self, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
        if send(tx, AgentEvent::Started, cancel).is_none() {
            return;
        }
        step();

        match self.provider {
            ProviderKind::Fake => self.run_fake(tx, cancel),
            ProviderKind::Unsupported => {
                let _ = send(
                    tx,
                    AgentEvent::Failed(crate::cli::commands::setup::UNSUPPORTED_PROVIDER_ROUTE_MESSAGE.to_string()),
                    cancel,
                );
            }
            ProviderKind::OpenCodeGo => self.run_provider::<opencode::OpenCodeGoClient>(tx, cancel),
            ProviderKind::OpenCodeZen => self.run_provider::<opencode::zen::OpenCodeZenClient>(tx, cancel),
            ProviderKind::ChatGptCodex => self.run_provider::<codex::ChatGptCodexClient>(tx, cancel),
        }
    }

    /// A streaming provider sends the prompt to its API, streams the response,
    /// dispatches any tool-use requests, feeds the tool results back as
    /// provider-native tool result messages, and repeats until the model stops
    /// requesting tools or the per-turn cap is hit.
    #[expect(
        clippy::cognitive_complexity,
        reason = "Provider turns intentionally centralize cancellation, tool permissions, and continuation state."
    )]
    fn run_provider<P>(&self, tx: &Sender<AgentEvent>, cancel: &CancelToken)
    where
        P: StreamingProvider,
    {
        let provider = match P::from_env_or_dotenv(&self.config.root) {
            Ok(provider) => provider,
            Err(e) => {
                let message = P::request_error_message(&e);
                tracing::error!(error = %message, "failed to load provider client");
                let _ = send(tx, AgentEvent::Failed(message), cancel);
                return;
            }
        };

        tracing::info!(
            provider = provider.name(),
            model = %self.config.model,
            cwd = %self.config.root.display(),
            messages = self.messages.len(),
            max_tool_iterations = self.config.max_tool_iterations,
            "starting provider agent run"
        );
        if send(tx, AgentEvent::Status(provider.load_status()), cancel).is_none() {
            return;
        }

        let model_metadata = match load_provider_metadata(&provider, &self.config.model, tx, cancel) {
            MetadataLoaded::Abort => return,
            MetadataLoaded::Loaded(metadata) => Some(metadata),
            MetadataLoaded::Unavailable => None,
        };

        let tool_defs = tools::runtime_tool_definitions(self.config.mcp_manager.as_deref());
        let tool_schemas = tools::tool_catalog_schemas(&tool_defs);
        let mut messages = if self.messages.is_empty() {
            vec![ProviderMessage::user(&self.prompt)]
        } else {
            self.messages.clone()
        };
        let mut tool_budget =
            thndrs_agent::ToolIterationBudget::new(self.config.max_tool_iterations, tools::MAX_TOOL_CONTINUATIONS);
        let mut wrote_file = false;
        let mut continuation = ProviderContinuation::default();
        let mut pending_reduction_receipts = Vec::new();
        let mut state_history = Vec::new();
        let mut workspace_freshness = 0_u64;

        loop {
            if cancel.is_cancelled() {
                tracing::warn!(
                    provider = provider.name(),
                    "provider run cancelled before provider request"
                );
                let _ = send(tx, AgentEvent::Cancelled, cancel);
                return;
            }

            match tool_budget.before_provider_request() {
                thndrs_agent::ToolBudgetDecision::Continue => {}
                thndrs_agent::ToolBudgetDecision::ContinueAfterBudgetMessage => {
                    let text = format!(
                        "[tool-budget]\nTool batch segment limit reached after {} total batches. Continue from the current state, avoid repeating completed work, and stop requesting tools once you can answer.",
                        tool_budget.total_batches()
                    );
                    tracing::warn!(
                        total_batches = tool_budget.total_batches(),
                        continuations_used = tool_budget.continuations_used(),
                        "continuing after tool-budget segment cap"
                    );
                    messages.push(ProviderMessage::user(&text));
                    if send(
                        tx,
                        AgentEvent::Status(format!(
                            "tool budget: auto-continue {}/{} after {} batches",
                            tool_budget.continuations_used(),
                            tools::MAX_TOOL_CONTINUATIONS,
                            tool_budget.total_batches()
                        )),
                        cancel,
                    )
                    .is_none()
                    {
                        return;
                    }
                }
                thndrs_agent::ToolBudgetDecision::Exhausted {
                    segment_iterations,
                    total_batches,
                    continuations_used,
                } => {
                    tracing::error!(
                        segment_iterations,
                        total_batches,
                        continuations_used,
                        "tool-call budget exhausted"
                    );
                    let _ = send(
                        tx,
                        AgentEvent::Failed(format!(
                            "tool-call budget exhausted ({total_batches} tool batches, {continuations_used} auto-continuations, {segment_iterations} in current segment)"
                        )),
                        cancel,
                    );
                    return;
                }
            }

            if send(
                tx,
                AgentEvent::Status(provider.request_status(&self.config.model)),
                cancel,
            )
            .is_none()
            {
                return;
            }

            let max_tokens = provider.token_budget(&self.config.model, model_metadata.as_ref());
            let request = ProviderTurnRequest {
                provider: &provider,
                model: &self.config.model,
                messages: &messages,
                max_tokens,
                reasoning_effort: self.config.reasoning_effort,
                reasoning_summary: self.config.reasoning_summary,
                tool_schemas: &tool_schemas,
                continuation: &continuation,
                turn_id: self.config.accounting_turn_id.as_deref().unwrap_or("turn_unknown"),
                context: &self.config.accounting_context,
                reduction_receipts: &pending_reduction_receipts,
            };
            let Some(mut turn) = request_provider_turn_with_retries(&request, tool_budget.total_batches(), tx, cancel)
            else {
                return;
            };
            pending_reduction_receipts.clear();
            if matches!(self.provider, ProviderKind::ChatGptCodex | ProviderKind::OpenCodeZen)
                && matches!(
                    provider.stream_format(&self.config.model),
                    Ok(StreamFormat::ChatGptCodexResponses)
                )
            {
                codex::record_response_items(&mut continuation, &messages, std::mem::take(&mut turn.response_items));
            }
            tracing::info!(
                text_chars = turn.assistant_text.chars().count(),
                tool_calls = turn.tool_requests.len(),
                "provider turn completed"
            );

            if turn.tool_requests.is_empty() {
                if turn.assistant_text.is_empty() && turn.stop_reason.as_deref() == Some("max_tokens") {
                    let _ = send(
                        tx,
                        AgentEvent::Failed(format!(
                            "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
                        )),
                        cancel,
                    );
                    return;
                }
                if self.expects_write && !wrote_file {
                    let _ = send(
                        tx,
                        AgentEvent::Failed(String::from(
                            "model stopped without writing a file for an edit-like request",
                        )),
                        cancel,
                    );
                    return;
                }
                if append_steering_messages(&mut messages, self) {
                    tracing::info!(
                        provider = provider.name(),
                        "continuing provider run with queued steering messages"
                    );
                    continue;
                }
                let _ = send(tx, AgentEvent::Finished, cancel);
                return;
            }

            tool_budget.record_tool_batch();

            let mut assistant_blocks = Vec::new();
            if !turn.assistant_text.is_empty() {
                assistant_blocks.push(ProviderContentBlock::Text { text: turn.assistant_text });
            }

            let mut tool_results: Vec<ProviderMessage> = Vec::new();
            let mut response_tool_outputs = Vec::new();
            for req in &turn.tool_requests {
                if cancel.is_cancelled() {
                    tracing::warn!(provider = provider.name(), tool = %req.name, tool_id = %req.tool_use_id, "provider run cancelled before tool dispatch");
                    let _ = send(tx, AgentEvent::Cancelled, cancel);
                    return;
                }

                let tool_id = req.tool_use_id.clone();
                tracing::info!(tool = %req.name, tool_id = %tool_id, "dispatching tool request");
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

                let (mut output, write_result, shell_result) = match approve_tool_request(req, self, cancel) {
                    ToolPermissionDecision::Allow => dispatch_tool_request(req, self, cancel),
                    ToolPermissionDecision::Reject => (
                        ToolOutput::failed(&req.name, String::from("tool call rejected by ACP client")),
                        None,
                        None,
                    ),
                    ToolPermissionDecision::Cancelled => {
                        let _ = send(tx, AgentEvent::Cancelled, cancel);
                        return;
                    }
                };
                let status = output.status;
                let display_output = output.display_lines();
                if let Some(store) = &self.config.artifact_store {
                    match store.create_tool_evidence(&format!("tool:{tool_id}"), &display_output) {
                        Ok(artifact) => {
                            output.evidence.identity = format!("tool:{tool_id}");
                            output.evidence.artifact_handle = Some(artifact.metadata.handle);
                        }
                        Err(error) => {
                            tracing::warn!(tool = %req.name, tool_id = %tool_id, %error, "failed to preserve bounded tool evidence")
                        }
                    }
                }
                if write_result.is_some() && status == ToolStatus::Ok {
                    wrote_file = true;
                }
                let state_identity = tools::state_identity_for(req, &output, &self.config.root, workspace_freshness);
                let state_protected = status != ToolStatus::Ok || write_result.is_some();
                let (tool_result, result_content, reduced, projection_decision, state_record) = model_tool_result(
                    &tool_id,
                    &output,
                    shell_result.as_ref(),
                    &self.config.model_reduction,
                    state_identity,
                    state_protected,
                    &state_history,
                );
                if let Some(record) = state_record {
                    state_history.push(record);
                }
                if write_result.is_some() || req.name == tools::shell::NAME {
                    workspace_freshness = workspace_freshness.saturating_add(1);
                }
                tracing::info!(tool = %req.name, tool_id = %tool_id, status = ?status, "tool request finished");
                if !matches!(
                    &projection_decision,
                    thndrs_agent::context::StateProjectionDecision::Retained
                ) && send(
                    tx,
                    AgentEvent::StateProjectionDecision { id: tool_id.clone(), decision: projection_decision },
                    cancel,
                )
                .is_none()
                {
                    return;
                }
                if send(
                    tx,
                    AgentEvent::ToolFinished {
                        id: tool_id.clone(),
                        output: display_output.clone(),
                        status,
                        write_result,
                        shell_result: shell_result.map(Box::new),
                    },
                    cancel,
                )
                .is_none()
                {
                    return;
                }

                let (input, input_reduction) = project_failed_tool_input(req, &output, &self.config.model_reduction);
                assistant_blocks.push(ProviderContentBlock::ToolUse {
                    id: tool_id.clone(),
                    name: req.name.clone(),
                    input,
                });

                for diagnostic in &reduced.diagnostics {
                    tracing::warn!(
                        reducer = diagnostic.reducer.map(|reducer| reducer.label()).unwrap_or("pipeline"),
                        code = %diagnostic.code,
                        message = %diagnostic.message,
                        "model projection reducer kept the baseline"
                    );
                }
                pending_reduction_receipts.extend(reduced.receipts);
                if let Some(receipt) = input_reduction {
                    pending_reduction_receipts.push(receipt);
                }
                tool_results.push(tool_result);
                response_tool_outputs.push((tool_id, result_content));
            }

            messages.push(ProviderMessage::assistant_blocks(assistant_blocks));
            messages.extend(tool_results);
            if matches!(self.provider, ProviderKind::ChatGptCodex | ProviderKind::OpenCodeZen)
                && matches!(
                    provider.stream_format(&self.config.model),
                    Ok(StreamFormat::ChatGptCodexResponses)
                )
            {
                for (call_id, output) in response_tool_outputs {
                    codex::record_tool_output(&mut continuation, &call_id, &output, messages.len());
                }
            }
            append_steering_messages(&mut messages, self);
        }
    }

    /// Deterministic fake provider: emits reasoning, a tool-use request, assistant
    /// text, and finishes. Demonstrates the tool dispatch path end-to-end.
    fn run_fake(&self, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
        use AgentEvent::*;
        match send(tx, ReasoningDelta(String::from("Let me think about this... ")), cancel) {
            None => return,
            Some(_) => step(),
        }

        match send(
            tx,
            ReasoningDelta(String::from("The repo is a Rust terminal coding harness.")),
            cancel,
        ) {
            None => return,
            Some(_) => step(),
        }

        if self.config.model == "fake-agent-slow" {
            for _ in 0..200 {
                if cancel.is_cancelled() {
                    let _ = send(tx, Cancelled, cancel);
                    return;
                }
                step();
            }
        }

        if self.config.search_mode != WebSearchMode::None {
            let search_req = ToolUseRequest::new(
                String::from("web_search"),
                serde_json::json!({ "query": "rust terminal coding harness" }).to_string(),
                String::from("search-0"),
            );
            let search_id = String::from("search-0");
            match send(
                tx,
                ToolStarted {
                    id: search_id.clone(),
                    name: search_req.name.clone(),
                    arguments: search_req.arguments.clone(),
                },
                cancel,
            ) {
                None => return,
                Some(_) => step(),
            }

            let search_config = self.config.search_config();
            let (search_output, _, _) =
                tools::dispatch_full_with_search(&search_req, &self.config.root, &search_config);
            let search_status = search_output.status;
            let search_display_output = search_output.display_lines();
            match send(
                tx,
                ToolFinished {
                    id: search_id,
                    output: search_display_output,
                    status: search_status,
                    write_result: None,
                    shell_result: None,
                },
                cancel,
            ) {
                None => return,
                Some(_) => {
                    step();
                }
            }
        }

        let tool_req = ToolUseRequest::new(
            String::from("read_file_range"),
            serde_json::json!({ "path": "Cargo.toml", "start_line": 1, "end_line": 5 }).to_string(),
            String::from("0"),
        );

        let tool_id = String::from("0");
        match send(
            tx,
            ToolStarted { id: tool_id.clone(), name: tool_req.name.clone(), arguments: tool_req.arguments.clone() },
            cancel,
        ) {
            None => return,
            Some(_) => step(),
        }

        let (output, _, _) = tools::dispatch_full(&tool_req, &self.config.root);
        let status = output.status;
        let display_output = output.display_lines();
        match send(
            tx,
            ToolFinished { id: tool_id, output: display_output, status, write_result: None, shell_result: None },
            cancel,
        ) {
            None => return,
            Some(_) => step(),
        }

        if self.config.model == "fake-agent-shell" {
            let shell_req = ToolUseRequest::new(
                String::from("run_shell"),
                serde_json::json!({ "program": "printf", "args": ["acp-permission-smoke\n"] }).to_string(),
                String::from("shell-0"),
            );
            let shell_id = String::from("shell-0");
            match send(
                tx,
                ToolStarted {
                    id: shell_id.clone(),
                    name: shell_req.name.clone(),
                    arguments: shell_req.arguments.clone(),
                },
                cancel,
            ) {
                None => return,
                Some(_) => step(),
            }

            let (shell_output, write_result, shell_result) = match approve_tool_request(&shell_req, self, cancel) {
                ToolPermissionDecision::Allow => dispatch_tool_request(&shell_req, self, cancel),
                ToolPermissionDecision::Reject => (
                    ToolOutput::failed(&shell_req.name, String::from("tool call rejected by ACP client")),
                    None,
                    None,
                ),
                ToolPermissionDecision::Cancelled => {
                    let _ = send(tx, AgentEvent::Cancelled, cancel);
                    return;
                }
            };
            let shell_status = shell_output.status;
            let shell_display_output = shell_output.display_lines();
            match send(
                tx,
                ToolFinished {
                    id: shell_id,
                    output: shell_display_output,
                    status: shell_status,
                    write_result,
                    shell_result: shell_result.map(Box::new),
                },
                cancel,
            ) {
                None => return,
                Some(_) => step(),
            }
        }
        match send(tx, AssistantDelta(String::from("This is a ")), cancel) {
            None => return,
            Some(_) => step(),
        }

        match send(tx, AssistantDelta(String::from("fake streaming response.")), cancel) {
            None => return,
            Some(_) => step(),
        }
        let _ = tx.send(Finished);
    }
}

#[derive(Default)]
struct AnthropicStreamState {
    tool_blocks: HashMap<usize, ToolUseBuilder>,
    tool_requests: Vec<ToolUseRequest>,
    assistant_text: String,
    stop_reason: Option<String>,
    provider_content_blocks: Vec<String>,
    usage: ProviderUsageComponents,
}

#[cfg(test)]
impl AnthropicStreamState {
    fn collect(
        &mut self, event_type: &str, data: &str, tx: &Sender<AgentEvent>, cancel: &CancelToken,
    ) -> Result<(), String> {
        collect_anthropic_event(event_type, data, self, tx, cancel)
    }
}

struct ProviderTurnRequest<'a, P>
where
    P: StreamingProvider,
{
    provider: &'a P,
    model: &'a str,
    messages: &'a [ProviderMessage],
    max_tokens: u32,
    reasoning_effort: ReasoningEffort,
    reasoning_summary: ReasoningSummary,
    tool_schemas: &'a serde_json::Value,
    continuation: &'a ProviderContinuation,
    turn_id: &'a str,
    context: &'a [thndrs_agent::ContextItemSnapshot],
    reduction_receipts: &'a [thndrs_agent::ContextReductionReceipt],
}

/// Build the provider-native tool result from the independent model projection.
///
/// The display projection and structured evidence remain owned by `output`.
/// An explicit applied reducer configuration adds only the bounded aggregate
/// dashboard to the model result; shadow-only measurement leaves the model
/// request unchanged.
fn model_tool_result(
    tool_id: &str, output: &ToolOutput, shell_result: Option<&ProcessResult>,
    config: &thndrs_agent::context::ReductionConfig,
    state_identity: Option<thndrs_agent::context::StateProjectionIdentity>, state_protected: bool,
    state_history: &[thndrs_agent::context::StateProjectionRecord],
) -> (
    ProviderMessage,
    String,
    thndrs_agent::context::ReductionResult,
    thndrs_agent::context::StateProjectionDecision,
    Option<thndrs_agent::context::StateProjectionRecord>,
) {
    let baseline = output.model_lines();
    let command_projection = shell_result.and_then(|result| {
        output.evidence.artifact_handle.as_deref().and_then(|handle| {
            tools::command_projection::project(&format!("tool:{tool_id}"), &baseline, result, handle, config)
        })
    });
    let projection_input = command_projection.as_ref().map_or_else(
        || baseline.clone(),
        |projection| {
            if projection.receipt.mode == thndrs_agent::ContextReductionMode::Applied {
                projection.lines.clone()
            } else {
                baseline.clone()
            }
        },
    );
    let mut reduced = thndrs_agent::reduce_lines(&format!("tool:{tool_id}"), projection_input, config);
    if let Some(projection) = command_projection {
        reduced.receipts.insert(0, projection.receipt.clone());
        reduced.dashboard.receipts.insert(0, projection.receipt.clone());
        if projection.receipt.mode == thndrs_agent::ContextReductionMode::Applied {
            reduced.dashboard.before_bytes = thndrs_agent::measure_lines(&baseline);
            reduced.dashboard.before_lines = baseline.len();
            reduced.dashboard.routine_omissions = reduced.dashboard.before_lines.saturating_sub(reduced.lines.len());
        }
    }
    let mut state_candidate = thndrs_agent::context::StateProjectionCandidate::new(
        format!("tool:{tool_id}"),
        reduced.lines.clone(),
        state_identity,
    );
    if state_protected {
        state_candidate = state_candidate.protected();
    }
    let state_reduction = thndrs_agent::reduce_state_identical(&state_candidate, state_history, config);
    let state_record = state_reduction.history_record(&state_candidate);
    let projection_decision = state_reduction
        .receipt
        .as_ref()
        .filter(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::Applied)
        .map_or(thndrs_agent::context::StateProjectionDecision::Retained, |_| {
            state_reduction.decision.clone()
        });
    if let Some(receipt) = state_reduction.receipt {
        reduced.receipts.push(receipt.clone());
        reduced.dashboard.receipts.push(receipt.clone());
        if receipt.mode == thndrs_agent::ContextReductionMode::Applied {
            reduced.lines = state_reduction.lines;
            reduced.dashboard.after_bytes = thndrs_agent::measure_lines(&reduced.lines);
            reduced.dashboard.after_lines = reduced.lines.len();
            reduced.dashboard.routine_omissions = reduced.dashboard.before_lines.saturating_sub(reduced.lines.len());
        }
    }
    let suppressed_duplicate = matches!(
        state_reduction.decision,
        thndrs_agent::context::StateProjectionDecision::DuplicateOf { .. }
    ) && reduced.lines.is_empty();
    let mut content = if suppressed_duplicate {
        String::new()
    } else if reduced.lines.is_empty() {
        "(no output)".to_string()
    } else {
        reduced.lines.join("\n")
    };
    if reduced
        .receipts
        .iter()
        .any(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::Applied)
    {
        content.push('\n');
        content.push_str(&reduced.render_dashboard());
    }
    let message = ProviderMessage::tool_result(tool_id, &content, output.status == ToolStatus::Failed);
    (message, content, reduced, projection_decision, state_record)
}

/// Replace a failed non-command tool's oversized argument body only after the
/// bounded artifact store has returned a recovery handle. Shell argv remains
/// untouched: command-aware reduction projects output, never a user command.
fn project_failed_tool_input(
    request: &ToolUseRequest, output: &ToolOutput, config: &thndrs_agent::context::ReductionConfig,
) -> (serde_json::Value, Option<thndrs_agent::ContextReductionReceipt>) {
    let baseline = serde_json::from_str(&request.arguments).unwrap_or(serde_json::Value::Null);
    if request.name == tools::shell::NAME
        || output.status != ToolStatus::Failed
        || request.arguments.len() < FAILED_TOOL_INPUT_MIN_BYTES
        || (!config.failed_tool_input && !config.shadow)
    {
        return (baseline, None);
    }
    let Some(handle) = output.evidence.artifact_handle.as_deref() else {
        return (baseline, None);
    };

    let projected = serde_json::json!({
        "projection": "failed tool arguments omitted after failure; recover bounded redacted evidence from the recorded artifact",
        "tool_call_id": request.tool_use_id,
        "recovery_handle": handle,
        "audit": "original arguments remain in the tool-started audit record"
    });
    let after_bytes = serde_json::to_string(&projected).map_or(0, |json| json.len() as u64);
    let mode = if config.failed_tool_input {
        thndrs_agent::ContextReductionMode::Applied
    } else {
        thndrs_agent::ContextReductionMode::Shadow
    };
    let receipt = thndrs_agent::ContextReductionReceipt {
        item_id: format!("tool_input:{}", request.tool_use_id),
        method: FAILED_TOOL_INPUT_PROJECTION_METHOD.to_string(),
        version: FAILED_TOOL_INPUT_PROJECTION_VERSION.to_string(),
        before_bytes: request.arguments.len() as u64,
        after_bytes,
        lossy: true,
        mode,
        diagnostic: None,
    };
    if mode == thndrs_agent::ContextReductionMode::Applied {
        (projected, Some(receipt))
    } else {
        (baseline, Some(receipt))
    }
}

/// Best-effort classifier for prompts that should not finish without a
/// workspace write. This is intentionally narrow: it requires both a file-ish
/// reference and an edit/action verb.
pub fn prompt_expects_workspace_write(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    let fileish = lower.contains(".md")
        || lower.contains(".rs")
        || lower.contains(".toml")
        || lower.contains(".json")
        || lower.contains(".yaml")
        || lower.contains(".yml")
        || lower.contains("file")
        || lower.contains("todo");
    let action = [
        "add",
        "change",
        "document",
        "edit",
        "fix",
        "modify",
        "remove",
        "replace",
        "rewrite",
        "summarize",
        "update",
        "write",
    ]
    .iter()
    .any(|word| {
        lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|part| part == *word)
    });

    fileish && action
}

fn is_retryable_stream_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("cancel")
        || lower.contains("aborted")
        || lower.contains("max_tokens")
        || lower.contains("without writing")
        || lower.contains("provider returned only provider-side content blocks")
    {
        return false;
    }

    [
        "429",
        "500",
        "502",
        "503",
        "504",
        "overloaded",
        "rate limit",
        "server error",
        "service unavailable",
        "stream read error",
        "stream ended without",
        "connection",
        "timed out",
        "timeout",
        "provider error",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn sleep_with_cancel(delay: Duration, tx: &Sender<AgentEvent>, cancel: &CancelToken) -> bool {
    let mut slept = Duration::ZERO;
    let tick = Duration::from_millis(100);
    while slept < delay {
        if cancel.is_cancelled() {
            let _ = tx.send(AgentEvent::Cancelled);
            return false;
        }
        let remaining = delay.saturating_sub(slept);
        let nap = remaining.min(tick);
        thread::sleep(nap);
        slept += nap;
    }
    true
}

fn dispatch_tool_request(
    request: &ToolUseRequest, handle: &RunHandle, cancel: &CancelToken,
) -> (ToolOutput, Option<WriteResult>, Option<ProcessResult>) {
    if let Some(hook) = &handle.execution_hook
        && let Some(output) = hook.execute(request, &handle.config, cancel)
    {
        return output;
    }
    let search_config = handle.config.search_config();
    tools::dispatch_runtime_full_with_cancel_and_search_and_registry(
        // The application-owned registry keeps background children alive after
        // this tool call returns and lets the TUI cancel/reap them later.
        request,
        &handle.config.root,
        handle.config.mcp_manager.as_deref(),
        cancel,
        &search_config,
        handle.config.process_registry.as_ref(),
        &handle.config.extra_read_roots,
    )
}

fn approve_tool_request(request: &ToolUseRequest, handle: &RunHandle, cancel: &CancelToken) -> ToolPermissionDecision {
    if !requires_runtime_permission(&request.name) {
        return ToolPermissionDecision::Allow;
    }
    match &handle.permission_hook {
        Some(hook) => hook.decide(request, &handle.config, cancel),
        None => ToolPermissionDecision::Allow,
    }
}

fn requires_runtime_permission(tool_name: &str) -> bool {
    matches!(tool_name, "create_file" | "replace_range" | "write_patch" | "run_shell")
}

fn load_provider_metadata<P>(
    provider: &P, model: &str, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> MetadataLoaded<P::Metadata>
where
    P: StreamingProvider,
{
    match provider.load_metadata() {
        Ok(models) => {
            tracing::info!("loaded provider model metadata");
            if let Some(event) = provider.metadata_loaded_event(&models)
                && send(tx, event, cancel).is_none()
            {
                return MetadataLoaded::Abort;
            }
            if let Some(status) = provider.metadata_status(model, &models)
                && send(tx, AgentEvent::Status(status), cancel).is_none()
            {
                return MetadataLoaded::Abort;
            }
            MetadataLoaded::Loaded(models)
        }
        Err(e) => {
            let message = P::request_error_message(&e);
            if e.is_credential_rejected() {
                tracing::warn!(error = %message, "provider rejected credentials while loading model metadata");
                let _ = send(tx, AgentEvent::Failed(message), cancel);
                return MetadataLoaded::Abort;
            }
            tracing::warn!(error = %message, "failed to load provider model metadata; using fallback token budget");
            match send(
                tx,
                AgentEvent::Status(String::from(
                    "provider: model metadata unavailable; using fallback token budget",
                )),
                cancel,
            ) {
                None => MetadataLoaded::Abort,
                Some(_) => MetadataLoaded::Unavailable,
            }
        }
    }
}

fn request_provider_turn_with_retries<P>(
    request: &ProviderTurnRequest<'_, P>, iteration: usize, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Option<ProviderTurn>
where
    P: StreamingProvider,
{
    let mut retry_attempt = 0;
    loop {
        tracing::info!(
            iteration,
            messages = request.messages.len(),
            retry_attempt,
            "requesting provider turn"
        );
        let attempt_result = provider_request_attempt(request, iteration, retry_attempt + 1, tx, cancel);

        match attempt_result {
            Ok(turn) => return Some(turn),
            Err(error) if error.is_retryable::<P>() && retry_attempt < PROVIDER_RETRY_POLICY.max_retries => {
                retry_attempt += 1;
                if !send_retry_event(request.provider, error.message::<P>(), retry_attempt, tx, cancel) {
                    return None;
                }
            }
            Err(error) => {
                let message = error.message::<P>();
                tracing::error!(provider = request.provider.name(), error = %message, "provider attempt failed");
                let _ = send(tx, AgentEvent::Failed(message), cancel);
                return None;
            }
        }
    }
}

fn provider_request_attempt<P>(
    request: &ProviderTurnRequest<'_, P>, iteration: usize, attempt: u32, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<ProviderTurn, ProviderAttemptError>
where
    P: StreamingProvider,
{
    let provider_request = StreamingRequest {
        max_tokens: request.max_tokens,
        reasoning_effort: request.reasoning_effort,
        reasoning_summary: request.reasoning_summary,
        tools: request.tool_schemas,
        continuation: request.continuation,
    };
    let serialized_body = request
        .provider
        .serialized_request_body(request.model, request.messages, &provider_request)
        .map_err(ProviderAttemptError::Request)?;
    match request
        .provider
        .send_streaming_request(request.model, request.messages, &provider_request)
    {
        Ok(response) => {
            match send(
                tx,
                AgentEvent::Status(format!("provider: connected HTTP {}", response.status().as_u16())),
                cancel,
            ) {
                None => Err(ProviderAttemptError::Stream("cancelled".to_string())),
                Some(_) => stream_provider_response(
                    request.provider,
                    request.model,
                    response,
                    tx,
                    cancel,
                    request.max_tokens,
                )
                .map_err(ProviderAttemptError::Stream)
                .and_then(|mut turn| {
                    let stream_format = request
                        .provider
                        .stream_format(request.model)
                        .map_err(ProviderAttemptError::Request)?;
                    let provider_usage = turn.usage.take().and_then(|components| {
                        if components.input_tokens.is_none()
                            && components.output_tokens.is_none()
                            && components.cache_read_input_tokens.is_none()
                            && components.cache_creation_input_tokens.is_none()
                            && components.reasoning_tokens.is_none()
                        {
                            None
                        } else {
                            Some(
                                components
                                    .normalize(request.provider.name(), usage_rule_for_stream_format(stream_format)),
                            )
                        }
                    });
                    let mut accounting = ProviderRequestAccounting::from_serialized_request(
                        request.turn_id,
                        format!("{}:request:{iteration}", request.turn_id),
                        attempt,
                        request.provider.name(),
                        request.model,
                        &serialized_body,
                        request.context.to_vec(),
                    )
                    .with_reduction_receipts(request.reduction_receipts.to_vec());
                    accounting = accounting.with_model_projection(
                        request
                            .messages
                            .iter()
                            .map(|message| ModelProjectionMessage {
                                role: message.role.clone(),
                                content: match &message.content {
                                    crate::providers::ProviderMessageContent::Text(content) => content.clone(),
                                    crate::providers::ProviderMessageContent::Blocks(blocks) => {
                                        serde_json::to_string(blocks)
                                            .unwrap_or_else(|_| String::from("[unserializable blocks]"))
                                    }
                                },
                            })
                            .collect(),
                    );
                    accounting.provider_usage = provider_usage;
                    if send(tx, AgentEvent::RequestAccounting(Box::new(accounting)), cancel).is_none() {
                        return Err(ProviderAttemptError::Stream("cancelled".to_string()));
                    }
                    Ok(turn)
                }),
            }
        }
        Err(e) => Err(ProviderAttemptError::Request(e)),
    }
}

fn usage_rule_for_stream_format(format: StreamFormat) -> ProviderUsageRule {
    match format {
        StreamFormat::AnthropicMessages => ProviderUsageRule::AnthropicMessages,
        StreamFormat::OpenAiChat => ProviderUsageRule::OpenAiChat,
        StreamFormat::ChatGptCodexResponses => ProviderUsageRule::OpenAiResponses,
    }
}

fn send_retry_event<P>(
    provider: &P, message: String, retry_attempt: u32, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> bool
where
    P: StreamingProvider,
{
    let delay = PROVIDER_RETRY_POLICY.delay_for_attempt(retry_attempt);
    tracing::warn!(
        provider = provider.name(),
        attempt = retry_attempt,
        max_retries = PROVIDER_RETRY_POLICY.max_retries,
        delay_ms = delay.as_millis(),
        error = %message,
        "retrying provider attempt"
    );
    match send(
        tx,
        AgentEvent::Retrying {
            attempt: retry_attempt,
            max_attempts: PROVIDER_RETRY_POLICY.max_retries,
            delay_ms: delay.as_millis() as u64,
            error: message,
        },
        cancel,
    ) {
        None => false,
        Some(_) => sleep_with_cancel(delay, tx, cancel),
    }
}

fn append_steering_messages(messages: &mut Vec<ProviderMessage>, handle: &RunHandle) -> bool {
    match handle.steering.as_ref() {
        Some(rx) => {
            let mut appended = false;
            while let Ok(text) = rx.try_recv() {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                messages.push(ProviderMessage::user(&format!("[steering]\n{trimmed}")));
                appended = true;
            }
            if appended {
                tracing::debug!(messages = messages.len(), "appended steering messages");
            }
            appended
        }
        None => false,
    }
}

/// Read an Anthropic-compatible SSE streaming response, converting events to [`AgentEvent`]
/// instances and collecting any tool-use requests plus the assistant text.
///
/// Returns a [`TurnOutput`] with the tool-use requests and the accumulated
/// assistant text, or an error message if the stream failed.
fn stream_anthropic_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken, max_tokens: u32,
) -> Result<ProviderTurn, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut buffer = String::new();
    let mut state = AnthropicStreamState::default();
    let mut event_count = 0usize;
    let mut saw_response = false;
    tracing::info!("reading Anthropic-compatible SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading Anthropic-compatible SSE stream");
            return Err("cancelled".to_string());
        }

        match line_result {
            Ok(line) => {
                buffer.push_str(&line);
                buffer.push('\n');

                if line.is_empty() {
                    let events = anthropic::parse_sse_chunk(&buffer);
                    buffer.clear();

                    for (event_type, data) in events {
                        event_count += 1;
                        log_sse_event(event_count, &event_type, &data);
                        if !saw_response {
                            saw_response = true;
                            if send(tx, AgentEvent::Status(String::from("provider: receiving SSE")), cancel).is_none() {
                                return Err("cancelled".to_string());
                            }
                        }
                        collect_anthropic_event(&event_type, &data, &mut state, tx, cancel)?;
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "failed reading Anthropic-compatible SSE stream");
                return Err(format!("stream read error: {e}"));
            }
        }
    }

    if !buffer.is_empty() {
        let events = anthropic::parse_sse_chunk(&buffer);
        for (event_type, data) in events {
            event_count += 1;
            log_sse_event(event_count, &event_type, &data);
            if !saw_response {
                saw_response = true;
                if send(tx, AgentEvent::Status(String::from("provider: receiving SSE")), cancel).is_none() {
                    return Err("cancelled".to_string());
                }
            }
            collect_anthropic_event(&event_type, &data, &mut state, tx, cancel)?;
        }
    }

    for (_, block) in state.tool_blocks {
        if let Some(req) = block.finish() {
            state.tool_requests.push(req);
        }
    }

    if state.assistant_text.is_empty() && state.tool_requests.is_empty() {
        tracing::error!(
            event_count,
            "Anthropic-compatible stream ended without assistant text or tool calls"
        );
        if state.stop_reason.as_deref() == Some("max_tokens") {
            return Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            ));
        }
        if !state.provider_content_blocks.is_empty() {
            let blocks = state.provider_content_blocks.join(", ");
            return Err(format!(
                "provider returned only provider-side content blocks ({blocks}) and no assistant text or tool calls; retry with --websearch none"
            ));
        }
        return Err(format!(
            "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
        ));
    }

    tracing::info!(
        event_count,
        text_chars = state.assistant_text.chars().count(),
        tool_calls = state.tool_requests.len(),
        "finished reading Anthropic-compatible SSE stream"
    );
    let _ = send(
        tx,
        AgentEvent::Status(format!(
            "provider: stream ended ({event_count} SSE events, {} text chars, {} tool calls)",
            state.assistant_text.chars().count(),
            state.tool_requests.len()
        )),
        cancel,
    );

    Ok(ProviderTurn {
        tool_requests: state.tool_requests,
        assistant_text: state.assistant_text,
        stop_reason: state.stop_reason,
        response_items: Vec::new(),
        usage: Some(state.usage),
    })
}

fn stream_provider_response<P: StreamingProvider>(
    provider: &P, model: &str, resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken,
    max_tokens: u32,
) -> Result<ProviderTurn, String> {
    match provider
        .stream_format(model)
        .map_err(|e| P::request_error_message(&e))?
    {
        StreamFormat::OpenAiChat => stream_openai_chat_response(resp, tx, cancel, max_tokens),
        StreamFormat::AnthropicMessages => stream_anthropic_response(resp, tx, cancel, max_tokens),
        StreamFormat::ChatGptCodexResponses => stream_chatgpt_codex_response(resp, tx, cancel, max_tokens),
    }
}

fn stream_openai_chat_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken, max_tokens: u32,
) -> Result<ProviderTurn, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut assistant_text = String::new();
    let mut tool_blocks: HashMap<usize, ChatToolCallBuilder> = HashMap::new();
    let mut tool_requests = Vec::new();
    let mut event_count = 0usize;
    let mut saw_response = false;
    let mut stop_reason = None;
    let mut usage = ProviderUsageComponents::default();
    tracing::info!("reading OpenAI-compatible chat-completions SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading OpenAI-compatible SSE stream");
            return Err("cancelled".to_string());
        }

        let line = line_result.map_err(|e| {
            tracing::error!(error = %e, "failed reading OpenAI-compatible SSE stream");
            format!("stream read error: {e}")
        })?;
        if !line.starts_with("data:") {
            continue;
        }

        if !saw_response {
            saw_response = true;
            if send(tx, AgentEvent::Status(String::from("provider: receiving SSE")), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }

        for data in openai::parse_chat_sse_chunk(&(line + "\n")) {
            event_count += 1;
            let events = openai::parse_chat_sse_event(&data);
            for event in events {
                match &event {
                    openai::ChatSseEvent::Usage { input_tokens, output_tokens } => {
                        usage.merge_snapshot(&ProviderUsageComponents::new(*input_tokens, *output_tokens));
                    }
                    openai::ChatSseEvent::UsageComponents(components) => usage.merge_snapshot(components),
                    _ => {}
                }
                collect_openai_chat_event(
                    event,
                    &mut tool_blocks,
                    &mut tool_requests,
                    &mut assistant_text,
                    &mut stop_reason,
                    tx,
                    cancel,
                )?;
            }
        }
    }

    for (_, block) in tool_blocks {
        if let Some(req) = block.finish() {
            tool_requests.push(req);
        }
    }

    if assistant_text.is_empty() && tool_requests.is_empty() {
        tracing::error!(
            event_count,
            "OpenAI-compatible stream ended without assistant text or tool calls"
        );
        return match stop_reason.as_deref() {
            Some("length") => Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            )),
            _ => Err(format!(
                "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
            )),
        };
    }

    tracing::info!(
        event_count,
        text_chars = assistant_text.chars().count(),
        tool_calls = tool_requests.len(),
        "finished reading OpenAI-compatible SSE stream"
    );
    let _ = send(
        tx,
        AgentEvent::Status(format!(
            "provider: stream ended ({event_count} SSE events, {} text chars, {} tool calls)",
            assistant_text.chars().count(),
            tool_requests.len()
        )),
        cancel,
    );

    Ok(ProviderTurn { tool_requests, assistant_text, stop_reason, response_items: Vec::new(), usage: Some(usage) })
}

fn collect_openai_chat_event(
    event: openai::ChatSseEvent, tool_blocks: &mut HashMap<usize, ChatToolCallBuilder>,
    tool_requests: &mut Vec<ToolUseRequest>, assistant_text: &mut String, stop_reason: &mut Option<String>,
    tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<(), String> {
    match event {
        openai::ChatSseEvent::TextDelta(text) => {
            assistant_text.push_str(&text);
            if send(tx, AgentEvent::AssistantDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::ReasoningDelta(text) => {
            if send(tx, AgentEvent::ReasoningDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::ToolCallStart { index, id, name } => {
            tool_blocks.insert(index, ChatToolCallBuilder { id, name, arguments_json: String::new() });
        }
        openai::ChatSseEvent::ToolCallArgumentsDelta { index, arguments } => {
            let block = tool_blocks.entry(index).or_insert_with(|| ChatToolCallBuilder {
                id: format!("call_{index}"),
                name: String::new(),
                arguments_json: String::new(),
            });
            block.arguments_json.push_str(&arguments);
        }
        openai::ChatSseEvent::FinishReason(reason) => {
            *stop_reason = Some(reason.clone());
            if reason == "tool_calls" {
                let finished = std::mem::take(tool_blocks);
                for (_, block) in finished {
                    if let Some(req) = block.finish() {
                        tool_requests.push(req);
                    }
                }
            }
        }
        openai::ChatSseEvent::ResponseStatus(status) => {
            if matches!(status.as_str(), "failed" | "cancelled" | "canceled") {
                return Err(format!("provider stream status: {status}"));
            }
            if send(tx, AgentEvent::Status(format!("provider: status {status}")), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::Error(message) => {
            tracing::error!(error = %message, "provider emitted SSE error");
            return Err(format!("provider error: {message}"));
        }
        openai::ChatSseEvent::Usage { .. } => {}
        openai::ChatSseEvent::UsageComponents(_) => {}
        openai::ChatSseEvent::Malformed(payload) => {
            return Err(format!("malformed provider stream payload: {payload}"));
        }
        openai::ChatSseEvent::Done | openai::ChatSseEvent::Other => {}
    }

    Ok(())
}

fn stream_chatgpt_codex_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken, max_tokens: u32,
) -> Result<ProviderTurn, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut assistant_text = String::new();
    let mut tool_blocks: HashMap<String, ChatToolCallBuilder> = HashMap::new();
    let mut tool_requests = Vec::new();
    let mut event_count = 0usize;
    let mut saw_response = false;
    let mut stop_reason = None;
    let mut response_items = Vec::new();
    let mut usage = ProviderUsageComponents::default();
    tracing::info!("reading ChatGPT Codex Responses SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading ChatGPT Codex SSE stream");
            return Err("cancelled".to_string());
        }

        let line = line_result.map_err(|e| {
            tracing::error!(error = %e, "failed reading ChatGPT Codex SSE stream");
            format!("stream read error: {e}")
        })?;
        if !line.starts_with("data:") {
            continue;
        }

        if !saw_response {
            saw_response = true;
            if send(
                tx,
                AgentEvent::Status(String::from("provider: receiving ChatGPT Codex SSE")),
                cancel,
            )
            .is_none()
            {
                return Err("cancelled".to_string());
            }
        }

        for data in codex::parse_responses_sse_chunk(&(line + "\n")) {
            event_count += 1;
            for event in codex::parse_responses_sse_event(&data) {
                match &event {
                    codex::ResponsesSseEvent::Usage { input_tokens, output_tokens } => {
                        usage.merge_snapshot(&ProviderUsageComponents::new(*input_tokens, *output_tokens));
                    }
                    codex::ResponsesSseEvent::UsageComponents(components) => usage.merge_snapshot(components),
                    _ => {}
                }
                if let codex::ResponsesSseEvent::OutputItem(item) = &event {
                    response_items.push(item.clone());
                }
                collect_chatgpt_codex_event(
                    event,
                    &mut tool_blocks,
                    &mut tool_requests,
                    &mut assistant_text,
                    &mut stop_reason,
                    tx,
                    cancel,
                )?;
            }
        }
    }

    for (_, block) in tool_blocks {
        if let Some(req) = block.finish() {
            tool_requests.push(req);
        }
    }

    if assistant_text.is_empty() && tool_requests.is_empty() {
        tracing::error!(
            event_count,
            "ChatGPT Codex stream ended without assistant text or tool calls"
        );
        return match stop_reason.as_deref() {
            Some("incomplete" | "length") => Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            )),
            _ => Err(format!(
                "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
            )),
        };
    }

    tracing::info!(
        event_count,
        text_chars = assistant_text.chars().count(),
        tool_calls = tool_requests.len(),
        "finished reading ChatGPT Codex SSE stream"
    );
    let _ = send(
        tx,
        AgentEvent::Status(format!(
            "provider: stream ended ({event_count} SSE events, {} text chars, {} tool calls)",
            assistant_text.chars().count(),
            tool_requests.len()
        )),
        cancel,
    );

    Ok(ProviderTurn { tool_requests, assistant_text, stop_reason, response_items, usage: Some(usage) })
}

fn collect_chatgpt_codex_event(
    event: codex::ResponsesSseEvent, tool_blocks: &mut HashMap<String, ChatToolCallBuilder>,
    tool_requests: &mut Vec<ToolUseRequest>, assistant_text: &mut String, stop_reason: &mut Option<String>,
    tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<(), String> {
    match event {
        codex::ResponsesSseEvent::TextDelta(text) => {
            assistant_text.push_str(&text);
            if send(tx, AgentEvent::AssistantDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        codex::ResponsesSseEvent::ReasoningDelta(text) => {
            if send(tx, AgentEvent::ReasoningDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        codex::ResponsesSseEvent::ToolCallStart { id, call_id, name } => {
            tool_blocks.insert(
                id,
                ChatToolCallBuilder { id: call_id, name, arguments_json: String::new() },
            );
        }
        codex::ResponsesSseEvent::ToolCallArgumentsDelta { id, arguments } => {
            let block = tool_blocks.entry(id.clone()).or_insert_with(|| ChatToolCallBuilder {
                id,
                name: String::new(),
                arguments_json: String::new(),
            });
            block.arguments_json.push_str(&arguments);
        }
        codex::ResponsesSseEvent::ToolCallDone { id, call_id, name, arguments } => {
            let remove_id = id.clone();
            let block = tool_blocks.entry(id.clone()).or_insert_with(|| ChatToolCallBuilder {
                id: id.clone(),
                name: String::new(),
                arguments_json: String::new(),
            });
            if let Some(call_id) = call_id {
                block.id = call_id;
            }
            if !name.is_empty() {
                block.name = name;
            }
            if !arguments.is_empty() {
                block.arguments_json = arguments;
            }
            if let Some(block) = tool_blocks.remove(&remove_id)
                && let Some(req) = block.finish()
            {
                tool_requests.push(req);
            }
        }
        codex::ResponsesSseEvent::ResponseStatus(status) => {
            match status.as_str() {
                "completed" => *stop_reason = Some(status.clone()),
                "failed" | "incomplete" | "cancelled" | "canceled" => {
                    return Err(format!("provider stream status: {status}"));
                }
                _ => {}
            }
            if send(tx, AgentEvent::Status(format!("provider: status {status}")), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        codex::ResponsesSseEvent::Error(message) => {
            tracing::error!(error = %message, "ChatGPT Codex emitted SSE error");
            return Err(format!("provider error: {message}"));
        }
        codex::ResponsesSseEvent::Usage { .. } => {}
        codex::ResponsesSseEvent::UsageComponents(_) => {}
        codex::ResponsesSseEvent::OutputItem(_) => {}
        codex::ResponsesSseEvent::Malformed(payload) => {
            return Err(format!("malformed provider stream payload: {payload}"));
        }
        codex::ResponsesSseEvent::Done | codex::ResponsesSseEvent::Other => {}
    }

    Ok(())
}

fn log_sse_event(seq: usize, event_type: &str, data: &str) {
    let (content_type, delta_type, stop_reason) = summarize_sse_data(data);
    tracing::info!(
        seq,
        event_type,
        content_type = content_type.as_deref().unwrap_or(""),
        delta_type = delta_type.as_deref().unwrap_or(""),
        stop_reason = stop_reason.as_deref().unwrap_or(""),
        "received SSE event"
    );
}

fn summarize_sse_data(data: &str) -> (Option<String>, Option<String>, Option<String>) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
    let content_type = v
        .get("content_block")
        .and_then(|cb| cb.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let delta_type = v
        .get("delta")
        .and_then(|d| d.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let stop_reason = v
        .get("delta")
        .and_then(|d| d.get("stop_reason"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    (content_type, delta_type, stop_reason)
}

fn collect_anthropic_event(
    event_type: &str, data: &str, state: &mut AnthropicStreamState, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<(), String> {
    let sse_event = anthropic::parse_sse_event(event_type, data);

    if let Some(usage) = extract_usage(data) {
        state.usage.merge_snapshot(&usage);
    }

    if event_type == "content_block_start"
        && let Some((index, block)) = extract_tool_use_start(data)
    {
        state.tool_blocks.insert(index, block);
    }

    if event_type == "content_block_start" {
        collect_content_block_start_text(
            data,
            &mut state.assistant_text,
            &mut state.provider_content_blocks,
            tx,
            cancel,
        )?;
    }

    match &sse_event {
        anthropic::SseEvent::TextDelta(text) => state.assistant_text.push_str(text),
        anthropic::SseEvent::InputJsonDelta { index, partial_json } => {
            if let Some(block) = state.tool_blocks.get_mut(index) {
                block.input_json.push_str(partial_json);
            }
        }
        anthropic::SseEvent::ContentBlockStop { index } => {
            if let Some(index) = index
                && let Some(block) = state.tool_blocks.remove(index)
                && let Some(req) = block.finish()
            {
                state.tool_requests.push(req);
            }
        }
        anthropic::SseEvent::MessageDelta { stop_reason: Some(reason) } => {
            state.stop_reason = Some(reason.clone());
            tracing::info!(stop_reason = %reason, "provider message stop reason");
        }
        anthropic::SseEvent::Error(msg) => {
            tracing::error!(error = %msg, "provider emitted SSE error");
            return Err(format!("provider error: {msg}"));
        }
        _ => {}
    }

    if let Some(agent_event) = anthropic::sse_to_agent_event(&sse_event)
        && send(tx, agent_event, cancel).is_none()
    {
        return Err("cancelled".to_string());
    }

    Ok(())
}

fn extract_usage(data: &str) -> Option<ProviderUsageComponents> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let usage = v
        .get("usage")
        .or_else(|| v.get("message").and_then(|m| m.get("usage")))
        .or_else(|| v.get("delta").and_then(|d| d.get("usage")))?;
    let input_tokens = usage.get("input_tokens").and_then(|t| t.as_u64());
    let output_tokens = usage.get("output_tokens").and_then(|t| t.as_u64());
    let cache_read_input_tokens = usage.get("cache_read_input_tokens").and_then(|value| value.as_u64());
    let cache_creation_input_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|value| value.as_u64());
    if input_tokens.is_none()
        && output_tokens.is_none()
        && cache_read_input_tokens.is_none()
        && cache_creation_input_tokens.is_none()
    {
        None
    } else {
        Some(ProviderUsageComponents {
            input_tokens,
            output_tokens,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            reasoning_tokens: None,
        })
    }
}

fn collect_content_block_start_text(
    data: &str, assistant_text: &mut String, provider_content_blocks: &mut Vec<String>, tx: &Sender<AgentEvent>,
    cancel: &CancelToken,
) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
    let Some(block) = v.get("content_block") else {
        return Ok(());
    };

    match block.get("type").and_then(|t| t.as_str()) {
        Some("text") => {
            let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
            if !text.is_empty() {
                assistant_text.push_str(text);
                if send(tx, AgentEvent::AssistantDelta(text.to_string()), cancel).is_none() {
                    return Err("cancelled".to_string());
                }
            }
        }
        Some("thinking") => {
            let thinking = block.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
            if !thinking.is_empty() && send(tx, AgentEvent::ReasoningDelta(thinking.to_string()), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        Some(other) => {
            provider_content_blocks.push(other.to_string());
            tracing::info!(content_type = other, "unhandled content block start");
        }
        None => {}
    }

    Ok(())
}

fn extract_tool_use_start(data: &str) -> Option<(usize, ToolUseBuilder)> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
    let cb = v.get("content_block")?;
    let block_type = cb.get("type").and_then(|t| t.as_str())?;
    if block_type != "tool_use" {
        return None;
    }
    let name = cb.get("name").and_then(|n| n.as_str())?.to_string();
    let id = cb.get("id").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let initial_input = cb.get("input").cloned().unwrap_or(serde_json::Value::Null);
    Some((
        index,
        ToolUseBuilder { id, name, initial_input, input_json: String::new() },
    ))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;
    use crate::cli::WebSearchMode;
    use crate::providers;
    use crate::tools::{self, AgentRunConfig, MAX_TOOL_ITERATIONS};
    use std::path::{Path, PathBuf};

    fn config() -> AgentRunConfig {
        AgentRunConfig::new(
            PathBuf::from("."),
            String::from("fake-agent"),
            WebSearchMode::DuckDuckGo,
        )
    }

    struct MetadataErrorProvider {
        code: u16,
    }

    impl StreamingProvider for MetadataErrorProvider {
        type Metadata = ();

        fn name(&self) -> &'static str {
            "metadata-test"
        }

        fn load_status(&self) -> String {
            "provider: loading metadata-test".to_string()
        }

        fn request_status(&self, _model: &str) -> String {
            "provider: requesting metadata-test".to_string()
        }

        fn from_env_or_dotenv(_root: &Path) -> providers::Result<Self> {
            Ok(Self { code: 401 })
        }

        fn load_metadata(&self) -> providers::Result<Self::Metadata> {
            Err(ProviderError::Status { code: self.code, body: "metadata endpoint rejected request".to_string() })
        }

        fn token_budget(&self, _model: &str, _metadata: Option<&Self::Metadata>) -> u32 {
            1
        }

        fn serialized_request_body(
            &self, _model: &str, _messages: &[ProviderMessage], _request: &StreamingRequest<'_>,
        ) -> providers::Result<Vec<u8>> {
            panic!("a rejected metadata request must abort before serializing the prompt")
        }

        fn send_streaming_request(
            &self, _model: &str, _messages: &[ProviderMessage], _request: &StreamingRequest<'_>,
        ) -> providers::Result<ureq::http::Response<ureq::Body>> {
            panic!("a rejected metadata request must abort before sending the prompt")
        }

        fn stream_format(&self, _model: &str) -> providers::Result<StreamFormat> {
            Ok(StreamFormat::AnthropicMessages)
        }

        fn request_error_message(error: &ProviderError) -> String {
            error.failure_message("metadata-test rate limit")
        }

        fn is_retryable_request_error(error: &ProviderError) -> bool {
            error.is_retryable()
        }
    }

    fn dispatch_output(req: &ToolUseRequest, root: &Path) -> tools::ToolOutput {
        tools::dispatch_full(req, root).0
    }

    #[test]
    fn provider_retry_policy_and_classification_match_defaults() {
        assert_eq!(PROVIDER_RETRY_POLICY.max_retries, 4);
        assert_eq!(PROVIDER_RETRY_POLICY.delay_for_attempt(1), Duration::from_millis(2500));
        assert_eq!(
            PROVIDER_RETRY_POLICY.delay_for_attempt(4),
            Duration::from_millis(20_000)
        );

        assert!(
            ProviderAttemptError::Request(providers::ProviderError::Status {
                code: 503,
                body: "temporarily unavailable".to_string(),
            })
            .is_retryable::<opencode::OpenCodeGoClient>()
        );
        assert!(
            ProviderAttemptError::Stream("stream read error: connection lost".to_string())
                .is_retryable::<opencode::OpenCodeGoClient>()
        );
        assert!(
            !ProviderAttemptError::Request(providers::ProviderError::Status {
                code: 401,
                body: "unauthorized".to_string()
            })
            .is_retryable::<opencode::OpenCodeGoClient>()
        );
        assert!(
            !ProviderAttemptError::Stream(
                "provider stopped at max_tokens (32768) before producing assistant text".to_string()
            )
            .is_retryable::<opencode::OpenCodeGoClient>()
        );
    }

    #[test]
    fn rejected_metadata_aborts_before_sending_the_prompt() {
        let (_steering_tx, steering_rx) = mpsc::channel();
        let handle = RunHandle::provider_with_steering(config(), Vec::new(), false, steering_rx);
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();

        handle.run_provider::<MetadataErrorProvider>(&tx, &cancel);

        let events: Vec<AgentEvent> = rx.try_iter().collect();
        assert!(events.iter().any(|event| {
            matches!(event, AgentEvent::Failed(message) if message == "authentication failed (HTTP 401)")
        }));
        assert!(!events.iter().any(|event| {
            matches!(event, AgentEvent::Status(message) if message.contains("fallback token budget"))
        }));
    }

    #[test]
    fn unavailable_metadata_uses_the_fallback_token_budget() {
        let provider = MetadataErrorProvider { code: 503 };
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();

        let metadata = load_provider_metadata(&provider, "metadata-test", &tx, &cancel);

        assert!(matches!(metadata, MetadataLoaded::Unavailable));
        assert_eq!(
            rx.try_recv().expect("fallback status"),
            AgentEvent::Status("provider: model metadata unavailable; using fallback token budget".to_string())
        );
    }

    #[test]
    fn fake_stream_emits_expected_sequence() {
        let handle = RunHandle::fake(config(), String::new());
        let rx = handle.spawn();

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
    fn fake_stream_with_duckduckgo_search_emits_search_tool_event() {
        let mut cfg = config();
        cfg.search_mode = WebSearchMode::DuckDuckGo;
        let handle = RunHandle::fake(cfg, String::new());
        let rx = handle.spawn();

        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        let has_search = events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolStarted { name, .. } if name == "web_search"));
        assert!(has_search, "DuckDuckGo search should emit web_search tool event");
    }

    #[test]
    fn fake_stream_with_none_search_skips_search_and_returns_assistant_text() {
        let mut cfg = config();
        cfg.search_mode = WebSearchMode::None;
        let handle = RunHandle::fake(cfg, String::new());
        let rx = handle.spawn();

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
        let rx = handle.spawn();
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

        let rx = handle.spawn();
        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        assert!(
            !events.contains(&AgentEvent::Finished),
            "cancelled run must not finish normally"
        );
        assert!(
            events.contains(&AgentEvent::Cancelled),
            "cancelled run must notify the UI before its channel closes"
        );
    }

    #[test]
    fn cancellation_notification_is_sent_after_token_is_cancelled() {
        let (tx, rx) = mpsc::channel();
        let token = CancelToken::new();
        token.cancel();

        assert_eq!(send(&tx, AgentEvent::Cancelled, &token), Some(()));
        assert_eq!(rx.recv().expect("cancellation event"), AgentEvent::Cancelled);
    }

    #[test]
    fn dispatch_tool_find_files_success() {
        let req = ToolUseRequest::new(
            String::from("find_files"),
            serde_json::json!({ "pattern": "mod.rs" }).to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("src/cli"));
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.display.lines.iter().any(|p| p.contains("cli/mod.rs")));
    }

    #[test]
    fn dispatch_tool_read_file_range_success() {
        let req = ToolUseRequest::new(
            String::from("read_file_range"),
            serde_json::json!({
                "path": "Cargo.toml",
                "start_line": 1,
                "end_line": 3
            })
            .to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.display.lines.len(), 3);
    }

    #[test]
    fn dispatch_tool_unknown_name_fails() {
        let req = ToolUseRequest::new(
            String::from("nonexistent_tool"),
            String::from("{}"),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("unknown tool")));
    }

    #[test]
    fn dispatch_tool_malformed_arguments_falls_back_to_defaults() {
        let req = ToolUseRequest::new(
            String::from("find_files"),
            String::from("not valid json"),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("src"));
        assert_eq!(output.status, ToolStatus::Ok);
    }

    #[test]
    fn max_tool_iterations_is_reasonable() {
        let cap = MAX_TOOL_ITERATIONS;
        assert!(cap >= 4 && cap <= 16, "per-turn cap should be 4..=16, got {cap}");
    }

    #[test]
    fn extract_tool_use_start_returns_none_for_text_block() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        assert!(extract_tool_use_start(data).is_none());
    }

    #[test]
    fn extract_tool_use_start_returns_builder_for_tool_use_block() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"find_files","input":{"pattern":"cli"}}}"#;
        let (_index, block) = extract_tool_use_start(data).expect("should extract");
        let req = block.finish().expect("should finish");
        assert_eq!(req.name, "find_files");
        assert!(req.arguments.contains("cli"));
        assert_eq!(req.tool_use_id, "toolu_1");
    }

    #[test]
    fn parse_tool_use_fixture_extracts_request_with_id() {
        let sse = include_str!("./providers/fixtures/tool_use_turn.sse");
        let chunks = anthropic::parse_sse_chunk(sse);

        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        for (event_type, data) in &chunks {
            let sse_event = anthropic::parse_sse_event(event_type, data);
            if let anthropic::SseEvent::Other(ref t) = sse_event
                && t.starts_with("content_block_start")
                && let Some((_index, block)) = extract_tool_use_start(data)
                && let Some(req) = block.finish()
            {
                tool_requests.push(req);
            }
            if let anthropic::SseEvent::TextDelta(ref text) = sse_event {
                assistant_text.push_str(text);
            }
        }

        assert_eq!(tool_requests.len(), 1);
        let req = &tool_requests[0];
        assert_eq!(req.name, "find_files");
        assert_eq!(req.tool_use_id, "toolu_01");
        assert!(req.arguments.contains("Cargo"));
        assert_eq!(assistant_text, "Let me look that up.");
    }

    #[test]
    fn collect_anthropic_event_reconstructs_streamed_tool_input_json() {
        let (tx, _rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut state = AnthropicStreamState::default();

        state.collect(
            "content_block_start",
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"find_files","input":{}}}"#,
            &tx,
            &cancel,
        )
        .expect("collect event");
        state
            .collect(
                "content_block_delta",
                &serde_json::json!({
                    "type": "content_block_delta",
                    "index": 1,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": "{\"pattern\""
                    }
                })
                .to_string(),
                &tx,
                &cancel,
            )
            .expect("collect event");
        state
            .collect(
                "content_block_delta",
                &serde_json::json!({
                    "type": "content_block_delta",
                    "index": 1,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": ":\"Cargo\"}"
                    }
                })
                .to_string(),
                &tx,
                &cancel,
            )
            .expect("collect event");
        state
            .collect(
                "content_block_stop",
                r#"{"type":"content_block_stop","index":1}"#,
                &tx,
                &cancel,
            )
            .expect("collect event");

        assert_eq!(state.tool_requests.len(), 1);
        assert_eq!(state.tool_requests[0].name, "find_files");
        assert_eq!(state.tool_requests[0].tool_use_id, "toolu_1");
        assert_eq!(state.tool_requests[0].arguments, r#"{"pattern":"Cargo"}"#);
    }

    #[test]
    fn collect_anthropic_event_tracks_provider_side_content_blocks() {
        let (tx, _rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut state = AnthropicStreamState::default();

        state.collect(
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"srv_1","name":"web_search","input":{}}}"#,
            &tx,
            &cancel,
        )
        .expect("collect event");
        state.collect(
            "content_block_start",
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"web_search_tool_result","content":[]}}"#,
            &tx,
            &cancel,
        )
        .expect("collect event");

        assert_eq!(
            state.provider_content_blocks,
            vec!["server_tool_use".to_string(), "web_search_tool_result".to_string()]
        );
        assert!(state.assistant_text.is_empty());
        assert!(state.tool_requests.is_empty());
    }

    #[test]
    fn collect_openai_chat_event_maps_text_reasoning_and_usage() {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut tool_blocks = HashMap::new();
        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        let mut stop_reason = None;

        collect_openai_chat_event(
            openai::ChatSseEvent::TextDelta("hello".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("text delta");
        collect_openai_chat_event(
            openai::ChatSseEvent::ReasoningDelta("thinking".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("reasoning delta");
        collect_openai_chat_event(
            openai::ChatSseEvent::Usage { input_tokens: 2, output_tokens: 3 },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("usage");

        assert_eq!(assistant_text, "hello");
        let events: Vec<AgentEvent> = rx.try_iter().collect();
        assert!(events.contains(&AgentEvent::AssistantDelta("hello".to_string())));
        assert!(events.contains(&AgentEvent::ReasoningDelta("thinking".to_string())));
        assert!(!events.iter().any(|event| matches!(event, AgentEvent::Usage { .. })));
    }

    #[test]
    fn collect_openai_chat_event_finishes_tool_calls_on_finish_reason() {
        let (tx, _rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut tool_blocks = HashMap::new();
        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        let mut stop_reason = None;

        collect_openai_chat_event(
            openai::ChatSseEvent::ToolCallStart { index: 0, id: "call_1".to_string(), name: "find_files".to_string() },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("tool start");
        collect_openai_chat_event(
            openai::ChatSseEvent::ToolCallArgumentsDelta { index: 0, arguments: r#"{"pattern":"Cargo"}"#.to_string() },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("tool args");
        collect_openai_chat_event(
            openai::ChatSseEvent::FinishReason("tool_calls".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("finish");

        assert_eq!(stop_reason.as_deref(), Some("tool_calls"));
        assert_eq!(tool_requests.len(), 1);
        assert_eq!(tool_requests[0].name, "find_files");
        assert_eq!(tool_requests[0].tool_use_id, "call_1");
        assert_eq!(tool_requests[0].arguments, r#"{"pattern":"Cargo"}"#);
    }

    #[test]
    fn collect_openai_chat_event_handles_status_and_failures() {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut tool_blocks = HashMap::new();
        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        let mut stop_reason = None;

        collect_openai_chat_event(
            openai::ChatSseEvent::ResponseStatus("queued".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("queued status");
        assert!(matches!(
            rx.try_recv(),
            Ok(AgentEvent::Status(status)) if status == "provider: status queued"
        ));

        let failed = collect_openai_chat_event(
            openai::ChatSseEvent::ResponseStatus("failed".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect_err("failed status should fail");
        assert!(failed.contains("provider stream status: failed"));

        let backend = collect_openai_chat_event(
            openai::ChatSseEvent::Error("backend failed".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect_err("backend error should fail");
        assert!(backend.contains("provider error: backend failed"));

        let malformed = collect_openai_chat_event(
            openai::ChatSseEvent::Malformed("{bad".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect_err("malformed payload should fail");
        assert!(malformed.contains("malformed provider stream payload"));
        assert!(!ProviderAttemptError::Stream(malformed).is_retryable::<opencode::OpenCodeGoClient>());
    }

    #[test]
    fn collect_chatgpt_codex_event_maps_text_reasoning_and_usage() {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut tool_blocks = HashMap::new();
        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        let mut stop_reason = None;

        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::TextDelta("hello".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("text delta");
        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ReasoningDelta("thinking".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("reasoning delta");
        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::Usage { input_tokens: 3, output_tokens: 5 },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("usage");

        assert_eq!(assistant_text, "hello");
        let events: Vec<AgentEvent> = rx.try_iter().collect();
        assert!(events.contains(&AgentEvent::AssistantDelta("hello".to_string())));
        assert!(events.contains(&AgentEvent::ReasoningDelta("thinking".to_string())));
        assert!(!events.iter().any(|event| matches!(event, AgentEvent::Usage { .. })));
    }

    #[test]
    fn collect_chatgpt_codex_event_finishes_tool_calls_on_done() {
        let (tx, _rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut tool_blocks = HashMap::new();
        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        let mut stop_reason = None;

        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ToolCallStart {
                id: "fc_1".to_string(),
                call_id: "call_1".to_string(),
                name: "find_files".to_string(),
            },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("tool start");
        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ToolCallArgumentsDelta {
                id: "fc_1".to_string(),
                arguments: r#"{"pattern":"Car"#.to_string(),
            },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("tool args");
        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ToolCallDone {
                id: "fc_1".to_string(),
                call_id: Some("call_1".to_string()),
                name: "find_files".to_string(),
                arguments: r#"{"pattern":"Cargo"}"#.to_string(),
            },
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("tool done");

        assert!(tool_blocks.is_empty());
        assert_eq!(tool_requests.len(), 1);
        assert_eq!(tool_requests[0].name, "find_files");
        assert_eq!(tool_requests[0].tool_use_id, "call_1");
        assert_eq!(tool_requests[0].arguments, r#"{"pattern":"Cargo"}"#);
    }

    #[test]
    fn collect_chatgpt_codex_event_handles_statuses_and_failures() {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut tool_blocks = HashMap::new();
        let mut tool_requests = Vec::new();
        let mut assistant_text = String::new();
        let mut stop_reason = None;

        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ResponseStatus("queued".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("queued status");
        collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ResponseStatus("completed".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect("completed status");
        assert_eq!(stop_reason.as_deref(), Some("completed"));
        let events: Vec<AgentEvent> = rx.try_iter().collect();
        assert!(events.contains(&AgentEvent::Status("provider: status queued".to_string())));
        assert!(events.contains(&AgentEvent::Status("provider: status completed".to_string())));

        let failed = collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::ResponseStatus("incomplete".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect_err("incomplete status should fail");
        assert!(failed.contains("provider stream status: incomplete"));

        let backend = collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::Error("backend failed".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect_err("backend error should fail");
        assert!(backend.contains("provider error: backend failed"));

        let malformed = collect_chatgpt_codex_event(
            codex::ResponsesSseEvent::Malformed("{bad".to_string()),
            &mut tool_blocks,
            &mut tool_requests,
            &mut assistant_text,
            &mut stop_reason,
            &tx,
            &cancel,
        )
        .expect_err("malformed payload should fail");
        assert!(malformed.contains("malformed provider stream payload"));
        assert!(!ProviderAttemptError::Stream(malformed).is_retryable::<opencode::OpenCodeGoClient>());
    }

    #[test]
    fn append_steering_messages_adds_user_messages() {
        let (tx, rx) = mpsc::channel();
        tx.send("look at tests first".to_string()).expect("send steering");
        drop(tx);

        let handle = RunHandle::provider_with_steering(config(), Vec::new(), false, rx);
        let mut messages = Vec::new();

        assert!(append_steering_messages(&mut messages, &handle));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert!(messages[0].as_text().contains("[steering]"));
        assert!(messages[0].as_text().contains("look at tests first"));
    }

    fn handle_with_permission(decision: ToolPermissionDecision) -> RunHandle {
        let (_tx, rx) = mpsc::channel();
        RunHandle::provider_with_steering(config(), Vec::new(), false, rx)
            .with_permission_hook(ToolPermissionHook::new(move |_request, _config, _cancel| decision))
    }

    fn approve_request(name: &str, decision: ToolPermissionDecision) -> ToolPermissionDecision {
        let request = ToolUseRequest::new(name.to_string(), "{}".to_string(), "tool-1".to_string());
        let handle = handle_with_permission(decision);
        approve_tool_request(&request, &handle, &CancelToken::new())
    }

    #[test]
    fn permission_hook_allows_file_write_tool() {
        assert_eq!(
            approve_request("create_file", ToolPermissionDecision::Allow),
            ToolPermissionDecision::Allow
        );
    }

    #[test]
    fn permission_hook_rejects_file_write_tool() {
        assert_eq!(
            approve_request("replace_range", ToolPermissionDecision::Reject),
            ToolPermissionDecision::Reject
        );
    }

    #[test]
    fn permission_hook_allows_shell_tool() {
        assert_eq!(
            approve_request("run_shell", ToolPermissionDecision::Allow),
            ToolPermissionDecision::Allow
        );
    }

    #[test]
    fn permission_hook_rejects_shell_tool() {
        assert_eq!(
            approve_request("run_shell", ToolPermissionDecision::Reject),
            ToolPermissionDecision::Reject
        );
    }

    #[test]
    fn permission_hook_cancels_sensitive_tool() {
        assert_eq!(
            approve_request("write_patch", ToolPermissionDecision::Cancelled),
            ToolPermissionDecision::Cancelled
        );
    }

    #[test]
    fn read_only_tool_bypasses_permission_hook() {
        assert_eq!(
            approve_request("read_file_range", ToolPermissionDecision::Reject),
            ToolPermissionDecision::Allow
        );
    }

    #[test]
    fn prompt_expects_workspace_write_for_file_edit_request() {
        assert!(prompt_expects_workspace_write(
            "Looking at completed work in TODO.md, can you summarize them like the completed sections?"
        ));
        assert!(prompt_expects_workspace_write("update README.md with install notes"));
    }

    #[test]
    fn prompt_expects_workspace_write_ignores_plain_file_questions() {
        assert!(!prompt_expects_workspace_write("what does TODO.md contain?"));
        assert!(!prompt_expects_workspace_write("summarize the project architecture"));
    }

    #[test]
    fn provider_kind_routes_open_code_prefixes_separately() {
        assert_eq!(
            ProviderKind::for_model("opencode/big-pickle"),
            ProviderKind::OpenCodeZen
        );
        assert_eq!(
            ProviderKind::for_model("opencode-go/kimi-k2.7-code"),
            ProviderKind::OpenCodeGo
        );
        assert_eq!(
            ProviderKind::for_model("chatgpt-codex/gpt-5.5"),
            ProviderKind::ChatGptCodex
        );
        assert_eq!(ProviderKind::for_model("big-pickle"), ProviderKind::Unsupported);
    }

    #[test]
    fn dispatch_read_url_rejects_private_network() {
        let req = ToolUseRequest::new(
            String::from("read_url"),
            serde_json::json!({ "url": "http://127.0.0.1/secret" }).to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("private network")));
    }

    #[test]
    fn dispatch_read_url_rejects_non_public_scheme() {
        let req = ToolUseRequest::new(
            String::from("read_url"),
            serde_json::json!({ "url": "file:///etc/passwd" }).to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("unsupported")));
    }

    #[test]
    fn tool_definitions_include_web_search_and_read_url() {
        let defs = tools::tool_definitions();
        let names = defs.iter().map(|d| d.name.as_ref()).collect::<Vec<&str>>();
        assert!(names.contains(&"web_search"));
        assert!(names.contains(&"read_url"));
        assert!(names.contains(&"run_shell"), "tool catalog should include run_shell");
    }

    #[test]
    fn dispatch_run_shell_success() {
        let req = ToolUseRequest::new(
            String::from("run_shell"),
            serde_json::json!({ "program": "echo", "args": ["hello"] }).to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.name, "run_shell");
        assert!(output.display.lines.iter().any(|l| l.contains("hello")));
    }

    #[test]
    fn dispatch_run_shell_failure() {
        let req = ToolUseRequest {
            name: String::from("run_shell"),
            arguments: serde_json::json!({ "program": "sh", "args": ["-c", "exit 1"] }).to_string(),
            tool_use_id: String::from("toolu_test"),
        };
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("exit 1")));
    }

    #[test]
    fn dispatch_run_shell_missing_program_fails() {
        let req = ToolUseRequest::new(
            String::from("run_shell"),
            serde_json::json!({ "args": ["test"] }).to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("."));
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_ref().is_some_and(|e| e.contains("missing")));
    }

    #[test]
    fn agent_dispatch_cancellation_stops_run_shell() {
        let dir = tempfile::tempdir().expect("temp dir");
        let handle = RunHandle::fake(
            AgentRunConfig::new(dir.path().to_path_buf(), "fake-agent".to_string(), WebSearchMode::None),
            String::new(),
        );
        let request = ToolUseRequest::new(
            "run_shell".to_string(),
            serde_json::json!({ "argv": ["sh", "-c", "exec sleep 30"] }).to_string(),
            "call_1".to_string(),
        );
        let cancel = handle.cancel.clone();
        let canceller = cancel.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            canceller.cancel();
        });
        let started = std::time::Instant::now();

        let (output, _, process) = dispatch_tool_request(&request, &handle, &cancel);

        stopper.join().expect("cancellation thread");
        let process = process.expect("shell process result");
        assert_eq!(output.status, ToolStatus::Failed);
        assert_eq!(process.status, crate::tools::shell::ProcessStatus::Cancelled);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn tool_display_output_includes_failure_detail_without_changing_success_output() {
        let success = ToolOutput::ok("find_files", vec!["src/lib.rs".to_string()]);
        assert_eq!(success.display_lines(), vec!["src/lib.rs"]);

        let failure = ToolOutput::failed(
            "run_shell",
            "missing command: provide non-empty 'argv', 'command', or 'program'",
        );
        assert_eq!(
            failure.display_lines(),
            vec!["error: missing command: provide non-empty 'argv', 'command', or 'program'"]
        );
    }

    #[test]
    fn provider_tool_result_reduces_model_only_and_keeps_provider_structure() {
        let mut reduction = thndrs_agent::context::ReductionConfig::disabled();
        reduction.repeated_line = true;
        let output = ToolOutput::ok("run_shell", vec!["same".to_string(); 10]);
        let display_before = output.display_lines();

        let (message, content, result, decision, _) =
            model_tool_result("toolu_1", &output, None, &reduction, None, false, &[]);

        assert_eq!(output.display_lines(), display_before);
        assert_eq!(output.model.lines, vec!["same".to_string(); 10]);
        assert!(content.contains("same [repeated 10 times]"));
        assert!(content.contains("<reduction_dashboard>"));
        assert!(result.changed());
        assert_eq!(result.receipts[0].mode, thndrs_agent::ContextReductionMode::Applied);
        assert_eq!(decision, thndrs_agent::context::StateProjectionDecision::Retained);

        assert_eq!(message.role, "user");
        let value = serde_json::to_value(&message).expect("provider message serializes");
        assert_eq!(value["content"][0]["type"], "tool_result");
        assert_eq!(value["content"][0]["tool_use_id"], "toolu_1");
        assert_eq!(value["content"][0]["content"], content);
    }

    #[test]
    fn command_projection_retains_operational_failure_evidence_and_recovery() {
        let process = ProcessResult {
            process_id: None,
            command: vec!["cargo".to_string(), "test".to_string()],
            cwd: PathBuf::from("/workspace"),
            status: crate::tools::shell::ProcessStatus::Failed,
            exit_code: Some(101),
            stdout: vec!["test parser::middle_failure ... FAILED".to_string()],
            stderr: vec![
                "warning: unused import".to_string(),
                "error[E0308]: mismatched types".to_string(),
                "  --> crates/thndrs/src/core/parser/mod.rs:42:9".to_string(),
                "test result: FAILED. 0 passed; 1 failed".to_string(),
            ],
            output_truncated: true,
            elapsed: Duration::from_millis(87),
            kind: crate::tools::shell::ProcessKind::OneShot,
        };
        let mut output = process.to_tool_output();
        output.evidence.artifact_handle = Some("artifact_v1_command_failure".to_string());
        let mut reduction = thndrs_agent::context::ReductionConfig::disabled();
        reduction.command_result = true;

        let (message, content, result, _, _) =
            model_tool_result("toolu_1", &output, Some(&process), &reduction, None, true, &[]);

        for evidence in [
            "command: cargo test",
            "working_directory: /workspace",
            "status: failed",
            "exit_code: 101",
            "duration_ms: 87",
            "truncated: true",
            "warning: unused import",
            "error[E0308]",
            "crates/thndrs/src/core/parser/mod.rs:42:9",
            "parser::middle_failure",
            "test result: FAILED",
            "artifact_v1_command_failure",
        ] {
            assert!(content.contains(evidence), "missing {evidence}: {content}");
        }
        assert!(result.receipts.iter().any(|receipt| {
            receipt.method == tools::command_projection::COMMAND_RESULT_PROJECTION_METHOD
                && receipt.mode == thndrs_agent::ContextReductionMode::Applied
        }));
        let value = serde_json::to_value(message).expect("provider message serializes");
        assert_eq!(value["content"][0]["type"], "tool_result");
    }

    #[test]
    fn failed_large_tool_input_requires_artifact_and_never_rewrites_shell_argv() {
        let request = ToolUseRequest::new(
            "write_patch",
            serde_json::json!({ "patch": "x".repeat(FAILED_TOOL_INPUT_MIN_BYTES) }).to_string(),
            "toolu_1",
        );
        let mut output = ToolOutput::failed("write_patch", "patch did not apply");
        output.evidence.artifact_handle = Some("artifact_v1_failed_patch".to_string());
        let mut reduction = thndrs_agent::context::ReductionConfig::disabled();
        reduction.failed_tool_input = true;

        let (projected, receipt) = project_failed_tool_input(&request, &output, &reduction);
        assert!(!projected.to_string().contains(&"x".repeat(100)));
        assert_eq!(projected["recovery_handle"], "artifact_v1_failed_patch");
        assert_eq!(
            receipt.expect("receipt").mode,
            thndrs_agent::ContextReductionMode::Applied
        );

        output.evidence.artifact_handle = None;
        let (baseline, receipt) = project_failed_tool_input(&request, &output, &reduction);
        assert!(baseline.to_string().contains(&"x".repeat(100)));
        assert!(receipt.is_none());

        let shell = ToolUseRequest::new("run_shell", request.arguments, "toolu_2");
        output.evidence.artifact_handle = Some("artifact_v1_shell".to_string());
        let (shell_baseline, receipt) = project_failed_tool_input(&shell, &output, &reduction);
        assert!(shell_baseline.to_string().contains(&"x".repeat(100)));
        assert!(receipt.is_none());
    }

    #[test]
    fn failed_large_tool_input_provider_request_references_recoverable_artifact() {
        let directory = tempfile::tempdir().expect("temporary artifact directory");
        let store = crate::artifacts::ArtifactStore::new(directory.path());
        let request = ToolUseRequest::new(
            "write_patch",
            serde_json::json!({ "patch": "x".repeat(FAILED_TOOL_INPUT_MIN_BYTES) }).to_string(),
            "toolu_1",
        );
        let mut output = ToolOutput::failed("write_patch", "patch did not apply");
        let artifact = store
            .create_tool_evidence("tool:toolu_1", &output.display_lines())
            .expect("persist bounded artifact");
        output.evidence.artifact_handle = Some(artifact.metadata.handle.clone());
        let mut reduction = thndrs_agent::context::ReductionConfig::disabled();
        reduction.failed_tool_input = true;

        let (input, receipt) = project_failed_tool_input(&request, &output, &reduction);
        let provider_request = ProviderMessage::assistant_blocks(vec![ProviderContentBlock::ToolUse {
            id: request.tool_use_id.clone(),
            name: request.name,
            input,
        }]);
        let serialized = serde_json::to_value(provider_request).expect("provider request serializes");
        let projected_input = &serialized["content"][0]["input"];

        assert_eq!(projected_input["recovery_handle"], artifact.metadata.handle);
        assert!(!serialized.to_string().contains(&"x".repeat(100)));
        assert_eq!(
            receipt.expect("applied receipt").mode,
            thndrs_agent::ContextReductionMode::Applied
        );
        let recovery = store.recover(&artifact.metadata.handle).expect("recover artifact");
        assert!(
            recovery
                .content
                .expect("artifact content")
                .contains("patch did not apply")
        );
    }

    #[test]
    fn mcp_output_does_not_use_command_projection_without_a_tool_specific_contract() {
        let mut reduction = thndrs_agent::context::ReductionConfig::disabled();
        reduction.command_result = true;
        let output = ToolOutput::ok("mcp__docs__search", vec!["plain MCP response".to_string()]);

        let (_, content, result, _, _) = model_tool_result("toolu_1", &output, None, &reduction, None, false, &[]);

        assert_eq!(content, "plain MCP response");
        assert!(
            result
                .receipts
                .iter()
                .all(|receipt| receipt.method != tools::command_projection::COMMAND_RESULT_PROJECTION_METHOD)
        );
    }

    #[test]
    fn duplicate_tool_results_keep_provider_structure_and_record_the_canonical_call() {
        let mut reduction = thndrs_agent::context::ReductionConfig::disabled();
        reduction.state_identical = true;
        let identity = thndrs_agent::context::StateProjectionIdentity::new("file:src/lib.rs:1:2", "content-a");
        let output = ToolOutput::ok("read_file_range", vec!["1: first".to_string(), "2: second".to_string()]);

        let (_, _, _, first_decision, first_record) =
            model_tool_result("toolu_1", &output, None, &reduction, identity.clone(), false, &[]);
        let history = vec![first_record.expect("state record")];
        let (message, content, result, decision, _) =
            model_tool_result("toolu_2", &output, None, &reduction, identity, false, &history);

        assert_eq!(first_decision, thndrs_agent::context::StateProjectionDecision::Retained);
        assert_eq!(
            decision,
            thndrs_agent::context::StateProjectionDecision::DuplicateOf { canonical_id: "tool:toolu_1".to_string() }
        );
        assert!(content.contains("<reduction_dashboard>"));
        assert!(!content.contains("1: first"));
        assert!(
            result
                .receipts
                .iter()
                .any(|receipt| receipt.method == "state_identical_evidence"
                    && receipt.mode == thndrs_agent::ContextReductionMode::Applied)
        );

        let value = serde_json::to_value(&message).expect("provider message serializes");
        assert_eq!(value["content"][0]["type"], "tool_result");
        assert_eq!(value["content"][0]["tool_use_id"], "toolu_2");
    }
}
