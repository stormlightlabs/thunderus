//! ACP server request and notification handlers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, Content, ContentBlock, ContentChunk, Implementation, InitializeRequest,
    InitializeResponse, McpCapabilities, NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind,
    PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome, RequestPermissionRequest,
    SessionCapabilities, SessionConfigOption, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, StopReason, TextContent, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, UsageUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Dispatch, Error, Lines, Result};
use futures::channel::oneshot;
use futures::future::{Either, select};
use futures::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt, Sink, Stream, StreamExt};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::agent::prompt_expects_workspace_write;
use crate::agent::{CancelToken, ToolPermissionDecision, ToolPermissionHook};
use crate::app::AgentEvent;
use crate::cli::WebSearchMode;
use crate::harness::HarnessHandle;
use crate::prompt;
use crate::server::ServerConfig;
use crate::server::config_options::{ConfigOptionValue, acp_config_options, validate_config_option};
use crate::server::events::{SessionUpdateIntent, classify_tool, map_agent_event, sanitize_tool_payload};
use crate::server::session::{AcpServerSession, AcpSessionStore, validate_and_normalize_cwd};
use crate::session::{AcpPermissionOptionRecord, AcpSessionMetadata, SCHEMA_VERSION, SessionRecord, SessionWriter};
use crate::tools::AgentRunConfig;
use crate::tools::ToolUseRequest;

const PERMISSION_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
                    agent_name: String::from("thndrs-acp-server"),
                    acp_session_id: session_id.clone(),
                    command: String::from("thndrs-acp-server"),
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
        }
    }

    fn config_options_for_session(&self, session_id: &str) -> Result<Vec<SessionConfigOption>, String> {
        let session = self.session(session_id)?;
        let model = session.metadata.model.unwrap_or_else(|| self.config.model.clone());
        let websearch = session
            .metadata
            .websearch
            .or_else(|| websearch_mode(&self.config.websearch))
            .unwrap_or(WebSearchMode::Auto);
        Ok(acp_config_options(&model, websearch))
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
        let model = match option {
            ConfigOptionValue::Model(model) => Some(model.clone()),
            ConfigOptionValue::WebSearch(_) => session.metadata.model,
        };
        let websearch = match option {
            ConfigOptionValue::Model(_) => session.metadata.websearch,
            ConfigOptionValue::WebSearch(mode) => Some(*mode),
        };
        inner
            .sessions
            .update_session_metadata(session_id, model, websearch)
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

/// Run the ACP server over stdio with all v1 baseline handlers registered.
pub async fn run_stdio(config: ServerConfig) -> Result<()> {
    let state = ServerState::new(config);
    let initialize_state = state.clone();
    let new_session_state = state.clone();
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
                match tokio::task::spawn_blocking(move || prompt(&state, &request, &connection)).await {
                    Ok(Ok(response)) => responder.respond(response),
                    Ok(Err(error)) => responder.respond_with_error(Error::invalid_params().data(error)),
                    Err(error) => responder
                        .respond_with_error(Error::internal_error().data(format!("ACP prompt task failed: {error}"))),
                }
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

pub(crate) fn initialize(state: &ServerState, request: &InitializeRequest) -> InitializeResponse {
    state.record_client_info(request);
    InitializeResponse::new(negotiate_protocol_version(request.protocol_version))
        .agent_info(Some(
            Implementation::new("thndrs", env!("CARGO_PKG_VERSION")).title(Some(String::from("thndrs"))),
        ))
        .agent_capabilities(capabilities(state))
}

fn new_session(state: &ServerState, request: &NewSessionRequest) -> Result<NewSessionResponse, String> {
    if !request.cwd.is_absolute() {
        return Err(format!("ACP session cwd must be absolute: {}", request.cwd.display()));
    }
    let cwd = validate_and_normalize_cwd(&request.cwd, None).map_err(|err| err.to_string())?;
    let session_id = state.create_session(&cwd)?;
    let config_options = state.config_options_for_session(&session_id)?;
    Ok(NewSessionResponse::new(session_id).config_options(config_options))
}

pub(crate) fn set_config_option(
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
                    turn_id(request.session_id.0.as_ref()),
                    connection.clone(),
                );
                let (_steering_tx, steering_rx) = mpsc::channel();
                drop(_steering_tx);
                crate::harness::HarnessTurn::provider_with_steering_and_permissions(
                    config,
                    messages,
                    expects_write,
                    steering_rx,
                    permission_hook,
                )
                .start()
            }
        },
    )
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
                    kind: permission_option_kind_label(&option.kind).to_string(),
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
    let sent = connection.send_request(permission);
    if connection
        .spawn(async move {
            let _ = tx.send(sent.block_task().await);
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
                        outcome: permission_decision_label(&decision).to_string(),
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

fn permission_decision_label(decision: &ToolPermissionDecision) -> &'static str {
    match decision {
        ToolPermissionDecision::Allow => "allowed",
        ToolPermissionDecision::Reject => "rejected",
        ToolPermissionDecision::Cancelled => "cancelled",
    }
}

fn permission_option_kind_label(kind: &PermissionOptionKind) -> &'static str {
    match kind {
        PermissionOptionKind::AllowOnce => "allow_once",
        PermissionOptionKind::AllowAlways => "allow_always",
        PermissionOptionKind::RejectOnce => "reject_once",
        PermissionOptionKind::RejectAlways => "reject_always",
        _ => "other",
    }
}

pub(crate) fn execute_prompt(
    state: &ServerState, request: &PromptRequest, mut on_update: impl FnMut(SessionUpdateIntent) -> Result<(), String>,
    run_harness: impl FnOnce(AgentRunConfig, Vec<crate::providers::ProviderMessage>, bool, String) -> HarnessHandle,
) -> Result<PromptResponse, String> {
    let session_id = request.session_id.0.to_string();
    let prompt = text_prompt(&request.prompt)?;

    if state.is_cancelled(&session_id) {
        return Ok(PromptResponse::new(StopReason::Cancelled));
    }

    let turn_guard = state.begin_turn(&session_id)?;
    persist_user_prompt(state, &session_id, &prompt);
    let response = run_prompt_turn(&prompt, state, &session_id, &mut on_update, run_harness)?;

    drop(turn_guard);

    Ok(response)
}

/// Run one harness-backed ACP prompt turn and stream updates while pending.
fn run_prompt_turn(
    prompt: &str, state: &ServerState, session_id: &str,
    on_update: &mut impl FnMut(SessionUpdateIntent) -> Result<(), String>,
    run_harness: impl FnOnce(AgentRunConfig, Vec<crate::providers::ProviderMessage>, bool, String) -> HarnessHandle,
) -> Result<PromptResponse, String> {
    let session = state.session(session_id)?;
    let websearch = session
        .metadata
        .websearch
        .or_else(|| websearch_mode(&state.config.websearch))
        .unwrap_or(WebSearchMode::Auto);
    let model = session.metadata.model.unwrap_or_else(|| state.config.model.clone());

    let config = AgentRunConfig::new(session.cwd, model, websearch);
    let bundle = prompt::PromptBundle::new(&config.root, &config.model, config.search_mode, &[], &[], prompt);
    let messages = crate::prompt::lower_to_umans_messages(&bundle);
    let expects_write = prompt_expects_workspace_write(prompt);
    let handle = run_harness(config, messages, expects_write, prompt.to_string());
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
    let mut persisted = PersistedTurn::new(turn_id(session_id));
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

fn persist_user_prompt(state: &ServerState, session_id: &str, prompt: &str) {
    state.append_record(
        session_id,
        SessionRecord::User {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: crate::utils::datetime::now_iso8601(),
            turn_id: turn_id(session_id),
            text: prompt.to_string(),
        },
    );
}

fn turn_id(session_id: &str) -> String {
    format!("{session_id}-turn")
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

fn capabilities(_state: &ServerState) -> AgentCapabilities {
    AgentCapabilities::new()
        .load_session(false)
        .prompt_capabilities(PromptCapabilities::new())
        .mcp_capabilities(McpCapabilities::new())
        .session_capabilities(SessionCapabilities::new())
}

fn negotiate_protocol_version(requested: ProtocolVersion) -> ProtocolVersion {
    let _ = requested;
    ProtocolVersion::V1
}

fn text_prompt(blocks: &[ContentBlock]) -> Result<String, String> {
    let mut text = String::new();
    for block in blocks {
        match block {
            ContentBlock::Text(content) => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&content.text);
            }
            other => {
                return Err(format!("unsupported ACP prompt content block: {other:?}"));
            }
        }
    }
    Ok(text)
}

fn send_update(
    connection: &ConnectionTo<Client>, session_id: agent_client_protocol::schema::v1::SessionId,
    intent: SessionUpdateIntent,
) -> Result<(), String> {
    let Some(update) = lower_update_intent(intent) else {
        return Ok(());
    };
    connection
        .send_notification(SessionNotification::new(session_id, update))
        .map_err(|err| format!("failed to send ACP session update: {err}"))
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
                .content(content_from_text_if_any(&output_text));
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

fn tool_call_status(intent: crate::server::events::ToolStatusIntent) -> ToolCallStatus {
    match intent {
        crate::server::events::ToolStatusIntent::InProgress => ToolCallStatus::InProgress,
        crate::server::events::ToolStatusIntent::Completed => ToolCallStatus::Completed,
        crate::server::events::ToolStatusIntent::Failed => ToolCallStatus::Failed,
    }
}

fn json_text_or_value(raw: String) -> serde_json::Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::Value::String(raw))
}

fn content_from_text_if_any(text: &str) -> Option<Vec<ToolCallContent>> {
    if text.is_empty() {
        return None;
    }

    Some(vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
        TextContent::new(text.to_string()),
    )))])
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
        let prompt = text_prompt(&[
            ContentBlock::Text(TextContent::new("first")),
            ContentBlock::Text(TextContent::new("second")),
        ])
        .expect("text prompt");

        assert_eq!("first\nsecond", prompt);
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
            .load_session(false)
            .prompt_capabilities(PromptCapabilities::new())
            .mcp_capabilities(McpCapabilities::new())
            .session_capabilities(SessionCapabilities::new());
        assert_eq!(response.agent_capabilities, expected);
    }

    #[test]
    fn initializes_with_protocol_fallback_to_supported_version() {
        let request = InitializeRequest::new(ProtocolVersion::from(2u16));
        let response = initialize(&state(), &request);

        assert_eq!(response.protocol_version, ProtocolVersion::V1);
        assert!(response.agent_capabilities.prompt_capabilities == PromptCapabilities::new());
    }

    #[test]
    fn rejects_unsupported_prompt_blocks() {
        let err = text_prompt(&[ContentBlock::ResourceLink(
            agent_client_protocol::schema::v1::ResourceLink::new("file:///tmp/a", "text/plain"),
        )])
        .expect_err("unsupported block rejected");

        assert!(err.contains("unsupported ACP prompt content block"));
    }
}
