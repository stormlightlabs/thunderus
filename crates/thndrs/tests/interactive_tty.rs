//! Black-box coverage for the interactive terminal contract.

#![cfg(unix)]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tempfile::tempdir;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn interactive_terminal_protocol_smoke_without_alternate_screen_or_mouse_capture() {
    let workspace = tempdir().expect("create workspace");
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows: 24, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("open pseudo-terminal");

    let mut command = CommandBuilder::new(thndrs_binary());
    command.arg("--cwd");
    command.arg(workspace.path());
    command.args(["--model", "fake-agent", "--ephemeral", "--websearch", "none"]);
    command.cwd(workspace.path());
    command.env("TERM", "xterm-256color");

    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("start thndrs in pseudo-terminal");
    drop(pair.slave);
    let mut writer = pair.master.take_writer().expect("open pseudo-terminal writer");
    let mut reader = pair.master.try_clone_reader().expect("open pseudo-terminal reader");
    let (output_tx, output_rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut chunk = [0_u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    if output_tx.send(chunk[..read].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut output = Vec::new();
    receive_until(&output_rx, &mut output, b"\x1b[5 q", TEST_TIMEOUT);

    writer.write_all(b"/mcp\r").expect("send local slash command");
    writer.flush().expect("flush local slash command");
    receive_until(&output_rx, &mut output, b"no MCP servers configured", TEST_TIMEOUT);

    writer.write_all(b"\x03").expect("send Ctrl+C");
    writer.flush().expect("flush Ctrl+C");
    let deadline = Instant::now() + TEST_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll thndrs process") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill timed-out thndrs process");
            panic!("interactive thndrs process did not exit after Ctrl+C");
        }
        thread::sleep(Duration::from_millis(20));
    };

    drop(writer);
    while let Ok(chunk) = output_rx.recv_timeout(Duration::from_millis(100)) {
        output.extend_from_slice(&chunk);
    }
    reader_thread.join().expect("join pseudo-terminal reader");

    assert!(status.success(), "interactive thndrs exited unsuccessfully: {status:?}");
    assert_contains(&output, b"\x1b[?2004h", "enable bracketed paste");
    assert_contains(&output, b"\x1b[?2004l", "disable bracketed paste");
    assert_contains(&output, b"\x1b[5 q", "select a blinking terminal cursor");
    assert_contains(&output, b"\x1b[?25h", "restore cursor visibility");
    assert_contains(&output, b"\x1b[0 q", "restore the default cursor shape");
    assert_contains(
        &output,
        b"Ask for change, run a command, or inspect the repo.",
        "render the startup banner",
    );
    assert_contains(&output, b"no MCP servers configured", "render a local command result");

    for sequence in [
        b"\x1b[?1049h".as_slice(),
        b"\x1b[?1047h".as_slice(),
        b"\x1b[?47h".as_slice(),
        b"\x1b[?1000h".as_slice(),
        b"\x1b[?1002h".as_slice(),
        b"\x1b[?1003h".as_slice(),
        b"\x1b[?1006h".as_slice(),
        b"\x1b[?1015h".as_slice(),
    ] {
        assert!(
            !contains_bytes(&output, sequence),
            "interactive terminal unexpectedly emitted {sequence:?}: {}",
            String::from_utf8_lossy(&output)
        );
    }
}

fn receive_until(receiver: &Receiver<Vec<u8>>, output: &mut Vec<u8>, needle: &[u8], timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !contains_bytes(output, needle) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for {needle:?}: {}",
            String::from_utf8_lossy(output)
        );
        let chunk = receiver
            .recv_timeout(remaining)
            .expect("pseudo-terminal output ended early");
        output.extend_from_slice(&chunk);
    }
}

fn assert_contains(output: &[u8], needle: &[u8], behavior: &str) {
    assert!(
        contains_bytes(output, needle),
        "terminal did not {behavior}; missing {needle:?}: {}",
        String::from_utf8_lossy(output)
    );
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn thndrs_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_thndrs"))
}
