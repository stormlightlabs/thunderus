use std::io::Cursor;
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
            "{\"version\":1,\"id\":\"future\",\"command\":\"model.select\",\"model\":\"x\"}\n",
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
fn peer_disconnect_is_detected() {
    let input = Cursor::new(
        b"{\"version\":1,\"id\":\"init\",\"command\":\"initialize\",\"supported_versions\":[1]}\n".to_vec(),
    );
    let cli = Cli { ephemeral: true, model: "fake-agent".to_string(), ..Cli::default() };
    let error = run_stdio(&cli, input, Vec::new(), Vec::new()).expect_err("disconnect is an error");
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
}
