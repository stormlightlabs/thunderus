use std::{
    io::{self, Read},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use super::MAX_OUTPUT_BYTES;

/// Result of a capped subprocess execution.
pub struct CommandResult {
    pub exit_code: i32,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Run a command with a timeout, capping stdout/stderr bytes.
///
/// Uses `try_wait` polling to implement timeout without extra dependencies.
/// Kills the process if it exceeds the timeout.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> io::Result<CommandResult> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = stdout_tx.send(read_capped(stdout));
    });
    std::thread::spawn(move || {
        let _ = stderr_tx.send(read_capped(stderr));
    });

    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                let stdout_buf = stdout_rx.recv().unwrap_or_else(|_| Vec::new());
                let stderr_buf = stderr_rx.recv().unwrap_or_else(|_| Vec::new());

                return Ok(CommandResult {
                    exit_code: status.code().unwrap_or(-1),
                    success: status.success(),
                    stdout: String::from_utf8_lossy(&stdout_buf).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr_buf).into_owned(),
                });
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "command exceeded timeout"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn read_capped<R: Read>(mut reader: R) -> Vec<u8> {
    let cap = MAX_OUTPUT_BYTES;
    let mut out = Vec::with_capacity(4096);
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.len() < cap {
                    let remaining = cap - out.len();
                    out.extend_from_slice(&buf[..n.min(remaining)]);
                }
            }
        }
    }
    out
}

/// Truncate a Vec of strings to `max_results` entries.
pub fn truncate_results(results: Vec<String>, max_results: usize) -> Vec<String> {
    if results.len() <= max_results { results } else { results.into_iter().take(max_results).collect() }
}

/// Check whether a command exists on the system.
pub fn command_exists(name: &str) -> bool {
    Command::new("which")
        .args([name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_results_caps_count() {
        let items: Vec<String> = (0..200).map(|i| format!("item{i}")).collect();
        let result = truncate_results(items, 10);
        assert_eq!(result.len(), 10);
    }
}
