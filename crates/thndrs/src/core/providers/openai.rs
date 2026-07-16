//! Provider-neutral OpenAI-compatible chat-completions helpers.

use crate::providers::{ProviderContentBlock, ProviderImageSource, ProviderMessage, ProviderMessageContent};
use thndrs_agent::ProviderUsageComponents;

/// Parsed OpenAI-compatible chat-completions stream event.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatSseEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart { index: usize, id: String, name: String },
    ToolCallArgumentsDelta { index: usize, arguments: String },
    FinishReason(String),
    ResponseStatus(String),
    Error(String),
    Usage { input_tokens: u64, output_tokens: u64 },
    UsageComponents(ProviderUsageComponents),
    Done,
    Malformed(String),
    Other,
}

/// Build an OpenAI-compatible `POST /chat/completions` request body.
pub fn build_chat_request_body(
    model: &str, messages: &[ProviderMessage], max_tokens: u32, stream: bool, tools: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": openai_messages(messages),
        "max_tokens": max_tokens,
        "stream": stream,
    });
    if let Some(t) = tools
        && !t.as_array().is_some_and(|arr| arr.is_empty())
    {
        body["tools"] = openai_tools(t);
    }
    body
}

/// Parse raw OpenAI-compatible SSE data lines.
pub fn parse_chat_sse_chunk(chunk: &str) -> Vec<String> {
    chunk
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(|data| data.trim_start().to_string()))
        .collect()
}

/// Parse one OpenAI-compatible SSE `data:` payload.
pub fn parse_chat_sse_event(data: &str) -> Vec<ChatSseEvent> {
    if data.trim() == "[DONE]" {
        return vec![ChatSseEvent::Done];
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return vec![ChatSseEvent::Malformed(data.chars().take(120).collect())];
    };

    let mut events = Vec::new();
    if let Some(error) = extract_chat_error(&value) {
        events.push(ChatSseEvent::Error(error));
    }
    if let Some(status) = extract_chat_status(&value) {
        events.push(ChatSseEvent::ResponseStatus(status));
    }
    if let Some(usage) = extract_chat_usage(&value) {
        if usage.cache_read_input_tokens.is_some()
            || usage.cache_creation_input_tokens.is_some()
            || usage.reasoning_tokens.is_some()
        {
            events.push(ChatSseEvent::UsageComponents(usage));
        } else if let (Some(input_tokens), Some(output_tokens)) = (usage.input_tokens, usage.output_tokens) {
            events.push(ChatSseEvent::Usage { input_tokens, output_tokens });
        } else {
            events.push(ChatSseEvent::UsageComponents(usage));
        }
    }

    let Some(choices) = value.get("choices").and_then(|v| v.as_array()) else {
        return if events.is_empty() { vec![ChatSseEvent::Other] } else { events };
    };

    for choice in choices {
        if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str())
            && !reason.is_empty()
        {
            events.push(ChatSseEvent::FinishReason(reason.to_string()));
        }

        let Some(delta) = choice.get("delta") else {
            continue;
        };

        if let Some(text) = delta.get("content").and_then(|v| v.as_str())
            && !text.is_empty()
        {
            events.push(ChatSseEvent::TextDelta(text.to_string()));
        }
        if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str())
            && !reasoning.is_empty()
        {
            events.push(ChatSseEvent::ReasoningDelta(reasoning.to_string()));
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for call in tool_calls {
                let index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                    let name = call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    events.push(ChatSseEvent::ToolCallStart { index, id: id.to_string(), name });
                }
                if let Some(arguments) = call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    && !arguments.is_empty()
                {
                    events.push(ChatSseEvent::ToolCallArgumentsDelta { index, arguments: arguments.to_string() });
                }
            }
        }
    }

    if events.is_empty() { vec![ChatSseEvent::Other] } else { events }
}

fn extract_chat_error(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| match error {
            serde_json::Value::String(message) => Some(message.clone()),
            serde_json::Value::Object(_) => error
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
                .or_else(|| error.get("code").and_then(|code| code.as_str()).map(str::to_string)),
            _ => None,
        })
        .or_else(|| {
            if value.get("type").and_then(|event_type| event_type.as_str()) == Some("error") {
                value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .map(str::to_string)
            } else {
                None
            }
        })
}

fn extract_chat_status(value: &serde_json::Value) -> Option<String> {
    value
        .get("status")
        .or_else(|| value.get("response").and_then(|response| response.get("status")))
        .and_then(|status| status.as_str())
        .filter(|status| {
            matches!(
                *status,
                "completed" | "failed" | "cancelled" | "canceled" | "queued" | "in_progress"
            )
        })
        .map(str::to_string)
}

fn extract_chat_usage(value: &serde_json::Value) -> Option<ProviderUsageComponents> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("normalizedUsage"))
        .filter(|usage| !usage.is_null())?;
    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("inputTokens"))
        .and_then(|v| v.as_u64());
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("outputTokens"))
        .and_then(|v| v.as_u64());
    if input_tokens.is_none() && output_tokens.is_none() {
        return None;
    }
    let details = usage
        .get("prompt_tokens_details")
        .or_else(|| usage.get("input_tokens_details"));
    Some(ProviderUsageComponents {
        input_tokens,
        output_tokens,
        cache_read_input_tokens: details
            .and_then(|details| details.get("cached_tokens"))
            .and_then(|value| value.as_u64()),
        cache_creation_input_tokens: None,
        reasoning_tokens: usage
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(|value| value.as_u64()),
    })
}

fn openai_messages(messages: &[ProviderMessage]) -> serde_json::Value {
    serde_json::Value::Array(messages.iter().flat_map(openai_messages_for_provider_message).collect())
}

fn openai_messages_for_provider_message(message: &ProviderMessage) -> Vec<serde_json::Value> {
    match &message.content {
        ProviderMessageContent::Text(text) => {
            vec![serde_json::json!({"role": message.role, "content": text})]
        }
        ProviderMessageContent::Blocks(blocks) if message.role == "assistant" => {
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            for block in blocks {
                match block {
                    ProviderContentBlock::Text { text: block_text } => text.push_str(block_text),
                    ProviderContentBlock::Image { .. } => {}
                    ProviderContentBlock::ToolUse { id, name, input } => {
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string()),
                            }
                        }));
                    }
                    ProviderContentBlock::ToolResult { .. } => {}
                }
            }
            let mut msg = serde_json::json!({
                "role": "assistant",
                "content": text,
            });
            if !tool_calls.is_empty() {
                msg["tool_calls"] = serde_json::Value::Array(tool_calls);
            }
            vec![msg]
        }
        ProviderMessageContent::Blocks(blocks) if message.role == "user" && blocks.iter().any(is_user_media_block) => {
            vec![serde_json::json!({"role": message.role, "content": openai_user_content(blocks)})]
        }
        ProviderMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ProviderContentBlock::Text { text } => Some(serde_json::json!({"role": message.role, "content": text})),
                ProviderContentBlock::ToolResult { tool_use_id, content, .. } => Some(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": content,
                })),
                ProviderContentBlock::ToolUse { .. } | ProviderContentBlock::Image { .. } => None,
            })
            .collect(),
    }
}

fn is_user_media_block(block: &ProviderContentBlock) -> bool {
    matches!(block, ProviderContentBlock::Image { .. })
}

fn openai_user_content(blocks: &[ProviderContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ProviderContentBlock::Text { text } => Some(serde_json::json!({"type": "text", "text": text})),
            ProviderContentBlock::Image { source } => Some(openai_image_content(source)),
            ProviderContentBlock::ToolUse { .. } | ProviderContentBlock::ToolResult { .. } => None,
        })
        .collect()
}

fn openai_image_content(source: &ProviderImageSource) -> serde_json::Value {
    match source {
        ProviderImageSource::Base64 { media_type, data } => serde_json::json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{media_type};base64,{data}"),
            },
        }),
    }
}

fn openai_tools(tools: &serde_json::Value) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                let name = tool.get("name")?;
                let description = tool.get("description")?;
                let input_schema = tool.get("input_schema")?;
                Some(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description,
                        "parameters": input_schema,
                    }
                }))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{providers::ProviderMessage, tools};

    #[test]
    fn build_chat_request_body_converts_registry_tool_catalog() {
        let messages = vec![ProviderMessage::user("inspect the project")];
        let defs = tools::tool_definitions();
        let catalog = tools::tool_catalog_schemas(&defs);
        let body = build_chat_request_body("test-model", &messages, 4096, true, Some(&catalog));

        let request_names: Vec<&str> = body["tools"]
            .as_array()
            .expect("tools should be present")
            .iter()
            .map(|tool| tool["function"]["name"].as_str().expect("tool name"))
            .collect();
        let definition_names: Vec<&str> = defs.iter().map(|definition| definition.name.as_ref()).collect();

        assert_eq!(request_names, definition_names);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["parameters"], defs[0].input_schema);
    }

    #[test]
    fn parse_chat_sse_chunk_accepts_optional_space_after_data_colon() {
        assert_eq!(
            parse_chat_sse_chunk("data:{\"a\":1}\ndata: {\"b\":2}\n"),
            vec![r#"{"a":1}"#.to_string(), r#"{"b":2}"#.to_string()]
        );
    }

    #[test]
    fn parse_chat_sse_event_reports_malformed_payloads() {
        assert_eq!(
            parse_chat_sse_event("{not json"),
            vec![ChatSseEvent::Malformed("{not json".to_string())]
        );
    }

    #[test]
    fn parse_chat_sse_event_extracts_backend_errors() {
        assert_eq!(
            parse_chat_sse_event(r#"{"error":{"message":"quota exceeded"}}"#),
            vec![ChatSseEvent::Error("quota exceeded".to_string())]
        );
        assert_eq!(
            parse_chat_sse_event(r#"{"type":"error","message":"backend failed"}"#),
            vec![ChatSseEvent::Error("backend failed".to_string())]
        );
    }

    #[test]
    fn parse_chat_sse_event_extracts_response_statuses() {
        for status in ["completed", "failed", "cancelled", "queued", "in_progress"] {
            assert_eq!(
                parse_chat_sse_event(&format!(r#"{{"status":"{status}"}}"#)),
                vec![ChatSseEvent::ResponseStatus(status.to_string())]
            );
        }
    }

    #[test]
    fn parse_chat_sse_event_retains_cache_and_reasoning_components() {
        let events = parse_chat_sse_event(
            r#"{"usage":{"prompt_tokens":100,"completion_tokens":12,"prompt_tokens_details":{"cached_tokens":40},"completion_tokens_details":{"reasoning_tokens":5}}}"#,
        );
        assert!(events.contains(&ChatSseEvent::UsageComponents(ProviderUsageComponents {
            input_tokens: Some(100),
            output_tokens: Some(12),
            cache_read_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            reasoning_tokens: Some(5),
        })));
    }
}
