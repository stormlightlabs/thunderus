//! Versioned, frontend-neutral protocol and stdio bridge.
//!
//! The bridge exposes application snapshots and semantic events without giving
//! frontends access to provider payloads, credentials, or session writers.

mod protocol;

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::app::{self, Action, App, Msg, RunState};
use crate::cli::Cli;
use crate::harness::AgentSlot;
use crate::maybe_spawn_agent;

pub use protocol::{
    Command, CommandEnvelope, ErrorCode, FrontendEvent, FrontendSnapshot, PROTOCOL_VERSION, ProtocolMessage,
    ResponseResult,
};

const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_COMMAND_BYTES: usize = 1024 * 1024;

/// Run the frontend protocol over caller-owned newline-delimited streams.
///
/// Input is read on a dedicated thread so cancellation and shutdown commands
/// remain available while an agent turn is streaming.
pub fn run_stdio<R, W, E>(cli: &Cli, input: R, mut output: W, mut diagnostics: E) -> io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write,
    E: Write,
{
    let (input_tx, input_rx) = mpsc::channel();
    thread::spawn(move || read_commands(input, &input_tx));

    let mut bridge = Bridge::new(cli);
    loop {
        while let Ok(input) = input_rx.try_recv() {
            match input {
                Input::Command(command) => {
                    if bridge.handle_command(command, &mut output, &mut diagnostics)? {
                        bridge.stop_agent();
                        return Ok(());
                    }
                }
                Input::Malformed(message) => bridge.write_protocol_error(
                    ErrorCode::MalformedCommand,
                    bounded_diagnostic(&message),
                    &mut output,
                )?,
                Input::Disconnected => {
                    bridge.stop_agent();
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "frontend peer disconnected without shutdown",
                    ));
                }
            }
        }

        if bridge.drain_one_agent_event(&mut output, &mut diagnostics)? {
            continue;
        }
        match input_rx.recv_timeout(INPUT_POLL_INTERVAL) {
            Ok(input) => match input {
                Input::Command(command) => {
                    if bridge.handle_command(command, &mut output, &mut diagnostics)? {
                        bridge.stop_agent();
                        return Ok(());
                    }
                }
                Input::Malformed(message) => bridge.write_protocol_error(
                    ErrorCode::MalformedCommand,
                    bounded_diagnostic(&message),
                    &mut output,
                )?,
                Input::Disconnected => {
                    bridge.stop_agent();
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "frontend peer disconnected without shutdown",
                    ));
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bridge.stop_agent();
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "frontend input reader terminated unexpectedly",
                ));
            }
        }
    }
}

enum Input {
    Command(CommandEnvelope),
    Malformed(String),
    Disconnected,
}

fn read_commands<R: BufRead>(mut input: R, sender: &mpsc::Sender<Input>) {
    loop {
        let mut bytes = Vec::new();
        match io::Read::take(&mut input, (MAX_COMMAND_BYTES + 1) as u64).read_until(b'\n', &mut bytes) {
            Ok(0) => {
                let _ = sender.send(Input::Disconnected);
                return;
            }
            Ok(_) if bytes.len() > MAX_COMMAND_BYTES => {
                let _ = sender.send(Input::Malformed(format!("command exceeds {MAX_COMMAND_BYTES} bytes")));
                if !bytes.ends_with(b"\n") {
                    let mut discarded = Vec::new();
                    let _ = input.read_until(b'\n', &mut discarded);
                }
            }
            Ok(_) => {
                while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                    bytes.pop();
                }
                let parsed = serde_json::from_slice(&bytes)
                    .map(Input::Command)
                    .unwrap_or_else(|error| Input::Malformed(error.to_string()));
                if sender.send(parsed).is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(Input::Malformed(format!("failed to read command: {error}")));
                let _ = sender.send(Input::Disconnected);
                return;
            }
        }
    }
}

struct Bridge {
    app: App,
    agent: Option<AgentSlot>,
    initialized: bool,
    sequence: u64,
}

impl Bridge {
    fn new(cli: &Cli) -> Self {
        Self { app: App::from_cli(cli), agent: None, initialized: false, sequence: 0 }
    }

    fn handle_command<W: Write, E: Write>(
        &mut self, envelope: CommandEnvelope, output: &mut W, diagnostics: &mut E,
    ) -> io::Result<bool> {
        if envelope.id.trim().is_empty() {
            self.write_protocol_error(ErrorCode::InvalidRequest, "request id must not be empty", output)?;
            return Ok(false);
        }
        if !self.initialized && !matches!(envelope.command, Command::Initialize { .. }) {
            self.write_response(
                envelope.id,
                Err((
                    ErrorCode::NotInitialized,
                    "initialize must be the first command".to_string(),
                )),
                output,
            )?;
            return Ok(false);
        }

        match envelope.command {
            Command::Initialize { supported_versions } => {
                if self.initialized {
                    self.write_response(
                        envelope.id,
                        Err((
                            ErrorCode::AlreadyInitialized,
                            "frontend is already initialized".to_string(),
                        )),
                        output,
                    )?;
                } else if envelope.version != PROTOCOL_VERSION || !supported_versions.contains(&PROTOCOL_VERSION) {
                    self.write_response(
                        envelope.id,
                        Err((
                            ErrorCode::UnsupportedVersion,
                            format!("supported protocol version is {PROTOCOL_VERSION}"),
                        )),
                        output,
                    )?;
                } else {
                    self.initialized = true;
                    let snapshot = FrontendSnapshot::from_app(&self.app, self.sequence);
                    self.write_response(
                        envelope.id,
                        Ok(ResponseResult::Initialized { protocol_version: PROTOCOL_VERSION, snapshot }),
                        output,
                    )?;
                }
            }
            Command::StateSnapshot => {
                if !self.ensure_version(envelope.version, &envelope.id, output)? {
                    return Ok(false);
                }
                self.write_response(
                    envelope.id,
                    Ok(ResponseResult::Snapshot { snapshot: FrontendSnapshot::from_app(&self.app, self.sequence) }),
                    output,
                )?;
            }
            Command::TurnSubmit { text } => {
                if !self.ensure_version(envelope.version, &envelope.id, output)? {
                    return Ok(false);
                }
                if text.trim().is_empty() {
                    self.write_response(
                        envelope.id,
                        Err((ErrorCode::InvalidRequest, "turn text must not be empty".to_string())),
                        output,
                    )?;
                } else if self.app.runtime.run_state != RunState::Idle || self.agent.is_some() {
                    self.write_response(
                        envelope.id,
                        Err((ErrorCode::Busy, "an agent turn is already active".to_string())),
                        output,
                    )?;
                } else if let Some(message) = app::submit_user_turn(&mut self.app, text) {
                    apply_message(&mut self.app, message);
                    self.write_response(envelope.id, Ok(ResponseResult::Accepted), output)?;
                    maybe_spawn_agent(&mut self.app, &mut self.agent);
                } else {
                    self.write_response(
                        envelope.id,
                        Err((ErrorCode::SetupRequired, "provider setup is required".to_string())),
                        output,
                    )?;
                }
            }
            Command::TurnCancel => {
                if !self.ensure_version(envelope.version, &envelope.id, output)? {
                    return Ok(false);
                }
                if let Some(cancel) = self.agent.as_ref().map(|agent| agent.cancel.clone()) {
                    apply_message(&mut self.app, Msg::Action(Action::Cancel));
                    cancel.cancel();
                    self.write_response(envelope.id, Ok(ResponseResult::Accepted), output)?;
                } else {
                    self.write_response(
                        envelope.id,
                        Err((ErrorCode::NoActiveRun, "there is no active turn".to_string())),
                        output,
                    )?;
                }
            }
            Command::Shutdown => {
                if !self.ensure_version(envelope.version, &envelope.id, output)? {
                    return Ok(false);
                }
                self.write_response(envelope.id, Ok(ResponseResult::Shutdown), output)?;
                return Ok(true);
            }
            Command::QueueSubmit { .. }
            | Command::QueueDelete { .. }
            | Command::PermissionRespond { .. }
            | Command::SessionNew
            | Command::SessionLoad { .. }
            | Command::SessionClose
            | Command::ModelSelect { .. }
            | Command::ReasoningSelect { .. } => {
                if !self.ensure_version(envelope.version, &envelope.id, output)? {
                    return Ok(false);
                }
                self.write_response(
                    envelope.id,
                    Err((
                        ErrorCode::UnsupportedCommand,
                        "command is not available in this milestone".to_string(),
                    )),
                    output,
                )?;
            }
        }
        let _ = diagnostics.flush();
        Ok(false)
    }

    fn ensure_version<W: Write>(&self, version: u16, id: &str, output: &mut W) -> io::Result<bool> {
        if version == PROTOCOL_VERSION {
            Ok(true)
        } else {
            let message = ProtocolMessage::response_error(
                id.to_string(),
                ErrorCode::UnsupportedVersion,
                format!("supported protocol version is {PROTOCOL_VERSION}"),
            );
            write_message(output, &message)?;
            Ok(false)
        }
    }

    fn drain_one_agent_event<W: Write, E: Write>(&mut self, output: &mut W, diagnostics: &mut E) -> io::Result<bool> {
        let Some(slot) = self.agent.as_mut() else { return Ok(false) };
        match slot.receiver.try_recv() {
            Ok(event) => {
                let terminal = matches!(
                    event,
                    app::AgentEvent::Finished | app::AgentEvent::Cancelled | app::AgentEvent::Failed(_)
                );
                let projected = FrontendEvent::from_agent_event(&event);
                apply_message(&mut self.app, Msg::Agent(event));
                if let Some(event) = projected {
                    self.sequence = self.sequence.saturating_add(1);
                    write_message(output, &ProtocolMessage::event(self.sequence, event))?;
                }
                if terminal {
                    if let Some(mut settled) = self.agent.take() {
                        if let Err(error) = settled.receiver.wait() {
                            writeln!(
                                diagnostics,
                                "thndrs frontend: agent worker failed: {}",
                                bounded_diagnostic(&error.to_string())
                            )?;
                        }
                    }
                }
                Ok(true)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(false),
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(mut settled) = self.agent.take() {
                    let result = settled.receiver.wait();
                    writeln!(diagnostics, "thndrs frontend: agent stream ended unexpectedly")?;
                    result.map_err(|error| io::Error::other(format!("agent worker failed: {error}")))?;
                }
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "agent stream ended without a terminal event",
                ))
            }
        }
    }

    fn stop_agent(&mut self) {
        if let Some(mut agent) = self.agent.take() {
            agent.cancel.cancel();
            let _ = agent.receiver.wait();
        }
    }

    fn write_response<W: Write>(
        &self, id: String, result: Result<ResponseResult, (ErrorCode, String)>, output: &mut W,
    ) -> io::Result<()> {
        let message = match result {
            Ok(result) => ProtocolMessage::response_ok(id, result),
            Err((code, message)) => ProtocolMessage::response_error(id, code, message),
        };
        write_message(output, &message)
    }

    fn write_protocol_error<W: Write>(
        &self, code: ErrorCode, message: impl Into<String>, output: &mut W,
    ) -> io::Result<()> {
        write_message(output, &ProtocolMessage::protocol_error(code, message.into()))
    }
}

fn apply_message(app: &mut App, message: Msg) {
    let mut next = Some(message);
    while let Some(message) = next {
        next = app::update_with_effects(app, &message).follow_up;
    }
}

fn write_message<W: Write>(output: &mut W, message: &ProtocolMessage) -> io::Result<()> {
    serde_json::to_writer(&mut *output, message).map_err(io::Error::other)?;
    writeln!(output)?;
    output.flush()
}

fn bounded_diagnostic(message: &str) -> String {
    crate::utils::truncate_ellipsis(&crate::tools::shell::redact_secrets(message), 512)
}

#[cfg(test)]
mod tests;
