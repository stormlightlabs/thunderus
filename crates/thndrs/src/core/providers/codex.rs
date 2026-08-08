//! ChatGPT Codex provider foundations.
//!
//! This provider targets the ChatGPT-backed Codex Responses endpoint. The
//! endpoint is not a stable OpenAI Platform API, so the public model ids keep a
//! separate `chatgpt-codex/` prefix and status copy labels it experimental.

use std::borrow::Cow;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use http::HeaderMap;

use crate::cli::{ReasoningEffort, ReasoningSummary};
use crate::{
    app::AgentEvent,
    providers::{
        self, KnownModel, ProviderContentBlock, ProviderContinuation, ProviderError, ProviderMessage,
        ProviderMessageContent, Result, StreamFormat, StreamingProvider, StreamingRequest, provider_http_agent,
    },
    thndrs_core::auth::{self, ChatGptCodexAuth},
};
use thndrs_agent::ProviderUsageComponents;

/// ChatGPT Codex backend base URL.
pub const BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

/// ChatGPT Codex Responses endpoint.
pub const RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";

const MODELS_PATH: &str = "models";

/// User/config-facing model id prefix.
pub const MODEL_PREFIX: &str = "chatgpt-codex/";

/// Display-only prefix for ChatGPT Codex models in the terminal UI.
pub const DISPLAY_MODEL_PREFIX: &str = "codex/";

/// Recommended request budget for known ChatGPT Codex models.
pub const DEFAULT_RECOMMENDED_MAX_TOKENS: u32 = 32_768;

/// Parsed ChatGPT Codex Responses stream event.
#[derive(Clone, Debug, PartialEq)]
pub enum ResponsesSseEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart {
        id: String,
        call_id: String,
        name: String,
    },
    ToolCallArgumentsDelta {
        id: String,
        arguments: String,
    },
    ToolCallDone {
        id: String,
        call_id: Option<String>,
        name: String,
        arguments: String,
    },
    ResponseStatus(String),
    Error(String),
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    UsageComponents(ProviderUsageComponents),
    /// Complete output item retained only for in-memory continuation.
    OutputItem(serde_json::Value),
    Done,
    Malformed(String),
    Other,
}

/// A parsed reset time from the undocumented ChatGPT Codex usage headers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodexUsageReset {
    /// Seconds since the Unix epoch.
    UnixSeconds(u64),
    /// An RFC 3339 timestamp that the application preserves for display.
    Rfc3339(String),
}

/// One primary or secondary usage window from a ChatGPT Codex response.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CodexUsageWindow {
    /// Percentage of the window already consumed.
    pub used_percent: Option<u8>,
    /// Duration of the provider window in minutes.
    pub window_minutes: Option<u32>,
    /// When the provider says the window resets.
    pub reset_at: Option<CodexUsageReset>,
}

/// Credit availability reported in a ChatGPT Codex response.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CodexCredits {
    /// Whether the account has credits available.
    pub has_credits: Option<bool>,
    /// Whether credit use is unlimited.
    pub unlimited: Option<bool>,
    /// Remaining credit balance when it is a whole number.
    pub balance: Option<u64>,
}

/// Application-owned quota state parsed from undocumented ChatGPT Codex headers.
///
/// The parser deliberately accepts each field independently. Missing or malformed
/// headers simply leave that field unavailable rather than affecting a request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CodexUsageStatus {
    /// Short-window quota.
    pub primary: CodexUsageWindow,
    /// Long-window quota.
    pub secondary: CodexUsageWindow,
    /// Credit availability.
    pub credits: CodexCredits,
}

impl CodexUsageStatus {
    /// Parse known ChatGPT Codex quota headers without exposing wire data to the agent crate.
    pub fn from_response_headers(headers: &HeaderMap) -> Option<Self> {
        let status = Self {
            primary: usage_window_from_headers(headers, "primary"),
            secondary: usage_window_from_headers(headers, "secondary"),
            credits: CodexCredits {
                has_credits: header_value(headers, "x-codex-credits-has-credits").and_then(parse_bool),
                unlimited: header_value(headers, "x-codex-credits-unlimited").and_then(parse_bool),
                balance: header_value(headers, "x-codex-credits-balance").and_then(parse_u64),
            },
        };
        (!status.is_empty()).then_some(status)
    }

    /// Render the compact quota segment for the TUI status row.
    pub fn compact_status(&self) -> Option<String> {
        self.compact_status_at(SystemTime::now())
    }

    /// Render the compact quota segment at a fixed time.
    pub fn compact_status_at(&self, now: SystemTime) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(window) = format_usage_window("p", &self.primary, now) {
            parts.push(window);
        }
        if let Some(window) = format_usage_window("s", &self.secondary, now) {
            parts.push(window);
        }
        if let Some(credits) = format_credits(&self.credits) {
            parts.push(credits);
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    fn is_empty(&self) -> bool {
        self.primary == CodexUsageWindow::default()
            && self.secondary == CodexUsageWindow::default()
            && self.credits == CodexCredits::default()
    }
}

/// Concrete ChatGPT Codex API client.
pub struct ChatGptCodexClient {
    base_url: String,
    auth: ChatGptCodexAuth,
    agent: ureq::Agent,
}

impl ChatGptCodexClient {
    /// Create a client from `CHATGPT_CODEX_ACCESS_TOKEN` or `~/.thndrs/auth.json`.
    pub fn from_env_or_dotenv(_workspace_root: &Path) -> Result<Self> {
        let auth = auth::resolve_chatgpt_codex_auth().map_err(|error| provider_error_from_auth_error(&error))?;
        tracing::debug!("loaded ChatGPT Codex auth");
        Ok(Self::new(BASE_URL, auth))
    }

    /// Create a client with explicit auth material.
    pub fn new(base_url: &str, auth: ChatGptCodexAuth) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), auth, agent: provider_http_agent() }
    }

    fn build_auth_headers(&self) -> Vec<(String, String)> {
        vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", self.auth.access_token),
            ),
            ("chatgpt-account-id".to_string(), self.auth.account_id.clone()),
            ("originator".to_string(), "thndrs".to_string()),
        ]
    }

    /// Build the headers for a streaming Responses request.
    pub fn build_responses_headers(&self) -> Vec<(String, String)> {
        let mut headers = self.build_auth_headers();
        headers.extend([
            ("OpenAI-Beta".to_string(), "responses=experimental".to_string()),
            ("accept".to_string(), "text/event-stream".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ]);
        headers
    }

    /// Build a Responses-like streaming request body.
    pub fn build_responses_request_body(
        model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        Self::build_responses_request_body_with_reasoning(
            model,
            messages,
            tools,
            ReasoningEffort::default(),
            ReasoningSummary::default(),
            &ProviderContinuation::default(),
        )
    }

    /// Build a Responses streaming request body for the ChatGPT Codex route.
    pub fn build_responses_request_body_with_reasoning(
        model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>, effort: ReasoningEffort,
        summary: ReasoningSummary, continuation: &ProviderContinuation,
    ) -> Result<serde_json::Value> {
        let raw_model = raw_model_id(model)?;
        let effort = if is_gpt_5_6_raw_model(raw_model) { effort } else { ReasoningEffort::Auto };
        Ok(build_openai_responses_request_body(
            raw_model,
            messages,
            tools,
            effort,
            summary,
            continuation,
        ))
    }

    /// Build a standard Responses request for an already-normalized model id.
    pub fn build_openai_responses_request_body(
        raw_model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>, effort: ReasoningEffort,
        summary: ReasoningSummary, continuation: &ProviderContinuation,
    ) -> serde_json::Value {
        build_openai_responses_request_body(raw_model, messages, tools, effort, summary, continuation)
    }

    /// Verify the configured credential against the lightweight model catalog.
    pub fn probe_authentication(&self) -> Result<()> {
        let url = format!("{}/{MODELS_PATH}", self.base_url);
        let mut request = self.agent.get(&url);
        for (key, value) in self.build_auth_headers() {
            request = request.header(&key, &value);
        }
        let mut response = request
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response
                .body_mut()
                .read_to_string()
                .unwrap_or_else(|error| format!("failed to read error body: {error}"));
            return Err(ProviderError::Status { code: status, body: providers::summarize_error_body(&body) });
        }

        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| ProviderError::Http(error.to_string()))?;
        serde_json::from_str::<serde_json::Value>(&body)
            .map(|_| ())
            .map_err(|error| ProviderError::Json(format!("ChatGPT Codex model catalog JSON parse failed: {error}")))
    }

    /// Send a streaming request to `POST /responses`.
    pub fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>, effort: ReasoningEffort,
        summary: ReasoningSummary, continuation: &ProviderContinuation,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let body =
            Self::build_responses_request_body_with_reasoning(model, messages, tools, effort, summary, continuation)?;
        let url = format!("{}/responses", self.base_url);
        let mut request = self.agent.post(&url);
        for (key, value) in self.build_responses_headers() {
            request = request.header(&key, &value);
        }
        let mut response = request
            .config()
            .http_status_as_error(false)
            .build()
            .send_json(&body)
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response
                .body_mut()
                .read_to_string()
                .unwrap_or_else(|e| format!("failed to read error body: {e}"));
            return Err(ProviderError::Status { code: status, body: providers::summarize_error_body(&body) });
        }
        Ok(response)
    }
}

impl StreamingProvider for ChatGptCodexClient {
    type Metadata = ();

    fn name(&self) -> &'static str {
        "ChatGPT Codex"
    }

    fn load_status(&self) -> String {
        String::from("provider: loading ChatGPT Codex auth")
    }

    fn request_status(&self, model: &str) -> String {
        format!("provider: POST /backend-api/codex/responses model={model} (ChatGPT-backed, experimental)")
    }

    fn from_env_or_dotenv(root: &Path) -> Result<Self> {
        ChatGptCodexClient::from_env_or_dotenv(root)
    }

    fn load_metadata(&self) -> Result<Self::Metadata> {
        Ok(())
    }

    fn metadata_loaded_event(&self, _metadata: &Self::Metadata) -> Option<AgentEvent> {
        None
    }

    fn metadata_status(&self, model: &str, _metadata: &Self::Metadata) -> Option<String> {
        raw_model_id(model)
            .ok()
            .map(|raw| format!("model: chatgpt-codex/{raw}  ChatGPT/Codex"))
    }

    fn token_budget(&self, _model: &str, _metadata: Option<&Self::Metadata>) -> u32 {
        DEFAULT_RECOMMENDED_MAX_TOKENS
    }

    fn serialized_request_body(
        &self, model: &str, messages: &[ProviderMessage], request: &StreamingRequest<'_>,
    ) -> Result<Vec<u8>> {
        let body = Self::build_responses_request_body_with_reasoning(
            model,
            messages,
            Some(request.tools),
            request.reasoning_effort,
            request.reasoning_summary,
            request.continuation,
        )?;
        providers::serialize_request_body(&body)
    }

    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], request: &StreamingRequest<'_>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        ChatGptCodexClient::send_streaming_request(
            self,
            model,
            messages,
            Some(request.tools),
            request.reasoning_effort,
            request.reasoning_summary,
            request.continuation,
        )
    }

    fn stream_format(&self, model: &str) -> Result<StreamFormat> {
        raw_model_id(model)?;
        Ok(StreamFormat::ChatGptCodexResponses)
    }

    fn request_error_message(error: &ProviderError) -> String {
        error_message(error)
    }

    fn is_retryable_request_error(error: &ProviderError) -> bool {
        is_retryable_error(error)
    }
}

/// Verify the configured ChatGPT Codex credential using the model catalog.
pub fn probe_env_or_dotenv_authentication(workspace_root: &Path) -> Result<()> {
    ChatGptCodexClient::from_env_or_dotenv(workspace_root)?.probe_authentication()
}

fn provider_error_from_auth_error(error: &auth::AuthError) -> ProviderError {
    if error.is_verification_unavailable() {
        ProviderError::AuthUnavailable(error.to_string())
    } else {
        ProviderError::Auth(error.to_string())
    }
}

fn build_openai_responses_request_body(
    raw_model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>, effort: ReasoningEffort,
    summary: ReasoningSummary, continuation: &ProviderContinuation,
) -> serde_json::Value {
    let (instructions, input) = responses_input(messages);
    let input = continuation_input(continuation, messages).unwrap_or(input);
    let mut body = serde_json::json!({
        "model": raw_model,
        "store": false,
        "stream": true,
        "instructions": instructions,
        "input": input,
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "text": { "verbosity": "low" },
        "include": ["reasoning.encrypted_content"],
    });
    if effort != ReasoningEffort::Auto {
        let mut reasoning = serde_json::json!({ "effort": effort.label() });
        if summary == ReasoningSummary::Auto {
            reasoning["summary"] = serde_json::Value::String("auto".to_string());
        }
        body["reasoning"] = reasoning;
    }
    if let Some(tool_schemas) = tools {
        let converted = responses_tools(tool_schemas);
        if !converted.as_array().is_some_and(|arr| arr.is_empty()) {
            body["tools"] = converted;
        }
    }
    body
}

/// Whether `model` is a ChatGPT Codex model id.
pub fn is_model_id(model: &str) -> bool {
    model.strip_prefix(MODEL_PREFIX).is_some_and(|raw| !raw.is_empty())
}

/// Strip `chatgpt-codex/` from a model id.
pub fn raw_model_id(model: &str) -> Result<&str> {
    model
        .strip_prefix(MODEL_PREFIX)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| ProviderError::invalid_model_id("ChatGPT Codex", MODEL_PREFIX, model))
}

/// Render a concise display label without changing the configured model id.
pub fn display_model_id(model: &str) -> Cow<'_, str> {
    match model.strip_prefix(MODEL_PREFIX) {
        Some(raw) if !raw.is_empty() => Cow::Owned(format!("{DISPLAY_MODEL_PREFIX}{raw}")),
        _ => Cow::Borrowed(model),
    }
}

/// Whether this ChatGPT Codex model supports GPT-5.6 reasoning controls.
pub fn supports_reasoning_effort(model: &str) -> bool {
    raw_model_id(model).is_ok_and(is_gpt_5_6_raw_model)
}

/// Return the currently verified ChatGPT Codex reasoning controls.
pub fn reasoning_options(model: &str) -> Vec<ReasoningEffort> {
    if supports_reasoning_effort(model) {
        vec![
            ReasoningEffort::Auto,
            ReasoningEffort::None,
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::Xhigh,
            ReasoningEffort::Max,
        ]
    } else {
        vec![ReasoningEffort::Auto]
    }
}

/// Current ChatGPT Codex models from the provider expansion plan.
pub fn known_models() -> Vec<KnownModel> {
    vec![
        KnownModel { id: "chatgpt-codex/gpt-5.6-sol", description: "ChatGPT-backed Codex GPT-5.6 Sol, experimental" },
        KnownModel {
            id: "chatgpt-codex/gpt-5.6-terra",
            description: "ChatGPT-backed Codex GPT-5.6 Terra, experimental",
        },
        KnownModel { id: "chatgpt-codex/gpt-5.6-luna", description: "ChatGPT-backed Codex GPT-5.6 Luna, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.5", description: "ChatGPT-backed Codex, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.4", description: "ChatGPT-backed Codex, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.4-mini", description: "ChatGPT-backed Codex mini, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.3-codex-spark", description: "ChatGPT-backed Codex Spark, experimental" },
    ]
}

/// Extend an in-memory Responses history after one streamed response.
pub fn record_response_items(
    continuation: &mut ProviderContinuation, messages: &[ProviderMessage], response_items: Vec<serde_json::Value>,
) {
    if response_items.is_empty() {
        return;
    }

    let mut items = match continuation.responses_items() {
        Some((items, consumed_messages)) => {
            let mut items = items.to_vec();
            items.extend(responses_input_items(&messages[consumed_messages..]));
            items
        }
        None => responses_input_items(messages),
    };
    items.extend(response_items);
    continuation.set_responses_items(items, messages.len());
}

/// Append a tool result to an in-memory Responses history.
pub fn record_tool_output(
    continuation: &mut ProviderContinuation, call_id: &str, output: &str, consumed_messages: usize,
) {
    let Some((items, _)) = continuation.responses_items() else {
        return;
    };
    let mut items = items.to_vec();
    items.push(serde_json::json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": output,
    }));
    continuation.set_responses_items(items, consumed_messages);
}

/// Convert a ChatGPT Codex error into a human-readable failure string.
pub fn error_message(err: &ProviderError) -> String {
    match err {
        ProviderError::AuthUnavailable(message) => format!(
            "ChatGPT Codex credential verification unavailable: {message}; retry `thndrs setup --provider chatgpt-codex`"
        ),
        _ => err.failure_message("ChatGPT Codex usage limit exceeded"),
    }
}

pub fn is_retryable_error(err: &ProviderError) -> bool {
    match err {
        ProviderError::Status { code, body } if terminal_status_error(*code, body) => false,
        _ => err.is_retryable(),
    }
}

/// Parse raw ChatGPT Codex Responses SSE data lines.
pub fn parse_responses_sse_chunk(chunk: &str) -> Vec<String> {
    chunk
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(|data| data.trim_start().to_string()))
        .collect()
}

/// Parse one ChatGPT Codex Responses SSE `data:` payload.
pub fn parse_responses_sse_event(data: &str) -> Vec<ResponsesSseEvent> {
    if data.trim() == "[DONE]" {
        return vec![ResponsesSseEvent::Done];
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return vec![ResponsesSseEvent::Malformed(data.chars().take(120).collect())];
    };

    let mut events = Vec::new();
    let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or_default();
    if let Some(error) = extract_responses_error(&value, event_type) {
        events.push(ResponsesSseEvent::Error(error));
    }
    if let Some(status) = extract_responses_status(&value, event_type) {
        events.push(ResponsesSseEvent::ResponseStatus(status));
    }
    if let Some(usage) = extract_responses_usage(&value) {
        if usage.cache_read_input_tokens.is_some()
            || usage.cache_creation_input_tokens.is_some()
            || usage.reasoning_tokens.is_some()
        {
            events.push(ResponsesSseEvent::UsageComponents(usage));
        } else if let (Some(input_tokens), Some(output_tokens)) = (usage.input_tokens, usage.output_tokens) {
            events.push(ResponsesSseEvent::Usage { input_tokens, output_tokens });
        } else {
            events.push(ResponsesSseEvent::UsageComponents(usage));
        }
    }
    if let Some(text) = extract_responses_text_delta(&value, event_type) {
        events.push(ResponsesSseEvent::TextDelta(text));
    }
    if let Some(reasoning) = extract_responses_reasoning_delta(&value, event_type) {
        events.push(ResponsesSseEvent::ReasoningDelta(reasoning));
    }
    if event_type == "response.output_item.done"
        && let Some(item) = value.get("item")
    {
        events.push(ResponsesSseEvent::OutputItem(item.clone()));
    }
    if let Some(tool_event) = extract_responses_tool_event(&value, event_type) {
        events.push(tool_event);
    }

    if events.is_empty() { vec![ResponsesSseEvent::Other] } else { events }
}

fn usage_window_from_headers(headers: &HeaderMap, name: &str) -> CodexUsageWindow {
    let prefix = format!("x-codex-{name}");
    CodexUsageWindow {
        used_percent: header_value(headers, &format!("{prefix}-used-percent")).and_then(parse_percent),
        window_minutes: header_value(headers, &format!("{prefix}-window-minutes")).and_then(parse_window_minutes),
        reset_at: header_value(headers, &format!("{prefix}-reset-at")).and_then(parse_reset_at),
    }
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn parse_percent(value: &str) -> Option<u8> {
    value.parse().ok().filter(|percent: &u8| *percent <= 100)
}

fn parse_window_minutes(value: &str) -> Option<u32> {
    value.parse().ok().filter(|minutes: &u32| *minutes > 0)
}

fn parse_u64(value: &str) -> Option<u64> {
    value.parse().ok()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_reset_at(value: &str) -> Option<CodexUsageReset> {
    if let Some(seconds) = parse_u64(value) {
        return Some(CodexUsageReset::UnixSeconds(seconds));
    }

    valid_rfc3339_utc(value).then(|| CodexUsageReset::Rfc3339(value.to_string()))
}

fn valid_rfc3339_utc(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !bytes.ends_with(b"Z")
    {
        return false;
    }

    let Some(year) = parse_fixed_u32(&bytes[0..4]) else { return false };
    let Some(month) = parse_fixed_u32(&bytes[5..7]) else { return false };
    let fields = [
        (&bytes[0..4], 1, 9_999),
        (&bytes[5..7], 1, 12),
        (&bytes[8..10], 1, days_in_month(year, month)),
        (&bytes[11..13], 0, 23),
        (&bytes[14..16], 0, 59),
        (&bytes[17..19], 0, 60),
    ];
    if fields.iter().any(|(digits, min, max)| {
        !digits.iter().all(u8::is_ascii_digit)
            || parse_fixed_u32(digits).is_none_or(|value| value < *min || value > *max)
    }) {
        return false;
    }

    let fraction = &bytes[19..bytes.len() - 1];
    fraction.is_empty() || (fraction[0] == b'.' && fraction.len() > 1 && fraction[1..].iter().all(u8::is_ascii_digit))
}

fn parse_fixed_u32(digits: &[u8]) -> Option<u32> {
    (!digits.is_empty() && digits.iter().all(u8::is_ascii_digit)).then(|| {
        digits
            .iter()
            .fold(0u32, |value, digit| value * 10 + u32::from(digit - b'0'))
    })
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn format_usage_window(label: &str, window: &CodexUsageWindow, now: SystemTime) -> Option<String> {
    let percent = window.used_percent?;
    let mut text = format!("{label}:{percent}%");
    if let Some(minutes) = window.window_minutes {
        text.push('/');
        text.push_str(&format_window_minutes(minutes));
    }
    if let Some(reset) = window.reset_at.as_ref() {
        text.push_str("->");
        text.push_str(&format_reset_at(reset, now));
    }
    Some(text)
}

fn format_window_minutes(minutes: u32) -> String {
    if minutes % 1_440 == 0 {
        format!("{}d", minutes / 1_440)
    } else if minutes % 60 == 0 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn format_reset_at(reset: &CodexUsageReset, now: SystemTime) -> String {
    match reset {
        CodexUsageReset::UnixSeconds(seconds) => UNIX_EPOCH
            .checked_add(Duration::from_secs(*seconds))
            .and_then(|time| time.duration_since(now).ok())
            .map_or_else(|| "now".to_string(), format_remaining),
        CodexUsageReset::Rfc3339(value) => value.clone(),
    }
}

fn format_remaining(duration: Duration) -> String {
    let minutes = duration.as_secs().div_ceil(60);
    if minutes >= 1_440 {
        format!("{}d", minutes / 1_440)
    } else if minutes >= 60 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn format_credits(credits: &CodexCredits) -> Option<String> {
    if credits.unlimited == Some(true) {
        Some("credits:unlimited".to_string())
    } else if credits.has_credits == Some(false) {
        Some("credits:none".to_string())
    } else {
        credits.balance.map(|balance| format!("credits:{balance}"))
    }
}

fn is_gpt_5_6_raw_model(model: &str) -> bool {
    matches!(model, "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna")
}

fn continuation_input(continuation: &ProviderContinuation, messages: &[ProviderMessage]) -> Option<serde_json::Value> {
    let (items, consumed_messages) = continuation.responses_items()?;
    let mut input = items.to_vec();
    input.extend(responses_input_items(&messages[consumed_messages..]));
    Some(serde_json::Value::Array(input))
}

fn extract_responses_text_delta(value: &serde_json::Value, event_type: &str) -> Option<String> {
    if event_type != "response.output_text.delta" {
        return None;
    }
    string_field(value, &["delta"])
}

fn extract_responses_reasoning_delta(value: &serde_json::Value, event_type: &str) -> Option<String> {
    if !event_type.contains("reasoning_summary") {
        return None;
    }
    string_field(value, &["delta", "text", "summary_text"])
}

fn extract_responses_tool_event(value: &serde_json::Value, event_type: &str) -> Option<ResponsesSseEvent> {
    if event_type == "response.output_item.added" {
        let item = value.get("item")?;
        if item.get("type").and_then(|v| v.as_str()) != Some("function_call") {
            return None;
        }
        let id = function_item_id(item)?;
        let call_id = function_call_id(item).unwrap_or_else(|| id.clone());
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        return Some(ResponsesSseEvent::ToolCallStart { id, call_id, name });
    }

    if event_type.contains("function_call_arguments.delta") {
        let id = event_item_id(value)?;
        let arguments = value.get("delta").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if arguments.is_empty() {
            return None;
        }
        return Some(ResponsesSseEvent::ToolCallArgumentsDelta { id, arguments });
    }

    if event_type.contains("function_call_arguments.done") || event_type == "response.output_item.done" {
        let item = value.get("item").unwrap_or(value);
        if item
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|kind| kind != "function_call")
        {
            return None;
        }
        let id = function_item_id(item).or_else(|| event_item_id(value))?;
        let call_id = function_call_id(item);
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let arguments = item
            .get("arguments")
            .or_else(|| value.get("arguments"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Some(ResponsesSseEvent::ToolCallDone { id, call_id, name, arguments });
    }

    None
}

fn extract_responses_error(value: &serde_json::Value, event_type: &str) -> Option<String> {
    value
        .get("error")
        .and_then(|error| match error {
            serde_json::Value::String(message) => Some(message.clone()),
            serde_json::Value::Object(_) => error
                .get("message")
                .or_else(|| error.get("code"))
                .and_then(|message| message.as_str())
                .map(str::to_string),
            _ => None,
        })
        .or_else(|| {
            if event_type == "error" || event_type.ends_with(".error") {
                string_field(value, &["message", "code"])
            } else {
                None
            }
        })
}

fn extract_responses_status(value: &serde_json::Value, event_type: &str) -> Option<String> {
    let status = value
        .get("status")
        .or_else(|| value.get("response").and_then(|response| response.get("status")))
        .and_then(|status| status.as_str())
        .or_else(|| event_type.strip_prefix("response."))
        .filter(|status| {
            matches!(
                *status,
                "completed" | "failed" | "incomplete" | "cancelled" | "canceled" | "queued" | "in_progress"
            )
        })?;
    Some(status.to_string())
}

fn extract_responses_usage(value: &serde_json::Value) -> Option<ProviderUsageComponents> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("response").and_then(|response| response.get("usage")))
        .filter(|usage| !usage.is_null())?;
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .or_else(|| usage.get("inputTokens"))
        .and_then(|v| v.as_u64());
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .or_else(|| usage.get("outputTokens"))
        .and_then(|v| v.as_u64());
    if input_tokens.is_none() && output_tokens.is_none() {
        return None;
    }
    let details = usage
        .get("input_tokens_details")
        .or_else(|| usage.get("prompt_tokens_details"));
    Some(ProviderUsageComponents {
        input_tokens,
        output_tokens,
        cache_read_input_tokens: details
            .and_then(|details| details.get("cached_tokens"))
            .and_then(|value| value.as_u64()),
        cache_creation_input_tokens: None,
        reasoning_tokens: usage
            .get("output_tokens_details")
            .or_else(|| usage.get("completion_tokens_details"))
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(|value| value.as_u64()),
    })
}

fn event_item_id(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["item_id", "output_item_id", "call_id"])
}

fn function_call_id(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["call_id"])
}

fn function_item_id(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["id", "item_id", "call_id"])
}

fn string_field(value: &serde_json::Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(|v| v.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn responses_input(messages: &[ProviderMessage]) -> (String, serde_json::Value) {
    let mut instructions = String::new();
    let mut input = Vec::new();
    for message in messages {
        if message.role == "system" {
            if !instructions.is_empty() {
                instructions.push_str("\n\n");
            }
            instructions.push_str(&message.as_text());
            continue;
        }
        input.extend(responses_items_for_message(message));
    }
    (instructions, serde_json::Value::Array(input))
}

fn responses_input_items(messages: &[ProviderMessage]) -> Vec<serde_json::Value> {
    match responses_input(messages).1 {
        serde_json::Value::Array(items) => items,
        _ => Vec::new(),
    }
}

fn responses_items_for_message(message: &ProviderMessage) -> Vec<serde_json::Value> {
    match &message.content {
        ProviderMessageContent::Text(text) => {
            vec![serde_json::json!({"role": message.role, "content": text})]
        }
        ProviderMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ProviderContentBlock::Text { text } => Some(serde_json::json!({"role": message.role, "content": text})),
                ProviderContentBlock::ToolResult { tool_use_id, content, .. } => Some(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": tool_use_id,
                    "output": content,
                })),
                ProviderContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({
                    "type": "function_call",
                    "call_id": id,
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string()),
                })),
                ProviderContentBlock::Image { .. } => None,
            })
            .collect(),
    }
}

fn responses_tools(tools: &serde_json::Value) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .as_array()
            .into_iter()
            .flatten()
            .map(|tool| {
                let function = tool.get("function").unwrap_or(tool);
                serde_json::json!({
                    "type": "function",
                    "name": function.get("name").cloned().unwrap_or(serde_json::Value::Null),
                    "description": function.get("description").cloned().unwrap_or(serde_json::Value::String(String::new())),
                    "parameters": function
                        .get("parameters")
                        .or_else(|| function.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
                    "strict": false,
                })
            })
            .collect(),
    )
}

fn terminal_status_error(code: u16, body: &str) -> bool {
    if matches!(code, 400..=404) {
        return true;
    }
    let lower = body.to_ascii_lowercase();
    lower.contains("subscription")
        || lower.contains("balance")
        || lower.contains("quota")
        || lower.contains("monthly")
        || lower.contains("usage limit")
        || lower.contains("insufficient")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::env;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    struct ModelsFixture {
        base_url: String,
        task: thread::JoinHandle<()>,
    }

    impl ModelsFixture {
        fn respond(status: &str, body: &str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock models server");
            let addr = listener.local_addr().expect("mock models server address");
            let status = status.to_string();
            let body = body.to_string();
            let task = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept mock models request");
                let mut request = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    loop {
                        let mut line = String::new();
                        let count = reader.read_line(&mut line).expect("read mock models request");
                        if count == 0 || line == "\r\n" || line == "\n" {
                            break;
                        }
                        request.push_str(&line);
                    }
                }
                let request = request.to_ascii_lowercase();
                assert!(request.starts_with("get /models "));
                assert!(request.contains("authorization: bearer test-token"));
                assert!(request.contains("chatgpt-account-id: acct_123"));

                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write mock models response");
            });
            Self { base_url: format!("http://{addr}"), task }
        }

        fn drop_connection() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock models server");
            let addr = listener.local_addr().expect("mock models server address");
            let task = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept mock models request");
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
            });
            Self { base_url: format!("http://{addr}"), task }
        }

        fn finish(self) {
            self.task.join().expect("mock models server");
        }
    }

    fn test_auth() -> ChatGptCodexAuth {
        ChatGptCodexAuth { access_token: "test-token".to_string(), account_id: "acct_123".to_string() }
    }

    #[test]
    fn parses_usage_windows_and_credits_from_response_headers() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("x-codex-primary-used-percent", "42"),
            ("x-codex-primary-window-minutes", "300"),
            ("x-codex-primary-reset-at", "7200"),
            ("x-codex-secondary-used-percent", "7"),
            ("x-codex-secondary-window-minutes", "10080"),
            ("x-codex-secondary-reset-at", "2030-01-02T03:04:05Z"),
            ("x-codex-credits-has-credits", "true"),
            ("x-codex-credits-unlimited", "false"),
            ("x-codex-credits-balance", "19"),
        ] {
            headers.insert(name, value.parse().expect("valid test header"));
        }

        let status = CodexUsageStatus::from_response_headers(&headers).expect("usage status");

        assert_eq!(status.primary.used_percent, Some(42));
        assert_eq!(status.primary.window_minutes, Some(300));
        assert_eq!(status.primary.reset_at, Some(CodexUsageReset::UnixSeconds(7200)));
        assert_eq!(status.secondary.used_percent, Some(7));
        assert_eq!(
            status.secondary.reset_at,
            Some(CodexUsageReset::Rfc3339("2030-01-02T03:04:05Z".to_string()))
        );
        assert_eq!(status.credits.balance, Some(19));
        assert_eq!(
            status.compact_status_at(UNIX_EPOCH),
            Some("p:42%/5h->2h s:7%/7d->2030-01-02T03:04:05Z credits:19".to_string())
        );
    }

    #[test]
    fn usage_headers_ignore_missing_and_malformed_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-codex-primary-used-percent",
            "101".parse().expect("valid test header"),
        );
        headers.insert(
            "x-codex-primary-window-minutes",
            "zero".parse().expect("valid test header"),
        );
        headers.insert(
            "x-codex-primary-reset-at",
            "2030-99-99T25:61:61Z".parse().expect("valid test header"),
        );
        headers.insert(
            "x-codex-secondary-reset-at",
            http::HeaderValue::from_bytes(&[0xff]).expect("opaque test header"),
        );
        headers.insert(
            "x-codex-credits-has-credits",
            "perhaps".parse().expect("valid test header"),
        );

        assert_eq!(CodexUsageStatus::from_response_headers(&headers), None);

        headers.insert(
            "x-codex-secondary-used-percent",
            "12".parse().expect("valid test header"),
        );
        let status = CodexUsageStatus::from_response_headers(&headers).expect("partial status");
        assert_eq!(status.secondary.used_percent, Some(12));
        assert_eq!(status.primary, CodexUsageWindow::default());
        assert_eq!(status.secondary.reset_at, None);
        assert_eq!(status.credits, CodexCredits::default());
    }

    #[test]
    fn raw_model_id_requires_prefix() {
        assert_eq!(raw_model_id("chatgpt-codex/gpt-5.5").unwrap(), "gpt-5.5");
        assert!(matches!(
            raw_model_id("gpt-5.5"),
            Err(ProviderError::InvalidModelId { .. })
        ));
    }

    #[test]
    fn known_models_include_chatgpt_codex_picker_entries() {
        let ids: Vec<&str> = known_models().iter().map(|model| model.id).collect();
        assert!(ids.contains(&"chatgpt-codex/gpt-5.6-sol"));
        assert!(ids.contains(&"chatgpt-codex/gpt-5.6-terra"));
        assert!(ids.contains(&"chatgpt-codex/gpt-5.6-luna"));
        assert!(ids.contains(&"chatgpt-codex/gpt-5.5"));
        assert!(ids.contains(&"chatgpt-codex/gpt-5.4"));
        assert!(ids.contains(&"chatgpt-codex/gpt-5.4-mini"));
        assert!(ids.contains(&"chatgpt-codex/gpt-5.3-codex-spark"));
    }

    #[test]
    fn missing_credentials_fail_before_network_access() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        let old_home = env::var_os("HOME");
        unsafe {
            env::set_var("HOME", &home);
        }

        let result = ChatGptCodexClient::from_env_or_dotenv(dir.path());

        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }
        assert!(
            matches!(result, Err(ProviderError::Auth(message)) if message.contains("run `thndrs login chatgpt-codex`"))
        );
    }

    #[test]
    fn unavailable_chatgpt_auth_is_not_treated_as_a_rejected_credential() {
        let error = provider_error_from_auth_error(&auth::AuthError::ChatGptCodexUnavailable(
            "token request failed with status 503".to_string(),
        ));

        assert!(matches!(error, ProviderError::AuthUnavailable(_)));
        assert!(!error.is_credential_rejected());
        assert!(is_retryable_error(&error));
        assert!(error_message(&error).contains("retry `thndrs setup --provider chatgpt-codex`"));
    }

    #[test]
    fn build_headers_include_expected_names_without_snapshotting_token() {
        let client = ChatGptCodexClient::new(BASE_URL, test_auth());
        let headers: HashMap<String, String> = client.build_responses_headers().into_iter().collect();
        assert!(headers.get("Authorization").unwrap().starts_with("Bearer "));
        assert_eq!(headers.get("chatgpt-account-id").unwrap(), "acct_123");
        assert_eq!(headers.get("originator").unwrap(), "thndrs");
        assert_eq!(headers.get("OpenAI-Beta").unwrap(), "responses=experimental");
        assert_eq!(headers.get("accept").unwrap(), "text/event-stream");
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn client_uses_shared_provider_timeouts() {
        let client = ChatGptCodexClient::new(BASE_URL, test_auth());
        let timeouts = client.agent.config().timeouts();

        assert_eq!(
            timeouts.global, None,
            "SSE streams must not have a total lifetime deadline"
        );
        assert_eq!(timeouts.per_call, None, "SSE streams must not have a per-call deadline");
        assert_eq!(timeouts.resolve, Some(providers::PROVIDER_CONNECT_TIMEOUT));
        assert_eq!(timeouts.connect, Some(providers::PROVIDER_CONNECT_TIMEOUT));
        assert_eq!(timeouts.send_request, Some(providers::PROVIDER_REQUEST_TIMEOUT));
        assert_eq!(timeouts.send_body, Some(providers::PROVIDER_REQUEST_TIMEOUT));
        assert_eq!(timeouts.recv_response, Some(providers::PROVIDER_RESPONSE_TIMEOUT));
        assert_eq!(timeouts.recv_body, Some(providers::PROVIDER_STREAM_IDLE_TIMEOUT));
    }

    #[test]
    fn codex_streaming_policy_allows_luna_xhigh_initial_gap_but_bounds_stalls() {
        const RECOMMENDED_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

        let client = ChatGptCodexClient::new(BASE_URL, test_auth());
        let timeouts = client.agent.config().timeouts();

        assert_eq!(
            timeouts.recv_response,
            Some(RECOMMENDED_RESPONSE_TIMEOUT),
            "Luna xhigh needs a finite five-minute initial response-header grace period"
        );
        assert_eq!(
            timeouts.recv_body,
            Some(providers::PROVIDER_STREAM_IDLE_TIMEOUT),
            "a response that has started must still fail after the finite body-idle bound"
        );
        assert!(timeouts.global.is_none(), "do not impose a total SSE lifetime");
        assert!(
            timeouts.per_call.is_none(),
            "do not impose a redirect-call deadline on SSE"
        );
    }

    #[test]
    fn codex_liveness_events_cover_initial_and_reasoning_gaps() {
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.in_progress"}"#),
            vec![ResponsesSseEvent::ResponseStatus("in_progress".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.reasoning_summary_text.delta","delta":"still thinking"}"#),
            vec![ResponsesSseEvent::ReasoningDelta("still thinking".to_string())]
        );
    }

    #[test]
    fn authentication_probe_uses_the_authenticated_models_catalog() {
        let fixture = ModelsFixture::respond("200 OK", r#"{"object":"list","data":[]}"#);
        let result = ChatGptCodexClient::new(&fixture.base_url, test_auth()).probe_authentication();

        fixture.finish();
        assert!(result.is_ok());
    }

    #[test]
    fn authentication_probe_preserves_rejected_credential_statuses() {
        for (status, code) in [("401 Unauthorized", 401), ("403 Forbidden", 403)] {
            let fixture = ModelsFixture::respond(status, r#"{"error":"rejected"}"#);
            let error = ChatGptCodexClient::new(&fixture.base_url, test_auth())
                .probe_authentication()
                .expect_err("credential should be rejected");

            fixture.finish();
            assert!(matches!(&error, ProviderError::Status { code: actual, .. } if *actual == code));
            assert!(error.is_credential_rejected());
            assert!(!error.is_retryable());
        }
    }

    #[test]
    fn authentication_probe_preserves_transient_statuses() {
        for (status, code) in [("429 Too Many Requests", 429), ("503 Service Unavailable", 503)] {
            let fixture = ModelsFixture::respond(status, r#"{"error":"unavailable"}"#);
            let error = ChatGptCodexClient::new(&fixture.base_url, test_auth())
                .probe_authentication()
                .expect_err("provider should be unavailable");

            fixture.finish();
            assert!(matches!(&error, ProviderError::Status { code: actual, .. } if *actual == code));
            assert!(!error.is_credential_rejected());
            assert!(error.is_retryable());
        }
    }

    #[test]
    fn authentication_probe_treats_connection_failures_as_unavailable() {
        let fixture = ModelsFixture::drop_connection();
        let error = ChatGptCodexClient::new(&fixture.base_url, test_auth())
            .probe_authentication()
            .expect_err("connection should fail");

        fixture.finish();
        assert!(matches!(error, ProviderError::Http(_)));
        assert!(!error.is_credential_rejected());
    }

    #[test]
    fn authentication_probe_treats_malformed_catalogs_as_unavailable() {
        let fixture = ModelsFixture::respond("200 OK", "not-json");
        let error = ChatGptCodexClient::new(&fixture.base_url, test_auth())
            .probe_authentication()
            .expect_err("malformed catalog");

        fixture.finish();
        assert!(matches!(error, ProviderError::Json(_)));
        assert!(!error.is_credential_rejected());
    }

    #[test]
    fn build_responses_body_uses_raw_model_and_response_options() {
        let messages = vec![
            ProviderMessage {
                role: "system".to_string(),
                content: ProviderMessageContent::Text("be brief".to_string()),
            },
            ProviderMessage::user("hello"),
        ];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let body = ChatGptCodexClient::build_responses_request_body("chatgpt-codex/gpt-5.5", &messages, Some(&catalog))
            .expect("body");

        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["instructions"], "be brief");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["text"]["verbosity"], "low");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["name"], defs[0].name.as_ref());
    }

    #[test]
    fn build_responses_body_preserves_run_shell_argv_schema_from_anthropic_catalog() {
        let messages = vec![ProviderMessage::user("run a command")];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let body = ChatGptCodexClient::build_responses_request_body("chatgpt-codex/gpt-5.5", &messages, Some(&catalog))
            .expect("body");
        let run_shell = body["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .find(|tool| tool["name"] == "run_shell")
            .expect("run_shell tool");
        let definition = defs
            .iter()
            .find(|definition| definition.name.as_ref() == "run_shell")
            .expect("run_shell definition");

        assert_eq!(run_shell["parameters"], definition.input_schema);
        assert_eq!(run_shell["parameters"]["required"], serde_json::json!(["argv"]));
        assert_eq!(run_shell["parameters"]["properties"]["argv"]["type"], "array");
    }

    #[test]
    fn gpt_5_6_request_includes_reasoning_effort_and_optional_summary() {
        let messages = vec![ProviderMessage::user("inspect the project")];
        let body = ChatGptCodexClient::build_responses_request_body_with_reasoning(
            "chatgpt-codex/gpt-5.6-sol",
            &messages,
            None,
            ReasoningEffort::Xhigh,
            ReasoningSummary::Auto,
            &ProviderContinuation::default(),
        )
        .expect("body");

        assert_eq!(body["model"], "gpt-5.6-sol");
        assert_eq!(body["reasoning"]["effort"], "xhigh");
        assert_eq!(body["reasoning"]["summary"], "auto");
        assert_eq!(body["include"], serde_json::json!(["reasoning.encrypted_content"]));
    }

    #[test]
    fn terra_and_luna_requests_include_reasoning_effort() {
        for (model, effort) in [
            ("chatgpt-codex/gpt-5.6-terra", ReasoningEffort::None),
            ("chatgpt-codex/gpt-5.6-luna", ReasoningEffort::Max),
        ] {
            let body = ChatGptCodexClient::build_responses_request_body_with_reasoning(
                model,
                &[ProviderMessage::user("hello")],
                None,
                effort,
                ReasoningSummary::Off,
                &ProviderContinuation::default(),
            )
            .expect("body");

            assert_eq!(body["reasoning"]["effort"], effort.label());
        }
    }

    #[test]
    fn reasoning_effort_support_is_limited_to_gpt_5_6_models() {
        assert!(supports_reasoning_effort("chatgpt-codex/gpt-5.6-sol"));
        assert!(supports_reasoning_effort("chatgpt-codex/gpt-5.6-terra"));
        assert!(supports_reasoning_effort("chatgpt-codex/gpt-5.6-luna"));
        assert!(!supports_reasoning_effort("chatgpt-codex/gpt-5.5"));
    }

    #[test]
    fn display_model_id_uses_short_codex_prefix_only_for_codex_models() {
        assert_eq!(display_model_id("chatgpt-codex/gpt-5.6-terra"), "codex/gpt-5.6-terra");
        assert_eq!(display_model_id("legacy/default"), "legacy/default");
    }

    #[test]
    fn older_models_keep_the_existing_responses_payload() {
        let body = ChatGptCodexClient::build_responses_request_body_with_reasoning(
            "chatgpt-codex/gpt-5.5",
            &[ProviderMessage::user("hello")],
            None,
            ReasoningEffort::High,
            ReasoningSummary::Auto,
            &ProviderContinuation::default(),
        )
        .expect("body");

        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn continuation_replays_encrypted_response_items_and_tool_outputs() {
        let messages = vec![ProviderMessage::user("inspect")];
        let mut continuation = ProviderContinuation::default();
        record_response_items(
            &mut continuation,
            &messages,
            vec![serde_json::json!({
                "type": "reasoning",
                "encrypted_content": "opaque",
            })],
        );
        record_tool_output(&mut continuation, "call_1", "Cargo.toml", messages.len());

        let body = ChatGptCodexClient::build_responses_request_body_with_reasoning(
            "chatgpt-codex/gpt-5.6-sol",
            &messages,
            None,
            ReasoningEffort::Medium,
            ReasoningSummary::Off,
            &continuation,
        )
        .expect("body");
        let input = body["input"].as_array().expect("input array");

        assert!(input.iter().any(|item| item["encrypted_content"] == "opaque"));
        assert!(
            input
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == "call_1")
        );
        assert!(body["reasoning"].get("summary").is_none());
    }

    #[test]
    fn build_responses_body_uses_raw_model_for_text_only_turns() {
        let messages = vec![
            ProviderMessage {
                role: "system".to_string(),
                content: ProviderMessageContent::Text("be brief".to_string()),
            },
            ProviderMessage::user("hello"),
        ];
        let body =
            ChatGptCodexClient::build_responses_request_body("chatgpt-codex/gpt-5.5", &messages, None).expect("body");

        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["instructions"], "be brief");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"][0]["content"], "hello");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn build_responses_body_converts_tool_result_history() {
        let messages = vec![
            ProviderMessage {
                role: "assistant".to_string(),
                content: ProviderMessageContent::Blocks(vec![ProviderContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "find_files".to_string(),
                    input: serde_json::json!({ "pattern": "Cargo" }),
                }]),
            },
            ProviderMessage {
                role: "user".to_string(),
                content: ProviderMessageContent::Blocks(vec![ProviderContentBlock::ToolResult {
                    tool_use_id: "call_1".to_string(),
                    content: "Cargo.toml".to_string(),
                    is_error: Some(false),
                }]),
            },
        ];
        let body =
            ChatGptCodexClient::build_responses_request_body("chatgpt-codex/gpt-5.5", &messages, None).expect("body");

        assert_eq!(body["input"][0]["type"], "function_call");
        assert_eq!(body["input"][0]["call_id"], "call_1");
        assert_eq!(body["input"][0]["name"], "find_files");
        assert_eq!(body["input"][0]["arguments"], r#"{"pattern":"Cargo"}"#);
        assert_eq!(body["input"][1]["type"], "function_call_output");
        assert_eq!(body["input"][1]["call_id"], "call_1");
        assert_eq!(body["input"][1]["output"], "Cargo.toml");
        assert!(body["input"][1].get("is_error").is_none());
    }

    #[test]
    fn parse_responses_sse_chunk_accepts_optional_space_after_data_colon() {
        assert_eq!(
            parse_responses_sse_chunk("data:{\"a\":1}\ndata: {\"b\":2}\n"),
            vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]
        );
    }

    #[test]
    fn parse_responses_sse_text_reasoning_usage_and_statuses() {
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.output_text.delta","delta":"hi"}"#),
            vec![ResponsesSseEvent::TextDelta("hi".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.reasoning_summary_text.delta","delta":"thinking"}"#),
            vec![ResponsesSseEvent::ReasoningDelta("thinking".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":5,"output_tokens":8}}}"#
            ),
            vec![
                ResponsesSseEvent::ResponseStatus("completed".to_string()),
                ResponsesSseEvent::Usage { input_tokens: 5, output_tokens: 8 },
            ]
        );
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.in_progress"}"#),
            vec![ResponsesSseEvent::ResponseStatus("in_progress".to_string())]
        );
        assert_eq!(parse_responses_sse_event("[DONE]"), vec![ResponsesSseEvent::Done]);
    }

    #[test]
    fn parse_responses_sse_event_retains_cache_and_reasoning_components() {
        let events = parse_responses_sse_event(
            r#"{"type":"response.completed","usage":{"input_tokens":100,"output_tokens":12,"input_tokens_details":{"cached_tokens":40},"output_tokens_details":{"reasoning_tokens":5}}}"#,
        );
        assert!(
            events.contains(&ResponsesSseEvent::UsageComponents(ProviderUsageComponents {
                input_tokens: Some(100),
                output_tokens: Some(12),
                cache_read_input_tokens: Some(40),
                cache_creation_input_tokens: None,
                reasoning_tokens: Some(5),
            }))
        );
    }

    #[test]
    fn parse_responses_sse_text_done_does_not_repeat_finalized_text() {
        let text: String = [
            parse_responses_sse_event(r#"{"type":"response.output_text.delta","delta":"hi"}"#),
            parse_responses_sse_event(r#"{"type":"response.output_text.done","text":"hi"}"#),
        ]
        .into_iter()
        .flatten()
        .filter_map(|event| match event {
            ResponsesSseEvent::TextDelta(text) => Some(text),
            _ => None,
        })
        .collect();

        assert_eq!(text, "hi");
    }

    #[test]
    fn parse_responses_sse_tool_call_deltas_and_done() {
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.output_item.added","item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"find_files"}}"#
            ),
            vec![ResponsesSseEvent::ToolCallStart {
                id: "fc_1".to_string(),
                call_id: "call_1".to_string(),
                name: "find_files".to_string(),
            }]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"pattern\":\"Cargo\""}"#
            ),
            vec![ResponsesSseEvent::ToolCallArgumentsDelta {
                id: "fc_1".to_string(),
                arguments: r#"{"pattern":"Cargo""#.to_string(),
            }]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.output_item.done","item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"find_files","arguments":"{\"pattern\":\"Cargo\"}"}}"#
            ),
            vec![
                ResponsesSseEvent::OutputItem(serde_json::json!({
                    "id":"fc_1",
                    "type":"function_call",
                    "call_id":"call_1",
                    "name":"find_files",
                    "arguments":"{\"pattern\":\"Cargo\"}",
                })),
                ResponsesSseEvent::ToolCallDone {
                    id: "fc_1".to_string(),
                    call_id: Some("call_1".to_string()),
                    name: "find_files".to_string(),
                    arguments: r#"{"pattern":"Cargo"}"#.to_string(),
                }
            ]
        );
    }

    #[test]
    fn parse_responses_sse_backend_errors_and_malformed_payloads() {
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"error","message":"backend failed"}"#),
            vec![ResponsesSseEvent::Error("backend failed".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.failed","response":{"status":"failed"},"error":{"message":"bad auth"}}"#
            ),
            vec![
                ResponsesSseEvent::Error("bad auth".to_string()),
                ResponsesSseEvent::ResponseStatus("failed".to_string()),
            ]
        );
        assert_eq!(
            parse_responses_sse_event("{not json"),
            vec![ResponsesSseEvent::Malformed("{not json".to_string())]
        );
    }

    #[test]
    fn retryable_error_classification_matches_policy() {
        assert!(is_retryable_error(&ProviderError::Status {
            code: 429,
            body: "rate limit".into()
        }));
        for code in [500, 502, 503, 504] {
            assert!(is_retryable_error(&ProviderError::Status {
                code,
                body: "temporary server error".into()
            }));
        }
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 402,
            body: "monthly usage limit".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 403,
            body: "subscription required".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 429,
            body: "ChatGPT subscription quota exhausted".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Auth("missing".into())));
    }

    #[test]
    #[ignore = "requires real ChatGPT subscription credentials, CHATGPT_CODEX_ACCESS_TOKEN or ~/.thndrs/auth.json, and network access"]
    fn live_device_code_login_requires_chatgpt_subscription() {
        let code = auth::request_chatgpt_codex_device_code()
            .expect("real ChatGPT subscription prerequisites and network access are required");
        assert!(!code.device_auth_id.is_empty());
        assert!(!code.user_code.is_empty());
    }

    #[test]
    #[ignore = "requires real ChatGPT subscription credentials, browser login, localhost:1455 availability, and network access"]
    fn live_browser_pkce_login_requires_chatgpt_subscription() {
        let credentials = auth::login_chatgpt_codex_with_browser_pkce()
            .expect("real ChatGPT subscription browser login and network access are required");
        assert!(!credentials.access_token.is_empty());
        assert!(!credentials.account_id.is_empty());
    }

    #[test]
    #[ignore = "requires real ChatGPT subscription credentials in CHATGPT_CODEX_ACCESS_TOKEN or ~/.thndrs/auth.json and network access"]
    fn live_text_stream_requires_chatgpt_subscription() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = ChatGptCodexClient::from_env_or_dotenv(&workspace_root)
            .expect("real ChatGPT subscription credentials and network access are required");
        let messages = vec![ProviderMessage::user("Reply with exactly: ok")];
        let mut response = client
            .send_streaming_request(
                "chatgpt-codex/gpt-5.5",
                &messages,
                None,
                ReasoningEffort::default(),
                ReasoningSummary::default(),
                &ProviderContinuation::default(),
            )
            .expect("ChatGPT Codex text-only stream requires subscription credentials");
        let body = response.body_mut().read_to_string().expect("read body");
        assert!(body.contains("data:"));
    }

    #[test]
    #[ignore = "requires real ChatGPT subscription credentials, local tool-call support, and network access"]
    fn live_tool_call_requires_chatgpt_subscription() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = ChatGptCodexClient::from_env_or_dotenv(&workspace_root)
            .expect("real ChatGPT subscription credentials and network access are required");
        let messages = vec![ProviderMessage::user(
            "Use the find_files tool to look for Cargo.toml, then stop.",
        )];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let mut response = client
            .send_streaming_request(
                "chatgpt-codex/gpt-5.5",
                &messages,
                Some(&catalog),
                ReasoningEffort::default(),
                ReasoningSummary::default(),
                &ProviderContinuation::default(),
            )
            .expect("ChatGPT Codex tool-call stream requires subscription credentials");
        let body = response.body_mut().read_to_string().expect("read body");
        assert!(body.contains("function_call") || body.contains("find_files"));
    }

    #[test]
    #[ignore = "requires real ChatGPT subscription credentials and validates private GPT-5.6 ChatGPT Codex backend support"]
    fn live_gpt_5_6_models_support_each_reasoning_effort() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = ChatGptCodexClient::from_env_or_dotenv(&workspace_root)
            .expect("real ChatGPT subscription credentials are required");
        let messages = vec![ProviderMessage::user("Reply with exactly: ok")];

        for model in [
            "chatgpt-codex/gpt-5.6-sol",
            "chatgpt-codex/gpt-5.6-terra",
            "chatgpt-codex/gpt-5.6-luna",
        ] {
            for effort in ReasoningEffort::ALL {
                let mut response = client
                    .send_streaming_request(
                        model,
                        &messages,
                        None,
                        effort,
                        ReasoningSummary::Off,
                        &ProviderContinuation::default(),
                    )
                    .unwrap_or_else(|error| panic!("{model} rejected {}: {error}", effort.label()));
                let body = response.body_mut().read_to_string().expect("read body");
                assert!(body.contains("data:"), "{model} {} did not stream SSE", effort.label());
            }
        }
    }

    #[test]
    #[ignore = "requires expired real ChatGPT subscription credentials in ~/.thndrs/auth.json and network access"]
    fn live_expired_token_refresh_requires_chatgpt_subscription() {
        unsafe {
            env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
        }
        let auth = auth::resolve_chatgpt_codex_auth()
            .expect("expired real ChatGPT subscription credentials and network access are required");
        assert!(!auth.access_token.is_empty());
        assert!(!auth.account_id.is_empty());
    }
}
