//! Provider-neutral Anthropic-compatible Messages API helpers.

use crate::app::AgentEvent;
use crate::providers::ProviderMessage;

/// A parsed SSE event from an Anthropic-compatible streaming response.
#[derive(Clone, Debug, PartialEq)]
pub enum SseEvent {
    /// `event: message_start`
    MessageStart,
    /// `event: content_block_start` with content type `thinking`.
    ThinkingStart,
    /// `event: content_block_start` with content type `text`.
    TextStart,
    /// `event: content_block_delta` with a thinking delta.
    ThinkingDelta(String),
    /// `event: content_block_delta` with a text delta.
    TextDelta(String),
    /// `event: content_block_delta` with a partial tool input JSON delta.
    InputJsonDelta { index: usize, partial_json: String },
    /// `event: content_block_stop`
    ContentBlockStop { index: Option<usize> },
    /// `event: message_delta` with stop reason.
    MessageDelta { stop_reason: Option<String> },
    /// `event: message_stop`
    MessageStop,
    /// `event: error`
    Error(String),
    /// An unhandled event type.
    Other(String),
}

impl From<&SseEvent> for Option<AgentEvent> {
    fn from(event: &SseEvent) -> Self {
        match event {
            SseEvent::MessageStart => Some(AgentEvent::Started),
            SseEvent::TextDelta(text) => Some(AgentEvent::AssistantDelta(text.clone())),
            SseEvent::ThinkingDelta(text) => Some(AgentEvent::ReasoningDelta(text.clone())),
            SseEvent::Error(msg) => Some(AgentEvent::Failed(msg.clone())),
            _ => None,
        }
    }
}

/// Build an Anthropic-compatible `POST /messages` request body.
pub fn build_messages_request_body(
    model: &str, messages: &[ProviderMessage], max_tokens: u32, stream: bool, tools: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "stream": stream,
    });
    if let Some(t) = tools
        && !t.as_array().is_some_and(|arr| arr.is_empty())
    {
        body["tools"] = t.clone();
    }
    body
}

/// Parse a single Anthropic-compatible SSE event.
pub fn parse_sse_event(event_type: &str, data: &str) -> SseEvent {
    match event_type {
        "message_start" => SseEvent::MessageStart,
        "content_block_start" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let content_type = v
                .get("content_block")
                .and_then(|cb| cb.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match content_type {
                "thinking" => SseEvent::ThinkingStart,
                "text" => SseEvent::TextStart,
                _ => SseEvent::Other(format!("content_block_start: {content_type}")),
            }
        }
        "content_block_delta" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let delta_type = v
                .get("delta")
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match delta_type {
                "thinking_delta" => {
                    let text = v
                        .get("delta")
                        .and_then(|d| d.get("thinking"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    SseEvent::ThinkingDelta(text)
                }
                "text_delta" => {
                    let text = v
                        .get("delta")
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .replace("<think>", "")
                        .replace("</think>", "");
                    SseEvent::TextDelta(text)
                }
                "input_json_delta" => {
                    let partial_json = v
                        .get("delta")
                        .and_then(|d| d.get("partial_json"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    SseEvent::InputJsonDelta { index, partial_json }
                }
                _ => SseEvent::Other(format!("content_block_delta: {delta_type}")),
            }
        }
        "content_block_stop" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            SseEvent::ContentBlockStop { index: v.get("index").and_then(|i| i.as_u64()).map(|i| i as usize) }
        }
        "message_delta" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let stop_reason = v
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            SseEvent::MessageDelta { stop_reason }
        }
        "message_stop" => SseEvent::MessageStop,
        "error" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            SseEvent::Error(msg)
        }
        other => SseEvent::Other(other.to_string()),
    }
}

/// Convenience wrapper for `Option<AgentEvent>::from(&sse_event)`.
pub fn sse_to_agent_event(event: &SseEvent) -> Option<AgentEvent> {
    event.into()
}

/// Parse a raw Anthropic-compatible SSE chunk into `(event_type, data)` tuples.
pub fn parse_sse_chunk(chunk: &str) -> Vec<(String, String)> {
    let mut events = Vec::new();
    let mut current_event = String::new();
    let mut current_data = String::new();

    for line in chunk.lines() {
        if line.is_empty() {
            if !current_event.is_empty() {
                events.push((current_event.clone(), current_data.clone()));
            }
            current_event.clear();
            current_data.clear();
            continue;
        }
        if let Some(rest) = line.strip_prefix("event: ") {
            current_event = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("data: ") {
            if current_data.is_empty() {
                current_data = rest.to_string();
            } else {
                current_data.push('\n');
                current_data.push_str(rest);
            }
        }
    }

    if !current_event.is_empty() {
        events.push((current_event, current_data));
    }

    events
}
