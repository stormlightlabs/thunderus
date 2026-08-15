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
//! - Cancellation is cooperative: a shared [`CancelToken`] is checked by the
//!   worker thread between reads; when signalled the process is killed and the
//!   result is recorded as `Cancelled`.
//! - A [`ProcessRegistry`] tracks active commands, separating one-shot commands
//!   (waited on for completion) from long-lived background processes. A
//!   background child gets an independent cancellation handle, stays owned by
//!   the registry after the tool call returns, and is reaped on completion,
//!   explicit cancellation, application quit, or registry drop.
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
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
use process_wrap::std::{ChildWrapper, CommandWrap};

use super::{MAX_LINE_LEN, MAX_OUTPUT_BYTES, TIMEOUT_SECS, ToolDefinition, ToolOutput, ToolUseRequest, path};
use crate::app::ToolStatus;
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::utils;
use thndrs_agent::CancelToken;

/// Maximum number of output lines retained for the transcript/tool result.
const MAX_OUTPUT_LINES: usize = 200;
pub const NAME: &str = "run_shell";

type OwnedChild = Box<dyn ChildWrapper>;

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
    /// Registry id for an owned background process, if applicable.
    pub process_id: Option<u64>,
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
    /// Whether the captured command output was capped or line-truncated.
    pub output_truncated: bool,
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
            ProcessStatus::Failed => {
                let exit_code = self.exit_code.map_or_else(|| "?".to_string(), |code| code.to_string());
                format!(
                    "$ {argv} [{} failed exit {exit_code} {elapsed_ms}ms]",
                    self.kind.label()
                )
            }
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
        if self.output_truncated && !lines.iter().any(|line| line.contains("output truncated")) {
            lines.push(String::from("[command output truncated]"));
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
            ToolStatus::Running | ToolStatus::Ok => ToolOutput::ok(NAME, self.to_output_lines()),
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

/// A bounded snapshot of output retained for an active process.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessOutput {
    /// Redacted, line-capped stdout retained so far.
    pub stdout: Vec<String>,
    /// Redacted, line-capped stderr retained so far.
    pub stderr: Vec<String>,
}

/// A process snapshot returned by the registry.
#[derive(Clone, Debug)]
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
    /// Current lifecycle status.
    pub status: ProcessStatus,
    /// Bounded output retained so far.
    pub output: ProcessOutput,
    control: Option<Arc<ProcessControl>>,
}

impl ActiveProcess {
    /// Elapsed time since the process started.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        if let Some(control) = &self.control {
            control.cancel();
        } else {
            self.cancel.cancel();
        }
    }
}

/// Registry of active processes.
///
/// Tracks running commands by id. One-shot processes are removed when they
/// complete; background processes remain until explicitly removed or cancelled.
///
/// Wired into the live app: background `run_shell` results are registered
/// here, the `:bg` command lists them, and `cancel_all` runs on quit.
#[derive(Clone, Debug, Default)]
pub struct ProcessRegistry {
    inner: Arc<RegistryInner>,
}

#[derive(Debug, Default)]
struct RegistryInner {
    state: Mutex<RegistryState>,
    foreground_output: Mutex<Option<Arc<OutputCapture>>>,
}

#[derive(Debug, Default)]
struct RegistryState {
    next_id: u64,
    active: HashMap<u64, TrackedProcess>,
}

#[derive(Debug)]
struct TrackedProcess {
    id: u64,
    command: Vec<String>,
    cwd: PathBuf,
    kind: ProcessKind,
    cancel: CancelToken,
    started: Instant,
    control: Option<Arc<ProcessControl>>,
    output: Arc<OutputCapture>,
    result: Arc<Mutex<Option<ProcessResult>>>,
    worker: Option<JoinHandle<()>>,
    announced: bool,
}

impl TrackedProcess {
    fn synthetic(id: u64, command: Vec<String>, cwd: PathBuf, kind: ProcessKind, cancel: CancelToken) -> Self {
        Self {
            id,
            command,
            cwd,
            kind,
            cancel,
            started: Instant::now(),
            control: None,
            output: Arc::new(OutputCapture::default()),
            result: Arc::new(Mutex::new(None)),
            worker: None,
            announced: true,
        }
    }

    fn snapshot(&self) -> ActiveProcess {
        let result = self.result.lock().ok().and_then(|result| result.clone());
        let output = result.as_ref().map_or_else(
            || self.output.snapshot().0,
            |result| ProcessOutput { stdout: result.stdout.clone(), stderr: result.stderr.clone() },
        );
        ActiveProcess {
            id: self.id,
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            kind: self.kind,
            cancel: self.cancel.clone(),
            started: self.started,
            status: result.map_or(ProcessStatus::Running, |result| result.status),
            output,
            control: self.control.clone(),
        }
    }
}

#[derive(Debug)]
struct ProcessControl {
    cancel: CancelToken,
    child: Arc<Mutex<Option<OwnedChild>>>,
}

impl ProcessControl {
    fn cancel(&self) {
        self.cancel.cancel();
        if let Ok(mut child) = self.child.lock()
            && let Some(child) = child.as_mut()
        {
            let _ = child.start_kill();
        }
    }
}

#[derive(Debug, Default)]
struct OutputCapture {
    stdout: Mutex<Vec<u8>>,
    stderr: Mutex<Vec<u8>>,
    stdout_truncated: std::sync::atomic::AtomicBool,
    stderr_truncated: std::sync::atomic::AtomicBool,
    readers: AtomicUsize,
}

impl OutputCapture {
    fn append(&self, stdout: bool, bytes: &[u8]) {
        let (target, truncated) =
            if stdout { (&self.stdout, &self.stdout_truncated) } else { (&self.stderr, &self.stderr_truncated) };
        let Ok(mut target) = target.lock() else {
            return;
        };
        let remaining = MAX_OUTPUT_BYTES.saturating_sub(target.len());
        if bytes.len() > remaining {
            truncated.store(true, Ordering::SeqCst);
        }
        target.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
    }

    fn snapshot(&self) -> (ProcessOutput, bool) {
        let stdout = self.stdout.lock().map(|bytes| bytes.clone()).unwrap_or_default();
        let stderr = self.stderr.lock().map(|bytes| bytes.clone()).unwrap_or_default();
        let (stdout, stdout_truncated) = split_and_cap(&stdout);
        let (stderr, stderr_truncated) = split_and_cap(&stderr);
        (
            ProcessOutput { stdout, stderr },
            self.is_truncated() || stdout_truncated || stderr_truncated,
        )
    }

    fn is_truncated(&self) -> bool {
        self.stdout_truncated.load(Ordering::SeqCst) || self.stderr_truncated.load(Ordering::SeqCst)
    }
}

struct BackgroundMonitor {
    id: u64,
    command: Vec<String>,
    cwd: PathBuf,
    timeout: Duration,
    start: Instant,
    cancel: CancelToken,
    child: Arc<Mutex<Option<OwnedChild>>>,
    output: Arc<OutputCapture>,
    result_slot: Arc<Mutex<Option<ProcessResult>>>,
}

impl ProcessRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the output captured so far for the active one-shot command.
    pub fn foreground_output(&self) -> Option<ProcessOutput> {
        self.inner
            .foreground_output
            .lock()
            .ok()?
            .as_ref()
            .map(|output| output.snapshot().0)
    }

    fn set_foreground_output(&self, output: Arc<OutputCapture>) {
        *recover_lock(&self.inner.foreground_output) = Some(output);
    }

    fn clear_foreground_output(&self, output: &Arc<OutputCapture>) {
        let mut active = recover_lock(&self.inner.foreground_output);
        if active.as_ref().is_some_and(|current| Arc::ptr_eq(current, output)) {
            *active = None;
        }
    }

    /// Number of currently active processes (one-shot + background).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.state.lock().map(|state| state.active.len()).unwrap_or(0)
    }

    /// Whether the registry has no active processes.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of background processes.
    #[cfg(test)]
    pub fn background_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|process| {
                        process.kind == ProcessKind::Background && process.snapshot().status == ProcessStatus::Running
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Number of one-shot processes.
    #[cfg(test)]
    pub fn one_shot_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .map(|state| state.active.values().filter(|p| p.kind == ProcessKind::OneShot).count())
            .unwrap_or(0)
    }

    /// Register a new process and return its id.
    pub fn register(&self, command: Vec<String>, cwd: PathBuf, kind: ProcessKind, cancel: CancelToken) -> u64 {
        let mut state = recover_lock(&self.inner.state);
        let id = state.next_id;
        state.next_id += 1;
        state
            .active
            .insert(id, TrackedProcess::synthetic(id, command, cwd, kind, cancel));
        id
    }

    /// Look up an active process by id.
    pub fn get(&self, id: u64) -> Option<ActiveProcess> {
        let state = self.inner.state.lock().ok()?;
        state.active.get(&id).map(TrackedProcess::snapshot)
    }

    /// Request cancellation of a process by id.
    ///
    /// Returns `true` if the process existed and cancellation was signalled.
    pub fn cancel(&self, id: u64) -> bool {
        let Some(process) = self.get(id) else {
            return false;
        };
        if process.status != ProcessStatus::Running {
            return false;
        }
        process.cancel();
        true
    }

    /// Remove a completed process from the registry.
    pub fn remove(&self, id: u64) -> Option<ActiveProcess> {
        let mut tracked = self.inner.state.lock().ok()?.active.remove(&id)?;
        if let Some(control) = &tracked.control {
            control.cancel();
        }
        join_worker(tracked.worker.take());
        Some(tracked.snapshot())
    }

    /// Cancel all active processes.
    pub fn cancel_all(&self) {
        let state = recover_lock(&self.inner.state);
        for process in state.active.values() {
            if process.snapshot().status == ProcessStatus::Running {
                if let Some(control) = &process.control {
                    control.cancel();
                } else {
                    process.cancel.cancel();
                }
            }
        }
    }

    /// Iterate over active process ids.
    #[cfg(test)]
    pub fn ids(&self) -> impl Iterator<Item = u64> {
        self.inner
            .state
            .lock()
            .map(|state| state.active.keys().copied().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Iterate over active background process ids.
    pub fn background_ids(&self) -> impl Iterator<Item = u64> {
        self.inner
            .state
            .lock()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|process| {
                        process.kind == ProcessKind::Background && process.snapshot().status == ProcessStatus::Running
                    })
                    .map(|process| process.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
    }

    /// Mark an application-visible background process as announced.
    pub fn announce(&self, id: u64) -> bool {
        let Ok(mut state) = self.inner.state.lock() else {
            return false;
        };
        let Some(process) = state.active.get_mut(&id) else {
            return false;
        };
        process.announced = true;
        true
    }

    /// Return completed background results after their start event was observed.
    pub fn drain_completed(&self) -> Vec<ProcessResult> {
        self.drain_completed_inner(false)
    }

    /// Cancel and reap every owned process, returning all final results.
    pub fn shutdown(&self) -> Vec<ProcessResult> {
        let tracked = {
            let mut state = recover_lock(&self.inner.state);
            for process in state.active.values() {
                if let Some(control) = &process.control {
                    control.cancel();
                } else {
                    process.cancel.cancel();
                }
            }
            state.active.drain().map(|(_, process)| process).collect::<Vec<_>>()
        };

        let mut results = Vec::new();
        for process in tracked {
            join_worker(process.worker);
            if let Ok(mut result) = process.result.lock()
                && let Some(result) = result.take()
            {
                results.push(result);
            }
        }
        results.sort_by_key(|result| result.process_id);
        results
    }

    fn drain_completed_inner(&self, include_unannounced: bool) -> Vec<ProcessResult> {
        let tracked = {
            let mut state = recover_lock(&self.inner.state);
            if include_unannounced {
                for process in state.active.values() {
                    if let Some(control) = &process.control {
                        control.cancel();
                    } else {
                        process.cancel.cancel();
                    }
                }
            }
            let mut ids = Vec::new();
            for (id, process) in &state.active {
                let completed = process.result.lock().ok().is_some_and(|result| result.is_some());
                if (include_unannounced || process.announced) && completed {
                    ids.push(*id);
                }
            }
            ids.into_iter()
                .filter_map(|id| state.active.remove(&id))
                .collect::<Vec<_>>()
        };

        let mut results = Vec::new();
        for process in tracked {
            join_worker(process.worker);
            if let Ok(mut result) = process.result.lock()
                && let Some(result) = result.take()
            {
                results.push(result);
            }
        }
        results.sort_by_key(|result| result.process_id);
        results
    }

    pub fn spawn_background(
        &self, args: &ShellArgs, cwd: PathBuf, child: OwnedChild, start: Instant, cancel: CancelToken,
    ) -> u64 {
        let argv = args.argv();
        let timeout = args.timeout.unwrap_or(Duration::from_secs(TIMEOUT_SECS));
        let child = Arc::new(Mutex::new(Some(child)));
        let control = Arc::new(ProcessControl { cancel: cancel.clone(), child: child.clone() });
        let output = Arc::new(OutputCapture::default());
        let result = Arc::new(Mutex::new(None));
        if let Ok(mut child_guard) = child.lock()
            && let Some(child) = child_guard.as_mut()
        {
            if let Some(stdout) = child.stdout().take() {
                let _ = spawn_output_reader(stdout, output.clone(), true);
            }
            if let Some(stderr) = child.stderr().take() {
                let _ = spawn_output_reader(stderr, output.clone(), false);
            }
        }

        let mut state = recover_lock(&self.inner.state);
        let id = state.next_id;
        state.next_id += 1;
        let output_for_worker = output.clone();
        let result_for_worker = result.clone();
        let cancel_for_worker = cancel.clone();
        let cwd_for_worker = cwd.clone();
        let worker = std::thread::spawn(move || {
            BackgroundMonitor {
                id,
                command: argv,
                cwd: cwd_for_worker,
                timeout,
                start,
                cancel: cancel_for_worker,
                child,
                output: output_for_worker,
                result_slot: result_for_worker,
            }
            .run();
        });
        state.active.insert(
            id,
            TrackedProcess {
                id,
                command: args.argv(),
                cwd,
                kind: ProcessKind::Background,
                cancel,
                started: start,
                control: Some(control),
                output,
                result,
                worker: Some(worker),
                announced: false,
            },
        );
        id
    }
}

impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            let _ = self.shutdown();
        }
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

Prefer narrower tools when they fit. Use for build, test, format, and inspection.

Runs as thndrs with its permissions, not in a sandbox. Output is capped,
truncated, and redacted; timeouts are enforced. With background=true, the
interactive app owns the child, returns its registry id immediately, and
supports :bg listing and cancellation."#,
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

/// Execute a registry request for `run_shell`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    let cancel = CancelToken::new();
    execute_request_with_cancel_and_registry(request, ctx.root, &cancel, ctx.process_registry.as_ref())
}

/// Execute a `run_shell` request with the cancellation token for its enclosing
/// agent run.
///
/// The registry entry uses [`execute_request`] to preserve its stable generic
/// executor signature. The live agent dispatcher calls this variant so
/// stopping an agent also terminates its active shell child.
pub fn execute_request_with_cancel(request: &ToolUseRequest, root: &Path, cancel: &CancelToken) -> ToolExecution {
    execute_request_with_cancel_and_registry(request, root, cancel, None)
}

/// Execute a `run_shell` request with cancellation and an optional
/// application-owned background-process registry.
pub fn execute_request_with_cancel_and_registry(
    request: &ToolUseRequest, root: &Path, cancel: &CancelToken, registry: Option<&ProcessRegistry>,
) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(args) => execute_args(&args, root, cancel, registry),
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
/// - the [`CancelToken`] is signalled.
///
/// stdout/stderr are read on dedicated threads so that a process producing no
/// output (e.g. `sleep 30`) can still be killed on timeout/cancellation. The
/// captured output is capped at [`MAX_OUTPUT_BYTES`] bytes and
/// `MAX_OUTPUT_LINES` lines. Lines longer than `MAX_LINE_LEN` chars are
/// truncated with `...`.
pub fn run_command(args: &ShellArgs, root: &Path, cancel: &CancelToken) -> Result<ProcessResult, String> {
    run_command_with_registry(args, root, cancel, None)
}

/// Execute a shell command, handing background ownership to `registry`.
pub fn run_command_with_registry(
    args: &ShellArgs, root: &Path, cancel: &CancelToken, registry: Option<&ProcessRegistry>,
) -> Result<ProcessResult, String> {
    let cwd = resolve_cwd(root, &args.cwd)?;
    let argv = args.argv();

    if args.kind == ProcessKind::Background {
        let registry = registry
            .ok_or_else(|| String::from("background commands require an application-owned process registry"))?;
        let mut cmd = Command::new(&args.program);
        cmd.args(&args.args)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        let start = Instant::now();
        let child = spawn_owned_command(cmd).map_err(|error| format!("failed to spawn '{}': {error}", args.program))?;
        // Background ownership is independent from the enclosing agent turn:
        // cancelling one registry entry must not cancel its sibling tools or
        // the agent loop that started it.
        let process_cancel = CancelToken::new();
        let id = registry.spawn_background(args, cwd.clone(), child, start, process_cancel);
        return Ok(ProcessResult {
            process_id: Some(id),
            command: argv,
            cwd,
            status: ProcessStatus::Running,
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            output_truncated: false,
            elapsed: start.elapsed(),
            kind: ProcessKind::Background,
        });
    }

    run_foreground_command(args, cwd, argv, cancel, registry)
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
fn wait_with_timeout(
    child: &mut dyn ChildWrapper, timeout: &Duration, cancel: &CancelToken, start: &Instant,
) -> WaitOutcome {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // The direct child can exit while descendants retain its pipes.
                // Terminate the remaining owned group before joining readers.
                let _ = child.start_kill();
                return WaitOutcome::Exited(status.code().unwrap_or(-1));
            }
            Ok(None) => {
                if cancel.is_cancelled() {
                    let _ = child.kill();
                    return WaitOutcome::Cancelled;
                }
                if start.elapsed() > *timeout {
                    let _ = child.kill();
                    return WaitOutcome::Timeout;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
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

/// Split a byte buffer into lines, capping the line count, truncating long
/// lines, and redacting known secret patterns.
fn split_and_cap(buf: &[u8]) -> (Vec<String>, bool) {
    let content = String::from_utf8_lossy(buf);
    let mut line_truncated = false;
    let mut lines = Vec::new();
    for line in content.lines().take(MAX_OUTPUT_LINES) {
        let line = redact_secrets(line);
        line_truncated |= line.chars().count() > MAX_LINE_LEN;
        lines.push(utils::truncate_line(&line));
    }

    let total_lines = content.lines().count();
    if total_lines > MAX_OUTPUT_LINES {
        let extra = total_lines - MAX_OUTPUT_LINES;
        lines.push(format!("…({extra} more lines)"));
    }

    (lines, line_truncated || total_lines > MAX_OUTPUT_LINES)
}

fn execute_args(
    args: &ShellArgs, root: &Path, cancel: &CancelToken, registry: Option<&ProcessRegistry>,
) -> ToolExecution {
    if args.program.is_empty() {
        return ToolExecution::output(ToolOutput::failed(
            NAME,
            "missing command: provide non-empty 'argv', 'command', or 'program'".to_string(),
        ));
    }

    match run_command_with_registry(args, root, cancel, registry) {
        Ok(result) => ToolExecution::full(output_from_result(&result), None, Some(result)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error)),
    }
}

fn output_from_result(result: &ProcessResult) -> ToolOutput {
    result.to_tool_output()
}

fn join_worker(worker: Option<JoinHandle<()>>) {
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

fn recover_lock<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn spawn_output_reader<R: Read + Send + 'static>(
    mut reader: R, output: Arc<OutputCapture>, stdout: bool,
) -> JoinHandle<()> {
    output.readers.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        let mut chunk = [0_u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => output.append(stdout, &chunk[..n]),
                Err(ref error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        output.readers.fetch_sub(1, Ordering::SeqCst);
    })
}

impl BackgroundMonitor {
    fn run(self) {
        let Self { id, command, cwd, timeout, start, cancel, child, output, result_slot } = self;
        let outcome = loop {
            match try_wait_owned(&child) {
                Ok(Some(_status)) if cancel.is_cancelled() => break WaitOutcome::Cancelled,
                Ok(Some(status)) => break WaitOutcome::Exited(status.code().unwrap_or(-1)),
                Ok(None) => {
                    if cancel.is_cancelled() {
                        kill_and_reap(&child);
                        break WaitOutcome::Cancelled;
                    }
                    if start.elapsed() > timeout {
                        kill_and_reap(&child);
                        break WaitOutcome::Timeout;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => {
                    kill_and_reap(&child);
                    break WaitOutcome::Cancelled;
                }
            }
        };

        let drain_deadline = Instant::now() + Duration::from_millis(100);
        while output.readers.load(Ordering::SeqCst) > 0 && Instant::now() < drain_deadline {
            std::thread::sleep(Duration::from_millis(1));
        }

        let (status, exit_code) = match outcome {
            WaitOutcome::Exited(code) if code == 0 => (ProcessStatus::Ok, Some(code)),
            WaitOutcome::Exited(code) => (ProcessStatus::Failed, Some(code)),
            WaitOutcome::Timeout => (ProcessStatus::Timeout, None),
            WaitOutcome::Cancelled => (ProcessStatus::Cancelled, None),
        };
        let (captured, output_truncated) = output.snapshot();
        let result = ProcessResult {
            process_id: Some(id),
            command,
            cwd,
            status,
            exit_code,
            stdout: captured.stdout,
            stderr: captured.stderr,
            output_truncated,
            elapsed: start.elapsed(),
            kind: ProcessKind::Background,
        };
        let mut slot = recover_lock(&result_slot);
        *slot = Some(result);
    }
}

fn try_wait_owned(child: &Arc<Mutex<Option<OwnedChild>>>) -> io::Result<Option<std::process::ExitStatus>> {
    let mut guard = child
        .lock()
        .map_err(|_| io::Error::other("process child lock poisoned"))?;
    let Some(child) = guard.as_mut() else {
        return Ok(None);
    };
    match child.try_wait()? {
        Some(status) => {
            // Do not let a successful direct child detach descendants from the
            // registry. They belong to this process entry and end with it.
            let _ = child.start_kill();
            *guard = None;
            Ok(Some(status))
        }
        None => Ok(None),
    }
}

fn kill_and_reap(child: &Arc<Mutex<Option<OwnedChild>>>) {
    let Ok(mut guard) = child.lock() else {
        return;
    };
    if let Some(child) = guard.as_mut() {
        let _ = child.kill();
    }
    *guard = None;
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

fn spawn_owned_command(command: Command) -> io::Result<OwnedChild> {
    let mut command = CommandWrap::from(command);
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    command.wrap(JobObject);
    command.spawn()
}

fn run_foreground_command(
    args: &ShellArgs, cwd: PathBuf, argv: Vec<String>, cancel: &CancelToken, registry: Option<&ProcessRegistry>,
) -> Result<ProcessResult, String> {
    let mut cmd = Command::new(&args.program);
    cmd.args(&args.args)
        .current_dir(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let timeout = args.timeout.unwrap_or(Duration::from_secs(TIMEOUT_SECS));
    let start = Instant::now();

    let mut child = spawn_owned_command(cmd).map_err(|e| format!("failed to spawn '{}': {e}", args.program))?;

    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| String::from("failed to capture child stdout"))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| String::from("failed to capture child stderr"))?;

    let output = Arc::new(OutputCapture::default());
    let stdout_reader = spawn_output_reader(stdout, output.clone(), true);
    let stderr_reader = spawn_output_reader(stderr, output.clone(), false);
    if let Some(registry) = registry {
        registry.set_foreground_output(output.clone());
    }

    let final_status = wait_with_timeout(child.as_mut(), &timeout, cancel, &start);

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

    let stdout_read_ok = stdout_reader.join().is_ok();
    let stderr_read_ok = stderr_reader.join().is_ok();
    let (captured, mut output_truncated) = output.snapshot();
    output_truncated |= !stdout_read_ok || !stderr_read_ok;
    if let Some(registry) = registry {
        registry.clear_foreground_output(&output);
    }
    Ok(ProcessResult {
        process_id: None,
        command: argv,
        cwd,
        status,
        exit_code,
        stdout: captured.stdout,
        stderr: captured.stderr,
        output_truncated,
        elapsed,
        kind: ProcessKind::OneShot,
    })
}
