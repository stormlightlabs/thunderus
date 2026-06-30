use std::{
    io::{self, Read},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use super::Cap;

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

    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => {
                let mut stdout = child.stdout.take().expect("stdout piped");
                let mut stderr = child.stderr.take().expect("stderr piped");
                let mut stdout_buf = Vec::with_capacity(4096);
                let mut stderr_buf = Vec::with_capacity(1024);
                stdout.read_to_end(&mut stdout_buf)?;
                stderr.read_to_end(&mut stderr_buf)?;

                if stdout_buf.len() > Cap::MaxOutputBytes.into() {
                    stdout_buf.truncate(Cap::MaxOutputBytes.into());
                }
                if stderr_buf.len() > Cap::MaxOutputBytes.into() {
                    stderr_buf.truncate(Cap::MaxOutputBytes.into());
                }

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
