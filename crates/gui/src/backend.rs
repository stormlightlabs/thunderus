use super::model::BackendEvent;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use thndrs_core::Config;
use thndrs_providers::{ConversationEvent, ConversationLoop, create_provider};

const STREAM_CHUNK_BYTES: usize = 24;

#[derive(Debug)]
pub struct BackendBridge {
    pub request_tx: Sender<String>,
    pub event_rx: Receiver<BackendEvent>,
}

pub fn spawn_backend(workspace_root: PathBuf) -> BackendBridge {
    let (request_tx, request_rx) = mpsc::channel::<String>();
    let (event_tx, event_rx) = mpsc::channel::<BackendEvent>();
    tracing::info!("Spawning GUI backend worker workspace={}", workspace_root.display());

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::error!("Failed to create GUI worker runtime: {error}");
                let _ = event_tx.send(BackendEvent::Error(format!(
                    "Failed to create async runtime: {}",
                    error
                )));
                return;
            }
        };

        let config = Config::load_default().unwrap_or_default();
        let provider_name = config.default_provider.clone();
        tracing::info!("Initializing provider provider={provider_name}");
        let provider = match create_provider(&provider_name, &config) {
            Ok(provider) => Some(provider),
            Err(error) => {
                tracing::error!("Provider initialization failed provider={provider_name} error={error}");
                let _ = event_tx.send(BackendEvent::Error(format!(
                    "Provider initialization failed: {}",
                    error
                )));
                None
            }
        };

        let mut loop_handler = ConversationLoop::new(workspace_root);

        while let Ok(prompt) = request_rx.recv() {
            let prompt = prompt.trim().to_string();
            if prompt.is_empty() {
                continue;
            }
            tracing::info!("Dispatching GUI prompt chars={}", prompt.chars().count());

            let Some(provider) = provider.as_deref() else {
                let _ = event_tx.send(BackendEvent::ContentDelta(
                    "Provider is unavailable. Configure ~/.thunderus/config.toml with a valid API key.".to_string(),
                ));
                let _ = event_tx.send(BackendEvent::ContentDone { model: "offline".to_string() });
                continue;
            };

            let _ = event_tx.send(BackendEvent::Thinking(
                "Running provider loop with tool orchestration".to_string(),
            ));

            let mut final_content: Option<(String, String)> = None;
            let result = runtime.block_on(loop_handler.process_message(provider, &prompt, |event| match event {
                ConversationEvent::Thinking(thinking) => {
                    tracing::debug!("Provider thinking: {thinking}");
                    let _ = event_tx.send(BackendEvent::Thinking(thinking));
                }
                ConversationEvent::ToolCalling { id, name, arguments } => {
                    tracing::info!("Tool calling id={id} name={name}");
                    let _ = event_tx.send(BackendEvent::ToolCalling { id, name, arguments });
                }
                ConversationEvent::ToolCompleted { id, name, result, is_error } => {
                    tracing::info!("Tool completed id={id} name={name} error={is_error}");
                    let _ = event_tx.send(BackendEvent::ToolCompleted { id, name, result, is_error });
                }
                ConversationEvent::Content { content, reasoning_content: _reasoning_content, usage: _usage, model } => {
                    tracing::info!("Provider completed model={model} chars={}", content.chars().count());
                    final_content = Some((content, model));
                }
                ConversationEvent::Error(error) => {
                    tracing::error!("Conversation loop event error: {error}");
                    let _ = event_tx.send(BackendEvent::Error(error));
                }
            }));

            match result {
                Ok(_) => {
                    if let Some((content, model)) = final_content {
                        for chunk in chunk_text(&content, STREAM_CHUNK_BYTES) {
                            let _ = event_tx.send(BackendEvent::ContentDelta(chunk));
                            runtime.block_on(async {
                                tokio::time::sleep(Duration::from_millis(14)).await;
                            });
                        }
                        let _ = event_tx.send(BackendEvent::ContentDone { model });
                        tracing::info!("GUI prompt completed");
                    } else {
                        tracing::error!("Provider finished without content");
                        let _ = event_tx.send(BackendEvent::Error("Provider finished without content".to_string()));
                    }
                }
                Err(error) => {
                    tracing::error!("Conversation processing error: {error}");
                    let _ = event_tx.send(BackendEvent::Error(error));
                }
            }
        }

        tracing::info!("GUI backend worker channel closed");
    });

    BackendBridge { request_tx, event_rx }
}

pub fn chunk_text(input: &str, chunk_size: usize) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut output = Vec::new();
    let mut buffer = String::new();

    for ch in input.chars() {
        buffer.push(ch);
        if buffer.len() >= chunk_size {
            output.push(std::mem::take(&mut buffer));
        }
    }

    if !buffer.is_empty() {
        output.push(buffer);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_preserves_input_order() {
        let chunks = chunk_text("abcdefghi", 4);
        assert_eq!(chunks, vec!["abcd", "efgh", "i"]);
    }
}
