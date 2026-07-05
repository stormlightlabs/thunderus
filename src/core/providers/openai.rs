//! Provider-neutral OpenAI-compatible chat-completions helpers.

use crate::providers::{ProviderContentBlock, ProviderMessage, ProviderMessageContent};

/// Parsed OpenAI-compatible chat-completions stream event.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatSseEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart { index: usize, id: String, name: String },
    ToolCallArgumentsDelta { index: usize, arguments: String },
    FinishReason(String),
    Usage { input_tokens: u64, output_tokens: u64 },
    Done,
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
        .filter_map(|line| line.strip_prefix("data: ").map(str::to_string))
        .collect()
}

/// Parse one OpenAI-compatible SSE `data:` payload.
pub fn parse_chat_sse_event(data: &str) -> Vec<ChatSseEvent> {
    if data.trim() == "[DONE]" {
        return vec![ChatSseEvent::Done];
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return vec![ChatSseEvent::Other];
    };

    let mut events = Vec::new();
    if let Some((input_tokens, output_tokens)) = extract_chat_usage(&value) {
        events.push(ChatSseEvent::Usage { input_tokens, output_tokens });
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

fn extract_chat_usage(value: &serde_json::Value) -> Option<(u64, u64)> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("normalizedUsage"))
        .filter(|usage| !usage.is_null())?;
    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("inputTokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("outputTokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if input_tokens == 0 && output_tokens == 0 { None } else { Some((input_tokens, output_tokens)) }
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
        ProviderMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ProviderContentBlock::Text { text } => Some(serde_json::json!({"role": message.role, "content": text})),
                ProviderContentBlock::ToolResult { tool_use_id, content, .. } => Some(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": content,
                })),
                ProviderContentBlock::ToolUse { .. } => None,
            })
            .collect(),
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
}
