//! ACP server request and notification handlers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::*;
use agent_client_protocol::{Agent, Client, ConnectionTo, Dispatch, Error, JsonRpcRequest, Lines, Result};
use futures::channel::oneshot;
use futures::future::{Either, select};
use futures::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt, Sink, Stream, StreamExt};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::agent::prompt_expects_workspace_write;
use crate::agent::{ToolExecutionHook, ToolPermissionDecision, ToolPermissionHook};
use crate::app::AgentEvent;
use crate::cli::WebSearchMode;
use crate::harness::HarnessHandle;
use crate::mcp::config::{McpConfig, McpServerConfig, McpTransport};
use crate::mcp::manager::McpManager;
use crate::prompt;
use crate::providers::{ProviderContentBlock, ProviderImageSource, ProviderMessage};
use crate::server::ServerConfig;
use crate::server::config_options::{ConfigOptionValue, acp_config_options, validate_config_option};
use crate::server::events::{
    SessionUpdateIntent, ToolStatusIntent, classify_tool, map_agent_event, sanitize_tool_payload,
};
use crate::server::session::{AcpServerSession, AcpSessionStore, validate_and_normalize_cwd};
use crate::session::{
    AcpPermissionOptionRecord, AcpSessionMetadata, SCHEMA_VERSION, SessionReader, SessionRecord, SessionWriter,
};
use crate::tools::shell::{ProcessKind, ProcessResult, ProcessStatus, ShellArgs, redact_secrets};
use crate::tools::{AgentRunConfig, MAX_OUTPUT_BYTES, ToolOutput, ToolUseRequest};
use thndrs_agent::CancelToken;

const PERMISSION_POLL_INTERVAL: Duration = Duration::from_millis(100);
const TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Shared handler state for one ACP stdio server process.
#[derive(Clone, Debug)]
pub struct ServerState {
    config: ServerConfig,
    inner: Arc<Mutex<ServerStateInner>>,
}

#[derive(Debug, Default)]
struct ServerStateInner {
    next_local_session: u64,
    sessions: AcpSessionStore,
    cancelled: BTreeSet<String>,
    active_turn_cancels: BTreeMap<String, CancelToken>,
    client_info: Option<Implementation>,
    client_capabilities: ClientCapabilities,
}

impl ServerState {
    /// Create process-local ACP server state.
    pub fn new(config: ServerConfig) -> Self {
        Self { config, inner: Arc::new(Mutex::new(ServerStateInner::default())) }
    }

    /// Register a new ACP session and return its opaque session id.
    pub fn create_session(&self, cwd: &Path) -> Result<String, String> {
        if !cwd.is_absolute() {
            return Err(format!("ACP session cwd must be absolute: {}", cwd.display()));
        }

        let mut inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        inner.next_local_session = inner.next_local_session.saturating_add(1);
        let local_session_id = format!("local-acp-session-{}", inner.next_local_session);
        let session_id = inner
            .sessions
            .create_session(&local_session_id, cwd, None)
            .map_err(|err| err.to_string())?;
        inner
            .sessions
            .update_session_metadata(
                &session_id,
                Some(self.config.model.clone()),
                websearch_mode(&self.config.websearch),
            )
            .map_err(|err| err.to_string())?;
        inner
            .sessions
            .update_session_reasoning(
                &session_id,
                Some(self.config.reasoning_effort),
                Some(self.config.reasoning_summary),
            )
            .map_err(|err| err.to_string())?;

        if let Some(session_dir) = &self.config.session_dir {
            let mut writer = SessionWriter::create(
                session_dir,
                &local_session_id,
                &cwd.display().to_string(),
                "acp session",
                "thndrs",
                &self.config.model,
                &self.config.websearch,
                env!("CARGO_PKG_VERSION"),
                None,
            )
            .map_err(|error| {
                inner.sessions.remove_session(&session_id);
                format!("ACP session writer initialization failed: {error}")
            })?;

            writer
                .append_acp_session(&AcpSessionMetadata {
                    agent_name: String::from("thndrs"),
                    acp_session_id: session_id.clone(),
                    command: String::from("thndrs acp serve"),
                    protocol_version: format!("{:?}", ProtocolVersion::V1),
                    agent_info_name: None,
                    agent_info_version: None,
                    client_info_name: inner.client_info.as_ref().map(|info| info.name.clone()),
                    client_info_version: inner.client_info.as_ref().map(|info| info.version.clone()),
                })
                .map_err(|error| {
                    inner.sessions.remove_session(&session_id);
                    format!("ACP session metadata write failed: {error}")
                })?;

            inner
                .sessions
                .attach_session_writer(&session_id, writer)
                .map_err(|error| {
                    inner.sessions.remove_session(&session_id);
                    error.to_string()
                })?;
        }
        Ok(session_id)
    }

    fn record_client_info(&self, request: &InitializeRequest) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.client_info = request.client_info.clone();
            inner.client_capabilities = request.client_capabilities.clone();
        }
    }

    fn client_fs_capabilities(&self) -> FileSystemCapabilities {
        self.inner
            .lock()
            .map(|inner| inner.client_capabilities.fs.clone())
            .unwrap_or_default()
    }

    /// Return whether the initialized ACP client can run terminal commands.
    pub fn client_can_run_terminal(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.client_capabilities.terminal)
            .unwrap_or(false)
    }

    /// Return whether the initialized ACP client can serve editor-visible reads.
    pub fn client_can_read_text_files(&self) -> bool {
        self.client_fs_capabilities().read_text_file
    }

    /// Return whether the initialized ACP client can perform editor-visible writes.
    pub fn client_can_write_text_files(&self) -> bool {
        self.client_fs_capabilities().write_text_file
    }

    fn config_options_for_session(&self, session_id: &str) -> Result<Vec<SessionConfigOption>, String> {
        let session = self.session(session_id)?;
        let model = session.metadata.model.unwrap_or_else(|| self.config.model.clone());
        let websearch = session
            .metadata
            .websearch
            .or_else(|| websearch_mode(&self.config.websearch))
            .unwrap_or(WebSearchMode::Auto);
        let effort = session
            .metadata
            .reasoning_effort
            .unwrap_or(self.config.reasoning_effort);
        let summary = session
            .metadata
            .reasoning_summary
            .unwrap_or(self.config.reasoning_summary);
        Ok(acp_config_options(&model, websearch, effort, summary))
    }

    fn set_config_option(&self, session_id: &str, option: &ConfigOptionValue) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        let session = inner
            .sessions
            .session(session_id)
            .cloned()
            .ok_or_else(|| format!("missing session: {session_id}"))?;
        if let ConfigOptionValue::ReasoningEffort(effort) = option {
            let active_model = session.metadata.model.as_deref().unwrap_or(&self.config.model);
            if !crate::providers::reasoning_option_is_supported(active_model, *effort) {
                return Err(format!(
                    "reasoning control `{}` is not supported by {active_model}",
                    effort.label()
                ));
            }
        }
        let model = match option {
            ConfigOptionValue::Model(model) => Some(model.clone()),
            ConfigOptionValue::WebSearch(_)
            | ConfigOptionValue::ReasoningEffort(_)
            | ConfigOptionValue::ReasoningSummary(_) => session.metadata.model,
        };
        let websearch = match option {
            ConfigOptionValue::Model(_)
            | ConfigOptionValue::ReasoningEffort(_)
            | ConfigOptionValue::ReasoningSummary(_) => session.metadata.websearch,
            ConfigOptionValue::WebSearch(mode) => Some(*mode),
        };
        inner
            .sessions
            .update_session_metadata(session_id, model, websearch)
            .map_err(|error| error.to_string())?;
        let effort = match option {
            ConfigOptionValue::ReasoningEffort(effort) => Some(*effort),
            _ => session.metadata.reasoning_effort,
        };
        let summary = match option {
            ConfigOptionValue::ReasoningSummary(summary) => Some(*summary),
            _ => session.metadata.reasoning_summary,
        };
        inner
            .sessions
            .update_session_reasoning(session_id, effort, summary)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn append_record(&self, session_id: &str, record: SessionRecord) {
        if let Ok(mut inner) = self.inner.lock()
            && let Some(writer) = inner.sessions.session_writer_mut(session_id)
        {
            let _ = writer.append(record);
        }
    }

    fn session(&self, session_id: &str) -> Result<AcpServerSession, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        inner
            .sessions
            .session(session_id)
            .cloned()
            .ok_or_else(|| format!("unknown ACP session id `{session_id}`"))
    }

    fn attach_mcp_config(&self, session_id: &str, mcp_config: McpConfig) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        inner
            .sessions
            .attach_mcp_config(session_id, mcp_config)
            .map_err(|error| error.to_string())
    }

    fn list_sessions(&self, cwd: Option<&Path>) -> Result<Vec<SessionInfo>, String> {
        let mut sessions = BTreeMap::new();
        if let Some(session_dir) = &self.config.session_dir {
            for path in crate::session::list_session_files(session_dir) {
                if let Some(info) = session_info_from_file(&path, cwd) {
                    sessions.insert(info.session_id.0.to_string(), info);
                }
            }
        }

        let inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        for session in inner.sessions.sessions() {
            if cwd.is_some_and(|expected| expected != session.cwd) {
                continue;
            }
            sessions.insert(
                session.acp_session_id.clone(),
                SessionInfo::new(session.acp_session_id.clone(), session.cwd.clone())
                    .title(Some(session.metadata.local_session_id.clone())),
            );
        }

        Ok(sessions.into_values().collect())
    }

    fn load_session(&self, session_id: &str, cwd: &Path) -> Result<Vec<SessionRecord>, String> {
        let session_path = self.persisted_session_path(session_id)?;
        let records = SessionReader::read_records(&session_path);
        let stored_cwd = session_cwd(&records).unwrap_or_else(|| cwd.to_path_buf());
        if stored_cwd != validate_and_normalize_cwd(cwd, None).map_err(|error| error.to_string())? {
            return Err(format!(
                "session `{session_id}` belongs to cwd `{}`",
                stored_cwd.display()
            ));
        }
        let writer = SessionWriter::resume(&session_path, session_id).map_err(|error| error.to_string())?;
        self.attach_loaded_session(session_id, &stored_cwd, Some(writer))?;
        Ok(records)
    }

    fn resume_session(&self, session_id: &str, cwd: &Path) -> Result<(), String> {
        let session_path = self.persisted_session_path(session_id)?;
        let records = SessionReader::read_records(&session_path);
        let stored_cwd = session_cwd(&records).unwrap_or_else(|| cwd.to_path_buf());
        if stored_cwd != validate_and_normalize_cwd(cwd, None).map_err(|error| error.to_string())? {
            return Err(format!(
                "session `{session_id}` belongs to cwd `{}`",
                stored_cwd.display()
            ));
        }
        let writer = SessionWriter::resume(&session_path, session_id).map_err(|error| error.to_string())?;
        self.attach_loaded_session(session_id, &stored_cwd, Some(writer))?;
        Ok(())
    }

    fn attach_loaded_session(&self, session_id: &str, cwd: &Path, writer: Option<SessionWriter>) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        if inner.sessions.session(session_id).is_some() {
            return Ok(());
        }
        inner
            .sessions
            .attach_existing_session(session_id, session_id, cwd, writer)
            .map_err(|error| error.to_string())?;
        inner
            .sessions
            .update_session_metadata(
                session_id,
                Some(self.config.model.clone()),
                websearch_mode(&self.config.websearch),
            )
            .map_err(|error| error.to_string())?;
        inner
            .sessions
            .update_session_reasoning(
                session_id,
                Some(self.config.reasoning_effort),
                Some(self.config.reasoning_summary),
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn persisted_session_path(&self, session_id: &str) -> Result<PathBuf, String> {
        let Some(session_dir) = &self.config.session_dir else {
            return Err(String::from("session persistence is not configured"));
        };
        let path = session_dir.join(format!("{session_id}.jsonl"));
        if path.exists() { Ok(path) } else { Err(format!("unknown persisted session `{session_id}`")) }
    }

    fn close_session(&self, session_id: &str) -> Result<(), String> {
        self.cancel_session(session_id);
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        inner
            .sessions
            .remove_session(session_id)
            .map(|_| ())
            .ok_or_else(|| format!("missing session: {session_id}"))
    }

    fn delete_session(&self, session_id: &str) -> Result<(), String> {
        self.cancel_session(session_id);
        if let Ok(mut inner) = self.inner.lock() {
            inner.sessions.remove_session(session_id);
        }

        if let Some(session_dir) = &self.config.session_dir {
            let path = session_dir.join(format!("{session_id}.jsonl"));
            if path.exists() {
                return Err(String::from(
                    "session/delete is non-destructive until the persisted deletion policy is decided",
                ));
            }
        }
        Ok(())
    }

    fn begin_turn(&self, session_id: &str) -> Result<PromptTurnGuard, String> {
        {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| String::from("ACP server state lock poisoned"))?;
            inner.cancelled.remove(session_id);
            inner
                .sessions
                .begin_turn(session_id)
                .map_err(|error| error.to_string())?;
        }

        Ok(PromptTurnGuard { session_id: session_id.to_string(), state: self.clone() })
    }

    fn end_turn(&self, session_id: &str) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| String::from("ACP server state lock poisoned"))?;
        inner.sessions.end_turn(session_id).map_err(|error| error.to_string())
    }

    /// Mark a known session cancelled.
    pub fn cancel_session(&self, session_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            if !inner.sessions.is_turn_active(session_id) {
                return;
            }
            let first_cancel = inner.cancelled.insert(session_id.to_string());
            if let Some(token) = inner.active_turn_cancels.remove(session_id) {
                token.cancel();
            }
            if first_cancel && let Some(writer) = inner.sessions.session_writer_mut(session_id) {
                let _ = writer.append(SessionRecord::Cancelled {
                    schema_version: SCHEMA_VERSION,
                    seq: 0,
                    time: crate::utils::datetime::now_iso8601(),
                    turn_id: String::from("acp-active-turn"),
                    reason: String::from("cancelled by ACP client"),
                });
            }
        }
    }

    /// Associate a running turn's cancel token with the session.
    pub fn register_turn_cancel_token(&self, session_id: &str, token: CancelToken) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.active_turn_cancels.insert(session_id.to_string(), token);
        }
    }

    /// Drop any active-turn cancellation token for a finished turn.
    pub fn clear_turn_cancel_token(&self, session_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.active_turn_cancels.remove(session_id);
            inner.cancelled.remove(session_id);
        }
    }

    /// Return whether a session exists.
    pub fn has_session(&self, session_id: &str) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.sessions.session(session_id).is_some())
            .unwrap_or(false)
    }

    fn is_cancelled(&self, session_id: &str) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.cancelled.contains(session_id))
            .unwrap_or(false)
    }
}

#[derive(Debug)]
struct PersistedTurn {
    turn_id: String,
    assistant: String,
    reasoning: String,
    finished: bool,
}

impl PersistedTurn {
    fn new(turn_id: String) -> Self {
        Self { turn_id, assistant: String::new(), reasoning: String::new(), finished: false }
    }

    fn record_event(&mut self, state: &ServerState, session_id: &str, event: &AgentEvent) {
        match event {
            AgentEvent::AssistantDelta(text) => self.assistant.push_str(text),
            AgentEvent::ReasoningDelta(text) => self.reasoning.push_str(text),
            AgentEvent::Usage { input_tokens, output_tokens } => {
                state.append_record(
                    session_id,
                    SessionRecord::Usage {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        input_tokens: *input_tokens,
                        output_tokens: *output_tokens,
                    },
                );
            }
            AgentEvent::ToolStarted { id, name, arguments } => {
                state.append_record(
                    session_id,
                    SessionRecord::ToolStarted {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        turn_id: self.turn_id.clone(),
                        call_id: id.clone(),
                        name: name.clone(),
                        arguments: sanitize_tool_payload(arguments),
                        mcp: None,
                    },
                );
            }
            AgentEvent::ToolFinished { id, output, status, write_result, shell_result } => {
                state.append_record(
                    session_id,
                    SessionRecord::ToolFinished {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        turn_id: self.turn_id.clone(),
                        call_id: id.clone(),
                        status: *status,
                        output: output.clone(),
                        mcp: None,
                    },
                );
                if let Some(result) = write_result {
                    state.append_record(
                        session_id,
                        SessionRecord::FileWrite {
                            schema_version: SCHEMA_VERSION,
                            seq: 0,
                            time: crate::utils::datetime::now_iso8601(),
                            turn_id: self.turn_id.clone(),
                            op: result.op,
                            path: result.path.display().to_string(),
                            before_hash: result.before_hash,
                            before_bytes: result.before_bytes,
                            after_hash: result.after_hash,
                            after_bytes: result.after_bytes,
                            status: *status,
                        },
                    );
                }
                if let Some(result) = shell_result {
                    state.append_record(
                        session_id,
                        SessionRecord::ShellExec {
                            schema_version: SCHEMA_VERSION,
                            seq: 0,
                            time: crate::utils::datetime::now_iso8601(),
                            turn_id: self.turn_id.clone(),
                            command: crate::tools::shell::redact_secrets(&result.command.join(" ")),
                            cwd: result.cwd.display().to_string(),
                            process_status: result.status.label().to_string(),
                            exit_code: result.exit_code,
                            elapsed_ms: result.elapsed.as_millis() as u64,
                            kind: result.kind.label().to_string(),
                        },
                    );
                }
            }
            AgentEvent::Cancelled => {
                state.append_record(
                    session_id,
                    SessionRecord::Cancelled {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        turn_id: self.turn_id.clone(),
                        reason: String::from("cancelled by ACP client"),
                    },
                );
            }
            AgentEvent::Failed(error) => {
                state.append_record(
                    session_id,
                    SessionRecord::Failed {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        turn_id: self.turn_id.clone(),
                        error: error.clone(),
                    },
                );
            }
            _ => {}
        }
    }

    fn finish(&mut self, state: &ServerState, session_id: &str) {
        if self.finished {
            return;
        }
        self.finished = true;

        if !self.reasoning.is_empty() {
            state.append_record(
                session_id,
                SessionRecord::ReasoningFinished {
                    schema_version: SCHEMA_VERSION,
                    seq: 0,
                    time: crate::utils::datetime::now_iso8601(),
                    turn_id: self.turn_id.clone(),
                    text: self.reasoning.clone(),
                },
            );
        }
        if !self.assistant.is_empty() {
            state.append_record(
                session_id,
                SessionRecord::AssistantFinished {
                    schema_version: SCHEMA_VERSION,
                    seq: 0,
                    time: crate::utils::datetime::now_iso8601(),
                    turn_id: self.turn_id.clone(),
                    text: self.assistant.clone(),
                },
            );
        }
    }
}

struct PromptTurnGuard {
    session_id: String,
    state: ServerState,
}

impl Drop for PromptTurnGuard {
    fn drop(&mut self) {
        let _ = self.state.end_turn(&self.session_id);
        self.state.clear_turn_cancel_token(&self.session_id);
    }
}

/// Run the ACP server over stdio with all v1 baseline handlers registered.
pub async fn run_stdio(config: ServerConfig) -> Result<()> {
    let state = ServerState::new(config);
    let initialize_state = state.clone();
    let new_session_state = state.clone();
    let list_session_state = state.clone();
    let load_session_state = state.clone();
    let resume_session_state = state.clone();
    let close_session_state = state.clone();
    let delete_session_state = state.clone();
    let prompt_state = state.clone();
    let config_option_state = state.clone();
    let cancel_state = state;
    let (eof_tx, eof_rx) = oneshot::channel();
    let transport = Lines::new(
        stdout_line_sink(tokio::io::stdout().compat_write()),
        eof_signaling_lines(tokio::io::stdin().compat(), eof_tx),
    );

    let connection = Agent
        .builder()
        .name("thndrs")
        .on_receive_request(
            async move |request: InitializeRequest, responder, _connection| {
                let response = initialize(&initialize_state, &request);
                responder.respond(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: NewSessionRequest, responder, _connection| match new_session(
                &new_session_state,
                &request,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ListSessionsRequest, responder, _connection| match list_sessions(
                &list_session_state,
                &request,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: LoadSessionRequest, responder, connection: ConnectionTo<Client>| match load_session(
                &load_session_state,
                &request,
                &connection,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ResumeSessionRequest, responder, _connection| match resume_session(
                &resume_session_state,
                &request,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: CloseSessionRequest, responder, _connection| match close_session(
                &close_session_state,
                &request,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: DeleteSessionRequest, responder, _connection| match delete_session(
                &delete_session_state,
                &request,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: SetSessionConfigOptionRequest, responder, _connection| match set_config_option(
                &config_option_state,
                &request,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, connection: ConnectionTo<Client>| {
                let state = prompt_state.clone();
                let prompt_connection = connection.clone();
                connection.spawn(async move {
                    match tokio::task::spawn_blocking(move || prompt(&state, &request, &prompt_connection)).await {
                        Ok(Ok(response)) => responder.respond(response),
                        Ok(Err(error)) => responder.respond_with_error(Error::invalid_params().data(error)),
                        Err(error) => responder.respond_with_error(
                            Error::internal_error().data(format!("ACP prompt task failed: {error}")),
                        ),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |notification: CancelNotification, _connection| {
                cancel_state.cancel_session(notification.session_id.0.as_ref());
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_dispatch(
            async move |message: Dispatch, cx: ConnectionTo<Client>| {
                message.respond_with_error(Error::method_not_found().data("unhandled ACP method"), cx)
            },
            agent_client_protocol::on_receive_dispatch!(),
        )
        .connect_to(transport);

    match select(Box::pin(connection), Box::pin(eof_rx.map(|_| Ok(())))).await {
        Either::Left((result, _)) | Either::Right((result, _)) => result,
    }
}

pub fn initialize(state: &ServerState, request: &InitializeRequest) -> InitializeResponse {
    state.record_client_info(request);
    InitializeResponse::new(negotiate_protocol_version(request.protocol_version))
        .agent_info(Some(
            Implementation::new("thndrs", env!("CARGO_PKG_VERSION")).title(Some(String::from("thndrs"))),
        ))
        .agent_capabilities(capabilities(state))
}

pub fn set_config_option(
    state: &ServerState, request: &SetSessionConfigOptionRequest,
) -> Result<SetSessionConfigOptionResponse, String> {
    let session_id = request.session_id.0.as_ref();
    let option_id = request.config_id.0.as_ref();
    let value = request.value.to_string();
    let option = validate_config_option(option_id, &value).map_err(|error| error.to_string())?;
    state.set_config_option(session_id, &option)?;
    Ok(SetSessionConfigOptionResponse::new(
        state.config_options_for_session(session_id)?,
    ))
}

pub fn acp_mcp_config(servers: &[McpServer]) -> Result<McpConfig, String> {
    let mut config = McpConfig::default();
    for server in servers {
        match server {
            McpServer::Stdio(server) => {
                crate::mcp::config::validate_mcp_server_name(&server.name).map_err(|error| error.to_string())?;
                if server.command.as_os_str().is_empty() {
                    return Err(format!("MCP server `{}` has an empty stdio command", server.name));
                }
                config.servers.insert(
                    server.name.clone(),
                    McpServerConfig {
                        transport: McpTransport::Stdio,
                        command: server.command.display().to_string(),
                        args: server.args.clone(),
                        env: server
                            .env
                            .iter()
                            .map(|entry| (entry.name.clone(), entry.value.clone()))
                            .collect(),
                        ..McpServerConfig::default()
                    },
                );
            }
            McpServer::Http(server) => {
                return Err(format!(
                    "MCP server `{}` uses unsupported transport `http` in session/new",
                    server.name
                ));
            }
            McpServer::Sse(server) => {
                return Err(format!(
                    "MCP server `{}` uses unsupported transport `sse` in session/new",
                    server.name
                ));
            }
            _ => return Err(String::from("MCP server uses unsupported transport in session/new")),
        }
    }
    Ok(config)
}

pub fn execute_prompt(
    state: &ServerState, request: &PromptRequest, mut on_update: impl FnMut(SessionUpdateIntent) -> Result<(), String>,
    run_harness: impl FnOnce(AgentRunConfig, Vec<ProviderMessage>, bool, String) -> HarnessHandle,
) -> Result<PromptResponse, String> {
    let session_id = request.session_id.0.to_string();
    let prompt = assemble_prompt(&request.prompt)?;

    if state.is_cancelled(&session_id) {
        return Ok(PromptResponse::new(StopReason::Cancelled));
    }

    let turn_guard = state.begin_turn(&session_id)?;

    state.append_record(
        &session_id,
        SessionRecord::User {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: crate::utils::datetime::now_iso8601(),
            turn_id: format!("{session_id}-turn"),
            text: prompt.text.clone(),
        },
    );

    let response = run_prompt_turn(prompt, state, &session_id, &mut on_update, run_harness)?;

    drop(turn_guard);

    Ok(response)
}

/// Read editor-visible text from an ACP client when it advertised support.
pub async fn client_read_text_file(
    state: &ServerState, connection: &ConnectionTo<Client>, session_id: agent_client_protocol::schema::v1::SessionId,
    path: PathBuf, line: Option<u32>, limit: Option<u32>,
) -> Result<Option<String>, String> {
    if !state.client_can_read_text_files() {
        return Ok(None);
    }
    let request = ReadTextFileRequest::new(session_id, path).line(line).limit(limit);
    connection
        .send_request(request)
        .block_task()
        .await
        .map(|response| Some(response.content))
        .map_err(|error| format!("client fs/read_text_file failed: {error}"))
}

/// Write editor-visible text through an ACP client when it advertised support.
pub async fn client_write_text_file(
    state: &ServerState, connection: &ConnectionTo<Client>, session_id: agent_client_protocol::schema::v1::SessionId,
    path: PathBuf, content: String,
) -> Result<bool, String> {
    if !state.client_can_write_text_files() {
        return Ok(false);
    }
    connection
        .send_request(WriteTextFileRequest::new(session_id, path, content))
        .block_task()
        .await
        .map(|_| true)
        .map_err(|error| format!("client fs/write_text_file failed: {error}"))
}

fn new_session(state: &ServerState, request: &NewSessionRequest) -> Result<NewSessionResponse, String> {
    if !request.cwd.is_absolute() {
        return Err(format!("ACP session cwd must be absolute: {}", request.cwd.display()));
    }
    let cwd = validate_and_normalize_cwd(&request.cwd, None).map_err(|err| err.to_string())?;
    let mcp_config = acp_mcp_config(&request.mcp_servers)?;
    let session_id = state.create_session(&cwd)?;
    if !mcp_config.servers.is_empty() {
        state.attach_mcp_config(&session_id, mcp_config)?;
    }
    let config_options = state.config_options_for_session(&session_id)?;
    Ok(NewSessionResponse::new(session_id).config_options(config_options))
}

fn list_sessions(state: &ServerState, request: &ListSessionsRequest) -> Result<ListSessionsResponse, String> {
    if request.cursor.is_some() {
        return Ok(ListSessionsResponse::new(Vec::new()));
    }
    if let Some(cwd) = &request.cwd
        && !cwd.is_absolute()
    {
        return Err(format!("ACP session list cwd must be absolute: {}", cwd.display()));
    }
    Ok(ListSessionsResponse::new(state.list_sessions(request.cwd.as_deref())?))
}

fn load_session(
    state: &ServerState, request: &LoadSessionRequest, connection: &ConnectionTo<Client>,
) -> Result<LoadSessionResponse, String> {
    if !request.cwd.is_absolute() {
        return Err(format!(
            "ACP session load cwd must be absolute: {}",
            request.cwd.display()
        ));
    }
    let session_id = request.session_id.0.as_ref();
    let records = state.load_session(session_id, &request.cwd)?;
    for record in records {
        if let Some(update) = replay_record_update(&record) {
            send_update(connection, request.session_id.clone(), update)?;
        }
    }
    Ok(LoadSessionResponse::new().config_options(state.config_options_for_session(session_id)?))
}

fn resume_session(state: &ServerState, request: &ResumeSessionRequest) -> Result<ResumeSessionResponse, String> {
    if !request.cwd.is_absolute() {
        return Err(format!(
            "ACP session resume cwd must be absolute: {}",
            request.cwd.display()
        ));
    }
    let session_id = request.session_id.0.as_ref();
    state.resume_session(session_id, &request.cwd)?;
    Ok(ResumeSessionResponse::new().config_options(state.config_options_for_session(session_id)?))
}

fn close_session(state: &ServerState, request: &CloseSessionRequest) -> Result<CloseSessionResponse, String> {
    state.close_session(request.session_id.0.as_ref())?;
    Ok(CloseSessionResponse::new())
}

fn delete_session(state: &ServerState, request: &DeleteSessionRequest) -> Result<DeleteSessionResponse, String> {
    state.delete_session(request.session_id.0.as_ref())?;
    Ok(DeleteSessionResponse::new())
}

fn prompt(
    state: &ServerState, request: &PromptRequest, connection: &ConnectionTo<Client>,
) -> Result<PromptResponse, String> {
    let session_id = request.session_id.clone();
    execute_prompt(
        state,
        request,
        move |intent| send_update(connection, session_id.clone(), intent),
        {
            let connection = connection.clone();
            move |config, messages, expects_write, _prompt_text| {
                let permission_hook = server_permission_hook(
                    state.clone(),
                    request.session_id.clone(),
                    format!("{}-turn", request.session_id.0.as_ref()),
                    connection.clone(),
                );
                let execution_hook = server_execution_hook(state.clone(), request.session_id.clone(), connection);
                let (_steering_tx, steering_rx) = mpsc::channel();
                drop(_steering_tx);
                crate::harness::HarnessTurn::provider_with_steering_permissions_and_execution(
                    config,
                    messages,
                    expects_write,
                    steering_rx,
                    permission_hook,
                    execution_hook,
                )
                .start()
            }
        },
    )
}

fn server_execution_hook(
    state: ServerState, session_id: agent_client_protocol::schema::v1::SessionId, connection: ConnectionTo<Client>,
) -> ToolExecutionHook {
    ToolExecutionHook::new(move |request, config, cancel| {
        if request.name != "run_shell" || !state.client_can_run_terminal() {
            return None;
        }
        Some(execute_shell_in_client_terminal(
            &connection,
            session_id.clone(),
            request,
            config,
            cancel,
        ))
    })
}

fn server_permission_hook(
    state: ServerState, session_id: agent_client_protocol::schema::v1::SessionId, turn_id: String,
    connection: ConnectionTo<Client>,
) -> ToolPermissionHook {
    ToolPermissionHook::new(move |request, _config, cancel| {
        request_client_permission(&state, &connection, session_id.clone(), &turn_id, request, cancel)
    })
}

fn request_client_permission(
    state: &ServerState, connection: &ConnectionTo<Client>, session_id: agent_client_protocol::schema::v1::SessionId,
    turn_id: &str, request: &ToolUseRequest, cancel: &CancelToken,
) -> ToolPermissionDecision {
    if cancel.is_cancelled() {
        return ToolPermissionDecision::Cancelled;
    }

    let options = vec![
        PermissionOption::new("allow_once", "Allow once", PermissionOptionKind::AllowOnce),
        PermissionOption::new("reject_once", "Reject once", PermissionOptionKind::RejectOnce),
    ];
    let title = permission_title(request);
    let session_id_text = session_id.0.to_string();
    state.append_record(
        &session_id_text,
        SessionRecord::AcpPermissionRequest {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: crate::utils::datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            tool_call_id: request.tool_use_id.clone(),
            title: title.clone(),
            options: options
                .iter()
                .map(|option| AcpPermissionOptionRecord {
                    id: option.option_id.0.to_string(),
                    name: option.name.clone(),
                    kind: match &option.kind {
                        PermissionOptionKind::AllowOnce => "allow_once",
                        PermissionOptionKind::AllowAlways => "allow_always",
                        PermissionOptionKind::RejectOnce => "reject_once",
                        PermissionOptionKind::RejectAlways => "reject_always",
                        _ => "other",
                    }
                    .to_string(),
                })
                .collect(),
        },
    );

    let permission = RequestPermissionRequest::new(
        session_id,
        ToolCallUpdate::new(
            request.tool_use_id.clone(),
            ToolCallUpdateFields::new()
                .title(title)
                .kind(classify_tool(&request.name).to_acp_kind())
                .status(ToolCallStatus::InProgress)
                .raw_input(json_text_or_value(sanitize_tool_payload(&request.arguments))),
        ),
        options,
    );
    let (tx, rx) = mpsc::channel();
    if connection
        .send_request(permission)
        .on_receiving_result(async move |result| {
            let _ = tx.send(result);
            Ok(())
        })
        .is_err()
    {
        return ToolPermissionDecision::Reject;
    }

    loop {
        if cancel.is_cancelled() {
            return ToolPermissionDecision::Cancelled;
        }
        match rx.recv_timeout(PERMISSION_POLL_INTERVAL) {
            Ok(Ok(response)) => {
                let decision = permission_response_decision(response.outcome);
                state.append_record(
                    &session_id_text,
                    SessionRecord::AcpPermissionOutcome {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        turn_id: turn_id.to_string(),
                        tool_call_id: request.tool_use_id.clone(),
                        outcome: decision.outcome_label().to_string(),
                    },
                );
                return decision;
            }
            Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                state.append_record(
                    &session_id_text,
                    SessionRecord::AcpPermissionOutcome {
                        schema_version: SCHEMA_VERSION,
                        seq: 0,
                        time: crate::utils::datetime::now_iso8601(),
                        turn_id: turn_id.to_string(),
                        tool_call_id: request.tool_use_id.clone(),
                        outcome: "rejected".to_string(),
                    },
                );
                return ToolPermissionDecision::Reject;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn execute_shell_in_client_terminal(
    connection: &ConnectionTo<Client>, session_id: agent_client_protocol::schema::v1::SessionId,
    request: &ToolUseRequest, config: &AgentRunConfig, cancel: &CancelToken,
) -> (ToolOutput, Option<crate::tools::WriteResult>, Option<ProcessResult>) {
    let args = match crate::tools::shell::parse_arguments(&request.arguments) {
        Ok(args) if !args.program.trim().is_empty() => args,
        Ok(_) => return failed_shell_execution("missing or empty 'program' field"),
        Err(error) => return failed_shell_execution(&error.to_string()),
    };
    let cwd = match client_terminal_cwd(&config.root, &args.cwd) {
        Ok(cwd) => cwd,
        Err(error) => return failed_shell_execution(&error),
    };

    let create = CreateTerminalRequest::new(session_id.clone(), args.program.clone())
        .args(args.args.clone())
        .cwd(Some(cwd.clone()))
        .output_byte_limit(Some(MAX_OUTPUT_BYTES as u64));
    let create_response = match block_client_request(connection, create) {
        Ok(response) => response,
        Err(error) => return failed_shell_execution(&format!("client terminal/create failed: {error}")),
    };

    let terminal_id = create_response.terminal_id;
    let start = Instant::now();
    let (mut final_output, exit_status) = loop {
        if cancel.is_cancelled() {
            let _ = block_client_request(
                connection,
                KillTerminalRequest::new(session_id.clone(), terminal_id.clone()),
            );
            let output = block_client_request(
                connection,
                TerminalOutputRequest::new(session_id.clone(), terminal_id.clone()),
            )
            .unwrap_or_else(|_| TerminalOutputResponse::new(String::new(), false));
            break (output, None);
        }

        match block_client_request(
            connection,
            TerminalOutputRequest::new(session_id.clone(), terminal_id.clone()),
        ) {
            Ok(output) => {
                let status = output.exit_status.clone();
                if status.is_some() {
                    break (output, status);
                }
            }
            Err(error) => {
                let _ = block_client_request(connection, ReleaseTerminalRequest::new(session_id, terminal_id));
                return failed_shell_execution(&format!("client terminal/output failed: {error}"));
            }
        }

        std::thread::sleep(TERMINAL_POLL_INTERVAL);
    };

    let wait_status = match exit_status {
        Some(status) => status,
        None => TerminalExitStatus::new(),
    };
    let wait = block_client_request(
        connection,
        WaitForTerminalExitRequest::new(session_id.clone(), terminal_id.clone()),
    )
    .ok()
    .map(|response| response.exit_status)
    .unwrap_or(wait_status);
    if let Ok(output) = block_client_request(
        connection,
        TerminalOutputRequest::new(session_id.clone(), terminal_id.clone()),
    ) {
        final_output = output;
    }
    let _ = block_client_request(connection, ReleaseTerminalRequest::new(session_id, terminal_id));

    let result = process_result_from_terminal(&args, cwd, start.elapsed(), &final_output, &wait, cancel);
    let output = result.to_tool_output();
    (output, None, Some(result))
}

fn block_client_request<R>(
    connection: &ConnectionTo<Client>, request: R,
) -> std::result::Result<R::Response, agent_client_protocol::Error>
where
    R: JsonRpcRequest,
{
    futures::executor::block_on(connection.send_request(request).block_task())
}

fn failed_shell_execution(message: &str) -> (ToolOutput, Option<crate::tools::WriteResult>, Option<ProcessResult>) {
    (ToolOutput::failed("run_shell", message.to_string()), None, None)
}

fn client_terminal_cwd(root: &Path, cwd: &Option<PathBuf>) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("invalid workspace root for terminal: {error}"))?;
    let Some(cwd) = cwd else {
        return Ok(root);
    };
    if cwd.is_absolute() {
        return Err(String::from("run_shell cwd must be relative to the workspace root"));
    }
    let resolved = crate::tools::resolve_workspace_path(&root, cwd).map_err(|error| error.to_string())?;
    if !resolved.is_dir() {
        return Err(format!("working directory is not a directory: {}", resolved.display()));
    }
    Ok(resolved)
}

fn process_result_from_terminal(
    args: &ShellArgs, cwd: PathBuf, elapsed: Duration, output: &TerminalOutputResponse,
    exit_status: &TerminalExitStatus, cancel: &CancelToken,
) -> ProcessResult {
    let status = if cancel.is_cancelled() {
        ProcessStatus::Cancelled
    } else {
        match exit_status.exit_code {
            Some(0) => ProcessStatus::Ok,
            Some(_) => ProcessStatus::Failed,
            None => ProcessStatus::Failed,
        }
    };
    let mut stdout = output.output.lines().map(redact_secrets).collect::<Vec<_>>();
    if output.truncated {
        stdout.insert(0, "[terminal output truncated]".to_string());
    }
    ProcessResult {
        command: args.argv(),
        cwd,
        status,
        exit_code: exit_status.exit_code.and_then(|code| i32::try_from(code).ok()),
        stdout,
        stderr: Vec::new(),
        elapsed,
        kind: ProcessKind::OneShot,
    }
}

fn permission_response_decision(outcome: RequestPermissionOutcome) -> ToolPermissionDecision {
    match outcome {
        RequestPermissionOutcome::Cancelled => ToolPermissionDecision::Cancelled,
        RequestPermissionOutcome::Selected(selected) => {
            let option_id = selected.option_id.0.as_ref();
            if option_id.starts_with("allow") {
                ToolPermissionDecision::Allow
            } else {
                ToolPermissionDecision::Reject
            }
        }
        _ => ToolPermissionDecision::Reject,
    }
}

fn permission_title(request: &ToolUseRequest) -> String {
    match request.name.as_str() {
        "run_shell" => "Run shell command".to_string(),
        "create_file" => "Create file".to_string(),
        "replace_range" => "Edit file".to_string(),
        "write_patch" => "Apply patch".to_string(),
        _ => format!("Run {}", request.name),
    }
}

/// Run one harness-backed ACP prompt turn and stream updates while pending.
fn run_prompt_turn(
    prompt: PromptAssembly, state: &ServerState, session_id: &str,
    on_update: &mut impl FnMut(SessionUpdateIntent) -> Result<(), String>,
    run_harness: impl FnOnce(AgentRunConfig, Vec<ProviderMessage>, bool, String) -> HarnessHandle,
) -> Result<PromptResponse, String> {
    let session = state.session(session_id)?;
    let websearch = session
        .metadata
        .websearch
        .or_else(|| websearch_mode(&state.config.websearch))
        .unwrap_or(WebSearchMode::Auto);
    let model = session.metadata.model.unwrap_or_else(|| state.config.model.clone());

    let effort = session
        .metadata
        .reasoning_effort
        .unwrap_or(state.config.reasoning_effort);
    let summary = session
        .metadata
        .reasoning_summary
        .unwrap_or(state.config.reasoning_summary);
    let mut config = AgentRunConfig::new(session.cwd, model, websearch).with_reasoning(effort, summary);
    if let Some(mcp_config) = session.mcp_config {
        config = config.with_mcp_manager(Arc::new(McpManager::from_config(&mcp_config)));
    }
    let bundle = prompt::PromptBundle::new(&config.root, &config.model, config.search_mode, &[], &[], "");
    let mut messages = crate::prompt::lower_to_umans_messages(&bundle);
    if !prompt.provider_blocks.is_empty() {
        messages.push(ProviderMessage::user_blocks(prompt.provider_blocks.clone()));
    }
    let expects_write = prompt_expects_workspace_write(&prompt.text);
    let handle = run_harness(config, messages, expects_write, prompt.text);
    state.register_turn_cancel_token(session_id, handle.cancel.clone());
    if state.is_cancelled(session_id) {
        handle.cancel.cancel();
    }

    let response = run_prompt_handle(state, session_id, &handle, on_update)?;
    state.clear_turn_cancel_token(session_id);
    Ok(response)
}

fn run_prompt_handle(
    state: &ServerState, session_id: &str, handle: &HarnessHandle,
    on_update: &mut impl FnMut(SessionUpdateIntent) -> Result<(), String>,
) -> Result<PromptResponse, String> {
    let mut persisted = PersistedTurn::new(format!("{session_id}-turn"));
    loop {
        match handle.events.recv() {
            Ok(event) => {
                persisted.record_event(state, session_id, &event);
                for intent in map_agent_event(&event) {
                    on_update(intent)?;
                }
                if handle.cancel.is_cancelled() {
                    persisted.finish(state, session_id);
                    return Ok(PromptResponse::new(StopReason::Cancelled));
                }

                match event {
                    AgentEvent::Finished => {
                        persisted.finish(state, session_id);
                        return Ok(PromptResponse::new(StopReason::EndTurn));
                    }
                    AgentEvent::Cancelled => {
                        persisted.finish(state, session_id);
                        return Ok(PromptResponse::new(StopReason::Cancelled));
                    }
                    AgentEvent::Failed(_) => {
                        persisted.finish(state, session_id);
                        return Ok(PromptResponse::new(StopReason::Refusal));
                    }
                    _ => (),
                }
            }
            Err(_) if handle.cancel.is_cancelled() => {
                persisted.finish(state, session_id);
                return Ok(PromptResponse::new(StopReason::Cancelled));
            }
            Err(_) => return Err(String::from("prompt turn ended without a terminal event")),
        }
    }
}

fn capabilities(_state: &ServerState) -> AgentCapabilities {
    AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(PromptCapabilities::new().image(true).embedded_context(true))
        .mcp_capabilities(McpCapabilities::new())
        .session_capabilities(
            SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .resume(SessionResumeCapabilities::new())
                .close(SessionCloseCapabilities::new())
                .delete(SessionDeleteCapabilities::new()),
        )
}

fn negotiate_protocol_version(requested: ProtocolVersion) -> ProtocolVersion {
    let _ = requested;
    ProtocolVersion::V1
}

#[derive(Clone, Debug, PartialEq)]
struct PromptAssembly {
    text: String,
    provider_blocks: Vec<ProviderContentBlock>,
}

fn assemble_prompt(blocks: &[ContentBlock]) -> Result<PromptAssembly, String> {
    let mut text_parts = Vec::new();
    let mut provider_blocks = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(content) => {
                text_parts.push(content.text.clone());
                provider_blocks.push(ProviderContentBlock::Text { text: content.text.clone() });
            }
            ContentBlock::Image(content) => {
                let marker = image_prompt_marker(&content.mime_type, content.uri.as_deref());
                text_parts.push(marker);
                provider_blocks.push(ProviderContentBlock::Image {
                    source: ProviderImageSource::Base64 {
                        media_type: content.mime_type.clone(),
                        data: content.data.clone(),
                    },
                });
            }
            ContentBlock::ResourceLink(link) => {
                let marker = resource_link_prompt_marker(link);
                text_parts.push(marker.clone());
                provider_blocks.push(ProviderContentBlock::Text { text: marker });
            }
            ContentBlock::Resource(resource) => {
                let marker = embedded_resource_prompt_marker(resource);
                text_parts.push(marker.clone());
                provider_blocks.push(ProviderContentBlock::Text { text: marker });
            }
            ContentBlock::Audio(_) => {
                return Err(String::from("unsupported ACP prompt content block: audio"));
            }
            other => {
                return Err(format!("unsupported ACP prompt content block: {other:?}"));
            }
        }
    }
    Ok(PromptAssembly { text: text_parts.join("\n"), provider_blocks })
}

fn image_prompt_marker(mime_type: &str, uri: Option<&str>) -> String {
    match uri {
        Some(uri) => format!("[image: {mime_type}; uri={uri}]"),
        None => format!("[image: {mime_type}]"),
    }
}

fn resource_link_prompt_marker(link: &ResourceLink) -> String {
    let mut details = vec![link.uri.clone()];
    if let Some(mime_type) = &link.mime_type {
        details.push(format!("mime={mime_type}"));
    }
    if let Some(description) = &link.description {
        details.push(format!("description={description}"));
    }
    format!("[resource link: {} ({})]", link.name, details.join(", "))
}

fn embedded_resource_prompt_marker(resource: &EmbeddedResource) -> String {
    match &resource.resource {
        EmbeddedResourceResource::TextResourceContents(contents) => {
            let mime = contents.mime_type.as_deref().unwrap_or("text/plain");
            format!("[embedded resource: {} ({mime})]\n{}", contents.uri, contents.text)
        }
        EmbeddedResourceResource::BlobResourceContents(contents) => {
            let mime = contents.mime_type.as_deref().unwrap_or("application/octet-stream");
            format!(
                "[embedded resource: {} ({mime}); base64 bytes={}]",
                contents.uri,
                contents.blob.len()
            )
        }
        other => format!("[embedded resource: unsupported payload {other:?}]"),
    }
}

fn send_update(
    connection: &ConnectionTo<Client>, session_id: agent_client_protocol::schema::v1::SessionId,
    intent: SessionUpdateIntent,
) -> Result<(), String> {
    match lower_update_intent(intent) {
        Some(update) => connection
            .send_notification(SessionNotification::new(session_id, update))
            .map_err(|err| format!("failed to send ACP session update: {err}")),
        None => Ok(()),
    }
}

fn lower_update_intent(intent: SessionUpdateIntent) -> Option<SessionUpdate> {
    match intent {
        SessionUpdateIntent::AssistantDelta(text) => Some(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text)),
        ))),
        SessionUpdateIntent::ReasoningDelta(text) => Some(SessionUpdate::AgentThoughtChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text)),
        ))),
        SessionUpdateIntent::Usage { input_tokens, output_tokens } => Some(SessionUpdate::UsageUpdate(
            UsageUpdate::new(input_tokens, input_tokens.saturating_add(output_tokens)),
        )),
        SessionUpdateIntent::Status(text) => Some(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text)),
        ))),
        SessionUpdateIntent::ToolStarted { id, name, arguments, kind, locations } => Some(SessionUpdate::ToolCall(
            ToolCall::new(id, name)
                .status(ToolCallStatus::InProgress)
                .kind(kind.to_acp_kind())
                .locations(locations)
                .raw_input(json_text_or_value(arguments)),
        )),
        SessionUpdateIntent::ToolFinished { id, status, output, locations, .. } => {
            let output_text = output.join("\n");
            let fields = ToolCallUpdateFields::new()
                .status(tool_call_status(status))
                .raw_output(json_text_or_value(output_text.clone()))
                .locations(locations)
                .content(if output_text.is_empty() {
                    None
                } else {
                    Some(vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                        TextContent::new(output_text),
                    )))])
                });
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id, fields)))
        }
        SessionUpdateIntent::Failed(message) => Some(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(format!("prompt failed: {message}"))),
        ))),
        SessionUpdateIntent::Cancelled => Some(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("prompt was cancelled")),
        ))),
        SessionUpdateIntent::Finished => None,
    }
}

fn replay_record_update(record: &SessionRecord) -> Option<SessionUpdateIntent> {
    match record {
        SessionRecord::User { text, .. } => Some(SessionUpdateIntent::Status(format!("user: {text}"))),
        SessionRecord::AssistantFinished { text, .. } => Some(SessionUpdateIntent::AssistantDelta(text.clone())),
        SessionRecord::ReasoningFinished { text, .. } => Some(SessionUpdateIntent::ReasoningDelta(text.clone())),
        SessionRecord::Usage { input_tokens, output_tokens, .. } => {
            Some(SessionUpdateIntent::Usage { input_tokens: *input_tokens, output_tokens: *output_tokens })
        }
        SessionRecord::ToolStarted { call_id, name, arguments, .. } => Some(SessionUpdateIntent::ToolStarted {
            id: call_id.clone(),
            name: name.clone(),
            arguments: sanitize_tool_payload(arguments),
            kind: classify_tool(name),
            locations: vec![],
        }),
        SessionRecord::ToolFinished { call_id, status, output, .. } => Some(SessionUpdateIntent::ToolFinished {
            id: call_id.clone(),
            status: (*status).into(),
            output: output.clone(),
            kind: crate::server::events::ToolCallKind::Other,
            locations: vec![],
        }),
        SessionRecord::Cancelled { .. } => Some(SessionUpdateIntent::Cancelled),
        SessionRecord::Failed { error, .. } => Some(SessionUpdateIntent::Failed(error.clone())),
        _ => None,
    }
}

fn session_info_from_file(path: &Path, cwd_filter: Option<&Path>) -> Option<SessionInfo> {
    let records = SessionReader::read_records(path);
    let session_id = &records
        .iter()
        .find_map(|record| match record {
            SessionRecord::SessionMeta { session_id, .. } => Some(session_id.clone()),
            _ => None,
        })
        .or_else(|| path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string))?;
    let cwd = session_cwd(&records)?;
    if cwd_filter.is_some_and(|expected| expected != cwd) {
        return None;
    }

    let title = SessionReader::read_title(path);
    let updated_at = records.iter().rev().find_map(record_time);
    Some(
        SessionInfo::new(session_id.to_string(), cwd)
            .title(Some(title))
            .updated_at(updated_at),
    )
}

fn session_cwd(records: &[SessionRecord]) -> Option<PathBuf> {
    records.iter().find_map(|record| match record {
        SessionRecord::SessionMeta { cwd, .. } => Some(PathBuf::from(cwd)),
        _ => None,
    })
}

fn record_time(record: &SessionRecord) -> Option<String> {
    match record {
        SessionRecord::SessionMeta { time, .. }
        | SessionRecord::Context { time, .. }
        | SessionRecord::ContextLedger { time, .. }
        | SessionRecord::ContextPin { time, .. }
        | SessionRecord::ContextDrop { time, .. }
        | SessionRecord::ContextRecovery { time, .. }
        | SessionRecord::MemoryWrite { time, .. }
        | SessionRecord::MemoryDelete { time, .. }
        | SessionRecord::Compaction { time, .. }
        | SessionRecord::User { time, .. }
        | SessionRecord::PromptMetadata { time, .. }
        | SessionRecord::AssistantFinished { time, .. }
        | SessionRecord::ReasoningFinished { time, .. }
        | SessionRecord::Usage { time, .. }
        | SessionRecord::ToolStarted { time, .. }
        | SessionRecord::ToolFinished { time, .. }
        | SessionRecord::Cancelled { time, .. }
        | SessionRecord::Failed { time, .. }
        | SessionRecord::AcpSession { time, .. }
        | SessionRecord::SessionRenamed { time, .. }
        | SessionRecord::FileWrite { time, .. }
        | SessionRecord::McpConfigChanged { time, .. }
        | SessionRecord::ShellExec { time, .. }
        | SessionRecord::SkillActivated { time, .. }
        | SessionRecord::QueuedInput { time, .. }
        | SessionRecord::AcpPermissionRequest { time, .. }
        | SessionRecord::AcpPermissionOutcome { time, .. } => Some(time.clone()),
    }
}

fn tool_call_status(intent: ToolStatusIntent) -> ToolCallStatus {
    match intent {
        ToolStatusIntent::InProgress => ToolCallStatus::InProgress,
        ToolStatusIntent::Completed => ToolCallStatus::Completed,
        ToolStatusIntent::Failed => ToolCallStatus::Failed,
    }
}

fn json_text_or_value(raw: String) -> serde_json::Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::Value::String(raw))
}

fn websearch_mode(value: &str) -> Option<WebSearchMode> {
    match value {
        "auto" => Some(WebSearchMode::Auto),
        "native" => Some(WebSearchMode::Native),
        "exa" => Some(WebSearchMode::Exa),
        "none" => Some(WebSearchMode::None),
        _ => None,
    }
}

fn eof_signaling_lines<R>(
    reader: R, eof_tx: oneshot::Sender<()>,
) -> impl Stream<Item = std::io::Result<String>> + Send + 'static
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let lines = futures::io::BufReader::new(reader).lines();
    futures::stream::unfold((lines, Some(eof_tx)), |(mut lines, mut eof_tx)| async move {
        match lines.next().await {
            Some(line) => Some((line, (lines, eof_tx))),
            None => {
                if let Some(tx) = eof_tx.take() {
                    let _ = tx.send(());
                }
                None
            }
        }
    })
}

fn stdout_line_sink<W>(writer: W) -> impl Sink<String, Error = std::io::Error> + Send + 'static
where
    W: AsyncWrite + Send + 'static,
{
    futures::sink::unfold(Box::pin(writer), async move |mut writer, line: String| {
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
        Ok::<_, std::io::Error>(writer)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn state() -> ServerState {
        ServerState::new(ServerConfig::new(
            PathBuf::from("/tmp/workspace"),
            String::from("umans-coder"),
            String::from("auto"),
            None,
        ))
    }

    #[test]
    fn creates_opaque_session_ids() {
        let state = state();
        let workspace = tempfile::tempdir().expect("temp workspace");

        let first = state.create_session(workspace.path()).expect("first session");
        let second = state.create_session(workspace.path()).expect("second session");

        assert_eq!("acp-session-00000001", first);
        assert_eq!("acp-session-00000002", second);
        assert!(state.has_session(&first));
        assert!(state.has_session(&second));
    }

    #[test]
    fn rejects_relative_session_cwd() {
        let state = state();

        let err = state
            .create_session(Path::new("relative"))
            .expect_err("relative cwd rejected");

        assert!(err.contains("must be absolute"));
    }

    #[test]
    fn joins_text_prompt_blocks() {
        let prompt = assemble_prompt(&[
            ContentBlock::Text(TextContent::new("first")),
            ContentBlock::Text(TextContent::new("second")),
        ])
        .expect("text prompt");

        assert_eq!("first\nsecond", prompt.text);
        assert_eq!(
            prompt.provider_blocks,
            vec![
                ProviderContentBlock::Text { text: "first".to_string() },
                ProviderContentBlock::Text { text: "second".to_string() },
            ]
        );
    }

    #[test]
    fn assembles_rich_prompt_blocks() {
        let prompt = assemble_prompt(&[
            ContentBlock::Text(TextContent::new("look at this")),
            ContentBlock::Image(ImageContent::new("aGVsbG8=", "image/png").uri("file:///tmp/image.png")),
            ContentBlock::ResourceLink(
                ResourceLink::new("notes.md", "file:///tmp/notes.md").mime_type("text/markdown"),
            ),
            ContentBlock::Resource(EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(
                TextResourceContents::new("embedded notes", "file:///tmp/embedded.md").mime_type("text/markdown"),
            ))),
        ])
        .expect("rich prompt");

        assert!(prompt.text.contains("look at this"));
        assert!(prompt.text.contains("[image: image/png; uri=file:///tmp/image.png]"));
        assert!(
            prompt
                .text
                .contains("[resource link: notes.md (file:///tmp/notes.md, mime=text/markdown)]")
        );
        assert!(
            prompt
                .text
                .contains("[embedded resource: file:///tmp/embedded.md (text/markdown)]")
        );
        assert!(
            prompt
                .provider_blocks
                .iter()
                .any(|block| matches!(block, ProviderContentBlock::Image { .. }))
        );
    }

    #[test]
    fn initializes_with_supported_protocol_version() {
        let request = InitializeRequest::new(ProtocolVersion::V1);
        let response = initialize(&state(), &request);

        assert_eq!(response.protocol_version, ProtocolVersion::V1);
        assert_eq!(
            response.agent_info,
            Some(Implementation::new("thndrs", env!("CARGO_PKG_VERSION")).title(Some("thndrs".to_string())))
        );
        let expected = AgentCapabilities::new()
            .load_session(true)
            .prompt_capabilities(PromptCapabilities::new().image(true).embedded_context(true))
            .mcp_capabilities(McpCapabilities::new())
            .session_capabilities(
                SessionCapabilities::new()
                    .list(SessionListCapabilities::new())
                    .resume(SessionResumeCapabilities::new())
                    .close(SessionCloseCapabilities::new())
                    .delete(SessionDeleteCapabilities::new()),
            );
        assert_eq!(response.agent_capabilities, expected);
    }

    #[test]
    fn initializes_with_protocol_fallback_to_supported_version() {
        let request = InitializeRequest::new(ProtocolVersion::from(2u16));
        let response = initialize(&state(), &request);

        assert_eq!(response.protocol_version, ProtocolVersion::V1);
        assert!(
            response.agent_capabilities.prompt_capabilities
                == PromptCapabilities::new().image(true).embedded_context(true)
        );
    }

    #[test]
    fn records_client_filesystem_capabilities() {
        let state = state();
        let request = InitializeRequest::new(ProtocolVersion::V1).client_capabilities(
            ClientCapabilities::new().fs(FileSystemCapabilities::new().read_text_file(true).write_text_file(true)),
        );

        let _response = initialize(&state, &request);

        assert!(state.client_can_read_text_files());
        assert!(state.client_can_write_text_files());
    }

    #[test]
    fn list_sessions_reads_persisted_jsonl() {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let workspace_path = workspace.path().canonicalize().expect("canonical workspace");
        let session_dir = tempfile::tempdir().expect("temp sessions");
        let mut writer = SessionWriter::create(
            session_dir.path(),
            "persisted-1",
            &workspace_path.display().to_string(),
            "Saved Session",
            "thndrs",
            "umans-coder",
            "auto",
            env!("CARGO_PKG_VERSION"),
            None,
        )
        .expect("create persisted session");
        writer
            .append(SessionRecord::AssistantFinished {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: crate::utils::datetime::now_iso8601(),
                turn_id: "turn-1".to_string(),
                text: "saved answer".to_string(),
            })
            .expect("append assistant");
        let state = ServerState::new(ServerConfig::new(
            workspace_path.clone(),
            "umans-coder".to_string(),
            "auto".to_string(),
            Some(session_dir.path().to_path_buf()),
        ));

        let response =
            list_sessions(&state, &ListSessionsRequest::new().cwd(Some(workspace_path))).expect("list sessions");

        assert_eq!(response.sessions.len(), 1);
        assert_eq!(response.sessions[0].session_id.0.as_ref(), "persisted-1");
        assert_eq!(response.sessions[0].title.as_deref(), Some("Saved Session"));
    }

    #[test]
    fn load_session_replays_records_in_file_order() {
        let records = [
            SessionRecord::User {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: "2026-07-05T00:00:00Z".to_string(),
                turn_id: "turn-1".to_string(),
                text: "question".to_string(),
            },
            SessionRecord::ReasoningFinished {
                schema_version: SCHEMA_VERSION,
                seq: 2,
                time: "2026-07-05T00:00:01Z".to_string(),
                turn_id: "turn-1".to_string(),
                text: "thought".to_string(),
            },
            SessionRecord::AssistantFinished {
                schema_version: SCHEMA_VERSION,
                seq: 3,
                time: "2026-07-05T00:00:02Z".to_string(),
                turn_id: "turn-1".to_string(),
                text: "answer".to_string(),
            },
        ];

        let kinds = records
            .iter()
            .filter_map(replay_record_update)
            .map(|intent| match intent {
                SessionUpdateIntent::Status(_) => "status",
                SessionUpdateIntent::ReasoningDelta(_) => "reasoning",
                SessionUpdateIntent::AssistantDelta(_) => "assistant",
                _ => "other",
            })
            .collect::<Vec<_>>();

        assert_eq!(kinds, vec!["status", "reasoning", "assistant"]);
    }

    #[test]
    fn resume_session_attaches_without_replay_requirement() {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let workspace_path = workspace.path().canonicalize().expect("canonical workspace");
        let session_dir = tempfile::tempdir().expect("temp sessions");
        let writer = SessionWriter::create(
            session_dir.path(),
            "persisted-resume",
            &workspace_path.display().to_string(),
            "Resume Session",
            "thndrs",
            "umans-coder",
            "auto",
            env!("CARGO_PKG_VERSION"),
            None,
        )
        .expect("create persisted session");
        drop(writer);
        let state = ServerState::new(ServerConfig::new(
            workspace_path.clone(),
            "umans-coder".to_string(),
            "auto".to_string(),
            Some(session_dir.path().to_path_buf()),
        ));

        let response = resume_session(&state, &ResumeSessionRequest::new("persisted-resume", workspace_path))
            .expect("resume session");

        assert!(response.config_options.is_some());
        assert!(state.has_session("persisted-resume"));
    }

    #[test]
    fn rejects_unsupported_prompt_blocks() {
        let err = assemble_prompt(&[ContentBlock::Audio(AudioContent::new("aGVsbG8=", "audio/wav"))])
            .expect_err("unsupported block rejected");

        assert!(err.contains("unsupported ACP prompt content block"));
    }
}
