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
//! 3. Each tool-use request is dispatched via [`tools::dispatch_full`] and the
//!    result is emitted as a `ToolFinished` event appended to the transcript.
//! 4. For the Umans provider, tool results are fed back into the next turn:
//!    after each dispatched tool batch, the assistant message (with `tool_use`
//!    blocks) and the user message (with `tool_result` blocks) are appended to
//!    the message history, and the provider is re-requested.
//! 5. The loop enforces bounded tool-budget continuations to prevent recursive
//!    or unbounded tool-call loops while still allowing longer useful runs.
//! 6. Cancellation is cooperative: the loop checks the shared [`CancelToken`]
//!    between events, lines, and tool executions. When cancelled, it emits
//!    [`AgentEvent::Cancelled`] and stops.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use ureq::http::Response;

use crate::app::{AgentEvent, ToolStatus};
use crate::providers::{
    ProviderContentBlock, ProviderError, ProviderMessage, ProviderTurn, StreamFormat, StreamingProvider,
};
use crate::providers::{anthropic, openai, opencode, umans};
use crate::tools::{self, AgentRunConfig, ToolOutput, ToolUseRequest};

const PROVIDER_RETRY_POLICY: RetryPolicy = RetryPolicy::new(4, Duration::from_millis(2500));

/// Which provider drives this agent run.
///
/// The live app uses Umans. The fake provider is kept for deterministic tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    /// Deterministic fake provider, i.e. no network, scripted events.
    #[cfg(test)]
    Fake,
    /// Umans Code provider
    Umans,
    /// OpenCode Go provider.
    OpenCodeGo,
}

impl ProviderKind {
    pub fn for_model(model: &str) -> Self {
        if opencode::is_model_id(model) { ProviderKind::OpenCodeGo } else { ProviderKind::Umans }
    }

    pub fn label(self) -> &'static str {
        match self {
            #[cfg(test)]
            ProviderKind::Fake => "fake",
            ProviderKind::Umans => "umans",
            ProviderKind::OpenCodeGo => "opencode-go",
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

/// Decision returned by a tool permission hook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolPermissionDecision {
    /// The tool call may execute.
    Allow,
    /// The tool call must be rejected before execution.
    Reject,
    /// The prompt turn was cancelled while waiting for permission.
    Cancelled,
}

type ToolPermissionCallback =
    dyn Fn(&ToolUseRequest, &AgentRunConfig, &CancelToken) -> ToolPermissionDecision + Send + Sync;

/// Hook used by headless front ends to approve sensitive tool calls.
#[derive(Clone)]
pub struct ToolPermissionHook(Arc<ToolPermissionCallback>);

impl ToolPermissionHook {
    /// Create a permission hook from a callback.
    pub fn new(
        callback: impl Fn(&ToolUseRequest, &AgentRunConfig, &CancelToken) -> ToolPermissionDecision + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(callback))
    }

    fn decide(
        &self, request: &ToolUseRequest, config: &AgentRunConfig, cancel: &CancelToken,
    ) -> ToolPermissionDecision {
        (self.0)(request, config, cancel)
    }
}

impl std::fmt::Debug for ToolPermissionHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToolPermissionHook(..)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
}

impl RetryPolicy {
    pub const fn new(max_retries: u32, base_delay: Duration) -> Self {
        RetryPolicy { max_retries, base_delay }
    }

    fn delay_for_attempt(self, attempt: u32) -> Duration {
        self.base_delay * 2u32.saturating_pow(attempt.saturating_sub(1))
    }
}

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
            return None;
        }
        let arguments = if self.arguments_json.trim().is_empty() { "{}".to_string() } else { self.arguments_json };
        Some(ToolUseRequest::new(self.name, arguments, self.id))
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
}

impl RunHandle {
    /// Create a fake-provider run handle.
    #[cfg(test)]
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
        }
    }

    /// Attach a permission hook for sensitive tool calls.
    pub fn with_permission_hook(mut self, hook: ToolPermissionHook) -> Self {
        self.permission_hook = Some(hook);
        self
    }
}

#[derive(Default)]
struct AnthropicStreamState {
    tool_blocks: HashMap<usize, ToolUseBuilder>,
    tool_requests: Vec<ToolUseRequest>,
    assistant_text: String,
    stop_reason: Option<String>,
    provider_content_blocks: Vec<String>,
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
    search_mode: crate::cli::WebSearchMode,
    tool_schemas: &'a serde_json::Value,
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
    tracing::info!(provider = ?handle.provider, "starting agent thread");
    thread::spawn(move || run_agent(&handle, &tx, &cancel));
    rx
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
        #[cfg(test)]
        ProviderKind::Fake => run_fake(handle, tx, cancel),
        ProviderKind::Umans => run_provider::<umans::UmansClient>(handle, tx, cancel),
        ProviderKind::OpenCodeGo => run_provider::<opencode::OpenCodeGoClient>(handle, tx, cancel),
    }
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

/// A streaming provider sends the prompt to its API, streams the response,
/// dispatches any tool-use requests, feeds the tool results back as
/// provider-native tool result messages, and repeats until the model stops
/// requesting tools or the per-turn cap is hit.
fn run_provider<P>(handle: &RunHandle, tx: &Sender<AgentEvent>, cancel: &CancelToken)
where
    P: StreamingProvider,
{
    let provider = match P::from_env_or_dotenv(&handle.config.root) {
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
        model = %handle.config.model,
        cwd = %handle.config.root.display(),
        messages = handle.messages.len(),
        max_tool_iterations = handle.config.max_tool_iterations,
        "starting provider agent run"
    );
    if send(tx, AgentEvent::Status(provider.load_status()), cancel).is_none() {
        return;
    }

    let model_metadata = match load_provider_metadata(&provider, &handle.config.model, tx, cancel) {
        MetadataLoaded::Abort => return,
        MetadataLoaded::Loaded(metadata) => Some(metadata),
        MetadataLoaded::Unavailable => None,
    };

    let tool_defs = tools::runtime_tool_definitions(handle.config.mcp_manager.as_deref());
    let tool_schemas = tools::tool_catalog_schemas(&tool_defs);
    let mut messages = if handle.messages.is_empty() {
        vec![ProviderMessage::user(&handle.prompt)]
    } else {
        handle.messages.clone()
    };
    let mut tool_budget =
        tools::ToolIterationBudget::new(handle.config.max_tool_iterations, tools::MAX_TOOL_CONTINUATIONS);
    let mut wrote_file = false;

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
            tools::ToolBudgetDecision::Continue => {}
            tools::ToolBudgetDecision::ContinueAfterBudgetMessage => {
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
            tools::ToolBudgetDecision::Exhausted { segment_iterations, total_batches, continuations_used } => {
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
            AgentEvent::Status(provider.request_status(&handle.config.model, handle.config.search_mode)),
            cancel,
        )
        .is_none()
        {
            return;
        }

        let max_tokens = provider.token_budget(&handle.config.model, model_metadata.as_ref());
        let request = ProviderTurnRequest {
            provider: &provider,
            model: &handle.config.model,
            messages: &messages,
            max_tokens,
            search_mode: handle.config.search_mode,
            tool_schemas: &tool_schemas,
        };
        let Some(turn) = request_provider_turn_with_retries(&request, tool_budget.total_batches(), tx, cancel) else {
            return;
        };
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
                        "provider stopped at max_tokens ({}) before producing assistant text",
                        max_tokens
                    )),
                    cancel,
                );
                return;
            }
            if handle.expects_write && !wrote_file {
                let _ = send(
                    tx,
                    AgentEvent::Failed(String::from(
                        "model stopped without writing a file for an edit-like request",
                    )),
                    cancel,
                );
                return;
            }
            if append_steering_messages(&mut messages, handle) {
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

            let (output, write_result, shell_result) = match approve_tool_request(req, handle, cancel) {
                ToolPermissionDecision::Allow => {
                    tools::dispatch_runtime_full(req, &handle.config.root, handle.config.mcp_manager.as_deref())
                }
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
            if write_result.is_some() && status == ToolStatus::Ok {
                wrote_file = true;
            }
            tracing::info!(tool = %req.name, tool_id = %tool_id, status = ?status, "tool request finished");
            if send(
                tx,
                AgentEvent::ToolFinished {
                    id: tool_id.clone(),
                    output: output.output.clone(),
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

            let input: serde_json::Value = serde_json::from_str(&req.arguments).unwrap_or(serde_json::Value::Null);
            assistant_blocks.push(ProviderContentBlock::ToolUse { id: tool_id.clone(), name: req.name.clone(), input });

            let result_content = if output.output.is_empty() {
                output.error.unwrap_or_else(|| "(no output)".to_string())
            } else {
                output.output.join("\n")
            };
            let is_error = status == ToolStatus::Failed;
            tool_results.push(ProviderMessage::tool_result(&tool_id, &result_content, is_error));
        }

        messages.push(ProviderMessage::assistant_blocks(assistant_blocks));
        messages.extend(tool_results);
        append_steering_messages(&mut messages, handle);
    }
}

fn approve_tool_request(request: &ToolUseRequest, handle: &RunHandle, cancel: &CancelToken) -> ToolPermissionDecision {
    if !requires_runtime_permission(&request.name) {
        return ToolPermissionDecision::Allow;
    }
    let Some(hook) = &handle.permission_hook else {
        return ToolPermissionDecision::Allow;
    };
    hook.decide(request, &handle.config, cancel)
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
            tracing::warn!(error = %message, "failed to load provider model metadata; using fallback token budget");
            if send(
                tx,
                AgentEvent::Status(String::from(
                    "provider: model metadata unavailable; using fallback token budget",
                )),
                cancel,
            )
            .is_none()
            {
                return MetadataLoaded::Abort;
            }
            MetadataLoaded::Unavailable
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
        let attempt_result = provider_request_attempt(request, tx, cancel);

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
    request: &ProviderTurnRequest<'_, P>, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<ProviderTurn, ProviderAttemptError>
where
    P: StreamingProvider,
{
    match request.provider.send_streaming_request(
        request.model,
        request.messages,
        request.max_tokens,
        request.search_mode,
        request.tool_schemas,
    ) {
        Ok(response) => {
            if send(
                tx,
                AgentEvent::Status(format!("provider: connected HTTP {}", response.status().as_u16())),
                cancel,
            )
            .is_none()
            {
                return Err(ProviderAttemptError::Stream("cancelled".to_string()));
            }
            stream_provider_response(
                request.provider,
                request.model,
                response,
                tx,
                cancel,
                request.max_tokens,
            )
            .map_err(ProviderAttemptError::Stream)
        }
        Err(e) => Err(ProviderAttemptError::Request(e)),
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
    if send(
        tx,
        AgentEvent::Retrying {
            attempt: retry_attempt,
            max_attempts: PROVIDER_RETRY_POLICY.max_retries,
            delay_ms: delay.as_millis() as u64,
            error: message,
        },
        cancel,
    )
    .is_none()
    {
        return false;
    }
    sleep_with_cancel(delay, tx, cancel)
}

fn append_steering_messages(messages: &mut Vec<ProviderMessage>, handle: &RunHandle) -> bool {
    let Some(rx) = handle.steering.as_ref() else {
        return false;
    };

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
    tracing::info!("reading OpenCode Go chat-completions SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading OpenCode Go SSE stream");
            return Err("cancelled".to_string());
        }

        let line = line_result.map_err(|e| {
            tracing::error!(error = %e, "failed reading OpenCode Go SSE stream");
            format!("stream read error: {e}")
        })?;
        if !line.starts_with("data: ") {
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
                collect_opencode_chat_event(
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
            "OpenCode Go stream ended without assistant text or tool calls"
        );
        if stop_reason.as_deref() == Some("length") {
            return Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            ));
        }
        return Err(format!(
            "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
        ));
    }

    tracing::info!(
        event_count,
        text_chars = assistant_text.chars().count(),
        tool_calls = tool_requests.len(),
        "finished reading OpenCode Go SSE stream"
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

    Ok(ProviderTurn { tool_requests, assistant_text, stop_reason })
}

fn collect_opencode_chat_event(
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
        openai::ChatSseEvent::Usage { input_tokens, output_tokens } => {
            if send(tx, AgentEvent::Usage { input_tokens, output_tokens }, cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::Done | openai::ChatSseEvent::Other => {}
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

    if let Some((input_tokens, output_tokens)) = extract_usage(data)
        && send(tx, AgentEvent::Usage { input_tokens, output_tokens }, cancel).is_none()
    {
        return Err("cancelled".to_string());
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

fn extract_usage(data: &str) -> Option<(u64, u64)> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let usage = v
        .get("usage")
        .or_else(|| v.get("message").and_then(|m| m.get("usage")))
        .or_else(|| v.get("delta").and_then(|d| d.get("usage")))?;
    let input_tokens = usage.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
    let output_tokens = usage.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
    if input_tokens == 0 && output_tokens == 0 { None } else { Some((input_tokens, output_tokens)) }
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

/// Deterministic fake provider: emits reasoning, a tool-use request, assistant
/// text, and finishes. Demonstrates the tool dispatch path end-to-end.
#[cfg(test)]
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
        AgentEvent::ReasoningDelta(String::from("The repo is a Rust terminal coding harness.")),
        cancel,
    )
    .is_none()
    {
        return;
    }
    step();

    if handle.config.search_mode != crate::cli::WebSearchMode::None {
        let search_req = ToolUseRequest::new(
            String::from("web_search"),
            serde_json::json!({ "query": "rust terminal coding harness" }).to_string(),
            String::from("search-0"),
        );
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

        let (search_output, _, _) = tools::dispatch_full(&search_req, &handle.config.root);
        let search_status = search_output.status;
        match send(
            tx,
            AgentEvent::ToolFinished {
                id: search_id,
                output: search_output.output,
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

    let (output, _, _) = tools::dispatch_full(&tool_req, &handle.config.root);
    let status = output.status;
    if send(
        tx,
        AgentEvent::ToolFinished { id: tool_id, output: output.output, status, write_result: None, shell_result: None },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;
    use crate::cli::WebSearchMode;
    use crate::providers;
    use crate::tools::{self, AgentRunConfig, MAX_TOOL_ITERATIONS};
    use std::path::{Path, PathBuf};

    fn config() -> AgentRunConfig {
        AgentRunConfig::new(PathBuf::from("."), String::from("fake-agent"), WebSearchMode::Native)
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
            .is_retryable::<umans::UmansClient>()
        );
        assert!(
            ProviderAttemptError::Stream("stream read error: connection lost".to_string())
                .is_retryable::<umans::UmansClient>()
        );
        assert!(
            !ProviderAttemptError::Request(providers::ProviderError::Status {
                code: 401,
                body: "unauthorized".to_string()
            })
            .is_retryable::<umans::UmansClient>()
        );
        assert!(
            !ProviderAttemptError::Stream(
                "provider stopped at max_tokens (32768) before producing assistant text".to_string()
            )
            .is_retryable::<umans::UmansClient>()
        );
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
        let req = ToolUseRequest::new(
            String::from("find_files"),
            serde_json::json!({ "pattern": "mod.rs" }).to_string(),
            String::from("toolu_test"),
        );
        let output = dispatch_output(&req, Path::new("src/cli"));
        assert_eq!(output.status, ToolStatus::Ok);
        assert!(output.output.iter().any(|p| p.contains("cli/mod.rs")));
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
        assert_eq!(output.output.len(), 3);
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
    fn collect_umans_event_reconstructs_streamed_tool_input_json() {
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
    fn collect_umans_event_tracks_provider_side_content_blocks() {
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
        assert!(output.output.iter().any(|l| l.contains("hello")));
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
}
