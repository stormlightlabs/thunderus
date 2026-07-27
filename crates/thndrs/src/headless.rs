//! Headless single-prompt execution and output projection.
//!
//! This module shares the application lifecycle used by the TUI while keeping
//! command output, cooperative cancellation, and terminal exit codes local to
//! the non-interactive `thndrs run` surface.

use std::io::{self, IsTerminal, Read, Write};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use thndrs_agent::CancelToken;

use crate::app::{self, App, Msg, RunState, update};
use crate::cli::Cli;
use crate::cli::commands::run::{DEFAULT_STDIN_MAX_BYTES, RunCommand};
use crate::maybe_spawn_agent;

/// Frequency at which the headless runner observes Ctrl-C while waiting for an
/// agent event.
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

const EXIT_FAILURE: i32 = 1;
const EXIT_SETUP: i32 = 2;
const EXIT_POLICY: i32 = 3;
const EXIT_CANCELLED: i32 = 4;
const JSONL_SCHEMA_VERSION: u8 = 1;

/// Largest permitted standard-input limit, so a malformed invocation
/// cannot turn the headless command into an unbounded memory read.
const MAX_STDIN_MAX_BYTES: usize = 16 * 1024 * 1024;

static CANCELLATION: OnceLock<Mutex<Option<CancelToken>>> = OnceLock::new();
static CTRL_C_HANDLER: OnceLock<std::result::Result<(), String>> = OnceLock::new();

type Result<T> = std::result::Result<T, RunError>;

/// Terminal classification for a headless prompt run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Exit {
    Failure,
    Setup,
    Policy,
    Cancelled,
}

impl Exit {
    const fn code(self) -> i32 {
        match self {
            Self::Failure => EXIT_FAILURE,
            Self::Setup => EXIT_SETUP,
            Self::Policy => EXIT_POLICY,
            Self::Cancelled => EXIT_CANCELLED,
        }
    }
}

/// Headless output protocol and text-stream state for one run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeadlessOutput {
    Text { wrote_text: bool, ends_with_newline: bool },
    JsonLines,
}

impl HeadlessOutput {
    const fn from_jsonl(jsonl: bool) -> Self {
        if jsonl { Self::json_lines() } else { Self::text() }
    }

    const fn text() -> Self {
        Self::Text { wrote_text: false, ends_with_newline: false }
    }

    const fn json_lines() -> Self {
        Self::JsonLines
    }
}

/// Error returned after a headless run has reached a non-success terminal state.
#[derive(Debug)]
struct RunError {
    exit: Exit,
    message: String,
}

impl RunError {
    fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self { exit, message: message.into() }
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for RunError {}

/// Terminal event observed on the shared application event stream.
enum Terminal {
    Finished,
    Failed(String),
    Cancelled,
}

impl Terminal {
    fn from_event(event: &app::AgentEvent) -> Option<Self> {
        match event {
            app::AgentEvent::Finished => Some(Self::Finished),
            app::AgentEvent::Failed(message) => Some(Self::Failed(message.clone())),
            app::AgentEvent::Cancelled => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Provider-neutral event projection with stable JSON Lines names.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JsonEvent<'a> {
    Started {
        ephemeral: bool,
    },
    Status {
        message: &'a str,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    RequestAccounting {
        turn_id: &'a str,
        request_id: &'a str,
        attempt: u32,
        provider: &'a str,
        model: &'a str,
        serialized_bytes: u64,
        estimated_input_tokens: Option<u64>,
        provider_input_tokens: Option<u64>,
        provider_output_tokens: Option<u64>,
    },
    #[serde(rename = "text")]
    AssistantDelta {
        text: &'a str,
    },
    #[serde(rename = "reasoning")]
    ReasoningDelta {
        text: &'a str,
    },
    ToolStarted {
        id: &'a str,
        name: &'a str,
        arguments: &'a str,
    },
    ToolFinished {
        id: &'a str,
        status: &'static str,
        output: &'a [String],
    },
    StateProjection {
        id: &'a str,
        decision: &'static str,
        related_id: Option<&'a str>,
    },
    ModelMetadata {
        entries: &'a [(String, String)],
    },
    Retrying {
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        error: &'a str,
    },
    PermissionRequested {
        tool_call_id: &'a str,
        title: &'a str,
        selected: usize,
        options: Vec<JsonPermissionOption<'a>>,
    },
    PermissionResolved {
        tool_call_id: &'a str,
        outcome: &'a str,
    },
    AcpSession {
        agent_name: &'a str,
        session_id: &'a str,
        protocol_version: &'a str,
    },
    Completed,
    Failed {
        message: &'a str,
    },
    Cancelled,
}

impl<'a> JsonEvent<'a> {
    fn from_agent_event(event: &'a app::AgentEvent, ephemeral: bool) -> Self {
        match event {
            app::AgentEvent::Started => Self::Started { ephemeral },
            app::AgentEvent::Status(message) => Self::Status { message },
            app::AgentEvent::Usage { input_tokens, output_tokens } => {
                Self::Usage { input_tokens: *input_tokens, output_tokens: *output_tokens }
            }
            app::AgentEvent::RequestAccounting(accounting) => Self::RequestAccounting {
                turn_id: &accounting.turn_id,
                request_id: &accounting.request_id,
                attempt: accounting.attempt,
                provider: &accounting.provider,
                model: &accounting.model,
                serialized_bytes: accounting.serialized_bytes.value,
                estimated_input_tokens: accounting.estimated_input_tokens.value,
                provider_input_tokens: accounting
                    .provider_usage
                    .as_ref()
                    .and_then(|usage| usage.components.input_tokens),
                provider_output_tokens: accounting
                    .provider_usage
                    .as_ref()
                    .and_then(|usage| usage.components.output_tokens),
            },
            app::AgentEvent::AssistantDelta(text) => Self::AssistantDelta { text },
            app::AgentEvent::ReasoningDelta(text) => Self::ReasoningDelta { text },
            app::AgentEvent::ToolStarted { id, name, arguments } => Self::ToolStarted { id, name, arguments },
            app::AgentEvent::ToolFinished { id, output, status, .. } => {
                Self::ToolFinished { id, status: status.label(), output }
            }
            app::AgentEvent::StateProjectionDecision { id, decision } => {
                let (decision, related_id) = match decision {
                    thndrs_agent::context::StateProjectionDecision::Retained => ("retained", None),
                    thndrs_agent::context::StateProjectionDecision::DuplicateOf { canonical_id } => {
                        ("duplicate_of", Some(canonical_id.as_str()))
                    }
                    thndrs_agent::context::StateProjectionDecision::Supersedes { previous_id } => {
                        ("supersedes", Some(previous_id.as_str()))
                    }
                };
                Self::StateProjection { id, decision, related_id }
            }
            app::AgentEvent::ModelMetadataLoaded(entries) => Self::ModelMetadata { entries },
            app::AgentEvent::Retrying { attempt, max_attempts, delay_ms, error } => {
                Self::Retrying { attempt: *attempt, max_attempts: *max_attempts, delay_ms: *delay_ms, error }
            }
            app::AgentEvent::PermissionRequest(permission) => Self::PermissionRequested {
                tool_call_id: &permission.tool_call_id,
                title: &permission.title,
                selected: permission.selected,
                options: permission
                    .options
                    .iter()
                    .map(|option| JsonPermissionOption {
                        id: &option.id,
                        name: &option.name,
                        kind: option.kind.label(),
                    })
                    .collect(),
            },
            app::AgentEvent::PermissionResolved { tool_call_id, outcome } => {
                Self::PermissionResolved { tool_call_id, outcome }
            }
            app::AgentEvent::AcpSession(metadata) => Self::AcpSession {
                agent_name: &metadata.agent_name,
                session_id: &metadata.acp_session_id,
                protocol_version: &metadata.protocol_version,
            },
            app::AgentEvent::Finished => Self::Completed,
            app::AgentEvent::Failed(message) => Self::Failed { message },
            app::AgentEvent::Cancelled => Self::Cancelled,
        }
    }
}

/// Versioned JSON Lines record emitted by the headless event stream.
#[derive(Serialize)]
struct VersionedJsonEvent<'a> {
    version: u8,
    #[serde(flatten)]
    event: JsonEvent<'a>,
}

/// Stable permission-option projection for a headless JSON event.
#[derive(Serialize)]
struct JsonPermissionOption<'a> {
    id: &'a str,
    name: &'a str,
    kind: &'static str,
}

/// Process exit code for an application error.
///
/// Headless prompt runs reserve `0` for success, `1` for failures, `2` for
/// setup errors, `3` for policy errors, and `4` for cancellation.
///
/// Other application errors use `1`.
pub fn exit_code(error: &io::Error) -> i32 {
    error
        .get_ref()
        .and_then(|source| source.downcast_ref::<RunError>())
        .map_or(EXIT_FAILURE, |error| error.exit.code())
}

/// Run one prompt through the normal application lifecycle without terminal UI.
pub fn run_command(cli: &Cli, command: &RunCommand) -> io::Result<()> {
    let stdin = io::stdin();
    let prompt = resolve_prompt(command, &mut stdin.lock(), stdin.is_terminal()).map_err(io::Error::other)?;
    let cancellation = CancelToken::new();
    install_cancellation_handler(cancellation.clone())?;

    let stdout = io::stdout();
    let stderr = io::stderr();
    let result = run_with_io(
        cli,
        &prompt,
        HeadlessOutput::from_jsonl(command.jsonl),
        &mut stdout.lock(),
        &mut stderr.lock(),
        &cancellation,
    );
    clear_cancellation_handler();
    result.map_err(io::Error::other)
}

/// Register Ctrl-C as cooperative cancellation for the active headless turn.
fn install_cancellation_handler(cancellation: CancelToken) -> io::Result<()> {
    let slot = CANCELLATION.get_or_init(|| Mutex::new(None));
    let registration = CTRL_C_HANDLER.get_or_init(|| {
        ctrlc::set_handler(|| {
            let Some(slot) = CANCELLATION.get() else {
                return;
            };
            let cancellation = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            if let Some(cancellation) = cancellation {
                cancellation.cancel();
            }
        })
        .map_err(|error| error.to_string())
    });
    if let Err(error) = registration {
        return Err(io::Error::other(format!("failed to register Ctrl-C handler: {error}")));
    }
    *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(cancellation);
    Ok(())
}

/// Clear the active headless cancellation target after its run settles.
fn clear_cancellation_handler() {
    if let Some(slot) = CANCELLATION.get() {
        *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// Resolve the prompt from an optional argument and bounded piped input.
///
/// Interactive standard input is intentionally never read: without a prompt,
/// callers must pipe data instead.
fn resolve_prompt<Input: Read>(command: &RunCommand, input: &mut Input, input_is_terminal: bool) -> Result<String> {
    let piped = (!input_is_terminal)
        .then(|| read_piped_input(input, command.stdin_max_bytes))
        .transpose()?;

    match (command.prompt.as_deref(), piped.as_deref()) {
        (Some(prompt), Some(piped)) if !piped.is_empty() => Ok(format!("{prompt}\n\n{piped}")),
        (Some(prompt), _) => Ok(prompt.to_string()),
        (None, Some(piped)) if !piped.is_empty() => Ok(piped.to_string()),
        (None, Some(_)) => Err(RunError::new(
            Exit::Failure,
            "standard input was empty; pass a prompt or pipe non-empty UTF-8 input",
        )),
        (None, None) => Err(RunError::new(
            Exit::Failure,
            "pass a prompt or pipe non-empty UTF-8 input; interactive standard input is not read",
        )),
    }
}

/// Read at most one byte beyond the configured standard-input limit.
fn read_piped_input<Input: Read>(input: &mut Input, max_bytes: usize) -> Result<String> {
    if max_bytes == 0 || max_bytes > MAX_STDIN_MAX_BYTES {
        return Err(RunError::new(
            Exit::Failure,
            format!(
                "--stdin-max-bytes must be between 1 and {MAX_STDIN_MAX_BYTES} bytes (default: {DEFAULT_STDIN_MAX_BYTES})"
            ),
        ));
    }
    let bytes_to_read = max_bytes.checked_add(1).ok_or_else(|| {
        RunError::new(
            Exit::Failure,
            "--stdin-max-bytes is too large to enforce a bounded standard-input read",
        )
    })?;
    let mut bytes = Vec::with_capacity(bytes_to_read.min(DEFAULT_STDIN_MAX_BYTES));
    input
        .take(u64::try_from(bytes_to_read).map_err(|_| {
            RunError::new(
                Exit::Failure,
                "--stdin-max-bytes is too large to enforce a bounded standard-input read",
            )
        })?)
        .read_to_end(&mut bytes)
        .map_err(|error| RunError::new(Exit::Failure, format!("failed to read piped standard input: {error}")))?;
    if bytes.len() > max_bytes {
        return Err(RunError::new(
            Exit::Failure,
            format!("piped standard input exceeds --stdin-max-bytes={max_bytes}"),
        ));
    }
    String::from_utf8(bytes).map_err(|_| RunError::new(Exit::Failure, "piped standard input must be valid UTF-8"))
}

/// Run a prompt with caller-owned output streams and cancellation control.
///
/// This builds the same [`App`] and submits the same user turn as the TUI. It
/// only changes the projection: assistant text goes to `stdout`; all lifecycle
/// information goes to `stderr`.
fn run_with_io<Stdout: Write, Stderr: Write>(
    cli: &Cli, prompt: &str, mut output: HeadlessOutput, stdout: &mut Stdout, stderr: &mut Stderr,
    cancellation: &CancelToken,
) -> Result<()> {
    let mut app = App::from_cli(cli);
    if let Some(recovery) = &app.first_run_recovery {
        return Err(RunError::new(
            Exit::Setup,
            format!(
                "setup required for {} ({}); run `thndrs setup` before `thndrs run`",
                recovery.missing_label(),
                recovery.stage.label()
            ),
        ));
    }

    let Some(started) = app::submit_user_turn(&mut app, prompt.to_string()) else {
        return Err(RunError::new(
            Exit::Setup,
            "prompt could not start; run `thndrs setup` before `thndrs run`",
        ));
    };
    apply_message(&mut app, started);

    let mut agent = None;
    let mut cancellation_requested = false;
    let mut policy_error = None;

    loop {
        maybe_spawn_agent(&mut app, &mut agent);

        if cancellation.is_cancelled() {
            cancellation_requested = true;
            if let Some(slot) = &agent {
                slot.cancel.cancel();
            } else {
                finish_output(stdout, &mut output)?;
                return Err(RunError::new(Exit::Cancelled, "headless run cancelled"));
            }
        }

        let Some(slot) = &agent else {
            return match &app.run_state {
                RunState::Idle => finish_output(stdout, &mut output),
                RunState::Error(message) => {
                    finish_output(stdout, &mut output).and(Err(RunError::new(Exit::Failure, message.clone())))
                }
                RunState::Stopping => Err(RunError::new(
                    Exit::Cancelled,
                    "headless run cancelled before the agent started",
                )),
                RunState::Working => Err(RunError::new(Exit::Failure, "headless run did not start an agent")),
            };
        };

        match slot.receiver.recv_timeout(CANCEL_POLL_INTERVAL) {
            Ok(event) => {
                let terminal = Terminal::from_event(&event);
                let requested_permission = matches!(event, app::AgentEvent::PermissionRequest(_));
                if terminal.is_some() {
                    finish_output(stdout, &mut output)?;
                }
                write_event(stdout, stderr, &event, &mut output, app.is_ephemeral())?;
                apply_message(&mut app, Msg::Agent(event));

                if requested_permission {
                    policy_error = Some(String::from(
                        "headless runs cannot answer ACP permission requests; run the prompt in the TUI",
                    ));
                    app::cancel_pending_permission(&mut app);
                    if let Some(slot) = &agent {
                        slot.cancel.cancel();
                    }
                }

                if let Some(terminal) = terminal {
                    let Some(mut settled) = agent.take() else {
                        return Err(RunError::new(
                            Exit::Failure,
                            "headless agent event arrived without an active run",
                        ));
                    };
                    if let Err(error) = settled.receiver.wait() {
                        return Err(RunError::new(
                            Exit::Failure,
                            format!("headless agent worker failed: {error}"),
                        ));
                    }
                    if let Some(message) = policy_error.take() {
                        return Err(RunError::new(Exit::Policy, message));
                    }
                    if cancellation_requested {
                        return Err(RunError::new(Exit::Cancelled, "headless run cancelled"));
                    }
                    match terminal {
                        Terminal::Finished if app.run_state == RunState::Idle => return Ok(()),
                        Terminal::Finished => continue,
                        Terminal::Failed(message) => return Err(RunError::new(Exit::Failure, message)),
                        Terminal::Cancelled => return Err(RunError::new(Exit::Cancelled, "headless run cancelled")),
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let Some(mut settled) = agent.take() else {
                    return Err(RunError::new(
                        Exit::Failure,
                        "headless agent stream disconnected without an active run",
                    ));
                };
                let worker_result = settled.receiver.wait();
                finish_output(stdout, &mut output)?;
                if let Some(message) = policy_error.take() {
                    return Err(RunError::new(Exit::Policy, message));
                }
                if cancellation_requested {
                    return Err(RunError::new(Exit::Cancelled, "headless run cancelled"));
                }
                if let Err(error) = worker_result {
                    return Err(RunError::new(
                        Exit::Failure,
                        format!("headless agent worker failed: {error}"),
                    ));
                }
                return Err(RunError::new(
                    Exit::Failure,
                    "headless agent stream ended without a terminal event",
                ));
            }
        }
    }
}

/// Apply the app's pure update chain without asking a terminal surface to draw.
fn apply_message(app: &mut App, message: Msg) {
    let mut next = Some(message);
    while let Some(message) = next {
        next = update(app, &message);
    }
}

/// Project one agent event to the selected headless output contract.
fn write_event<Stdout: Write, Stderr: Write>(
    stdout: &mut Stdout, stderr: &mut Stderr, event: &app::AgentEvent, output: &mut HeadlessOutput, ephemeral: bool,
) -> Result<()> {
    match output {
        HeadlessOutput::Text { wrote_text, ends_with_newline } => {
            write_text_event(stdout, stderr, event, wrote_text, ends_with_newline)
        }
        HeadlessOutput::JsonLines => {
            write_jsonl_event(stdout, event, ephemeral).and_then(|()| write_diagnostic(stderr, event))
        }
    }
}

/// Project one event as interactive-style text.
fn write_text_event<Stdout: Write, Stderr: Write>(
    stdout: &mut Stdout, stderr: &mut Stderr, event: &app::AgentEvent, wrote_text: &mut bool,
    ends_with_newline: &mut bool,
) -> Result<()> {
    match event {
        app::AgentEvent::AssistantDelta(text) => {
            stdout
                .write_all(text.as_bytes())
                .and_then(|()| stdout.flush())
                .map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))?;
            *wrote_text = *wrote_text || !text.is_empty();
            if !text.is_empty() {
                *ends_with_newline = text.ends_with('\n');
            }
            Ok(())
        }
        _ => write_diagnostic(stderr, event),
    }
}

/// Emit one versioned provider-neutral event as a JSON Lines record.
fn write_jsonl_event<Stdout: Write>(stdout: &mut Stdout, event: &app::AgentEvent, ephemeral: bool) -> Result<()> {
    let json_event =
        VersionedJsonEvent { version: JSONL_SCHEMA_VERSION, event: JsonEvent::from_agent_event(event, ephemeral) };
    serde_json::to_writer(&mut *stdout, &json_event)
        .map_err(|error| RunError::new(Exit::Failure, format!("headless JSONL output failed: {error}")))?;
    writeln!(stdout)
        .and_then(|()| stdout.flush())
        .map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))
}

/// Write human lifecycle diagnostics without mixing them into machine output.
fn write_diagnostic<Stderr: Write>(stderr: &mut Stderr, event: &app::AgentEvent) -> Result<()> {
    match event {
        app::AgentEvent::Started => writeln!(stderr, "thndrs run: started"),
        app::AgentEvent::Status(message) => writeln!(stderr, "thndrs run: {message}"),
        app::AgentEvent::Usage { input_tokens, output_tokens } => {
            writeln!(stderr, "thndrs run: usage input={input_tokens} output={output_tokens}")
        }
        app::AgentEvent::RequestAccounting(accounting) => {
            writeln!(
                stderr,
                "thndrs run: request {} bytes",
                accounting.serialized_bytes.value
            )
        }
        app::AgentEvent::AssistantDelta(_) => Ok(()),
        app::AgentEvent::ReasoningDelta(text) => writeln!(stderr, "thndrs run: reasoning: {text}"),
        app::AgentEvent::ToolStarted { id, name, .. } => writeln!(stderr, "thndrs run: tool started: {name}#{id}"),
        app::AgentEvent::ToolFinished { id, status, .. } => {
            writeln!(stderr, "thndrs run: tool finished: {id} ({})", status.label())
        }
        app::AgentEvent::StateProjectionDecision { .. } | app::AgentEvent::ModelMetadataLoaded(_) => Ok(()),
        app::AgentEvent::Retrying { attempt, max_attempts, delay_ms, error } => {
            writeln!(
                stderr,
                "thndrs run: retry {attempt}/{max_attempts} in {delay_ms}ms after: {error}"
            )
        }
        app::AgentEvent::PermissionRequest(permission) => {
            writeln!(
                stderr,
                "thndrs run: permission required: {} ({})",
                permission.title, permission.tool_call_id
            )
        }
        app::AgentEvent::PermissionResolved { tool_call_id, outcome } => {
            writeln!(stderr, "thndrs run: permission {tool_call_id}: {outcome}")
        }
        app::AgentEvent::AcpSession(metadata) => {
            writeln!(
                stderr,
                "thndrs run: ACP session {} ({})",
                metadata.acp_session_id, metadata.agent_name
            )
        }
        app::AgentEvent::Finished => writeln!(stderr, "thndrs run: finished"),
        app::AgentEvent::Failed(message) => writeln!(stderr, "thndrs run: failed: {message}"),
        app::AgentEvent::Cancelled => writeln!(stderr, "thndrs run: cancelled"),
    }
    .map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))
}

/// Finish a text response on its own terminal line, then flush the output stream.
fn finish_output<Stdout: Write>(stdout: &mut Stdout, output: &mut HeadlessOutput) -> Result<()> {
    if let HeadlessOutput::Text { wrote_text: true, ends_with_newline: false } = output {
        writeln!(stdout).map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))?;
        *output = HeadlessOutput::Text { wrote_text: true, ends_with_newline: true };
    }
    stdout
        .flush()
        .map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::{Cursor, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use thndrs_agent::CancelToken;

    use super::{
        EXIT_CANCELLED, EXIT_FAILURE, EXIT_POLICY, EXIT_SETUP, Exit, HeadlessOutput, exit_code, resolve_prompt,
        run_with_io, write_jsonl_event,
    };
    use crate::app;
    use crate::cli::Cli;
    use crate::cli::commands::run::RunCommand;
    use crate::{config, session};

    #[derive(Clone)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl SharedWriter {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn bytes(&self) -> Vec<u8> {
            self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
        }
    }

    impl Write for SharedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .write(buffer)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct CancellingWriter {
        bytes: Vec<u8>,
        cancellation: CancelToken,
        cancellation_requested: bool,
    }

    impl CancellingWriter {
        fn new(cancellation: CancelToken) -> Self {
            Self { bytes: Vec::new(), cancellation, cancellation_requested: false }
        }

        fn into_bytes(self) -> Vec<u8> {
            self.bytes
        }
    }

    impl Write for CancellingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.bytes.write(buffer)
        }

        fn flush(&mut self) -> io::Result<()> {
            if !self.cancellation_requested {
                self.cancellation.cancel();
                self.cancellation_requested = true;
            }
            Ok(())
        }
    }

    struct NeverRead;

    impl io::Read for NeverRead {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("interactive input must not be read"))
        }
    }

    fn run_command(prompt: Option<&str>, stdin_max_bytes: usize) -> RunCommand {
        RunCommand { prompt: prompt.map(str::to_string), jsonl: false, stdin_max_bytes }
    }

    fn assert_jsonl_fixture(stdout: Vec<u8>, workspace: &Path, expected: &str) {
        let actual = String::from_utf8(stdout).expect("JSONL output is UTF-8");
        for line in actual.lines() {
            let event: serde_json::Value = serde_json::from_str(line).expect("stdout line is JSON");
            assert_eq!(event["version"], 1, "JSONL event uses the current schema version");
        }
        let canonical_workspace = workspace.canonicalize().expect("canonical workspace path");
        let normalized = actual
            .replace(&fixture_agent().display().to_string(), "<fake-acp-agent>")
            .replace(&canonical_workspace.display().to_string(), "<workspace>")
            .replace(&workspace.display().to_string(), "<workspace>");
        assert_eq!(normalized, expected);
    }

    fn fixture_agent() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("fake_acp_agent.py")
    }

    fn fixture_cli(cwd: &Path, script: &str) -> Cli {
        let mut agents = config::AcpAgentsConfig::new();
        agents.insert(
            "local".to_string(),
            config::AcpAgentConfig {
                command: "python3".to_string(),
                args: vec![fixture_agent().display().to_string(), script.to_string()],
                timeout_secs: 2,
                ..config::AcpAgentConfig::default()
            },
        );
        Cli {
            cwd: cwd.to_path_buf(),
            model: "acp:local".to_string(),
            acp_agents: agents,
            session_dir: Some(cwd.join("sessions")),
            ..Cli::default()
        }
    }

    #[test]
    fn exit_codes_are_stable() {
        assert_eq!(Exit::Failure.code(), 1);
        assert_eq!(Exit::Setup.code(), 2);
        assert_eq!(Exit::Policy.code(), 3);
        assert_eq!(Exit::Cancelled.code(), 4);
        assert_eq!(exit_code(&io::Error::other("unclassified failure")), EXIT_FAILURE);
    }

    #[test]
    fn resolves_piped_input_with_and_without_a_prompt_argument() {
        let mut input = Cursor::new("from standard input");
        assert_eq!(
            resolve_prompt(&run_command(Some("inspect this"), 64), &mut input, false)
                .expect("combine prompt and input"),
            "inspect this\n\nfrom standard input"
        );

        let mut input = Cursor::new("from standard input");
        assert_eq!(
            resolve_prompt(&run_command(None, 64), &mut input, false).expect("use piped prompt"),
            "from standard input"
        );
    }

    #[test]
    fn does_not_read_interactive_standard_input() {
        let mut input = NeverRead;
        assert_eq!(
            resolve_prompt(&run_command(Some("inspect this"), 64), &mut input, true).expect("use prompt argument"),
            "inspect this"
        );

        let error = resolve_prompt(&run_command(None, 64), &mut input, true)
            .expect_err("missing interactive prompt is actionable");
        assert!(error.message.contains("interactive standard input is not read"));
    }

    #[test]
    fn rejects_empty_invalid_and_oversized_piped_input() {
        let mut empty = Cursor::new("");
        let error = resolve_prompt(&run_command(None, 64), &mut empty, false).expect_err("empty input is rejected");
        assert!(error.message.contains("standard input was empty"));

        let mut invalid = Cursor::new(vec![0xff]);
        let error = resolve_prompt(&run_command(None, 64), &mut invalid, false).expect_err("invalid UTF-8 is rejected");
        assert!(error.message.contains("valid UTF-8"));

        let mut oversized = Cursor::new("12345");
        let error =
            resolve_prompt(&run_command(None, 4), &mut oversized, false).expect_err("oversized input is rejected");
        assert!(error.message.contains("--stdin-max-bytes=4"));

        let mut input = Cursor::new("prompt");
        let error =
            resolve_prompt(&run_command(None, 0), &mut input, false).expect_err("zero-byte input limit is rejected");
        assert!(error.message.contains("between 1 and"));
    }

    #[test]
    fn jsonl_has_stable_reasoning_usage_and_retry_shapes() {
        let events = [
            app::AgentEvent::ReasoningDelta("checking context".to_string()),
            app::AgentEvent::Usage { input_tokens: 13, output_tokens: 5 },
            app::AgentEvent::Retrying {
                attempt: 2,
                max_attempts: 3,
                delay_ms: 250,
                error: "temporary failure".to_string(),
            },
        ];
        let mut stdout = Vec::new();
        for event in &events {
            write_jsonl_event(&mut stdout, event, false).expect("serialize JSONL event");
        }

        let records: Vec<serde_json::Value> = String::from_utf8(stdout)
            .expect("JSONL output is UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSONL line parses"))
            .collect();
        assert_eq!(
            records[0],
            serde_json::json!({"version": 1, "type": "reasoning", "text": "checking context"})
        );
        assert_eq!(
            records[1],
            serde_json::json!({"version": 1, "type": "usage", "input_tokens": 13, "output_tokens": 5})
        );
        assert_eq!(
            records[2],
            serde_json::json!({
                "version": 1,
                "type": "retrying",
                "attempt": 2,
                "max_attempts": 3,
                "delay_ms": 250,
                "error": "temporary failure"
            })
        );
    }

    #[test]
    fn streams_assistant_text_and_records_the_normal_session_audit() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "lifecycle");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(
            &cli,
            "reply",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect("headless run succeeds");

        assert_eq!(
            String::from_utf8(stdout).expect("stdout is UTF-8"),
            "pong from fake ACP agent\n"
        );
        let diagnostics = String::from_utf8(stderr).expect("stderr is UTF-8");
        assert!(diagnostics.contains("thndrs run: started"));
        assert!(diagnostics.contains("thndrs run: finished"));
        assert!(!diagnostics.contains("pong from fake ACP agent"));

        let files = session::list_session_files(cli.session_dir.as_deref().expect("session directory"));
        assert_eq!(files.len(), 1);
        let transcript = session::SessionReader::read_transcript(&files[0]);
        assert!(
            transcript
                .iter()
                .any(|entry| matches!(entry, app::Entry::User { text } if text == "reply"))
        );
        assert!(transcript.iter().any(
            |entry| matches!(entry, app::Entry::Agent { text, streaming: false } if text == "pong from fake ACP agent")
        ));
    }

    #[test]
    fn ephemeral_jsonl_run_keeps_the_session_directory_empty() {
        let temp = tempfile::tempdir().expect("create workspace");
        let sessions_dir = temp.path().join("sessions");
        std::fs::create_dir(&sessions_dir).expect("create empty session directory");
        std::fs::write(temp.path().join("readme.txt"), "alpha\nbeta\n").expect("write fixture");
        let mut cli = fixture_cli(temp.path(), "fs-read");
        cli.ephemeral = true;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(
            &cli,
            "read the file",
            HeadlessOutput::json_lines(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect("ephemeral headless run succeeds");

        let events: Vec<serde_json::Value> = String::from_utf8(stdout)
            .expect("JSONL output is UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSONL event parses"))
            .collect();
        assert_eq!(
            events[0],
            serde_json::json!({"version": 1, "type": "started", "ephemeral": true})
        );
        assert!(
            std::fs::read_dir(&sessions_dir)
                .expect("read session directory")
                .next()
                .is_none()
        );
    }

    #[test]
    fn completes_assistant_output_before_a_terminal_diagnostic_on_a_shared_terminal() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "lifecycle");
        let shared = SharedWriter::new();
        let mut stdout = shared.clone();
        let mut stderr = shared.clone();

        run_with_io(
            &cli,
            "reply",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect("headless run succeeds");

        let output = String::from_utf8(shared.bytes()).expect("shared output is UTF-8");
        assert!(output.contains("pong from fake ACP agent\nthndrs run: finished\n"));
    }

    #[test]
    fn jsonl_lifecycle_matches_the_golden_fixture() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "lifecycle");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(
            &cli,
            "reply",
            HeadlessOutput::json_lines(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect("headless run succeeds");

        assert_jsonl_fixture(
            stdout,
            temp.path(),
            include_str!("../tests/fixtures/headless_jsonl_lifecycle.jsonl"),
        );
        assert!(
            String::from_utf8(stderr)
                .expect("stderr is UTF-8")
                .contains("thndrs run: finished")
        );
    }

    #[test]
    fn jsonl_tool_run_matches_the_golden_fixture() {
        let temp = tempfile::tempdir().expect("create workspace");
        std::fs::write(temp.path().join("readme.txt"), "alpha\nbeta\n").expect("write fixture");
        let cli = fixture_cli(temp.path(), "fs-read");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(
            &cli,
            "read the file",
            HeadlessOutput::json_lines(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect("headless tool run succeeds");

        assert_jsonl_fixture(
            stdout,
            temp.path(),
            include_str!("../tests/fixtures/headless_jsonl_tool.jsonl"),
        );
        assert!(
            !String::from_utf8(stderr)
                .expect("stderr is UTF-8")
                .contains("read: alpha")
        );
    }

    #[test]
    fn jsonl_failure_matches_the_golden_fixture() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "failure");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let error = run_with_io(
            &cli,
            "reply",
            HeadlessOutput::json_lines(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect_err("fixture provider fails");

        assert_eq!(error.exit, Exit::Failure);
        assert_jsonl_fixture(
            stdout,
            temp.path(),
            include_str!("../tests/fixtures/headless_jsonl_failure.jsonl"),
        );
    }

    #[test]
    fn jsonl_cancellation_matches_the_golden_fixture() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "cancel");
        let cancellation = CancelToken::new();
        let mut stdout = CancellingWriter::new(cancellation.clone());
        let mut stderr = Vec::new();

        let error = run_with_io(
            &cli,
            "wait",
            HeadlessOutput::json_lines(),
            &mut stdout,
            &mut stderr,
            &cancellation,
        )
        .expect_err("cancelled run does not succeed");

        assert_eq!(error.exit, Exit::Cancelled);
        assert_jsonl_fixture(
            stdout.into_bytes(),
            temp.path(),
            include_str!("../tests/fixtures/headless_jsonl_cancelled.jsonl"),
        );
    }

    #[test]
    fn keeps_tool_diagnostics_off_stdout_and_audits_tool_use() {
        let temp = tempfile::tempdir().expect("create workspace");
        std::fs::write(temp.path().join("readme.txt"), "alpha\nbeta\n").expect("write fixture");
        let cli = fixture_cli(temp.path(), "fs-read");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(
            &cli,
            "read the file",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect("headless tool run succeeds");

        let result = String::from_utf8(stdout).expect("stdout is UTF-8");
        let diagnostics = String::from_utf8(stderr).expect("stderr is UTF-8");
        assert_eq!(result, "read: alpha\nbeta\n");
        assert!(diagnostics.contains("tool started: acp.fs.read_text_file#"));
        assert!(diagnostics.contains("tool finished:"));
        assert!(!diagnostics.contains("read: alpha"));

        let files = session::list_session_files(cli.session_dir.as_deref().expect("session directory"));
        let records = session::SessionReader::read_records(&files[0]);
        assert!(
            records
                .iter()
                .any(|record| matches!(record, session::SessionRecord::ToolStarted { .. }))
        );
    }

    #[test]
    fn reports_setup_and_provider_failures_with_stable_codes() {
        let temp = tempfile::tempdir().expect("create workspace");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let setup = run_with_io(
            &Cli { cwd: temp.path().to_path_buf(), ..Cli::default() },
            "reply",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect_err("missing model requires setup");
        assert_eq!(setup.exit.code(), EXIT_SETUP);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());

        let mut cli = fixture_cli(temp.path(), "lifecycle");
        cli.acp_agents.get_mut("local").expect("fixture agent").command = "thndrs-missing-provider".to_string();
        let failure = run_with_io(
            &cli,
            "reply",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect_err("missing provider executable fails");
        assert_eq!(failure.exit.code(), EXIT_FAILURE);
    }

    #[test]
    fn rejects_interactive_permissions_with_the_policy_code() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "permission");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let error = run_with_io(
            &cli,
            "write the file",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &CancelToken::new(),
        )
        .expect_err("permission cannot be answered headlessly");

        assert_eq!(error.exit.code(), EXIT_POLICY);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .expect("stderr is UTF-8")
                .contains("permission required")
        );
    }

    #[test]
    fn returns_the_cancellation_code_after_cooperative_cancellation() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "cancel");
        let cancellation = CancelToken::new();
        let trigger = cancellation.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            trigger.cancel();
        });
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let error = run_with_io(
            &cli,
            "wait",
            HeadlessOutput::text(),
            &mut stdout,
            &mut stderr,
            &cancellation,
        )
        .expect_err("cancelled run does not succeed");
        canceller.join().expect("canceller joins");

        assert_eq!(error.exit.code(), EXIT_CANCELLED);
        assert!(String::from_utf8(stdout).expect("stdout is UTF-8").is_empty());
    }
}
