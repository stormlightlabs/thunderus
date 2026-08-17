//! Black-box coverage for the headless `run` command.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;
use thndrs_lib::session;

fn headless_command(workspace: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(thndrs_binary());
    command
        .arg("--cwd")
        .arg(workspace)
        .args(args)
        .env("HOME", fixture_home(workspace));
    command
}

fn run_with_piped_input(workspace: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = headless_command(workspace, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start headless command");
    child
        .stdin
        .take()
        .expect("pipe standard input")
        .write_all(input)
        .expect("write piped input");
    child.wait_with_output().expect("wait for headless command")
}

fn assert_user_prompt(workspace: &Path, expected: &str) {
    let files = session::list_session_files(&workspace.join(".thndrs").join("sessions"));
    assert_eq!(files.len(), 1);
    let transcript = session::SessionReader::read_transcript(&files[0]);
    assert!(
        transcript
            .iter()
            .any(|entry| matches!(entry, thndrs_lib::app::Entry::User { text } if text == expected))
    );
}

fn write_fixture_config(workspace: &Path, script: &str) {
    let config_dir = workspace.join(".thndrs");
    std::fs::create_dir_all(&config_dir).expect("create config directory");
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"model = "acp:local"
session_dir = "sessions"

[acp_agents.local]
command = "python3"
args = ["{}", "{script}"]
timeout_secs = 2
"#,
            fixture_agent().display()
        ),
    )
    .expect("write config");
}

fn fixture_home(workspace: &Path) -> PathBuf {
    workspace.join(".test-home")
}

fn fixture_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fake_acp_agent.py")
}

fn thndrs_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_thndrs"))
}

#[test]
fn run_streams_assistant_text_to_stdout_and_diagnostics_to_stderr() {
    let workspace = tempdir().expect("create workspace");
    write_fixture_config(workspace.path(), "lifecycle");

    let output = headless_command(workspace.path(), &["run", "reply"])
        .output()
        .expect("run headless command");

    assert!(
        output.status.success(),
        "headless command failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        "pong from fake ACP agent\n"
    );
    let diagnostics = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(diagnostics.contains("thndrs run: started"));
    assert!(diagnostics.contains("thndrs run: finished"));
    assert!(!diagnostics.contains("pong from fake ACP agent"));
}

#[test]
fn run_jsonl_streams_versioned_events_without_human_output_on_stdout() {
    let workspace = tempdir().expect("create workspace");
    write_fixture_config(workspace.path(), "lifecycle");

    let output = headless_command(
        workspace.path(),
        &[
            "run",
            "--jsonl",
            "--timeout-secs",
            "2",
            "--session-policy",
            "ephemeral",
            "--authority",
            "read-only",
            "--evidence-max-bytes",
            "64",
            "--resource-max-bytes",
            "65536",
            "reply",
        ],
    )
    .output()
    .expect("run JSONL headless command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdout line is JSON"))
        .collect();
    assert!(events.iter().all(|event| event["version"] == 1));
    assert!(events.iter().any(|event| event["type"] == "text"));
    assert!(events.iter().any(|event| event["type"] == "completed"));
    assert!(!stdout.contains("thndrs run:"));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr is UTF-8")
            .contains("thndrs run: finished")
    );
}

#[test]
fn run_uses_piped_input_with_and_without_a_prompt_argument() {
    let stdin_only_workspace = tempdir().expect("create stdin-only workspace");
    write_fixture_config(stdin_only_workspace.path(), "lifecycle");
    let output = run_with_piped_input(stdin_only_workspace.path(), &["run"], b"inspect the pipe");
    assert!(output.status.success());
    assert_user_prompt(stdin_only_workspace.path(), "inspect the pipe");

    let combined_workspace = tempdir().expect("create combined workspace");
    write_fixture_config(combined_workspace.path(), "lifecycle");
    let output = run_with_piped_input(combined_workspace.path(), &["run", "inspect this"], b"and this");
    assert!(output.status.success());
    assert_user_prompt(combined_workspace.path(), "inspect this\n\nand this");
}

#[test]
fn run_rejects_empty_invalid_and_oversized_piped_input() {
    for (args, input, expected) in [
        (vec!["run"], Vec::new(), "standard input was empty"),
        (vec!["run"], vec![0xff], "must be valid UTF-8"),
        (
            vec!["run", "--stdin-max-bytes", "4"],
            b"12345".to_vec(),
            "exceeds --stdin-max-bytes=4",
        ),
    ] {
        let workspace = tempdir().expect("create workspace");
        write_fixture_config(workspace.path(), "lifecycle");
        let output = run_with_piped_input(workspace.path(), &args, &input);

        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8(output.stderr)
                .expect("stderr is UTF-8")
                .contains(expected)
        );
    }
}

#[cfg(unix)]
#[test]
fn run_returns_the_cancellation_exit_code_after_sigint() {
    let workspace = tempdir().expect("create workspace");
    write_fixture_config(workspace.path(), "cancel");
    let mut child = Command::new(thndrs_binary())
        .arg("--cwd")
        .arg(workspace.path())
        .args(["run", "wait"])
        .env("HOME", fixture_home(workspace.path()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start headless command");
    let stderr = child.stderr.take().expect("capture diagnostics");
    let (started_tx, started_rx) = mpsc::channel();
    let diagnostics = thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut output = String::new();
        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line).expect("read diagnostics");
            if bytes == 0 {
                break;
            }
            if line.contains("thndrs run: started") {
                let _ = started_tx.send(());
            }
            output.push_str(&line);
        }
        output
    });

    if let Err(error) = started_rx.recv_timeout(Duration::from_secs(10)) {
        let _ = child.kill();
        let output = child.wait_with_output().expect("wait for timed-out headless command");
        let diagnostics = diagnostics.join().expect("join diagnostics reader");
        panic!(
            "headless command did not start: {error}\nstdout:\n{}\nstderr:\n{diagnostics}",
            String::from_utf8_lossy(&output.stdout),
        );
    }
    let status = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("send SIGINT to headless command");
    assert!(status.success(), "SIGINT helper failed with {status}");

    let output = child.wait_with_output().expect("wait for headless command");
    let diagnostics = diagnostics.join().expect("join diagnostics reader");
    assert_eq!(output.status.code(), Some(4));
    assert!(diagnostics.contains("thndrs run: cancelled"));
    assert!(diagnostics.contains("thndrs: headless run cancelled"));
}
