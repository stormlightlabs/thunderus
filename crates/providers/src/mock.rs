use crate::Provider;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use thunderus_core::Result;
use tokio_stream::Stream;

/// Mock response types for deterministic testing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MockResponse {
    Text { content: String },
    ToolCall { name: String, args: serde_json::Value },
    Error { message: String },
    Sequence { events: Vec<MockEvent> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum MockEvent {
    Token { text: String },
    ToolCall { name: String, args: serde_json::Value },
    Done,
}

/// Mock configuration from TOML file
#[derive(Debug, Deserialize)]
struct MockConfig {
    responses: Vec<MockResponse>,
}

/// Mock provider for deterministic testing without API calls
pub struct MockProvider {
    responses: Vec<MockResponse>,
    current: Arc<AtomicUsize>,
}

impl MockProvider {
    pub fn new(responses_file: Option<String>) -> Self {
        let responses = if let Some(path) = responses_file {
            Self::load_responses(&path)
        } else {
            vec![MockResponse::Text { content: "Mock response - configure responses_file in config".to_string() }]
        };

        Self { responses, current: Arc::new(AtomicUsize::new(0)) }
    }

    fn load_responses(path: &str) -> Vec<MockResponse> {
        let config_path = Path::new(path);
        if !config_path.exists() {
            tracing::warn!("Mock responses file not found: {}", path);
            return vec![MockResponse::Text { content: format!("Mock responses file not found: {}", path) }];
        }

        match fs::read_to_string(config_path) {
            Ok(content) => match toml::from_str::<MockConfig>(&content) {
                Ok(config) => config.responses,
                Err(e) => {
                    tracing::error!("Failed to parse mock responses: {}", e);
                    vec![MockResponse::Error { message: format!("Failed to parse mock responses: {}", e) }]
                }
            },
            Err(e) => {
                tracing::error!("Failed to read mock responses file: {}", e);
                vec![MockResponse::Error { message: format!("Failed to read mock responses file: {}", e) }]
            }
        }
    }

    fn get_next_response(&self) -> MockResponse {
        let index = self.current.fetch_add(1, Ordering::SeqCst);
        if index < self.responses.len() {
            self.responses[index].clone()
        } else {
            MockResponse::Text {
                content: format!(
                    "No more mock responses configured (requested: {}, available: {})",
                    index + 1,
                    self.responses.len()
                ),
            }
        }
    }
}

#[async_trait::async_trait]
impl Provider for MockProvider {
    async fn stream_chat<'a>(
        &'a self, _request: ChatRequest, _cancel_token: CancelToken,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'a>>> {
        let response = self.get_next_response();

        let stream = async_stream::stream! {
            match response {
                MockResponse::Text { content } => {
                    yield StreamEvent::Token(content);
                }
                MockResponse::ToolCall { name, args } => {
                    let call = ToolCall::new("mock_id", name, args);
                    yield StreamEvent::ToolCall(vec![call]);
                }
                MockResponse::Error { message } => {
                    yield StreamEvent::Error(message);
                }
                MockResponse::Sequence { events } => {
                    for event in events {
                        match event {
                            MockEvent::Token { text } => {
                                yield StreamEvent::Token(text);
                            }
                            MockEvent::ToolCall { name, args } => {
                                let call = ToolCall::new("mock_id", name, args);
                                yield StreamEvent::ToolCall(vec![call]);
                            }
                            MockEvent::Done => {
                                yield StreamEvent::Done;
                                return;
                            }
                        }
                    }
                }
            }
            yield StreamEvent::Done;
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use crate::RecordedRequest;

    use super::*;

    #[test]
    fn test_mock_provider_creation() {
        let provider = MockProvider::new(None);
        assert!(!provider.responses.is_empty());
    }

    #[test]
    fn test_mock_response_parsing() {
        let toml = r#"
[[responses]]
type = "text"
content = "Hello, world!"

[[responses]]
type = "toolcall"
name = "grep"
args = { pattern = "test" }

[[responses]]
type = "sequence"
events = [
    { event = "token", text = "Partial" },
    { event = "done" }
]
"#;

        let config: MockConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.responses.len(), 3);
        assert!(matches!(config.responses[0], MockResponse::Text { .. }));
        assert!(matches!(config.responses[1], MockResponse::ToolCall { .. }));
        assert!(matches!(config.responses[2], MockResponse::Sequence { .. }));
    }

    #[test]
    fn test_recorded_request_from_chat_request() {
        let chat_req = ChatRequest::builder().add_message(ChatMessage::user("Hello")).build();

        let recorded_req = RecordedRequest::from(chat_req);
        assert_eq!(recorded_req.messages.len(), 1);
    }

    #[test]
    fn test_mock_event_parsing() {
        let toml = r#"
[[responses]]
type = "sequence"
events = [
    { event = "token", text = "Test" },
    { event = "toolcall", name = "read", args = { file_path = "/tmp/file" } },
    { event = "done" }
]
"#;

        let config: MockConfig = toml::from_str(toml).unwrap();
        if let MockResponse::Sequence { events } = &config.responses[0] {
            assert_eq!(events.len(), 3);
            assert!(matches!(events[0], MockEvent::Token { .. }));
            assert!(matches!(events[1], MockEvent::ToolCall { .. }));
            assert!(matches!(events[2], MockEvent::Done));
        } else {
            panic!("Expected Sequence response");
        }
    }
}
