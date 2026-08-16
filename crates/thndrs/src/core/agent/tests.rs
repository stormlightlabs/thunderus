
use super::*;
use crate::app::ToolStatus;
use crate::cli::WebSearchMode;
use crate::providers;
use crate::tools::{self, AgentRunConfig};
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
    assert!(
        !events
            .iter()
            .any(|event| { matches!(event, AgentEvent::Status(message) if message.contains("fallback token budget")) })
    );
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
fn fake_stream_tool_ids_are_scoped_to_the_turn() {
    let mut first = config();
    first.accounting_turn_id = Some("turn-1".to_string());
    let mut second = config();
    second.accounting_turn_id = Some("turn-2".to_string());

    assert_ne!(fake_tool_id(&first, "0"), fake_tool_id(&second, "0"));
    assert_eq!(fake_tool_id(&first, "search-0"), "turn-1-search-0");
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
    let sse = include_str!("../providers/fixtures/tool_use_turn.sse");
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

#[test]
fn edit_like_request_can_finish_with_a_clarifying_response() {
    assert!(!stopped_without_expected_write(
        "Should I build on the existing changes?",
        true,
        false
    ));
    assert!(stopped_without_expected_write("", true, false));
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
fn mcp_tool_uses_permission_hook() {
    assert_eq!(
        approve_request("mcp__docs__search", ToolPermissionDecision::Reject),
        ToolPermissionDecision::Reject
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
fn prompt_expects_workspace_write_ignores_explicit_read_only_instructions() {
    assert!(!prompt_expects_workspace_write(
        "Review the changes and tell me what should be shortened. Do not edit files."
    ));
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
