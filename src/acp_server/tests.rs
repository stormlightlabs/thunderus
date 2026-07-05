//! Tests for ACP server primitives.

use std::path::Path;

use crate::cli::WebSearchMode;
use tempfile::tempdir;

use super::config_options::{
    ConfigOptionValue, MODEL_CONFIG_OPTION_ID, WEBSEARCH_CONFIG_OPTION_ID, initial_config_option_ids,
    initial_config_options, validate_config_option,
};
use super::events::{SessionUpdateIntent, ToolStatusIntent, map_agent_event};
use super::session::{
    AcpSessionError, AcpSessionStore, LocalSessionMetadata, generate_session_id, validate_and_normalize_cwd,
};
use crate::app::{AgentEvent, ToolStatus};

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
            status: ToolStatusIntent::Cancelled,
            output: vec!["cancelled".to_string()],
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
        &[MODEL_CONFIG_OPTION_ID, WEBSEARCH_CONFIG_OPTION_ID]
    );
    let option_ids: Vec<&str> = initial_config_options().iter().map(|option| option.id).collect();
    assert!(option_ids.contains(&MODEL_CONFIG_OPTION_ID));
    assert!(option_ids.contains(&WEBSEARCH_CONFIG_OPTION_ID));

    let model = validate_config_option(MODEL_CONFIG_OPTION_ID, "claude-3-opus").expect("model");
    assert!(matches!(model, ConfigOptionValue::Model(model) if model == "claude-3-opus"));

    let ws = validate_config_option(WEBSEARCH_CONFIG_OPTION_ID, "native").expect("websearch");
    assert!(matches!(ws, ConfigOptionValue::WebSearch(WebSearchMode::Native)));

    assert!(validate_config_option(MODEL_CONFIG_OPTION_ID, "").is_err());
    assert!(validate_config_option("missing", "x").is_err());
    assert!(validate_config_option(WEBSEARCH_CONFIG_OPTION_ID, "bad").is_err());
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
        LocalSessionMetadata { local_session_id: "local-meta".to_string(), model: None, websearch: None }
    );

    store
        .update_session_metadata(
            session_id.as_str(),
            Some("model-x".to_string()),
            Some(WebSearchMode::Exa),
        )
        .expect("update metadata");

    let session = store.session(session_id.as_str()).expect("session");
    assert_eq!(session.metadata.model.as_deref(), Some("model-x"));
    assert_eq!(session.metadata.websearch, Some(WebSearchMode::Exa));
}
