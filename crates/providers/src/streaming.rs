//! Streaming support for OpenAI-compatible providers
//!
//! Handles SSE parsing and delta reassembly for streaming responses.

use super::Result;
use serde::Deserialize;
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
    #[serde(default)]
    pub usage: Option<StreamUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StreamDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Delta {
        content: Option<String>,
        reasoning_content: Option<String>,
    },
    Done {
        usage: Option<StreamUsage>,
    },
    Error(String),
}

pub struct StreamResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<StreamUsage>,
    pub model: String,
}

pub async fn collect_stream(response: reqwest::Response) -> Result<StreamResponse> {
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut finish_reason = None;
    let mut usage = None;
    let mut model = String::new();

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        let text = String::from_utf8_lossy(&chunk);

        for line in text.lines() {
            let line = line.trim();

            if line.is_empty() || !line.starts_with("data: ") {
                continue;
            }

            let data = &line[6..];

            if data == "[DONE]" {
                break;
            }

            match serde_json::from_str::<StreamChunk>(data) {
                Ok(chunk) => {
                    model = chunk.model;

                    if let Some(choice) = chunk.choices.first() {
                        if let Some(c) = &choice.delta.content {
                            content.push_str(c);
                        }
                        if let Some(rc) = &choice.delta.reasoning_content {
                            reasoning_content.push_str(rc);
                        }
                        if choice.finish_reason.is_some() {
                            finish_reason = choice.finish_reason.clone();
                        }
                    }

                    if let Some(u) = chunk.usage {
                        usage = Some(u);
                    }
                }
                Err(e) => tracing::debug!("Failed to parse stream chunk: {} - data: {}", e, data),
            }
        }
    }

    Ok(StreamResponse {
        content,
        reasoning_content: if reasoning_content.is_empty() { None } else { Some(reasoning_content) },
        finish_reason,
        usage,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stream_chunk() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "Hello"
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.id, "chatcmpl-123");
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_parse_stream_chunk_with_reasoning() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "kimi-k2.5",
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "The answer",
                    "reasoning_content": "Let me think..."
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content, Some("The answer".to_string()));
        assert_eq!(
            chunk.choices[0].delta.reasoning_content,
            Some("Let me think...".to_string())
        );
    }

    #[test]
    fn test_parse_stream_done() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].finish_reason, Some("stop".to_string()));
        assert!(chunk.usage.is_some());
    }
}
