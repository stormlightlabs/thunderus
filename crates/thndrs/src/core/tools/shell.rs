//! Shell/process manager.
//!
//! Runs commands from the workspace root with output streaming, timeouts,
//! cancellation, and a process registry that tracks active commands.
//!
//! ## Design
//!
//! - Commands run from the workspace root by default. An optional `cwd` can be
//!   specified relative to the root; paths escaping the root are rejected.
//! - Output is captured from piped stdout/stderr. The blocking read happens on
//!   a worker thread; the TUI drains the result through the normal tool event
//!   channel so it never blocks.
//! - Timeouts kill the process and produce a `Timeout` status.
//! - Cancellation is cooperative: a shared [`CancelFlag`] is checked by the
//!   worker thread between reads; when signalled the process is killed and the
//!   result is recorded as `Cancelled`.
//! - A [`ProcessRegistry`] tracks active commands, separating one-shot commands
//!   (waited on for completion) from long-lived background processes (left
//!   running and tracked by id).
//!
//! ## Safety
//!
//! - `fd --exec`, `rg --pre`, `sed -i`, `awk system()` and arbitrary
//!   shell-string execution are not exposed by this module — the model provides
//!   an argv array, never a shell string.
//! - The command runs via `std::process::Command` argv; no `/bin/sh -c`.
//! - stdout/stderr bytes are capped at [`MAX_OUTPUT_BYTES`]; lines truncate
//!   at `MAX_LINE_LEN`.
//! - Paths are contained to the workspace root.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::{MAX_OUTPUT_BYTES, TIMEOUT_SECS, ToolDefinition, ToolOutput, ToolUseRequest, path};
use crate::app::ToolStatus;
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::utils;
use thndrs_agent::CancelToken;

/// Maximum number of output lines retained for the transcript/tool result.
const MAX_OUTPUT_LINES: usize = 200;
pub(crate) const NAME: &str = "run_shell";

/// Outcome of waiting for a process, honoring timeout and cancellation.
enum WaitOutcome {
    Exited(i32),
    Timeout,
    Cancelled,
}

/// Process lifecycle status recorded by the registry and in the transcript.
///
/// Mirrors [`ToolStatus`] but adds `Timeout` and `Cancelled` which are
/// process-specific terminal states.
///
/// `Running` is only reached for background processes tracked by the registry;
/// one-shot commands always complete before the status is observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ProcessStatus {
    /// Still running.
    Running,
    /// Exited with status 0.
    Ok,
    /// Exited with a non-zero status.
    Failed,
    /// Killed after exceeding the timeout.
    Timeout,
    /// Killed after a cancellation request.
    Cancelled,
}

impl ProcessStatus {
    /// One-word label used in transcript display and session records.
    pub fn label(&self) -> &'static str {
        match self {
            ProcessStatus::Running => "running",
            ProcessStatus::Ok => "ok",
            ProcessStatus::Failed => "failed",
            ProcessStatus::Timeout => "timeout",
            ProcessStatus::Cancelled => "cancelled",
        }
    }

    /// Convert to the transcript-level tool status.
    pub const fn to_tool_status(self) -> ToolStatus {
        match self {
            ProcessStatus::Running => ToolStatus::Running,
            ProcessStatus::Ok => ToolStatus::Ok,
            ProcessStatus::Failed | ProcessStatus::Timeout => ToolStatus::Failed,
            ProcessStatus::Cancelled => ToolStatus::Cancelled,
        }
    }
}

impl From<ProcessStatus> for ToolStatus {
    fn from(status: ProcessStatus) -> Self {
        status.to_tool_status()
    }
}

/// Whether a command is a one-shot (waited for completion) or a long-lived
/// background process (tracked separately by the registry).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ProcessKind {
    /// Waited for completion; result is captured synchronously by the caller.
    OneShot,
    /// Left running after dispatch; tracked by id in the registry.
    Background,
}

impl ProcessKind {
    /// Lowercase label used in display and records.
    pub fn label(&self) -> &'static str {
        match self {
            ProcessKind::OneShot => "one-shot",
            ProcessKind::Background => "background",
        }
    }
}

impl fmt::Display for ProcessKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Structured result of a process execution.
///
/// Captures the command, working directory, exit status, stdout/stderr
/// (capped and line-truncated), and elapsed time. This is the audit record
/// persisted for session records; the full raw output is never stored beyond
/// the byte cap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessResult {
    /// The argv that was run (program + args).
    pub command: Vec<String>,
    /// Working directory the command ran in.
    pub cwd: PathBuf,
    /// Final lifecycle status.
    pub status: ProcessStatus,
    /// Exit code if the process exited normally, else `None`.
    pub exit_code: Option<i32>,
    /// Captured stdout, line-capped and byte-capped.
    pub stdout: Vec<String>,
    /// Captured stderr, line-capped and byte-capped.
    pub stderr: Vec<String>,
    /// Wall-clock elapsed time.
    pub elapsed: Duration,
    /// Whether this was a one-shot or background process.
    pub kind: ProcessKind,
}

impl ProcessResult {
    /// Render a compact single-line summary for transcript display.
    pub fn summary(&self) -> String {
        let argv = self.command.join(" ");
        let elapsed_ms = self.elapsed.as_millis();
        match self.status {
            ProcessStatus::Running => format!("$ {argv} [{}]", self.kind.label()),
            other => format!("$ {argv} [{} {} {}ms]", self.kind.label(), other.label(), elapsed_ms),
        }
    }

    /// Lines for the tool [`ToolOutput`]: summary followed by stdout/stderr
    /// markers and content. The summary line is also redacted in case the
    /// command argv itself contains secret-like values.
    pub fn to_output_lines(&self) -> Vec<String> {
        let mut lines = vec![redact_secrets(&self.summary())];
        if !self.stdout.is_empty() {
            lines.push(String::from("── stdout ──"));
            lines.extend(self.stdout.iter().cloned());
        }
        if !self.stderr.is_empty() {
            lines.push(String::from("── stderr ──"));
            lines.extend(self.stderr.iter().cloned());
        }
        lines
    }

    /// Build a failed [`ToolOutput`] from this result.
    pub fn to_failed_output(&self) -> ToolOutput {
        let err = match self.status {
            ProcessStatus::Timeout => {
                format!("command timed out after {}ms", self.elapsed.as_millis())
            }
            ProcessStatus::Cancelled => String::from("command cancelled"),
            _ => {
                let code = self.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string());
                format!("command failed (exit {code})")
            }
        };
        ToolOutput::failed("run_shell", err)
    }

    /// Build the [`ToolOutput`] corresponding to this process result.
    pub fn to_tool_output(&self) -> ToolOutput {
        match ToolStatus::from(self.status) {
            ToolStatus::Ok => ToolOutput::ok(NAME, self.to_output_lines()),
            _ => {
                let mut output = self.to_failed_output();
                let lines = self.to_output_lines();
                output.display.lines = lines.clone();
                output.model.lines = lines;
                output
            }
        }
    }
}

/// A running process tracked by the registry.
#[derive(Debug)]
pub struct ActiveProcess {
    /// Unique id assigned by the registry.
    pub id: u64,
    /// The argv that was run.
    pub command: Vec<String>,
    /// Working directory. Stored for audit/display.
    pub cwd: PathBuf,
    /// One-shot or background.
    pub kind: ProcessKind,
    /// Cancellation flag shared with the worker thread.
    pub cancel: CancelToken,
    /// When the process started.
    pub started: Instant,
}

impl ActiveProcess {
    /// Elapsed time since the process started.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// Registry of active processes.
///
/// Tracks running commands by id. One-shot processes are removed when they
/// complete; background processes remain until explicitly removed or cancelled.
///
/// Wired into the live app: background `run_shell` results are registered
/// here, the `:bg` command lists them, and `cancel_all` runs on quit.
#[derive(Debug, Default)]
pub struct ProcessRegistry {
    next_id: u64,
    active: HashMap<u64, ActiveProcess>,
}

impl ProcessRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of currently active processes (one-shot + background).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Whether the registry has no active processes.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Number of background processes.
    #[cfg(test)]
    pub fn background_count(&self) -> usize {
        self.active
            .values()
            .filter(|p| p.kind == ProcessKind::Background)
            .count()
    }

    /// Number of one-shot processes.
    #[cfg(test)]
    pub fn one_shot_count(&self) -> usize {
        self.active.values().filter(|p| p.kind == ProcessKind::OneShot).count()
    }

    /// Register a new process and return its id.
    pub fn register(&mut self, command: Vec<String>, cwd: PathBuf, kind: ProcessKind, cancel: CancelToken) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.active.insert(
            id,
            ActiveProcess { id, command, cwd, kind, cancel, started: Instant::now() },
        );
        id
    }

    /// Look up an active process by id.
    pub fn get(&self, id: u64) -> Option<&ActiveProcess> {
        self.active.get(&id)
    }

    /// Request cancellation of a process by id.
    ///
    /// Returns `true` if the process existed and cancellation was signalled.
    #[cfg(test)]
    pub fn cancel(&mut self, id: u64) -> bool {
        if let Some(p) = self.active.get(&id) {
            p.cancel();
            true
        } else {
            false
        }
    }

    /// Remove a completed process from the registry.
    #[cfg(test)]
    pub fn remove(&mut self, id: u64) -> Option<ActiveProcess> {
        self.active.remove(&id)
    }

    /// Cancel all active processes.
    pub fn cancel_all(&mut self) {
        for p in self.active.values() {
            p.cancel();
        }
    }

    /// Iterate over active process ids.
    #[cfg(test)]
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.active.keys().copied()
    }

    /// Iterate over active background process ids.
    pub fn background_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.active
            .values()
            .filter(|p| p.kind == ProcessKind::Background)
            .map(|p| p.id)
    }
}

/// Arguments for a shell command execution.
#[derive(Clone, Debug)]
pub struct ShellArgs {
    /// Program to run (e.g. `"cargo"`, `"ls"`, `"echo"`).
    pub program: String,
    /// Argv after the program.
    pub args: Vec<String>,
    /// Optional working directory relative to the workspace root.
    /// Defaults to the workspace root.
    pub cwd: Option<PathBuf>,
    /// Wall-clock timeout. Defaults to [`TIMEOUT_SECS`].
    pub timeout: Option<Duration>,
    /// One-shot or background.
    pub kind: ProcessKind,
}

impl ShellArgs {
    /// The full argv (program + args).
    pub fn argv(&self) -> Vec<String> {
        let mut v = vec![self.program.clone()];
        v.extend(self.args.iter().cloned());
        v
    }
}

/// Provider-visible definition for `run_shell`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"run_shell

Run an argv command in the workspace and capture stdout, stderr, and exit status.

Prefer narrow tools when they fit: find_files, search_text, read_file_range,
create_file, replace_range, read_url. Use for build, test, format, inspection.

Runs as thndrs with its permissions — not sandboxed. Avoid destructive commands
unless explicitly requested. Output is capped, truncated, and redacted.
Timeouts enforced."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "argv": { "type": "array", "minItems": 1, "items": { "type": "string" }, "description": "Full argv: program followed by its arguments." },
                "cwd": { "type": "string", "description": "Optional working directory relative to the workspace root." },
                "timeout_ms": { "type": "integer", "minimum": 1, "description": "Optional timeout in milliseconds." },
                "background": { "type": "boolean", "description": "If true, run as a long-lived background process." }
            },
            "required": ["argv"]
        }),
    )
}

/// Parse provider JSON arguments for `run_shell`.
pub fn parse_arguments(arguments: &str) -> Result<ShellArgs, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments)
        .map_err(|error| ToolError::InvalidArguments(format!("invalid JSON: {error}")))?;
    let (program, cmd_args) = parse_argv(&args)?;
    let cwd = args.get("cwd").and_then(|value| value.as_str()).map(PathBuf::from);
    let timeout = match optional_u64(&args, "timeout_ms")? {
        Some(0) => {
            return Err(ToolError::InvalidArguments(
                "'timeout_ms' must be greater than zero".to_string(),
            ));
        }
        Some(milliseconds) => Some(Duration::from_millis(milliseconds)),
        None => optional_u64(&args, "timeout_secs")?.map(Duration::from_secs),
    };
    let kind = if args
        .get("background")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        ProcessKind::Background
    } else {
        ProcessKind::OneShot
    };

    Ok(ShellArgs { program, args: cmd_args, cwd, timeout, kind })
}

fn parse_argv(args: &serde_json::Value) -> Result<(String, Vec<String>), ToolError> {
    if let Some((field, argv)) = args
        .get("argv")
        .map(|argv| ("argv", argv))
        .or_else(|| args.get("command").map(|command| ("command", command)))
    {
        let argv = argv
            .as_array()
            .ok_or_else(|| ToolError::InvalidArguments(format!("'{field}' must be an array")))?;
        let argv = argv
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ToolError::InvalidArguments(format!("{field}[{index}] must be a string")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (program, command_args) = argv
            .split_first()
            .ok_or_else(|| ToolError::InvalidArguments(format!("'{field}' must contain a program")))?;
        if program.is_empty() {
            return Err(ToolError::InvalidArguments(format!("{field}[0] must not be empty")));
        }
        return Ok((program.clone(), command_args.to_vec()));
    }

    let program = args
        .get("program")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let command_args = args
        .get("args")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok((program, command_args))
}

fn optional_u64(args: &serde_json::Value, field: &str) -> Result<Option<u64>, ToolError> {
    match args.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| ToolError::InvalidArguments(format!("'{field}' must be a non-negative integer"))),
    }
}

/// Execute a registry request for `run_shell`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    let cancel = CancelToken::new();
    execute_request_with_cancel(request, ctx.root, &cancel)
}

/// Execute a `run_shell` request with the cancellation token for its enclosing
/// agent run.
///
/// The registry entry uses [`execute_request`] to preserve its stable generic
/// executor signature. The live agent dispatcher calls this variant so
/// stopping an agent also terminates its active shell child.
pub(crate) fn execute_request_with_cancel(
    request: &ToolUseRequest, root: &Path, cancel: &CancelToken,
) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(args) => execute_args(&args, root, cancel),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

/// Run a shell command with streaming output capture, timeout, and
/// cancellation.
///
/// This is the synchronous execution path used for one-shot commands. The
/// blocking read runs on the calling thread; callers that need non-blocking
/// behavior should run this on a worker thread and drain the returned
/// [`ProcessResult`] through the agent event channel.
///
/// The process is killed if:
/// - the timeout elapses, or
/// - the [`CancelFlag`] is signalled.
///
/// stdout/stderr are read on dedicated threads so that a process producing no
/// output (e.g. `sleep 30`) can still be killed on timeout/cancellation. The
/// captured output is capped at [`MAX_OUTPUT_BYTES`] bytes and
/// [`MAX_OUTPUT_LINES`] lines. Lines longer than `MAX_LINE_LEN` chars are
/// truncated with `...`.
pub fn run_command(args: &ShellArgs, root: &Path, cancel: &CancelToken) -> Result<ProcessResult, String> {
    let cwd = resolve_cwd(root, &args.cwd)?;
    let argv = args.argv();

    let mut cmd = Command::new(&args.program);
    cmd.args(&args.args)
        .current_dir(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let timeout = args.timeout.unwrap_or(Duration::from_secs(TIMEOUT_SECS));
    let start = Instant::now();

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn '{}': {e}", args.program))?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let stdout_handle = std::thread::spawn(move || read_to_capped_vec(stdout));
    let stderr_handle = std::thread::spawn(move || read_to_capped_vec(stderr));

    let final_status = wait_with_timeout(&mut child, &timeout, cancel, &start);

    let elapsed = start.elapsed();
    let (status, exit_code) = match final_status {
        WaitOutcome::Exited(code) => {
            if code == 0 {
                (ProcessStatus::Ok, Some(code))
            } else {
                (ProcessStatus::Failed, Some(code))
            }
        }
        WaitOutcome::Timeout => (ProcessStatus::Timeout, None),
        WaitOutcome::Cancelled => (ProcessStatus::Cancelled, None),
    };

    let stdout_buf = stdout_handle.join().unwrap_or_default();
    let stderr_buf = stderr_handle.join().unwrap_or_default();

    Ok(ProcessResult {
        command: argv,
        cwd,
        status,
        exit_code,
        stdout: split_and_cap(&stdout_buf),
        stderr: split_and_cap(&stderr_buf),
        elapsed,
        kind: args.kind,
    })
}

/// Execute a one-shot shell command and return a [`ToolOutput`] suitable for
/// the transcript and tool-result channel.
///
/// This is the entry point wired into [`crate::tools::dispatch_full`]. It runs
/// `run_command` on the calling thread (the agent loop already runs on a
/// background thread), then converts the result into a [`ToolOutput`].
#[cfg(test)]
pub fn exec(args: &ShellArgs, root: &Path) -> ToolOutput {
    let cancel = CancelToken::new();
    match run_command(args, root, &cancel) {
        Ok(result) => output_from_result(&result),
        Err(e) => ToolOutput::failed(NAME, e),
    }
}

/// Redact known secret patterns from a line of command output.
///
/// This is a best-effort deterministic redaction — it covers common formats
/// (API keys prefixed with `sk-`, bearer tokens, password assignments) but
/// cannot catch every possible secret. The patterns are intentionally simple
/// so they are predictable and auditable.
///
/// Redacted values are replaced with `[REDACTED]` so the user can see that a
/// secret was present and scrubbed.
pub fn redact_secrets(line: &str) -> String {
    let mut result = line.to_string();
    let sk_re = regex_lite::Regex::new(r"\bsk-[A-Za-z0-9_]{8,}").expect("valid regex");
    result = sk_re.replace_all(&result, "sk-[REDACTED]").to_string();

    let bearer_re = regex_lite::Regex::new(r"(?i)bearer\s+[A-Za-z0-9_\-\.]{10,}").expect("valid regex");
    result = bearer_re.replace_all(&result, "Bearer [REDACTED]").to_string();

    let assign_re = regex_lite::Regex::new(r"(?i)(password|passwd|api_key|apikey|access_token|secret)\s*[:=]\s*\S{4,}")
        .expect("valid regex");

    assign_re.replace_all(&result, "$1=[REDACTED]").to_string()
}

/// Wait for a child to exit, killing it if the timeout elapses or cancellation
/// is signalled.
fn wait_with_timeout(child: &mut Child, timeout: &Duration, cancel: &CancelToken, start: &Instant) -> WaitOutcome {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status.code().unwrap_or(-1)),
            Ok(None) => {
                if cancel.is_cancelled() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return WaitOutcome::Cancelled;
                }
                if start.elapsed() > *timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return WaitOutcome::Timeout;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return WaitOutcome::Cancelled;
            }
        }
    }
}

/// Resolve the working directory for a command, defaulting to the workspace
/// root. If `cwd` is provided it must be within `root`.
fn resolve_cwd(root: &Path, cwd: &Option<PathBuf>) -> Result<PathBuf, String> {
    match cwd {
        None => Ok(root.to_path_buf()),
        Some(rel) => {
            let resolved = path::resolve_within_root(root, &rel.to_string_lossy()).map_err(|e| e.to_string())?;
            if !resolved.is_dir() {
                return Err(format!("working directory is not a directory: {}", resolved.display()));
            }
            Ok(resolved)
        }
    }
}

/// Read a piped stream to a capped byte buffer. Runs on a dedicated reader
/// thread so the main thread can still poll try_wait for timeout/cancellation.
fn read_to_capped_vec<R: Read>(mut stream: R) -> Vec<u8> {
    let max_bytes: usize = MAX_OUTPUT_BYTES;
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = max_bytes.saturating_sub(buf.len());
                if remaining == 0 {
                    break;
                }
                let take = n.min(remaining);
                buf.extend_from_slice(&chunk[..take]);
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    buf
}

/// Split a byte buffer into lines, capping the line count, truncating long
/// lines, and redacting known secret patterns.
fn split_and_cap(buf: &[u8]) -> Vec<String> {
    let content = String::from_utf8_lossy(buf);
    let mut lines: Vec<String> = content
        .lines()
        .map(redact_secrets)
        .map(|line| utils::truncate_line(&line))
        .take(MAX_OUTPUT_LINES)
        .collect();

    let total_lines = content.lines().count();
    if total_lines > MAX_OUTPUT_LINES {
        let extra = total_lines - MAX_OUTPUT_LINES;
        lines.push(format!("…({extra} more lines)"));
    }

    lines
}

fn execute_args(args: &ShellArgs, root: &Path, cancel: &CancelToken) -> ToolExecution {
    if args.program.is_empty() {
        return ToolExecution::output(ToolOutput::failed(
            NAME,
            "missing command: provide non-empty 'argv', 'command', or 'program'".to_string(),
        ));
    }

    match run_command(args, root, cancel) {
        Ok(result) => ToolExecution::full(output_from_result(&result), None, Some(result)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error)),
    }
}

fn output_from_result(result: &ProcessResult) -> ToolOutput {
    result.to_tool_output()
}
