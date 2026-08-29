use std::io::Cursor;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::*;

fn lines(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8(bytes.to_vec())
        .expect("protocol output is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("protocol line is JSON"))
        .collect()
}

fn command(id: &str, command: Command) -> CommandEnvelope {
    CommandEnvelope { version: PROTOCOL_VERSION, id: id.to_string(), command }
}

#[test]
fn command_schema_uses_version_id_and_semantic_name() {
    let parsed: CommandEnvelope = serde_json::from_value(json!({
        "version": 1,
        "id": "request-1",
        "command": "turn.submit",
        "text": "hello"
    }))
    .expect("parse command");

    assert_eq!(
        parsed,
        command("request-1", Command::TurnSubmit { text: "hello".to_string() })
    );
}

#[test]
fn initialization_returns_snapshot_before_events() {
    let cli = Cli { ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let mut bridge = Bridge::new(&cli);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    bridge
        .handle_command(
            command(
                "init",
                Command::Initialize { supported_versions: vec![PROTOCOL_VERSION] },
            ),
            &mut stdout,
            &mut stderr,
        )
        .expect("initialize");

    let records = lines(&stdout);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["type"], "response");
    assert_eq!(records[0]["id"], "init");
    assert_eq!(records[0]["result"]["kind"], "initialized");
    assert_eq!(records[0]["result"]["protocol_version"], 1);
    assert!(records[0]["result"]["snapshot"]["transcript"].is_array());
    assert!(stderr.is_empty());
}

#[test]
fn fake_turn_projects_stream_tool_terminal_and_shutdown() {
    let temp = tempfile::tempdir().expect("workspace");
    let cli =
        Cli { cwd: temp.path().to_path_buf(), ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let mut bridge = Bridge::new(&cli);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    bridge
        .handle_command(
            command("init", Command::Initialize { supported_versions: vec![1] }),
            &mut stdout,
            &mut stderr,
        )
        .expect("initialize");
    bridge
        .handle_command(
            command("turn", Command::TurnSubmit { text: "inspect".to_string() }),
            &mut stdout,
            &mut stderr,
        )
        .expect("submit turn");

    let deadline = Instant::now() + Duration::from_secs(5);
    while bridge.agent.is_some() && Instant::now() < deadline {
        if !bridge
            .drain_one_agent_event(&mut stdout, &mut stderr)
            .expect("drain event")
        {
            thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(bridge.agent.is_none(), "fake turn settled before deadline");
    assert!(
        bridge
            .handle_command(command("shutdown", Command::Shutdown), &mut stdout, &mut stderr)
            .expect("shutdown")
    );

    let records = lines(&stdout);
    let event_types: Vec<&str> = records
        .iter()
        .filter_map(|record| record.get("event")?.get("type")?.as_str())
        .collect();
    assert!(event_types.contains(&"run.started"));
    assert!(event_types.contains(&"reasoning.delta"));
    assert!(event_types.contains(&"assistant.delta"));
    assert!(event_types.contains(&"tool.started"));
    assert!(event_types.contains(&"tool.finished"));
    assert!(event_types.contains(&"run.finished"));
    assert_eq!(
        records.last().and_then(|record| record["id"].as_str()),
        Some("shutdown")
    );
    assert!(stderr.is_empty());
}

#[test]
fn active_turn_uses_cooperative_cancellation() {
    let temp = tempfile::tempdir().expect("workspace");
    let cli =
        Cli { cwd: temp.path().to_path_buf(), ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let mut bridge = Bridge::new(&cli);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    bridge
        .handle_command(
            command("init", Command::Initialize { supported_versions: vec![1] }),
            &mut stdout,
            &mut stderr,
        )
        .expect("initialize");
    bridge
        .handle_command(
            command("turn", Command::TurnSubmit { text: "inspect".to_string() }),
            &mut stdout,
            &mut stderr,
        )
        .expect("submit turn");
    bridge
        .handle_command(command("cancel", Command::TurnCancel), &mut stdout, &mut stderr)
        .expect("request cancellation");

    let deadline = Instant::now() + Duration::from_secs(5);
    while bridge.agent.is_some() && Instant::now() < deadline {
        if !bridge
            .drain_one_agent_event(&mut stdout, &mut stderr)
            .expect("drain event")
        {
            thread::sleep(Duration::from_millis(5));
        }
    }

    let records = lines(&stdout);
    assert!(records.iter().any(|record| record["event"]["type"] == "run.cancelled"));
    assert!(bridge.agent.is_none());
}

#[test]
fn malformed_and_unsupported_commands_keep_stdout_protocol_only() {
    let input = Cursor::new(
        concat!(
            "not-json\n",
            "{\"version\":9,\"id\":\"bad-version\",\"command\":\"initialize\",\"supported_versions\":[9]}\n",
            "{\"version\":1,\"id\":\"init\",\"command\":\"initialize\",\"supported_versions\":[1]}\n",
            "{\"version\":1,\"id\":\"future\",\"command\":\"queue.submit\",\"text\":\"later\",\"target\":\"follow_up\"}\n",
            "{\"version\":1,\"id\":\"done\",\"command\":\"shutdown\"}\n"
        )
        .as_bytes()
        .to_vec(),
    );
    let cli = Cli { ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    run_stdio(&cli, input, &mut stdout, &mut stderr).expect("protocol exits through shutdown");

    let records = lines(&stdout);
    assert_eq!(records[0]["type"], "protocol_error");
    assert_eq!(records[1]["error"]["code"], "unsupported_version");
    assert_eq!(records[3]["error"]["code"], "unsupported_command");
    assert_eq!(records.last().and_then(|record| record["id"].as_str()), Some("done"));
    assert!(stderr.is_empty());
}

#[test]
fn initialization_exposes_commands_options_context_and_pending_permission() {
    let cli = Cli { ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let mut bridge = Bridge::new(&cli);
    bridge.app.refresh_context_ledger(None);
    let (sender, _receiver) = mpsc::channel();
    bridge
        .app
        .overlay
        .show_permission(crate::acp::permissions::PendingPermission {
            tool_call_id: "call-1".to_string(),
            title: "Run command?".to_string(),
            options: vec![crate::acp::permissions::PermissionOptionView {
                id: "allow-once".to_string(),
                name: "Allow once".to_string(),
                kind: crate::acp::permissions::PermissionKindView::AllowOnce,
            }],
            selected: 0,
            responder: sender,
        });
    let mut stdout = Vec::new();

    bridge
        .handle_command(
            command("init", Command::Initialize { supported_versions: vec![1] }),
            &mut stdout,
            &mut Vec::new(),
        )
        .expect("initialize");

    let snapshot = &lines(&stdout)[0]["result"]["snapshot"];
    assert!(snapshot["capabilities"]["commands"].as_array().is_some_and(|commands| {
        commands.iter().any(|command| command == "permission.respond")
            && commands.iter().any(|command| command == "model.select")
    }));
    assert!(
        snapshot["capabilities"]["models"]
            .as_array()
            .is_some_and(|models| !models.is_empty())
    );
    assert!(snapshot["context"].is_object());
    assert_eq!(snapshot["pending_permission"]["tool_call_id"], "call-1");
}

#[test]
fn permission_response_selects_the_exact_backend_option() {
    let cli = Cli { ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let mut bridge = Bridge::new(&cli);
    bridge.initialized = true;
    let (sender, receiver) = mpsc::channel();
    bridge
        .app
        .overlay
        .show_permission(crate::acp::permissions::PendingPermission {
            tool_call_id: "call-1".to_string(),
            title: "Run command?".to_string(),
            options: vec![crate::acp::permissions::PermissionOptionView {
                id: "reject-always".to_string(),
                name: "Always reject".to_string(),
                kind: crate::acp::permissions::PermissionKindView::RejectAlways,
            }],
            selected: 0,
            responder: sender,
        });
    let mut stdout = Vec::new();

    bridge
        .handle_command(
            command(
                "permission",
                Command::PermissionRespond {
                    tool_call_id: "call-1".to_string(),
                    option_id: Some("reject-always".to_string()),
                },
            ),
            &mut stdout,
            &mut Vec::new(),
        )
        .expect("respond");

    assert_eq!(
        receiver.recv().expect("permission decision"),
        crate::acp::permissions::PermissionDecision::Selected("reject-always".to_string())
    );
    assert!(bridge.app.overlay.permission().is_none());
    assert_eq!(lines(&stdout)[0]["result"]["kind"], "accepted");
}

#[test]
fn model_and_reasoning_selection_emit_backend_truth() {
    let temp = tempfile::tempdir().expect("workspace");
    let cli = Cli {
        cwd: temp.path().to_path_buf(),
        ephemeral: true,
        model: "chatgpt-codex/gpt-5.6-terra".to_string(),
        ..Cli::default()
    };
    let mut bridge = Bridge::new(&cli);
    bridge.initialized = true;
    let mut stdout = Vec::new();

    bridge
        .handle_command(
            command(
                "model",
                Command::ModelSelect { model: "chatgpt-codex/gpt-5.6-terra".to_string() },
            ),
            &mut stdout,
            &mut Vec::new(),
        )
        .expect("select model");
    bridge
        .handle_command(
            command("reasoning", Command::ReasoningSelect { effort: "high".to_string() }),
            &mut stdout,
            &mut Vec::new(),
        )
        .expect("select reasoning");

    let records = lines(&stdout);
    assert!(records.iter().any(|record| record["event"]["type"] == "model.updated"));
    assert!(
        records
            .iter()
            .any(|record| { record["event"]["type"] == "reasoning.updated" && record["event"]["effort"] == "high" })
    );
    assert_eq!(
        bridge.app.runtime.cli.reasoning_effort,
        crate::cli::ReasoningEffort::High
    );
}

#[test]
fn peer_disconnect_is_detected() {
    let input = Cursor::new(
        b"{\"version\":1,\"id\":\"init\",\"command\":\"initialize\",\"supported_versions\":[1]}\n".to_vec(),
    );
    let cli = Cli { ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let error = run_stdio(&cli, input, Vec::new(), Vec::new()).expect_err("disconnect is an error");
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
}
