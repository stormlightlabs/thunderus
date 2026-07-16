//! Tests for ACP server primitives.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use crate::cli::{ReasoningEffort, ReasoningSummary, WebSearchMode};
use crate::harness::{HarnessHandle, HarnessTurn};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AudioContent, ClientCapabilities, ContentBlock, EnvVariable, ImageContent, Implementation, InitializeRequest,
    McpServer, McpServerHttp, McpServerStdio, PromptRequest, ResourceLink, SetSessionConfigOptionRequest, StopReason,
    TextContent,
};
use tempfile::tempdir;

use super::config_options::{
    ConfigOptionValue, MODEL_CONFIG_OPTION_ID, REASONING_EFFORT_CONFIG_OPTION_ID, REASONING_SUMMARY_CONFIG_OPTION_ID,
    WEBSEARCH_CONFIG_OPTION_ID, initial_config_option_ids, initial_config_options, validate_config_option,
};
use super::events::{SessionUpdateIntent, ToolCallKind, ToolStatusIntent, classify_tool, map_agent_event};
use super::handlers::{ServerState, acp_mcp_config, execute_prompt, initialize, set_config_option};
use super::session::{
    AcpSessionError, AcpSessionStore, LocalSessionMetadata, generate_session_id, validate_and_normalize_cwd,
};

use crate::app::{AgentEvent, ToolStatus};
use crate::providers::{ProviderContentBlock, ProviderMessageContent};
use crate::server::ServerConfig;
use crate::session::{SessionReader, SessionRecord};
use serde_json::Value;
use thndrs_agent::CancelToken;

#[test]
fn maps_assistant_deltas() {
    assert_eq!(
        map_agent_event(&AgentEvent::AssistantDelta("hello".to_string())),
        vec![SessionUpdateIntent::AssistantDelta("hello".to_string())]
    );
}

#[test]
fn maps_reasoning_deltas() {
    assert_eq!(
        map_agent_event(&AgentEvent::ReasoningDelta("reasoning".to_string())),
        vec![SessionUpdateIntent::ReasoningDelta("reasoning".to_string())]
    );
}

#[test]
fn maps_status_events() {
    assert_eq!(
        map_agent_event(&AgentEvent::Status("busy".to_string())),
        vec![SessionUpdateIntent::Status("busy".to_string())]
    );
}

#[test]
fn maps_usage_events() {
    assert_eq!(
        map_agent_event(&AgentEvent::Usage { input_tokens: 12, output_tokens: 34 }),
        vec![SessionUpdateIntent::Usage { input_tokens: 12, output_tokens: 34 }]
    );
}

#[test]
fn maps_tool_started_events() {
    assert_eq!(
        map_agent_event(&AgentEvent::ToolStarted {
            id: "tool-1".to_string(),
            name: "read".to_string(),
            arguments: "{}".to_string(),
        }),
        vec![SessionUpdateIntent::ToolStarted {
            id: "tool-1".to_string(),
            name: "read".to_string(),
            arguments: "{}".to_string(),
            kind: ToolCallKind::Read,
            locations: vec![],
        }]
    );
}

#[test]
fn maps_tool_finished_with_ok_status() {
    assert_eq!(
        map_agent_event(&AgentEvent::ToolFinished {
            id: "tool-ok".to_string(),
            output: vec!["ok".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        }),
        vec![SessionUpdateIntent::ToolFinished {
            id: "tool-ok".to_string(),
            status: ToolStatusIntent::Completed,
            output: vec!["ok".to_string()],
            kind: ToolCallKind::Other,
            locations: vec![],
        }]
    );
}

#[test]
fn maps_tool_finished_with_running_status() {
    assert_eq!(
        map_agent_event(&AgentEvent::ToolFinished {
            id: "tool-running".to_string(),
            output: vec!["running".to_string()],
            status: ToolStatus::Running,
            write_result: None,
            shell_result: None,
        }),
        vec![SessionUpdateIntent::ToolFinished {
            id: "tool-running".to_string(),
            status: ToolStatusIntent::InProgress,
            output: vec!["running".to_string()],
            kind: ToolCallKind::Other,
            locations: vec![],
        }]
    );
}

#[test]
fn maps_tool_finished_with_failed_and_cancelled_status() {
    assert_eq!(
        map_agent_event(&AgentEvent::ToolFinished {
            id: "tool-fail".to_string(),
            output: vec!["failed".to_string()],
            status: ToolStatus::Failed,
            write_result: None,
            shell_result: None,
        }),
        vec![SessionUpdateIntent::ToolFinished {
            id: "tool-fail".to_string(),
            status: ToolStatusIntent::Failed,
            output: vec!["failed".to_string()],
            kind: ToolCallKind::Other,
            locations: vec![],
        }]
    );

    assert_eq!(
        map_agent_event(&AgentEvent::ToolFinished {
            id: "tool-cancel".to_string(),
            output: vec!["cancelled".to_string()],
            status: ToolStatus::Cancelled,
            write_result: None,
            shell_result: None,
        }),
        vec![SessionUpdateIntent::ToolFinished {
            id: "tool-cancel".to_string(),
            status: ToolStatusIntent::Failed,
            output: vec!["cancelled".to_string()],
            kind: ToolCallKind::Other,
            locations: vec![],
        }]
    );
}

#[test]
fn maps_failed_cancelled_and_finished_events() {
    assert_eq!(
        map_agent_event(&AgentEvent::Failed("boom".to_string())),
        vec![SessionUpdateIntent::Failed("boom".to_string())]
    );
    assert_eq!(
        map_agent_event(&AgentEvent::Cancelled),
        vec![SessionUpdateIntent::Cancelled]
    );
    assert_eq!(
        map_agent_event(&AgentEvent::Finished),
        vec![SessionUpdateIntent::Finished]
    );
}

#[test]
fn maps_tool_started_redacts_sensitive_arguments() {
    let intents = map_agent_event(&AgentEvent::ToolStarted {
        id: "tool-redact".to_string(),
        name: "run_shell".to_string(),
        arguments: "{\"password\":\"hunter2\",\"path\":\"/tmp/workspace/file.txt\"}".to_string(),
    });

    assert_eq!(intents.len(), 1);
    match &intents[..] {
        [SessionUpdateIntent::ToolStarted { arguments, kind, .. }] => {
            assert_eq!(*kind, ToolCallKind::Execute);
            assert!(arguments.contains("[REDACTED]"));
            assert!(!arguments.contains("hunter2"));
            assert!(arguments.contains("/tmp/workspace/file.txt"));
        }
        _ => panic!("expected tool-started intent"),
    }
}

#[test]
fn maps_tool_started_redacts_nested_json_secret_keys() {
    let intents = map_agent_event(&AgentEvent::ToolStarted {
        id: "tool-redact-nested".to_string(),
        name: "run_shell".to_string(),
        arguments: "{\"options\":{\"env\":{\"secret\":\"s3cr3t\",\"token\":\"abc123\"}}}".to_string(),
    });

    match &intents[..] {
        [SessionUpdateIntent::ToolStarted { arguments, .. }] => {
            assert!(arguments.contains("[REDACTED]"));
            assert!(!arguments.contains("s3cr3t"));
            assert!(!arguments.contains("abc123"));
        }
        _ => panic!("expected tool-started intent"),
    }
}

#[test]
fn maps_tool_started_infers_location_from_arguments() {
    let intents = map_agent_event(&AgentEvent::ToolStarted {
        id: "tool-loc".to_string(),
        name: "read_file_range".to_string(),
        arguments: "{\"path\":\"/tmp/notes.md\",\"start_line\":42}".to_string(),
    });

    assert_eq!(intents.len(), 1);
    match &intents[..] {
        [SessionUpdateIntent::ToolStarted { arguments, kind, locations, .. }] => {
            assert_eq!(*kind, ToolCallKind::Read);
            assert!(!locations.is_empty());
            assert!(arguments.contains("/tmp/notes.md"));
        }
        _ => panic!("expected tool-started intent"),
    }
}

#[test]
fn caps_and_redacts_tool_output() {
    let mut long = String::new();
    long.push('x');
    long.push_str(&"y".repeat(5000));

    let intents = map_agent_event(&AgentEvent::ToolFinished {
        id: "tool-long".to_string(),
        output: vec![long],
        status: ToolStatus::Ok,
        write_result: None,
        shell_result: None,
    });

    assert_eq!(intents.len(), 1);
    match &intents[..] {
        [SessionUpdateIntent::ToolFinished { output, .. }] => {
            assert_eq!(output.len(), 1);
            assert!(output[0].ends_with("...[truncated]"));
        }
        _ => panic!("expected tool-finished intent"),
    }
}

#[test]
fn classifies_tool_kinds_for_m8() {
    assert_eq!(classify_tool("find_files"), ToolCallKind::Read);
    assert_eq!(classify_tool("search_text"), ToolCallKind::Search);
    assert_eq!(classify_tool("create_file"), ToolCallKind::Edit);
    assert_eq!(classify_tool("run_shell"), ToolCallKind::Execute);
    assert_eq!(classify_tool("read_url"), ToolCallKind::Fetch);
    assert_eq!(classify_tool("think"), ToolCallKind::Think);
    assert_eq!(classify_tool("acp.terminal"), ToolCallKind::Execute);
    assert_eq!(classify_tool("mystery"), ToolCallKind::Other);
}

#[test]
fn map_events_and_updates_preserve_intent_order() {
    let events = [
        AgentEvent::Status("status".to_string()),
        AgentEvent::ToolStarted {
            id: "tool-ordered".to_string(),
            name: "read_file_range".to_string(),
            arguments: "{\"path\":\"/tmp/notes.md\"}".to_string(),
        },
        AgentEvent::ToolFinished {
            id: "tool-ordered".to_string(),
            output: vec!["line 1".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        },
        AgentEvent::ReasoningDelta("think".to_string()),
        AgentEvent::AssistantDelta("assistant".to_string()),
        AgentEvent::Finished,
    ];

    let updates: Vec<SessionUpdateIntent> = events.iter().flat_map(map_agent_event).collect();

    let mut kinds = Vec::new();
    for intent in updates {
        let kind = match intent {
            SessionUpdateIntent::Status(_) => "status",
            SessionUpdateIntent::ToolStarted { .. } => "tool_started",
            SessionUpdateIntent::ToolFinished { .. } => "tool_finished",
            SessionUpdateIntent::ReasoningDelta(_) => "reasoning",
            SessionUpdateIntent::AssistantDelta(_) => "assistant",
            _ => continue,
        };
        kinds.push(kind);
    }

    assert_eq!(
        kinds,
        vec!["status", "tool_started", "tool_finished", "reasoning", "assistant"]
    );
}

#[test]
fn ignores_non_streaming_events() {
    assert!(map_agent_event(&AgentEvent::Started).is_empty());
    assert!(
        map_agent_event(&AgentEvent::Retrying {
            attempt: 1,
            max_attempts: 2,
            delay_ms: 500,
            error: "timeout".to_string(),
        })
        .is_empty()
    );
}

#[test]
fn map_acp_to_local_session_and_generate_ids() {
    let mut store = AcpSessionStore::new();
    let workspace = tempdir().expect("temp workspace");

    let first_id = store
        .create_session("local-a", workspace.path(), None)
        .expect("create first session");
    let second_id = store
        .create_session("local-b", workspace.path(), None)
        .expect("create second session");

    assert_eq!(first_id, generate_session_id(1));
    assert_eq!(second_id, generate_session_id(2));
    assert_eq!(store.local_session_id(first_id.as_str()), Some("local-a"));
    assert_eq!(store.local_session_id(second_id.as_str()), Some("local-b"));
    assert_eq!(store.acp_session_id_for_local("local-b"), Some(second_id.as_str()));
}

#[test]
fn reject_duplicate_and_missing_session_ids() {
    let mut store = AcpSessionStore::new();
    let workspace = tempdir().expect("temp workspace");

    let _ = store
        .create_session("local-dupe", workspace.path(), None)
        .expect("create first");
    let duplicate = store.create_session("local-dupe", workspace.path(), None);
    assert!(matches!(
        duplicate,
        Err(AcpSessionError::DuplicateLocalSession { local_session_id }) if local_session_id == "local-dupe"
    ));

    let missing = store.begin_turn("ghost");
    assert!(matches!(
        missing,
        Err(AcpSessionError::MissingSession { acp_session_id }) if acp_session_id == "ghost"
    ));
}

#[test]
fn validate_and_normalize_session_cwd() {
    let workspace = tempdir().expect("temp workspace");
    let nested = workspace.path().join("repo").join("sub");
    std::fs::create_dir_all(&nested).expect("create nested");
    let canonical_expected = std::fs::canonicalize(&nested).expect("canonicalize");
    let normalized = validate_and_normalize_cwd(workspace.path(), Some(Path::new("repo/sub"))).expect("normalize");
    assert_eq!(normalized, canonical_expected);

    let invalid = validate_and_normalize_cwd(workspace.path(), Some(Path::new("missing")));
    assert!(invalid.is_err());

    let file_path = workspace.path().join("file.txt");
    std::fs::write(&file_path, "x").expect("file");
    let bad_type = validate_and_normalize_cwd(workspace.path(), Some(Path::new("file.txt")));
    assert!(bad_type.is_err());
}

#[test]
fn concurrent_turn_guard_blocks_second_turn_and_allows_after_end() {
    let mut store = AcpSessionStore::new();
    let workspace = tempdir().expect("temp workspace");
    let session_id = store
        .create_session("local-turn", workspace.path(), None)
        .expect("create session");
    store.begin_turn(session_id.as_str()).expect("first turn");
    let second = store.begin_turn(session_id.as_str());
    assert!(matches!(
        second,
        Err(AcpSessionError::TurnInProgress { acp_session_id }) if acp_session_id == session_id
    ));

    assert!(store.is_turn_active(session_id.as_str()));
    store.end_turn(session_id.as_str()).expect("end turn");
    store.begin_turn(session_id.as_str()).expect("second turn");
    store.end_turn(session_id.as_str()).expect("end second turn");

    let ended = store.end_turn(session_id.as_str());
    assert!(matches!(
        ended,
        Err(AcpSessionError::TurnNotActive { acp_session_id }) if acp_session_id == session_id
    ));
}

#[test]
fn config_options_have_stable_ids_and_validate_values() {
    assert_eq!(
        initial_config_option_ids(),
        &[
            MODEL_CONFIG_OPTION_ID,
            WEBSEARCH_CONFIG_OPTION_ID,
            REASONING_EFFORT_CONFIG_OPTION_ID,
            REASONING_SUMMARY_CONFIG_OPTION_ID,
        ]
    );
    let option_ids: Vec<&str> = initial_config_options().iter().map(|option| option.id).collect();
    assert!(option_ids.contains(&MODEL_CONFIG_OPTION_ID));
    assert!(option_ids.contains(&WEBSEARCH_CONFIG_OPTION_ID));
    assert!(option_ids.contains(&REASONING_EFFORT_CONFIG_OPTION_ID));
    assert!(option_ids.contains(&REASONING_SUMMARY_CONFIG_OPTION_ID));

    let model = validate_config_option(MODEL_CONFIG_OPTION_ID, "claude-3-opus").expect("model");
    assert!(matches!(model, ConfigOptionValue::Model(model) if model == "claude-3-opus"));

    let ws = validate_config_option(WEBSEARCH_CONFIG_OPTION_ID, "searxng").expect("websearch");
    assert!(matches!(ws, ConfigOptionValue::WebSearch(WebSearchMode::Searxng)));
    assert!(matches!(
        validate_config_option(REASONING_EFFORT_CONFIG_OPTION_ID, "xhigh"),
        Ok(ConfigOptionValue::ReasoningEffort(ReasoningEffort::Xhigh))
    ));
    assert!(matches!(
        validate_config_option(REASONING_SUMMARY_CONFIG_OPTION_ID, "auto"),
        Ok(ConfigOptionValue::ReasoningSummary(ReasoningSummary::Auto))
    ));

    assert!(validate_config_option(MODEL_CONFIG_OPTION_ID, "").is_err());
    assert!(validate_config_option("missing", "x").is_err());
    assert!(validate_config_option(WEBSEARCH_CONFIG_OPTION_ID, "bad").is_err());
    assert!(matches!(
        validate_config_option(REASONING_EFFORT_CONFIG_OPTION_ID, "max"),
        Ok(ConfigOptionValue::ReasoningEffort(ReasoningEffort::Max))
    ));
    assert!(validate_config_option(REASONING_EFFORT_CONFIG_OPTION_ID, "unbounded").is_err());
}

#[test]
fn session_metadata_placeholder_is_set_and_updatable() {
    let mut store = AcpSessionStore::new();
    let workspace = tempdir().expect("temp workspace");
    let session_id = store
        .create_session("local-meta", workspace.path(), None)
        .expect("create session");

    assert_eq!(
        store.session(session_id.as_str()).expect("session").metadata,
        LocalSessionMetadata {
            local_session_id: "local-meta".to_string(),
            model: None,
            websearch: None,
            reasoning_effort: None,
            reasoning_summary: None,
        }
    );

    store
        .update_session_metadata(
            session_id.as_str(),
            Some("model-x".to_string()),
            Some(WebSearchMode::Searxng),
        )
        .expect("update metadata");

    let session = store.session(session_id.as_str()).expect("session");
    assert_eq!(session.metadata.model.as_deref(), Some("model-x"));
    assert_eq!(session.metadata.websearch, Some(WebSearchMode::Searxng));
}

#[test]
fn set_config_option_updates_model_and_refreshes_options() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");

    let response = set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), MODEL_CONFIG_OPTION_ID, "chatgpt-codex/gpt-5.6-sol"),
    )
    .expect("set model");
    let json = serde_json::to_value(&response).expect("serialize response");

    assert!(
        json.to_string()
            .contains("\"currentValue\":\"chatgpt-codex/gpt-5.6-sol\"")
    );
    assert!(json.to_string().contains("\"configOptions\""));

    let response = set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), WEBSEARCH_CONFIG_OPTION_ID, "none"),
    )
    .expect("set websearch");
    let json = serde_json::to_value(&response).expect("serialize response");
    let text = json.to_string();

    assert!(text.contains("\"currentValue\":\"chatgpt-codex/gpt-5.6-sol\""));
    assert!(text.contains("\"currentValue\":\"none\""));

    let response = set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id, REASONING_EFFORT_CONFIG_OPTION_ID, "xhigh"),
    )
    .expect("set effort");
    assert!(
        serde_json::to_string(&response)
            .expect("serialize")
            .contains("\"currentValue\":\"xhigh\"")
    );
}

#[test]
fn set_config_option_rejects_invalid_ids_and_values() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");

    let missing = set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), "missing", "x"),
    );
    assert!(matches!(missing, Err(error) if error.contains("unknown config option")));

    let invalid = set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id, WEBSEARCH_CONFIG_OPTION_ID, "invalid"),
    );
    assert!(matches!(invalid, Err(error) if error.contains("must be one of")));
}

#[test]
fn execute_prompt_uses_selected_config_options() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");
    set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), MODEL_CONFIG_OPTION_ID, "chatgpt-codex/gpt-5.6-sol"),
    )
    .expect("set model");
    set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), WEBSEARCH_CONFIG_OPTION_ID, "none"),
    )
    .expect("set websearch");
    set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), REASONING_EFFORT_CONFIG_OPTION_ID, "high"),
    )
    .expect("set effort");
    set_config_option(
        &state,
        &SetSessionConfigOptionRequest::new(session_id.clone(), REASONING_SUMMARY_CONFIG_OPTION_ID, "auto"),
    )
    .expect("set summary");

    let response = execute_prompt(
        &state,
        &PromptRequest::new(session_id, vec![ContentBlock::Text(TextContent::new("check config"))]),
        |_intent| Ok(()),
        |config, _messages, _expects_write, _prompt| {
            assert_eq!(config.model, "chatgpt-codex/gpt-5.6-sol");
            assert_eq!(config.search_mode, WebSearchMode::None);
            assert_eq!(config.reasoning_effort, ReasoningEffort::High);
            assert_eq!(config.reasoning_summary, ReasoningSummary::Auto);
            HarnessTurn::fake(config, String::new()).start()
        },
    )
    .expect("prompt succeeds");

    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

#[test]
fn initialize_detects_client_terminal_capability() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let request =
        InitializeRequest::new(ProtocolVersion::V1).client_capabilities(ClientCapabilities::new().terminal(true));

    let _response = initialize(&state, &request);

    assert!(state.client_can_run_terminal());
}

#[test]
fn acp_mcp_config_accepts_no_servers() {
    let config = acp_mcp_config(&[]).expect("empty MCP config");

    assert!(config.servers.is_empty());
}

#[test]
fn acp_mcp_config_accepts_stdio_servers() {
    let server = McpServer::Stdio(
        McpServerStdio::new("docs", PathBuf::from("docs-mcp"))
            .args(vec!["--index".to_string()])
            .env(vec![EnvVariable::new("DOCS_TOKEN", "secret-value")]),
    );

    let config = acp_mcp_config(&[server]).expect("stdio MCP config");

    let server = &config.servers["docs"];
    assert_eq!(server.command, "docs-mcp");
    assert_eq!(server.args, vec!["--index"]);
    assert_eq!(server.env["DOCS_TOKEN"], "secret-value");
}

#[test]
fn acp_mcp_config_rejects_unsupported_transport_without_leaking_env() {
    let server = McpServer::Http(McpServerHttp::new("web", "https://mcp.example.test"));

    let error = acp_mcp_config(&[server]).expect_err("http MCP should be rejected");

    assert!(error.contains("unsupported transport"));
    assert!(error.contains("web"));
    assert!(!error.contains("https://mcp.example.test"));
}

fn state_for_tests(cwd: PathBuf, session_dir: Option<PathBuf>) -> ServerState {
    ServerState::new(ServerConfig::new(
        cwd,
        String::from("umans-coder"),
        String::from("duckduckgo"),
        session_dir,
    ))
}

#[test]
fn initialize_client_info_is_persisted_to_acp_session_metadata() {
    let workspace = tempdir().expect("temp workspace");
    let session_dir = tempdir().expect("temp session dir");
    let state = state_for_tests(workspace.path().to_path_buf(), Some(session_dir.path().to_path_buf()));
    let request = InitializeRequest::new(ProtocolVersion::V1).client_info(Implementation::new("fake-client", "1.2.3"));

    let _response = initialize(&state, &request);
    let session_id = state.create_session(workspace.path()).expect("session created");
    let payload = std::fs::read_dir(session_dir.path())
        .expect("read session dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .expect("session jsonl payload");

    assert!(payload.contains(&format!("\"acp_session_id\":\"{session_id}\"")));
    assert!(payload.contains("\"client_info_name\":\"fake-client\""));
    assert!(payload.contains("\"client_info_version\":\"1.2.3\""));
}

#[test]
fn attachs_local_session_writer_and_records_acp_session_metadata() {
    let workspace = tempdir().expect("temp workspace");
    let session_dir = tempdir().expect("temp session dir");
    let state = state_for_tests(workspace.path().to_path_buf(), Some(session_dir.path().to_path_buf()));

    let session_id = state.create_session(workspace.path()).expect("session created");
    let mut entries = std::fs::read_dir(session_dir.path())
        .expect("read session dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect::<Vec<_>>();

    assert_eq!(entries.len(), 1);
    let payload = std::fs::read_to_string(entries.remove(0)).expect("read session writer payload");
    assert!(payload.contains("\"acp_session\""));
    assert!(payload.contains(&format!("\"acp_session_id\":\"{session_id}\"")));
    assert!(payload.contains("thndrs acp serve"));
    assert!(payload.contains("\"local_session_id\":\"local-acp-session-1\""));
    let _ = session_id;
}

#[test]
fn execute_prompt_persists_acp_server_turn_records() {
    let workspace = tempdir().expect("temp workspace");
    let session_dir = tempdir().expect("temp session dir");
    let state = state_for_tests(workspace.path().to_path_buf(), Some(session_dir.path().to_path_buf()));
    let session_id = state.create_session(workspace.path()).expect("session created");

    let response = execute_prompt(
        &state,
        &PromptRequest::new(
            session_id,
            vec![ContentBlock::Text(TextContent::new("persist this turn"))],
        ),
        |_intent| Ok(()),
        |_config, _messages, _expects_write, _prompt| {
            let (event_tx, event_rx) = mpsc::channel();
            event_tx
                .send(AgentEvent::Usage { input_tokens: 3, output_tokens: 5 })
                .expect("send usage");
            event_tx
                .send(AgentEvent::ToolStarted {
                    id: "tool-persist".to_string(),
                    name: "read_file_range".to_string(),
                    arguments: "{\"path\":\"README.md\"}".to_string(),
                })
                .expect("send tool started");
            event_tx
                .send(AgentEvent::ToolFinished {
                    id: "tool-persist".to_string(),
                    output: vec!["ok".to_string()],
                    status: ToolStatus::Ok,
                    write_result: None,
                    shell_result: None,
                })
                .expect("send tool finished");
            event_tx
                .send(AgentEvent::ReasoningDelta("thinking".to_string()))
                .expect("send reasoning");
            event_tx
                .send(AgentEvent::AssistantDelta("answer".to_string()))
                .expect("send assistant");
            event_tx.send(AgentEvent::Finished).expect("send finished");
            drop(event_tx);
            HarnessHandle { events: event_rx, cancel: CancelToken::new() }
        },
    )
    .expect("prompt succeeds");

    assert_eq!(response.stop_reason, StopReason::EndTurn);
    let session_path = std::fs::read_dir(session_dir.path())
        .expect("read session dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .expect("session path");
    let records = SessionReader::read_records(&session_path);

    assert!(
        records
            .iter()
            .any(|record| matches!(record, SessionRecord::User { text, .. } if text == "persist this turn"))
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(record, SessionRecord::Usage { input_tokens: 3, output_tokens: 5, .. }))
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(record, SessionRecord::ToolStarted { call_id, .. } if call_id == "tool-persist"))
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(record, SessionRecord::ToolFinished { call_id, .. } if call_id == "tool-persist"))
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(record, SessionRecord::ReasoningFinished { text, .. } if text == "thinking"))
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(record, SessionRecord::AssistantFinished { text, .. } if text == "answer"))
    );
}

#[test]
fn execute_prompt_updates_and_finishes() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");
    let updates = Arc::new(Mutex::new(Vec::<SessionUpdateIntent>::new()));
    let capture = updates.clone();

    let response = execute_prompt(
        &state,
        &PromptRequest::new(
            session_id,
            vec![ContentBlock::Text(TextContent::new("summarize this project"))],
        ),
        move |intent| {
            capture.lock().expect("lock updates").push(intent);
            Ok(())
        },
        |config, _messages, _expects_write, _prompt| HarnessTurn::fake(config, String::new()).start(),
    )
    .expect("prompt succeeds");

    assert_eq!(response.stop_reason, StopReason::EndTurn);
    let updates = updates.lock().expect("lock updates");
    assert!(
        updates
            .iter()
            .any(|intent| matches!(intent, SessionUpdateIntent::AssistantDelta(_)))
    );
    assert!(
        updates
            .iter()
            .any(|intent| matches!(intent, SessionUpdateIntent::ReasoningDelta(_)))
    );
}

#[test]
fn execute_prompt_preserves_update_order() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");
    let updates = Arc::new(Mutex::new(Vec::<SessionUpdateIntent>::new()));
    let capture = updates.clone();
    let (event_tx, event_rx) = mpsc::channel();
    let event_rx = Arc::new(Mutex::new(Some(event_rx)));
    let (started_tx, started_rx) = mpsc::channel();

    let prompt_handle = {
        let state = state;
        let capture = capture;
        let event_rx = event_rx;
        let started_tx = started_tx;
        thread::spawn(move || {
            execute_prompt(
                &state,
                &PromptRequest::new(session_id, vec![ContentBlock::Text(TextContent::new("ordered prompt"))]),
                move |intent| {
                    capture.lock().expect("lock updates").push(intent);
                    Ok(())
                },
                move |_config, _messages, _expects_write, _prompt| {
                    started_tx.send(()).expect("prompt harness started");
                    let mut guard = event_rx.lock().expect("acquire harness event channel");
                    HarnessHandle {
                        events: guard.take().expect("event receiver available"),
                        cancel: CancelToken::new(),
                    }
                },
            )
        })
    };

    started_rx.recv().expect("prompt harness started");
    event_tx
        .send(AgentEvent::Status("starting".to_string()))
        .expect("send status");
    event_tx
        .send(AgentEvent::ToolStarted {
            id: "ordered-tool".to_string(),
            name: "read_file_range".to_string(),
            arguments: "{\"path\":\"/tmp/notes.md\"}".to_string(),
        })
        .expect("send tool started");
    event_tx
        .send(AgentEvent::ToolFinished {
            id: "ordered-tool".to_string(),
            output: vec!["first line".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        })
        .expect("send tool finished");
    event_tx
        .send(AgentEvent::ReasoningDelta("ordered reasoning".to_string()))
        .expect("send reasoning");
    event_tx
        .send(AgentEvent::AssistantDelta("ordered assistant".to_string()))
        .expect("send assistant");
    event_tx.send(AgentEvent::Finished).expect("send finished");

    let response = prompt_handle.join().expect("prompt thread").expect("prompt succeeds");
    assert_eq!(response.stop_reason, StopReason::EndTurn);

    let updates = updates.lock().expect("lock updates");
    let kinds: Vec<&str> = updates
        .iter()
        .filter_map(|intent| match intent {
            SessionUpdateIntent::Status(_) => Some("status"),
            SessionUpdateIntent::ToolStarted { .. } => Some("tool_started"),
            SessionUpdateIntent::ToolFinished { .. } => Some("tool_finished"),
            SessionUpdateIntent::ReasoningDelta(_) => Some("reasoning"),
            SessionUpdateIntent::AssistantDelta(_) => Some("assistant"),
            _ => None,
        })
        .collect();

    assert_eq!(
        kinds,
        vec!["status", "tool_started", "tool_finished", "reasoning", "assistant"]
    );
}

#[test]
fn cancel_session_signals_active_turn_token_but_clears_for_next_prompt() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");
    let token = CancelToken::new();

    let (event_tx, event_rx) = mpsc::channel();
    let event_rx = Arc::new(Mutex::new(Some(event_rx)));
    let (started_tx, started_rx) = mpsc::channel();
    let token_for_turn = token.clone();
    let state_for_turn = state.clone();
    let session_for_turn = session_id.clone();
    let turn_thread = thread::spawn(move || {
        execute_prompt(
            &state_for_turn,
            &PromptRequest::new(
                session_for_turn,
                vec![ContentBlock::Text(TextContent::new("long turn"))],
            ),
            |_intent| Ok(()),
            move |_config, _messages, _expects_write, _prompt| {
                started_tx.send(()).expect("turn started");
                let mut guard = event_rx.lock().expect("acquire harness event channel");
                HarnessHandle { events: guard.take().expect("event receiver available"), cancel: token_for_turn }
            },
        )
    });

    started_rx.recv().expect("turn running");
    let deadline = std::time::Instant::now()
        .checked_add(Duration::from_secs(1))
        .unwrap_or_else(std::time::Instant::now);
    while !token.is_cancelled() {
        if std::time::Instant::now() > deadline {
            break;
        }
        state.cancel_session(&session_id);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(token.is_cancelled());

    event_tx.send(AgentEvent::Finished).expect("finish turn");
    let first = turn_thread.join().expect("first turn").expect("first turn succeeds");
    assert_eq!(first.stop_reason, StopReason::Cancelled);

    let second = execute_prompt(
        &state,
        &PromptRequest::new(session_id, vec![ContentBlock::Text(TextContent::new("next prompt"))]),
        |_intent| Ok(()),
        |config, _messages, _expects_write, _prompt| HarnessTurn::fake(config, String::new()).start(),
    )
    .expect("second turn succeeds");
    assert_eq!(second.stop_reason, StopReason::EndTurn);
}

#[test]
fn cancelled_prompt_preserves_updates_sent_before_cancellation() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");
    let updates = Arc::new(Mutex::new(Vec::<SessionUpdateIntent>::new()));
    let capture = updates.clone();
    let token = CancelToken::new();

    let (event_tx, event_rx) = mpsc::channel();
    let event_rx = Arc::new(Mutex::new(Some(event_rx)));
    let (started_tx, started_rx) = mpsc::channel();
    let token_for_turn = token.clone();
    let state_for_turn = state.clone();
    let session_for_turn = session_id.clone();
    let turn_thread = thread::spawn(move || {
        execute_prompt(
            &state_for_turn,
            &PromptRequest::new(
                session_for_turn,
                vec![ContentBlock::Text(TextContent::new("long turn"))],
            ),
            move |intent| {
                capture.lock().expect("lock updates").push(intent);
                Ok(())
            },
            move |_config, _messages, _expects_write, _prompt| {
                started_tx.send(()).expect("turn started");
                let mut guard = event_rx.lock().expect("acquire harness event channel");
                HarnessHandle { events: guard.take().expect("event receiver available"), cancel: token_for_turn }
            },
        )
    });

    started_rx.recv().expect("turn running");
    event_tx
        .send(AgentEvent::AssistantDelta(String::from("partial answer")))
        .expect("send partial answer");
    let deadline = std::time::Instant::now()
        .checked_add(Duration::from_secs(1))
        .unwrap_or_else(std::time::Instant::now);
    while !token.is_cancelled() {
        if std::time::Instant::now() > deadline {
            break;
        }
        state.cancel_session(&session_id);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(token.is_cancelled());
    let _ = event_tx.send(AgentEvent::Finished);

    let response = turn_thread.join().expect("turn thread").expect("turn response");
    assert_eq!(response.stop_reason, StopReason::Cancelled);
    assert!(
        updates
            .lock()
            .expect("lock updates")
            .iter()
            .any(|intent| matches!(intent, SessionUpdateIntent::AssistantDelta(text) if text == "partial answer"))
    );
}

#[test]
fn cancel_session_records_local_session_jsonl_when_enabled() {
    let workspace = tempdir().expect("temp workspace");
    let session_dir = tempdir().expect("temp session dir");
    let state = state_for_tests(workspace.path().to_path_buf(), Some(session_dir.path().to_path_buf()));
    let session_id = state.create_session(workspace.path()).expect("session created");
    let token = CancelToken::new();
    let (event_tx, event_rx) = mpsc::channel();
    let event_rx = Arc::new(Mutex::new(Some(event_rx)));
    let (started_tx, started_rx) = mpsc::channel();
    let token_for_turn = token;
    let state_for_turn = state.clone();
    let session_for_turn = session_id.clone();
    let turn_thread = thread::spawn(move || {
        execute_prompt(
            &state_for_turn,
            &PromptRequest::new(
                session_for_turn,
                vec![ContentBlock::Text(TextContent::new("long turn"))],
            ),
            |_intent| Ok(()),
            move |_config, _messages, _expects_write, _prompt| {
                started_tx.send(()).expect("turn started");
                let mut guard = event_rx.lock().expect("acquire harness event channel");
                HarnessHandle { events: guard.take().expect("event receiver available"), cancel: token_for_turn }
            },
        )
    });

    started_rx.recv().expect("turn running");
    state.cancel_session(&session_id);
    event_tx.send(AgentEvent::Finished).expect("finish turn");
    let response = turn_thread.join().expect("turn thread").expect("turn response");
    assert_eq!(response.stop_reason, StopReason::Cancelled);

    let payload = std::fs::read_dir(session_dir.path())
        .expect("read session dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .expect("session jsonl payload");
    assert!(payload.contains("\"type\":\"cancelled\""));
    assert!(payload.contains("cancelled by ACP client"));
}

#[test]
fn cancel_after_completion_does_not_cancel_next_prompt() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");

    let first = execute_prompt(
        &state,
        &PromptRequest::new(
            session_id.clone(),
            vec![ContentBlock::Text(TextContent::new("first prompt"))],
        ),
        |_intent| Ok(()),
        |config, _messages, _expects_write, _prompt| HarnessTurn::fake(config, String::new()).start(),
    )
    .expect("first turn succeeds");
    assert_eq!(first.stop_reason, StopReason::EndTurn);

    state.cancel_session(&session_id);

    let second = execute_prompt(
        &state,
        &PromptRequest::new(session_id, vec![ContentBlock::Text(TextContent::new("second prompt"))]),
        |_intent| Ok(()),
        |config, _messages, _expects_write, _prompt| HarnessTurn::fake(config, String::new()).start(),
    )
    .expect("second turn succeeds");
    assert_eq!(second.stop_reason, StopReason::EndTurn);
}

#[test]
fn execute_prompt_rejects_unsupported_content() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");

    let request = PromptRequest::new(
        session_id,
        vec![ContentBlock::Audio(AudioContent::new("aGVsbG8=", "audio/wav"))],
    );
    let result = execute_prompt(
        &state,
        &request,
        |_intent| Ok(()),
        |_config, _messages, _expects_write, _prompt| panic!("harness should not be invoked for unsupported content"),
    );

    assert!(matches!(result, Err(err) if err.contains("unsupported ACP prompt content block")));
}

#[test]
fn execute_prompt_accepts_supported_rich_content() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");

    let request = PromptRequest::new(
        session_id,
        vec![
            ContentBlock::Text(TextContent::new("describe this")),
            ContentBlock::Image(ImageContent::new("aGVsbG8=", "image/png")),
            ContentBlock::ResourceLink(ResourceLink::new("notes.md", "file:///tmp/notes.md")),
        ],
    );
    let response = execute_prompt(
        &state,
        &request,
        |_intent| Ok(()),
        |config, messages, _expects_write, prompt| {
            assert!(prompt.contains("describe this"));
            assert!(prompt.contains("[image: image/png]"));
            assert!(prompt.contains("[resource link: notes.md (file:///tmp/notes.md)]"));
            assert!(messages.iter().any(|message| match &message.content {
                ProviderMessageContent::Blocks(blocks) => {
                    blocks
                        .iter()
                        .any(|block| matches!(block, ProviderContentBlock::Image { .. }))
                }
                ProviderMessageContent::Text(_) => false,
            }));
            HarnessTurn::fake(config, String::new()).start()
        },
    )
    .expect("rich prompt succeeds");

    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

#[test]
fn execute_prompt_rejects_missing_session_id() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);

    let result = execute_prompt(
        &state,
        &PromptRequest::new("missing-session", vec![ContentBlock::Text(TextContent::new("hello"))]),
        |_intent| Ok(()),
        |_config, _messages, _expects_write, _prompt| panic!("harness should not be invoked for missing session"),
    );

    assert!(matches!(result, Err(err) if err.contains("missing session: missing-session")));
}

#[test]
fn execute_prompt_rejects_concurrent_prompts_for_same_session() {
    let workspace = tempdir().expect("temp workspace");
    let state = state_for_tests(workspace.path().to_path_buf(), None);
    let session_id = state.create_session(workspace.path()).expect("session created");

    let (event_tx, event_rx) = mpsc::channel();
    let event_rx = Arc::new(Mutex::new(Some(event_rx)));
    let (started_tx, started_rx) = mpsc::channel();
    let first_state = state.clone();
    let first_session_id = session_id.clone();
    let first = thread::spawn(move || {
        execute_prompt(
            &first_state,
            &PromptRequest::new(first_session_id, vec![ContentBlock::Text(TextContent::new("first"))]),
            |_intent| Ok(()),
            move |_config, _messages, _expects_write, _prompt| {
                started_tx.send(()).expect("first harness started");
                let mut guard = event_rx.lock().expect("acquire harness event channel");
                HarnessHandle { events: guard.take().expect("event receiver available"), cancel: CancelToken::new() }
            },
        )
    });

    started_rx.recv().expect("first prompt harness started");
    let second = execute_prompt(
        &state,
        &PromptRequest::new(session_id, vec![ContentBlock::Text(TextContent::new("second"))]),
        |_intent| Ok(()),
        |_config, _messages, _expects_write, _prompt| panic!("second harness should not be created during active turn"),
    );
    assert!(matches!(second, Err(err) if err.contains("turn already active for session")));

    event_tx.send(AgentEvent::Finished).expect("finish first harness");
    let first = first.join().expect("first prompt thread");
    assert_eq!(first.unwrap().stop_reason, StopReason::EndTurn);
}

#[test]
fn fake_acp_client_smoke_runs_initialize_new_and_prompt() {
    let Some(_) = find_thndrs_binary() else {
        return;
    };
    let summary = run_fake_client(1, false);
    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["textPromptCapable"], true);
    assert_eq!(summary["updated"], true);
}

#[test]
fn fake_acp_client_smoke_runs_rich_content_prompt() {
    let Some(_) = find_thndrs_binary() else {
        return;
    };
    let summary = run_fake_client(1, true);
    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["richContent"], true);
    assert_eq!(summary["promptCapabilities"]["image"], true);
    assert_eq!(summary["promptCapabilities"]["embeddedContext"], true);
    assert_eq!(summary["updated"], true);
}

#[test]
fn fake_acp_client_falls_back_to_protocol_one_for_unsupported_requests() {
    let Some(_) = find_thndrs_binary() else {
        return;
    };
    let summary = run_fake_client(2, false);
    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["updated"], true);
}

fn run_fake_client(protocol_version: u16, rich_content: bool) -> Value {
    let server_path = find_thndrs_binary().expect("thndrs binary path");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fake_acp_client.py");
    let workspace = tempdir().expect("temp workspace");

    let mut command = Command::new("python3");
    command
        .arg(&fixture)
        .arg("--server")
        .arg(server_path)
        .arg("--server-arg")
        .arg("acp")
        .arg("--server-arg")
        .arg("serve")
        .arg("--protocol-version")
        .arg(protocol_version.to_string())
        .arg("--cwd")
        .arg(workspace.path());
    if rich_content {
        command.arg("--rich-content");
    }
    let output = command.output().expect("run fake ACP client fixture");

    assert!(
        output.status.success(),
        "fake ACP client fixture failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8(output.stdout).expect("fixture output is utf8");
    let summary = stdout
        .lines()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("fixture emitted json summary line");
    serde_json::from_str(summary).expect("summary is valid json")
}

fn find_thndrs_binary() -> Option<PathBuf> {
    if let Ok(path) = env::var("CARGO_BIN_EXE_thndrs") {
        return Some(PathBuf::from(path));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.parent().and_then(Path::parent).map_or_else(
        || manifest_dir.join("target"),
        |workspace_root| workspace_root.join("target"),
    );
    let candidates = [
        target_dir.join("debug").join("thndrs"),
        target_dir.join("debug").join("deps").join("thndrs"),
    ];
    candidates.into_iter().find(|path| path.exists())
}
