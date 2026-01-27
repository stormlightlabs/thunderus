use crate::Provider;
use crate::types::*;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use thunderus_core::Result;
use tokio_stream::Stream;

/// Replay mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    Record,
    Replay,
    Compare,
}

/// Recorded event for replay
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum RecordedEvent {
    Token { text: String },
    ToolCall { name: String, args: serde_json::Value },
    Done,
    Error { message: String },
}

/// Recording entry with request/response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingEntry {
    timestamp: u64,
    request: RecordedRequest,
    response_events: Vec<RecordedEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedRequest {
    pub(crate) messages: Vec<ChatMessage>,
    tools: Option<Vec<ToolSpec>>,
    tool_choice: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

impl From<ChatRequest> for RecordedRequest {
    fn from(req: ChatRequest) -> Self {
        Self {
            messages: req.messages,
            tools: req.tools,
            tool_choice: req.tool_choice,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        }
    }
}

/// Recording file format
#[derive(Debug, Serialize, Deserialize)]
struct RecordingFile {
    mode: String,
    entries: Vec<RecordingEntry>,
}

/// Replay provider for recording and replaying provider interactions
pub struct ReplayProvider {
    recording_path: PathBuf,
    mode: ReplayMode,
    inner_provider: Arc<dyn Provider>,
}

impl ReplayProvider {
    pub fn new(recording_path: PathBuf, mode: ReplayMode, inner_provider: Arc<dyn Provider>) -> Self {
        Self { recording_path, mode, inner_provider }
    }

    fn load_recordings(&self) -> Vec<RecordedEvent> {
        if !self.recording_path.exists() || self.mode == ReplayMode::Record {
            return Vec::new();
        }

        match fs::read_to_string(&self.recording_path) {
            Ok(content) => match serde_json::from_str::<RecordingFile>(&content) {
                Ok(file) => file
                    .entries
                    .into_iter()
                    .flat_map(|entry| entry.response_events)
                    .collect(),
                Err(e) => {
                    tracing::error!("Failed to parse recording file: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                tracing::error!("Failed to read recording file: {}", e);
                Vec::new()
            }
        }
    }

    fn save_recording(&self, request: RecordedRequest, events: Vec<RecordedEvent>) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = RecordingEntry { timestamp, request, response_events: events };
        let mut file = self.read_or_create_recording_file();
        file.entries.push(entry);

        if let Ok(content) = serde_json::to_string_pretty(&file)
            && let Err(e) = fs::write(&self.recording_path, content)
        {
            tracing::error!("Failed to write recording file: {}", e)
        }
    }

    fn read_or_create_recording_file(&self) -> RecordingFile {
        if self.recording_path.exists()
            && let Ok(content) = fs::read_to_string(&self.recording_path)
            && let Ok(file) = serde_json::from_str::<RecordingFile>(&content)
        {
            file
        } else {
            RecordingFile { mode: format!("{:?}", self.mode), entries: Vec::new() }
        }
    }
}

#[async_trait::async_trait]
impl Provider for ReplayProvider {
    async fn stream_chat<'a>(
        &'a self, request: ChatRequest, cancel_token: CancelToken,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'a>>> {
        let recorded_request = RecordedRequest::from(request.clone());

        match self.mode {
            ReplayMode::Record => {
                let inner_stream = self
                    .inner_provider
                    .stream_chat(request.clone(), cancel_token.clone())
                    .await?;

                let stream = async_stream::stream! {
                    let mut events = Vec::new();
                    use tokio_stream::StreamExt;
                    let mut inner_stream = Box::pin(inner_stream);

                    while let Some(event) = inner_stream.next().await {
                        let event_clone = event.clone();
                        match &event_clone {
                            StreamEvent::Token(text) => {
                                events.push(RecordedEvent::Token { text: text.clone() });
                            }
                            StreamEvent::ToolCall(calls) => {
                                for call in calls {
                                    events.push(RecordedEvent::ToolCall {
                                        name: call.function.name.clone(),
                                        args: call.function.arguments.clone(),
                                    });
                                }
                            }
                            StreamEvent::Done => {
                                events.push(RecordedEvent::Done);
                            }
                            StreamEvent::Error(msg) => {
                                events.push(RecordedEvent::Error { message: msg.clone() });
                            }
                        }
                        yield event;
                    }
                    self.save_recording(recorded_request, events);
                };

                Ok(Box::pin(stream))
            }
            ReplayMode::Replay => {
                let recordings = self.load_recordings();

                let stream = async_stream::stream! {
                    for event in recordings {
                        match event {
                            RecordedEvent::Token { text } => {
                                yield StreamEvent::Token(text);
                            }
                            RecordedEvent::ToolCall { name, args } => {
                                let call = ToolCall::new("replay_id", name, args);
                                yield StreamEvent::ToolCall(vec![call]);
                            }
                            RecordedEvent::Done => {
                                yield StreamEvent::Done;
                            }
                            RecordedEvent::Error { message } => {
                                yield StreamEvent::Error(message);
                            }
                        }
                    }
                };

                Ok(Box::pin(stream))
            }
            ReplayMode::Compare => {
                let inner_stream = self
                    .inner_provider
                    .stream_chat(request.clone(), cancel_token.clone())
                    .await?;

                let stream = async_stream::stream! {
                    let mut live_events = Vec::new();
                    use tokio_stream::StreamExt;
                    let mut inner_stream = Box::pin(inner_stream);

                    while let Some(event) = inner_stream.next().await {
                        let event_clone = event.clone();
                        match &event_clone {
                            StreamEvent::Token(text) => {
                                live_events.push(RecordedEvent::Token { text: text.clone() });
                            }
                            StreamEvent::ToolCall(calls) => {
                                for call in calls {
                                    live_events.push(RecordedEvent::ToolCall {
                                        name: call.function.name.clone(),
                                        args: call.function.arguments.clone(),
                                    });
                                }
                            }
                            StreamEvent::Done => {
                                live_events.push(RecordedEvent::Done);
                            }
                            StreamEvent::Error(msg) => {
                                live_events.push(RecordedEvent::Error { message: msg.clone() });
                            }
                        }
                        yield event;
                    }
                    self.save_recording(recorded_request, live_events);
                };

                Ok(Box::pin(stream))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recorded_event_serialization() {
        let event = RecordedEvent::Token { text: "test".to_string() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("token"));
    }

    #[test]
    fn test_recorded_request_from_chat_request() {
        let chat_req = ChatRequest::builder().add_message(ChatMessage::user("Hello")).build();

        let recorded_req = RecordedRequest::from(chat_req);
        assert_eq!(recorded_req.messages.len(), 1);
    }
}
