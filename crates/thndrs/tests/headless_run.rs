//! Black-box coverage for the headless `run` command.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;

fn write_fixture_config(workspace: &Path, script: &str) {
    let config_dir = workspace.join(".thndrs");
    std::fs::create_dir_all(&config_dir).expect("create config directory");
    std::fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"model = "acp:local"
websearch = "none"
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

    let output = Command::new(thndrs_binary())
        .arg("--cwd")
        .arg(workspace.path())
        .args(["run", "reply"])
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

#[cfg(unix)]
#[test]
fn run_returns_the_cancellation_exit_code_after_sigint() {
    let workspace = tempdir().expect("create workspace");
    write_fixture_config(workspace.path(), "cancel");
    let mut child = Command::new(thndrs_binary())
        .arg("--cwd")
        .arg(workspace.path())
        .args(["run", "wait"])
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
