//! Headless single-prompt execution and output projection.
//!
//! This module shares the application lifecycle used by the TUI while keeping
//! command output, cooperative cancellation, and terminal exit codes local to
//! the non-interactive `thndrs run` surface.

use std::io::{self, Write};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use thndrs_agent::CancelToken;

use crate::app::{self, App, Msg, RunState, update};
use crate::cli::Cli;
use crate::cli::commands::run::RunCommand;
use crate::maybe_spawn_agent;

/// Frequency at which the headless runner observes Ctrl-C while waiting for an
/// agent event.
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const EXIT_FAILURE: i32 = 1;
const EXIT_SETUP: i32 = 2;
const EXIT_POLICY: i32 = 3;
const EXIT_CANCELLED: i32 = 4;

static CANCELLATION: OnceLock<Mutex<Option<CancelToken>>> = OnceLock::new();
static CTRL_C_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();

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
    let cancellation = CancelToken::new();
    install_cancellation_handler(cancellation.clone())?;

    let stdout = io::stdout();
    let stderr = io::stderr();
    let result = run_with_io(
        cli,
        &command.prompt,
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

/// Run a prompt with caller-owned output streams and cancellation control.
///
/// This builds the same [`App`] and submits the same user turn as the TUI. It
/// only changes the projection: assistant text goes to `stdout`; all lifecycle
/// information goes to `stderr`.
fn run_with_io<Stdout: Write, Stderr: Write>(
    cli: &Cli, prompt: &str, stdout: &mut Stdout, stderr: &mut Stderr, cancellation: &CancelToken,
) -> Result<(), RunError> {
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
    let mut output = Output::default();
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
                write_event(stdout, stderr, &event, &mut output)?;
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

/// Text-stream state used to terminate a partial assistant response cleanly.
#[derive(Default)]
struct Output {
    wrote_text: bool,
    ends_with_newline: bool,
}

/// Project one agent event to headless output streams.
fn write_event<Stdout: Write, Stderr: Write>(
    stdout: &mut Stdout, stderr: &mut Stderr, event: &app::AgentEvent, output: &mut Output,
) -> Result<(), RunError> {
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
        app::AgentEvent::AssistantDelta(text) => {
            stdout
                .write_all(text.as_bytes())
                .and_then(|()| stdout.flush())
                .map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))?;
            output.wrote_text = output.wrote_text || !text.is_empty();
            if !text.is_empty() {
                output.ends_with_newline = text.ends_with('\n');
            }
            Ok(())
        }
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

/// Finish a streamed assistant response on its own terminal line.
fn finish_output<Stdout: Write>(stdout: &mut Stdout, output: &mut Output) -> Result<(), RunError> {
    if output.wrote_text && !output.ends_with_newline {
        writeln!(stdout).map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))?;
        output.ends_with_newline = true;
    }
    stdout
        .flush()
        .map_err(|error| RunError::new(Exit::Failure, format!("headless output failed: {error}")))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use thndrs_agent::CancelToken;

    use super::{EXIT_CANCELLED, EXIT_FAILURE, EXIT_POLICY, EXIT_SETUP, Exit, exit_code, run_with_io};
    use crate::app;
    use crate::cli::Cli;
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
    fn streams_assistant_text_and_records_the_normal_session_audit() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "lifecycle");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(&cli, "reply", &mut stdout, &mut stderr, &CancelToken::new()).expect("headless run succeeds");

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
    fn completes_assistant_output_before_a_terminal_diagnostic_on_a_shared_terminal() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "lifecycle");
        let shared = SharedWriter::new();
        let mut stdout = shared.clone();
        let mut stderr = shared.clone();

        run_with_io(&cli, "reply", &mut stdout, &mut stderr, &CancelToken::new()).expect("headless run succeeds");

        let output = String::from_utf8(shared.bytes()).expect("shared output is UTF-8");
        assert!(output.contains("pong from fake ACP agent\nthndrs run: finished\n"));
    }

    #[test]
    fn keeps_tool_diagnostics_off_stdout_and_audits_tool_use() {
        let temp = tempfile::tempdir().expect("create workspace");
        std::fs::write(temp.path().join("readme.txt"), "alpha\nbeta\n").expect("write fixture");
        let cli = fixture_cli(temp.path(), "fs-read");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_io(&cli, "read the file", &mut stdout, &mut stderr, &CancelToken::new())
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
        let failure = run_with_io(&cli, "reply", &mut stdout, &mut stderr, &CancelToken::new())
            .expect_err("missing provider executable fails");
        assert_eq!(failure.exit.code(), EXIT_FAILURE);
    }

    #[test]
    fn rejects_interactive_permissions_with_the_policy_code() {
        let temp = tempfile::tempdir().expect("create workspace");
        let cli = fixture_cli(temp.path(), "permission");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let error = run_with_io(&cli, "write the file", &mut stdout, &mut stderr, &CancelToken::new())
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

        let error = run_with_io(&cli, "wait", &mut stdout, &mut stderr, &cancellation)
            .expect_err("cancelled run does not succeed");
        canceller.join().expect("canceller joins");

        assert_eq!(error.exit.code(), EXIT_CANCELLED);
        assert!(String::from_utf8(stdout).expect("stdout is UTF-8").is_empty());
    }
}
