use super::config::parse_model_id;
use super::events::map_session_update;
use super::runner::{
    RunHandle, close_session, list_sessions, load_session, new_session_request, resume_session, spawn_run,
};
use crate::app::{AgentEvent, ToolStatus};
use crate::config::AcpAgentConfig;
use crate::mcp::config::{McpConfig, McpServerConfig, McpTransport};
use agent_client_protocol::schema::v1::{
    AgentCapabilities, Content, ContentBlock, ContentChunk, McpServer, Plan, PlanEntry, PlanEntryPriority,
    PlanEntryStatus, SessionUpdate, TextContent, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, UsageUpdate,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

fn collect(handle: RunHandle) -> Vec<AgentEvent> {
    spawn_run(handle).iter().collect()
}

#[test]
fn model_id_parser_accepts_valid_acp_names() {
    assert_eq!(parse_model_id("acp:claude"), Some("claude"));
    assert_eq!(parse_model_id("acp:zed-agent_1"), Some("zed-agent_1"));
    assert_eq!(parse_model_id("umans-coder"), None);
}

#[test]
fn model_id_parser_rejects_invalid_acp_names() {
    assert_eq!(parse_model_id("acp:"), None);
    assert_eq!(parse_model_id("acp:bad/name"), None);
    assert_eq!(parse_model_id("acp:bad name"), None);
}

#[test]
fn acp_runner_reports_missing_agent() {
    let events = collect(RunHandle::new(
        PathBuf::from("/repo"),
        "missing".to_string(),
        None,
        "hello".to_string(),
    ));

    assert_eq!(events.first(), Some(&AgentEvent::Started));
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Failed("ACP agent `missing` is not configured".to_string()))
    );
}

#[test]
fn acp_runner_reports_disabled_agent() {
    let agent = AcpAgentConfig { enabled: false, command: "agent".to_string(), ..AcpAgentConfig::default() };
    let events = collect(RunHandle::new(
        PathBuf::from("/repo"),
        "disabled".to_string(),
        Some(agent),
        "hello".to_string(),
    ));

    assert_eq!(events.first(), Some(&AgentEvent::Started));
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Failed("ACP agent `disabled` is disabled".to_string()))
    );
}

#[test]
fn maps_assistant_and_reasoning_chunks() {
    assert_eq!(
        map_session_update(SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("hello")
        )))),
        vec![AgentEvent::AssistantDelta("hello".to_string())]
    );
    assert_eq!(
        map_session_update(SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("thinking")
        )))),
        vec![AgentEvent::ReasoningDelta("thinking".to_string())]
    );
}

#[test]
fn maps_plan_and_usage_updates() {
    let plan = Plan::new(vec![PlanEntry::new(
        "do the thing",
        PlanEntryPriority::High,
        PlanEntryStatus::InProgress,
    )]);
    assert!(matches!(
        map_session_update(SessionUpdate::Plan(plan)).as_slice(),
        [AgentEvent::ReasoningDelta(text)] if text.contains("do the thing")
    ));

    assert_eq!(
        map_session_update(SessionUpdate::UsageUpdate(UsageUpdate::new(40, 100))),
        vec![AgentEvent::Usage { input_tokens: 40, output_tokens: 60 }]
    );
}

#[test]
fn maps_tool_start_and_completion_with_redaction() {
    let started = map_session_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-1", "run command").raw_input(Some(serde_json::json!({"token": "sk-secret"}))),
    ));
    assert_eq!(
        started,
        vec![AgentEvent::ToolStarted {
            id: "tool-1".to_string(),
            name: "run command".to_string(),
            arguments: "{\"token\":\"sk-[redacted]-secret\"}".to_string(),
        }]
    );

    let completed = map_session_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .content(Some(vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                TextContent::new("done"),
            )))])),
    )));
    assert_eq!(
        completed,
        vec![AgentEvent::ToolFinished {
            id: "tool-1".to_string(),
            output: vec!["done".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        }]
    );
}

#[test]
fn maps_unknown_like_metadata_updates_to_stable_status() {
    assert_eq!(
        map_session_update(SessionUpdate::AvailableCommandsUpdate(
            agent_client_protocol::schema::v1::AvailableCommandsUpdate::new(vec![])
        )),
        vec![AgentEvent::Status("acp: available commands updated".to_string())]
    );
}

#[test]
fn acp_runner_completes_fake_agent_lifecycle() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let agent = fake_agent_config("lifecycle", None);

    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "ping".to_string(),
    ));

    assert!(events.contains(&AgentEvent::Started));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Status(text) if text.contains("fake-acp-agent")))
    );
    assert!(events.contains(&AgentEvent::AssistantDelta("pong from fake ACP agent".to_string())));
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

#[test]
fn acp_runner_sends_session_cancel_on_local_cancel() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let agent = fake_agent_config("cancel", None);
    let handle = RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "wait".to_string(),
    );
    let cancel = handle.cancel.clone();
    let rx = spawn_run(handle);
    let mut events = Vec::new();

    loop {
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("agent event before cancel");
        let saw_prompt_update = matches!(event, AgentEvent::AssistantDelta(_));
        events.push(event);
        if saw_prompt_update {
            break;
        }
    }

    cancel.cancel();
    events.extend(collect_until_terminal(&rx, Duration::from_secs(3)));

    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::Status(text) if text.contains("sent session/cancel")) })
    );
    assert!(events.contains(&AgentEvent::Cancelled));
}

#[test]
fn acp_runner_cancels_pending_permission_on_local_cancel() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let agent = fake_agent_config("permission", None);
    let handle = RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "permit".to_string(),
    );
    let cancel = handle.cancel.clone();
    let rx = spawn_run(handle);
    let mut events = Vec::new();

    loop {
        let event = rx.recv_timeout(Duration::from_secs(2)).expect("permission event");
        let saw_permission = matches!(event, AgentEvent::PermissionRequest(_));
        events.push(event);
        if saw_permission {
            break;
        }
    }

    cancel.cancel();
    events.extend(collect_until_terminal(&rx, Duration::from_secs(3)));

    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::PermissionResolved { outcome, .. } if outcome == "cancelled") })
    );
}

#[test]
fn acp_runner_times_out_prompt_and_cleans_up() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let agent = fake_agent_config("timeout-prompt", Some(1));

    let rx = spawn_run(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "timeout".to_string(),
    ));
    let events = collect_until_terminal(&rx, Duration::from_secs(4));

    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::Status(text) if text.contains("sent session/cancel")) })
    );
    assert!(events.iter().any(|event| {
        matches!(event, AgentEvent::Failed(text) if text.contains("prompt timed out after 1 seconds"))
    }));
}

#[test]
fn acp_runner_times_out_initialize() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let agent = fake_agent_config("timeout-initialize", Some(1));

    let rx = spawn_run(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "timeout".to_string(),
    ));
    let events = collect_until_terminal(&rx, Duration::from_secs(4));

    assert!(events.iter().any(|event| {
        matches!(event, AgentEvent::Failed(text) if text.contains("initialize timed out after 1 seconds"))
    }));
}

#[test]
fn acp_runner_times_out_session_creation() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let agent = fake_agent_config("timeout-session", Some(1));

    let rx = spawn_run(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "timeout".to_string(),
    ));
    let events = collect_until_terminal(&rx, Duration::from_secs(4));

    assert!(events.iter().any(|event| {
        matches!(event, AgentEvent::Failed(text) if text.contains("session creation timed out after 1 seconds"))
    }));
}

#[test]
fn acp_runner_handles_fixture_filesystem_read_request() {
    let temp = tempfile::tempdir().expect("create temp dir");
    std::fs::write(temp.path().join("readme.txt"), "alpha\nbeta\n").expect("write fixture file");
    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(fake_agent_config("fs-read", None)),
        "read".to_string(),
    ));

    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::ToolStarted { name, .. } if name == "acp.fs.read_text_file") })
    );
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::ToolFinished { status: ToolStatus::Ok, .. }) })
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::AssistantDelta(text) if text.contains("read: alpha\nbeta")))
    );
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

#[test]
fn acp_runner_handles_fixture_filesystem_write_request() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(fake_agent_config("fs-write", None)),
        "write".to_string(),
    ));

    assert_eq!(
        std::fs::read_to_string(temp.path().join("acp-write.txt")).expect("written file"),
        "written by fake ACP\n"
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AgentEvent::ToolFinished { status: ToolStatus::Ok, write_result: Some(_), .. }
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::AssistantDelta(text) if text.contains("write ok")))
    );
}

#[test]
fn acp_runner_handles_fixture_unknown_update() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(fake_agent_config("unknown-update", None)),
        "unknown".to_string(),
    ));

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Status(text) if text == "acp: available commands updated"))
    );
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

#[test]
fn acp_runner_authenticates_with_agent_owned_method() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(fake_agent_config("auth-success", None)),
        "auth".to_string(),
    ));

    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::Status(text) if text.contains("authentication succeeded")) })
    );
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

#[test]
fn acp_runner_reports_authentication_failure() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let rx = spawn_run(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(fake_agent_config("auth-failure", None)),
        "auth".to_string(),
    ));
    let events = collect_until_terminal(&rx, Duration::from_secs(3));

    assert!(
        events
            .iter()
            .any(|event| { matches!(event, AgentEvent::Failed(text) if text.contains("authentication failed")) })
    );
}

#[test]
fn acp_runner_handles_fixture_terminal_lifecycle() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(fake_agent_config("terminal", None)),
        "terminal".to_string(),
    ));

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::ToolStarted { name, .. } if name == "acp.terminal"))
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            AgentEvent::ToolFinished { status: ToolStatus::Ok, shell_result: Some(_), .. }
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::AssistantDelta(text) if text.contains("terminal: terminal ok")))
    );
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

#[test]
fn acp_session_new_has_no_mcp_servers_without_mcp_config() {
    let (request, diagnostics) = new_session_request(PathBuf::from("/repo"), None, &AgentCapabilities::default());

    assert!(request.mcp_servers.is_empty());
    assert!(diagnostics.is_empty());
}

#[test]
fn acp_session_new_maps_enabled_stdio_mcp_config() {
    let mut config = McpConfig::default();
    let mut env = BTreeMap::new();
    env.insert("TOKEN".to_string(), "secret".to_string());
    config.servers.insert(
        "docs".to_string(),
        McpServerConfig {
            command: "docs-mcp".to_string(),
            args: vec!["--mode".to_string(), "stdio".to_string()],
            env,
            ..McpServerConfig::default()
        },
    );

    let (request, diagnostics) =
        new_session_request(PathBuf::from("/repo"), Some(&config), &AgentCapabilities::default());

    assert_eq!(diagnostics, vec!["acp: passing 1 MCP server through session/new"]);
    let [McpServer::Stdio(server)] = request.mcp_servers.as_slice() else {
        panic!("expected one stdio MCP server");
    };
    assert_eq!(server.name, "docs");
    assert_eq!(server.command, PathBuf::from("docs-mcp"));
    assert_eq!(server.args, vec!["--mode", "stdio"]);
    assert_eq!(server.env[0].name, "TOKEN");
    assert_eq!(server.env[0].value, "secret");
}

#[test]
fn acp_session_new_skips_unsupported_mcp_without_leaking_secrets() {
    let mut config = McpConfig::default();
    let mut headers = BTreeMap::new();
    headers.insert("Authorization".to_string(), "Bearer secret-token".to_string());
    config.servers.insert(
        "web".to_string(),
        McpServerConfig {
            transport: McpTransport::StreamableHttp,
            url: Some("https://mcp.example.test".to_string()),
            headers,
            ..McpServerConfig::default()
        },
    );

    let (request, diagnostics) =
        new_session_request(PathBuf::from("/repo"), Some(&config), &AgentCapabilities::default());

    assert!(request.mcp_servers.is_empty());
    assert_eq!(
        diagnostics,
        vec!["acp: MCP server `web` not passed because its transport is unsupported by the ACP agent"]
    );
    assert!(!diagnostics.join("\n").contains("secret-token"));
}

#[test]
fn acp_runner_passes_stdio_mcp_servers_to_session_new() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let mut config = McpConfig::default();
    config.servers.insert(
        "docs".to_string(),
        McpServerConfig {
            command: "docs-mcp".to_string(),
            args: vec!["--workspace".to_string(), temp.path().display().to_string()],
            ..McpServerConfig::default()
        },
    );

    let events = collect(
        RunHandle::new(
            temp.path().to_path_buf(),
            "fake".to_string(),
            Some(fake_agent_config("mcp-servers", None)),
            "mcp".to_string(),
        )
        .with_mcp_config(config),
    );

    assert!(events.iter().any(|event| {
        matches!(event, AgentEvent::Status(text) if text == "acp: passing 1 MCP server through session/new")
    }));
    assert!(events.iter().any(|event| {
        matches!(event, AgentEvent::AssistantDelta(text) if text.contains("\"name\":\"docs\"") && text.contains("\"command\":\"docs-mcp\""))
    }));
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

#[test]
fn acp_session_list_reports_unsupported_agent() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let err = list_sessions(
        "fake",
        fake_agent_config("lifecycle", Some(2)),
        temp.path().to_path_buf(),
    )
    .expect_err("unsupported list should fail");

    assert!(err.contains("does not advertise session/list support"));
}

#[test]
fn acp_session_list_returns_agent_sessions() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let sessions = list_sessions(
        "fake",
        fake_agent_config("sessions", Some(2)),
        temp.path().to_path_buf(),
    )
    .expect("list sessions");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "external-session-1");
    assert_eq!(sessions[0].cwd, temp.path());
    assert_eq!(sessions[0].title.as_deref(), Some("Fixture Session"));
    assert_eq!(sessions[0].updated_at.as_deref(), Some("2026-07-04T00:00:00Z"));
}

#[test]
fn acp_session_list_surfaces_agent_failure() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let err = list_sessions(
        "fake",
        fake_agent_config("sessions-failure", Some(2)),
        temp.path().to_path_buf(),
    )
    .expect_err("failed list should fail");

    assert!(err.contains("session list failed"));
}

#[test]
fn acp_session_load_replays_updates() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let events = load_session(
        "fake",
        fake_agent_config("sessions", Some(2)),
        temp.path().to_path_buf(),
        "external-session-1".to_string(),
    )
    .expect("load session");

    assert!(events.contains(&AgentEvent::AssistantDelta("replayed external-session-1".to_string())));
}

#[test]
fn acp_session_resume_returns_external_metadata() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let metadata = resume_session(
        "fake",
        fake_agent_config("sessions", Some(2)),
        temp.path().to_path_buf(),
        "external-session-1".to_string(),
    )
    .expect("resume session");

    assert_eq!(metadata.agent_name, "fake");
    assert_eq!(metadata.acp_session_id, "external-session-1");
    assert_eq!(metadata.agent_info_name.as_deref(), Some("fake-acp-agent"));
}

#[test]
fn acp_session_close_reports_closed_session() {
    let lines = close_session(
        "fake",
        fake_agent_config("sessions", Some(2)),
        "external-session-1".to_string(),
    )
    .expect("close session");

    assert_eq!(lines, vec!["acp: closed `fake` session external-session-1"]);
}

fn collect_until_terminal(rx: &mpsc::Receiver<AgentEvent>, timeout: Duration) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    loop {
        let event = rx.recv_timeout(timeout).expect("terminal event");
        let terminal = matches!(
            event,
            AgentEvent::Finished | AgentEvent::Failed(_) | AgentEvent::Cancelled
        );
        events.push(event);
        if terminal {
            break;
        }
    }
    events
}

fn fake_agent_config(script: &str, timeout_secs: Option<u64>) -> AcpAgentConfig {
    AcpAgentConfig {
        command: "python3".to_string(),
        args: vec![fake_agent_fixture().display().to_string(), script.to_string()],
        timeout_secs: timeout_secs.unwrap_or_else(|| AcpAgentConfig::default().timeout_secs),
        ..AcpAgentConfig::default()
    }
}

fn fake_agent_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fake_acp_agent.py")
}
