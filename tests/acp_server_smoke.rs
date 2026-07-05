//! Stdio smoke tests for the ACP agent server binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

#[test]
fn fake_client_smokes_initialize_session_and_prompt() {
    let summary = run_fake_client("prompt", 1, false);

    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["textPromptCapable"], true);
    assert_eq!(summary["updated"], true);
    assert_eq!(summary["stopReason"], "end_turn");
}

#[test]
fn fake_client_smokes_permission_approval() {
    let summary = run_fake_client("permission", 1, false);

    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["permissionRequests"], 1);
    assert_eq!(summary["stopReason"], "end_turn");
}

#[test]
fn fake_client_smokes_cancellation() {
    let summary = run_fake_client("cancel", 1, false);

    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["updated"], true);
    assert_eq!(summary["stopReason"], "cancelled");
}

#[test]
fn fake_client_smokes_malformed_request_handling() {
    let summary = run_fake_client("malformed", 1, false);

    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["malformedError"], true);
}

#[test]
fn fake_client_smokes_rich_content_prompt() {
    let summary = run_fake_client("prompt", 1, true);

    assert_eq!(summary["protocolVersion"], 1);
    assert_eq!(summary["richContent"], true);
    assert_eq!(summary["promptCapabilities"]["image"], true);
    assert_eq!(summary["promptCapabilities"]["embeddedContext"], true);
    assert_eq!(summary["updated"], true);
}

fn run_fake_client(scenario: &str, protocol_version: u16, rich_content: bool) -> Value {
    let server_path = acp_server_binary();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fake_acp_client.py");
    let workspace = tempdir().expect("temp workspace");
    std::fs::write(workspace.path().join("Cargo.toml"), "[package]\nname = \"smoke\"\n").expect("workspace Cargo.toml");

    let mut command = Command::new("python3");
    command
        .arg(&fixture)
        .arg("--server")
        .arg(server_path)
        .arg("--protocol-version")
        .arg(protocol_version.to_string())
        .arg("--cwd")
        .arg(workspace.path())
        .arg("--scenario")
        .arg(scenario);
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

fn acp_server_binary() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_thndrs-acp-server"));
    assert!(
        Path::new(&path).exists(),
        "thndrs-acp-server binary path does not exist: {}",
        path.display()
    );
    path
}
