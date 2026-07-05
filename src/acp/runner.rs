//! ACP run lifecycle.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::*;
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, JsonRpcRequest, LineDirection};
use futures::channel::oneshot;
use futures::future::{Either, FutureExt, select};

use crate::acp::{config, events, fs, permissions, terminal};
use crate::agent::CancelToken;
use crate::app::{AgentEvent, ToolStatus};
use crate::config::AcpAgentConfig;
use crate::session::AcpSessionMetadata;

const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatchSignal {
    Cancelled,
    TimedOut,
}

/// Handle for a client-side ACP run.
#[derive(Clone, Debug)]
pub struct RunHandle {
    pub root: PathBuf,
    pub name: String,
    pub agent: Option<AcpAgentConfig>,
    pub prompt: String,
    pub cancel: CancelToken,
}

impl RunHandle {
    /// Build an ACP run handle for `acp:<name>`.
    pub fn new(root: PathBuf, name: String, agent: Option<AcpAgentConfig>, prompt: String) -> Self {
        Self { root, name, agent, prompt, cancel: CancelToken::new() }
    }
}

/// ACP session listed by an external agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListedSession {
    /// Opaque external ACP session id.
    pub session_id: String,
    /// Workspace root reported by the ACP agent.
    pub cwd: PathBuf,
    /// Additional workspace roots reported by the ACP agent.
    pub additional_directories: Vec<PathBuf>,
    /// Human-readable title reported by the ACP agent.
    pub title: Option<String>,
    /// Last activity timestamp reported by the ACP agent.
    pub updated_at: Option<String>,
}

struct SessionRunContext<'a> {
    name: &'a str,
    command: &'a str,
    root: &'a Path,
    prompt: &'a str,
    cancel: &'a CancelToken,
    timeout_secs: u64,
    tx: &'a Sender<AgentEvent>,
}

/// Spawn an ACP run and return the normal agent event receiver.
pub fn spawn_run(handle: RunHandle) -> Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>();
    thread::spawn(move || run(&handle, &tx));
    rx
}

/// Log out of an ACP agent when the agent advertises logout support.
pub fn logout(name: &str, agent_config: AcpAgentConfig) -> Result<Vec<String>, String> {
    if !agent_config.enabled {
        return Err(format!("ACP agent `{name}` is disabled"));
    }
    futures::executor::block_on(logout_async(name.to_string(), agent_config))
}

/// List agent-owned ACP sessions when the agent advertises list support.
pub fn list_sessions(name: &str, agent_config: AcpAgentConfig, root: PathBuf) -> Result<Vec<ListedSession>, String> {
    validate_admin_agent(name, &agent_config)?;
    futures::executor::block_on(list_sessions_async(name.to_string(), agent_config, root))
}

/// Load an agent-owned ACP session and return replayed session update events.
pub fn load_session(
    name: &str, agent_config: AcpAgentConfig, root: PathBuf, session_id: String,
) -> Result<Vec<AgentEvent>, String> {
    validate_admin_agent(name, &agent_config)?;
    futures::executor::block_on(load_session_async(name.to_string(), agent_config, root, session_id))
}

/// Resume an agent-owned ACP session without replaying its history.
pub fn resume_session(
    name: &str, agent_config: AcpAgentConfig, root: PathBuf, session_id: String,
) -> Result<AcpSessionMetadata, String> {
    validate_admin_agent(name, &agent_config)?;
    futures::executor::block_on(resume_session_async(name.to_string(), agent_config, root, session_id))
}

/// Close an agent-owned ACP session when the agent advertises close support.
pub fn close_session(name: &str, agent_config: AcpAgentConfig, session_id: String) -> Result<Vec<String>, String> {
    validate_admin_agent(name, &agent_config)?;
    futures::executor::block_on(close_session_async(name.to_string(), agent_config, session_id))
}

async fn logout_async(name: String, agent_config: AcpAgentConfig) -> Result<Vec<String>, String> {
    let agent = build_agent(&agent_config)?;
    let timeout = Duration::from_secs(agent_config.timeout_secs.max(1));
    let cancel = CancelToken::new();
    let error_name = name.clone();

    Client
        .builder()
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let initialize = request_with_timeout(
                &connection,
                InitializeRequest::new(ProtocolVersion::V1),
                timeout,
                &name,
                "initialize",
                &cancel,
            )
            .await
            .map_err(timed_request_to_error)?;
            if initialize.protocol_version != ProtocolVersion::V1 {
                return Err(agent_client_protocol::util::internal_error(format!(
                    "ACP agent `{name}` selected unsupported protocol version {:?}",
                    initialize.protocol_version
                )));
            }
            if initialize.agent_capabilities.auth.logout.is_none() {
                return Ok(vec![format!("acp: `{name}` does not advertise logout support")]);
            }
            request_with_timeout(&connection, LogoutRequest::new(), timeout, &name, "logout", &cancel)
                .await
                .map_err(timed_request_to_error)?;
            Ok(vec![format!("acp: logged out `{name}`")])
        })
        .await
        .map_err(|err| format!("ACP agent `{error_name}` logout failed: {err}"))
}

async fn list_sessions_async(
    name: String, agent_config: AcpAgentConfig, root: PathBuf,
) -> Result<Vec<ListedSession>, String> {
    let agent = build_agent(&agent_config)?;
    let timeout = Duration::from_secs(agent_config.timeout_secs.max(1));
    let cancel = CancelToken::new();
    let error_name = name.clone();

    Client
        .builder()
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let initialize = initialize_for_admin(&connection, &name, timeout, &cancel).await?;
            if initialize.agent_capabilities.session_capabilities.list.is_none() {
                return Err(agent_client_protocol::util::internal_error(format!(
                    "ACP agent `{name}` does not advertise session/list support"
                )));
            }
            let mut sessions = Vec::new();
            let mut cursor = None;
            loop {
                let response = request_with_timeout(
                    &connection,
                    ListSessionsRequest::new()
                        .cwd(Some(root.clone()))
                        .cursor(cursor.clone()),
                    timeout,
                    &name,
                    "session list",
                    &cancel,
                )
                .await
                .map_err(timed_request_to_error)?;
                sessions.extend(response.sessions.into_iter().map(listed_session));
                let Some(next_cursor) = response.next_cursor else {
                    break;
                };
                cursor = Some(next_cursor);
            }
            Ok(sessions)
        })
        .await
        .map_err(|err| format!("ACP agent `{error_name}` session list failed: {err}"))
}

async fn load_session_async(
    name: String, agent_config: AcpAgentConfig, root: PathBuf, session_id: String,
) -> Result<Vec<AgentEvent>, String> {
    let agent = build_agent(&agent_config)?;
    let timeout = Duration::from_secs(agent_config.timeout_secs.max(1));
    let cancel = CancelToken::new();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let update_events = events.clone();
    let error_name = name.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                let mut guard = update_events.lock().expect("ACP session load event lock");
                guard.extend(events::map_session_update(notification.update));
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let initialize = initialize_for_admin(&connection, &name, timeout, &cancel).await?;
            if !initialize.agent_capabilities.load_session {
                return Err(agent_client_protocol::util::internal_error(format!(
                    "ACP agent `{name}` does not advertise session/load support"
                )));
            }
            request_with_timeout(
                &connection,
                LoadSessionRequest::new(SessionId::new(session_id), root),
                timeout,
                &name,
                "session load",
                &cancel,
            )
            .await
            .map_err(timed_request_to_error)?;
            Ok(events.lock().expect("ACP session load event lock").clone())
        })
        .await
        .map_err(|err| format!("ACP agent `{error_name}` session load failed: {err}"))
}

async fn resume_session_async(
    name: String, agent_config: AcpAgentConfig, root: PathBuf, session_id: String,
) -> Result<AcpSessionMetadata, String> {
    let agent = build_agent(&agent_config)?;
    let timeout = Duration::from_secs(agent_config.timeout_secs.max(1));
    let cancel = CancelToken::new();
    let command = config::redacted_command_display(&agent_config);
    let error_name = name.clone();

    Client
        .builder()
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let initialize = initialize_for_admin(&connection, &name, timeout, &cancel).await?;
            if initialize.agent_capabilities.session_capabilities.resume.is_none() {
                return Err(agent_client_protocol::util::internal_error(format!(
                    "ACP agent `{name}` does not advertise session/resume support"
                )));
            }
            request_with_timeout(
                &connection,
                ResumeSessionRequest::new(SessionId::new(session_id.clone()), root),
                timeout,
                &name,
                "session resume",
                &cancel,
            )
            .await
            .map_err(timed_request_to_error)?;
            Ok(AcpSessionMetadata {
                agent_name: name,
                acp_session_id: session_id,
                command,
                protocol_version: format!("{:?}", initialize.protocol_version),
                agent_info_name: initialize.agent_info.as_ref().map(|info| info.name.clone()),
                agent_info_version: initialize.agent_info.as_ref().map(|info| info.version.clone()),
            })
        })
        .await
        .map_err(|err| format!("ACP agent `{error_name}` session resume failed: {err}"))
}

async fn close_session_async(
    name: String, agent_config: AcpAgentConfig, session_id: String,
) -> Result<Vec<String>, String> {
    let agent = build_agent(&agent_config)?;
    let timeout = Duration::from_secs(agent_config.timeout_secs.max(1));
    let cancel = CancelToken::new();
    let error_name = name.clone();

    Client
        .builder()
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let initialize = initialize_for_admin(&connection, &name, timeout, &cancel).await?;
            if initialize.agent_capabilities.session_capabilities.close.is_none() {
                return Err(agent_client_protocol::util::internal_error(format!(
                    "ACP agent `{name}` does not advertise session/close support"
                )));
            }
            request_with_timeout(
                &connection,
                CloseSessionRequest::new(SessionId::new(session_id.clone())),
                timeout,
                &name,
                "session close",
                &cancel,
            )
            .await
            .map_err(timed_request_to_error)?;
            Ok(vec![format!("acp: closed `{name}` session {session_id}")])
        })
        .await
        .map_err(|err| format!("ACP agent `{error_name}` session close failed: {err}"))
}

fn run(handle: &RunHandle, tx: &Sender<AgentEvent>) {
    if send(tx, &handle.cancel, AgentEvent::Started).is_none() {
        return;
    }

    let Some(agent_config) = handle.agent.clone() else {
        let _ = send(
            tx,
            &handle.cancel,
            AgentEvent::Failed(format!("ACP agent `{}` is not configured", handle.name)),
        );
        return;
    };

    if !agent_config.enabled {
        let _ = send(
            tx,
            &handle.cancel,
            AgentEvent::Failed(format!("ACP agent `{}` is disabled", handle.name)),
        );
        return;
    }

    if send(
        tx,
        &handle.cancel,
        AgentEvent::Status(format!(
            "acp: starting `{}` with {}",
            handle.name,
            config::redacted_command_display(&agent_config)
        )),
    )
    .is_none()
    {
        return;
    }

    let result = futures::executor::block_on(run_async(handle.clone(), agent_config, tx.clone()));
    if let Err(message) = result {
        let _ = send(tx, &handle.cancel, AgentEvent::Failed(message));
    }
}

async fn run_async(handle: RunHandle, agent_config: AcpAgentConfig, tx: Sender<AgentEvent>) -> Result<(), String> {
    let agent = build_agent(&agent_config)?.with_debug({
        let tx = tx.clone();
        move |line, direction| {
            if direction == LineDirection::Stderr {
                let _ = tx.send(AgentEvent::Status(format!("acp stderr: {line}")));
            }
        }
    });

    let update_tx = tx.clone();
    let permission_tx = tx.clone();
    let read_tx = tx.clone();
    let write_tx = tx.clone();
    let root = handle.root.clone();
    let read_root = handle.root.clone();
    let write_root = handle.root.clone();
    let terminal_root = handle.root.clone();
    let prompt = handle.prompt.clone();
    let cancel = handle.cancel.clone();
    let permission_cancel = handle.cancel.clone();
    let name = handle.name.clone();
    let timeout_secs = agent_config.timeout_secs;
    let terminal_registry = Arc::new(terminal::TerminalRegistry::new());

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                for event in events::map_session_update(notification.update) {
                    let _ = update_tx.send(event);
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                let (decision_tx, decision_rx) = mpsc::channel();
                let pending = permissions::PendingPermission::from_request(&request, decision_tx);
                let request_id = pending.tool_call_id.clone();
                let _ = permission_tx.send(AgentEvent::PermissionRequest(pending));
                let outcome = loop {
                    if permission_cancel.is_cancelled() {
                        break RequestPermissionOutcome::Cancelled;
                    }
                    match decision_rx.recv_timeout(CANCEL_POLL_INTERVAL) {
                        Ok(permissions::PermissionDecision::Selected(option_id)) => {
                            break RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id));
                        }
                        Ok(permissions::PermissionDecision::Cancelled) | Err(RecvTimeoutError::Disconnected) => {
                            break RequestPermissionOutcome::Cancelled;
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                };
                let _ = permission_tx.send(AgentEvent::PermissionResolved {
                    tool_call_id: request_id,
                    outcome: permission_outcome_label(&outcome).to_string(),
                });
                responder.respond(RequestPermissionResponse::new(outcome))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: ReadTextFileRequest, responder, _cx| {
                let id = format!("acp-read-{}", request.path.display());
                let args = serde_json::json!({
                    "path": request.path.display().to_string(),
                    "line": request.line,
                    "limit": request.limit,
                })
                .to_string();
                let _ = read_tx.send(AgentEvent::ToolStarted {
                    id: id.clone(),
                    name: "acp.fs.read_text_file".to_string(),
                    arguments: args,
                });
                match fs::read_text_file(&request.path, &read_root, request.line, request.limit) {
                    Ok(result) => {
                        let _ = read_tx.send(AgentEvent::ToolFinished {
                            id,
                            output: result.output,
                            status: ToolStatus::Ok,
                            write_result: None,
                            shell_result: None,
                        });
                        responder.respond(ReadTextFileResponse::new(result.content))
                    }
                    Err(message) => {
                        let (output, status) = fs::failed_output(&message);
                        let _ = read_tx.send(AgentEvent::ToolFinished {
                            id,
                            output,
                            status,
                            write_result: None,
                            shell_result: None,
                        });
                        responder.respond_with_error(agent_client_protocol::util::internal_error(message))
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: WriteTextFileRequest, responder, _cx| {
                let id = format!("acp-write-{}", request.path.display());
                let args = serde_json::json!({
                    "path": request.path.display().to_string(),
                    "content_bytes": request.content.len(),
                })
                .to_string();
                let _ = write_tx.send(AgentEvent::ToolStarted {
                    id: id.clone(),
                    name: "acp.fs.write_text_file".to_string(),
                    arguments: args,
                });
                match fs::write_text_file(&request.path, &write_root, &request.content) {
                    Ok(result) => {
                        let _ = write_tx.send(AgentEvent::ToolFinished {
                            id,
                            output: result.output,
                            status: ToolStatus::Ok,
                            write_result: Some(result.write_result),
                            shell_result: None,
                        });
                        responder.respond(WriteTextFileResponse::new())
                    }
                    Err(message) => {
                        let (output, status) = fs::failed_output(&message);
                        let _ = write_tx.send(AgentEvent::ToolFinished {
                            id,
                            output,
                            status,
                            write_result: None,
                            shell_result: None,
                        });
                        responder.respond_with_error(agent_client_protocol::util::internal_error(message))
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminal_registry = terminal_registry.clone();
                let terminal_tx = tx.clone();
                let terminal_root = terminal_root.clone();
                async move |request: CreateTerminalRequest, responder, _cx| match terminal_registry
                    .create(&request, &terminal_root)
                {
                    Ok((response, event)) => {
                        let _ = terminal_tx.send(event);
                        responder.respond(response)
                    }
                    Err(message) => responder.respond_with_error(agent_client_protocol::util::internal_error(message)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminal_registry = terminal_registry.clone();
                let terminal_tx = tx.clone();
                async move |request: TerminalOutputRequest, responder, _cx| match terminal_registry.output(&request) {
                    Ok((response, event)) => {
                        let _ = terminal_tx.send(event);
                        responder.respond(response)
                    }
                    Err(message) => responder.respond_with_error(agent_client_protocol::util::internal_error(message)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminal_registry = terminal_registry.clone();
                let terminal_tx = tx.clone();
                async move |request: WaitForTerminalExitRequest, responder, _cx| match terminal_registry
                    .wait_for_exit(&request)
                {
                    Ok((response, event)) => {
                        let _ = terminal_tx.send(event);
                        responder.respond(response)
                    }
                    Err(message) => responder.respond_with_error(agent_client_protocol::util::internal_error(message)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminal_registry = terminal_registry.clone();
                let terminal_tx = tx.clone();
                async move |request: KillTerminalRequest, responder, _cx| match terminal_registry.kill(&request) {
                    Ok((response, event)) => {
                        let _ = terminal_tx.send(event);
                        responder.respond(response)
                    }
                    Err(message) => responder.respond_with_error(agent_client_protocol::util::internal_error(message)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let terminal_registry = terminal_registry.clone();
                let terminal_tx = tx.clone();
                async move |request: ReleaseTerminalRequest, responder, _cx| match terminal_registry.release(&request) {
                    Ok((response, event)) => {
                        let _ = terminal_tx.send(event);
                        responder.respond(response)
                    }
                    Err(message) => responder.respond_with_error(agent_client_protocol::util::internal_error(message)),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            let command = config::redacted_command_display(&agent_config);
            let context = SessionRunContext {
                name: &name,
                command: &command,
                root: &root,
                prompt: &prompt,
                cancel: &cancel,
                timeout_secs,
                tx: &tx,
            };
            run_session(connection, context).await
        })
        .await
        .map_err(|err| format!("ACP agent `{}` failed: {err}", handle.name))
}

fn permission_outcome_label(outcome: &RequestPermissionOutcome) -> &'static str {
    match outcome {
        RequestPermissionOutcome::Cancelled => "cancelled",
        RequestPermissionOutcome::Selected(_) => "selected",
        _ => "unknown",
    }
}

async fn authenticate_if_advertised(
    connection: &ConnectionTo<Agent>, name: &str, timeout: Duration, cancel: &CancelToken, tx: &Sender<AgentEvent>,
    methods: &[AuthMethod],
) -> Result<(), agent_client_protocol::Error> {
    let Some(method) = methods.first() else {
        return Ok(());
    };
    let method_id = method.id().clone();
    let _ = tx.send(AgentEvent::Status(format!(
        "acp: authenticating `{name}` with {}",
        method.name()
    )));
    match request_with_timeout(
        connection,
        AuthenticateRequest::new(method_id.clone()),
        timeout,
        name,
        "authentication",
        cancel,
    )
    .await
    {
        Ok(_) => {
            let _ = tx.send(AgentEvent::Status(format!(
                "acp: authentication succeeded for `{name}` via {}",
                method_id
            )));
            Ok(())
        }
        Err(TimedRequestError::Cancelled) => {
            let _ = tx.send(AgentEvent::Cancelled);
            Ok(())
        }
        Err(TimedRequestError::Timeout(message)) => Err(agent_client_protocol::util::internal_error(message)),
        Err(TimedRequestError::Protocol(error)) => {
            let _ = tx.send(AgentEvent::Failed(format!(
                "ACP agent `{name}` authentication failed for method {method_id}"
            )));
            Err(error)
        }
    }
}

async fn initialize_for_admin(
    connection: &ConnectionTo<Agent>, name: &str, timeout: Duration, cancel: &CancelToken,
) -> Result<agent_client_protocol::schema::v1::InitializeResponse, agent_client_protocol::Error> {
    let initialize = request_with_timeout(
        connection,
        InitializeRequest::new(ProtocolVersion::V1),
        timeout,
        name,
        "initialize",
        cancel,
    )
    .await
    .map_err(timed_request_to_error)?;
    if initialize.protocol_version != ProtocolVersion::V1 {
        return Err(agent_client_protocol::util::internal_error(format!(
            "ACP agent `{name}` selected unsupported protocol version {:?}",
            initialize.protocol_version
        )));
    }
    let (tx, _rx) = mpsc::channel();
    authenticate_if_advertised(connection, name, timeout, cancel, &tx, &initialize.auth_methods).await?;
    Ok(initialize)
}

async fn run_session(
    connection: ConnectionTo<Agent>, context: SessionRunContext<'_>,
) -> Result<(), agent_client_protocol::Error> {
    let SessionRunContext { name, command, root, prompt, cancel, timeout_secs, tx } = context;
    if cancel.is_cancelled() {
        let _ = tx.send(AgentEvent::Cancelled);
        return Ok(());
    }

    let timeout = Duration::from_secs(timeout_secs.max(1));
    let initialize_request = InitializeRequest::new(ProtocolVersion::V1).client_capabilities(
        ClientCapabilities::new()
            .fs(FileSystemCapabilities::new().read_text_file(true).write_text_file(true))
            .terminal(true),
    );
    let initialize = match request_with_timeout(&connection, initialize_request, timeout, name, "initialize", cancel)
        .await
    {
        Ok(response) => response,
        Err(TimedRequestError::Cancelled) => {
            let _ = tx.send(AgentEvent::Cancelled);
            return Ok(());
        }
        Err(TimedRequestError::Timeout(message)) => return Err(agent_client_protocol::util::internal_error(message)),
        Err(TimedRequestError::Protocol(error)) => return Err(error),
    };
    if initialize.protocol_version != ProtocolVersion::V1 {
        let _ = tx.send(AgentEvent::Failed(format!(
            "ACP agent `{name}` selected unsupported protocol version {:?}",
            initialize.protocol_version
        )));
        return Ok(());
    }
    authenticate_if_advertised(&connection, name, timeout, cancel, tx, &initialize.auth_methods).await?;
    let protocol_version = format!("{:?}", initialize.protocol_version);
    let agent_info = initialize.agent_info;
    if let Some(info) = &agent_info {
        let _ = tx.send(AgentEvent::Status(format!(
            "acp: connected to {} {}",
            info.name, info.version
        )));
    } else {
        let _ = tx.send(AgentEvent::Status(format!("acp: connected to `{name}`")));
    }

    let session = match request_with_timeout(
        &connection,
        NewSessionRequest::new(root.to_path_buf()),
        timeout,
        name,
        "session creation",
        cancel,
    )
    .await
    {
        Ok(response) => response,
        Err(TimedRequestError::Cancelled) => {
            let _ = tx.send(AgentEvent::Cancelled);
            return Ok(());
        }
        Err(TimedRequestError::Timeout(message)) => return Err(agent_client_protocol::util::internal_error(message)),
        Err(TimedRequestError::Protocol(error)) => return Err(error),
    };
    let _ = tx.send(AgentEvent::Status(format!("acp: session {}", session.session_id)));
    let _ = tx.send(AgentEvent::AcpSession(AcpSessionMetadata {
        agent_name: name.to_string(),
        acp_session_id: session.session_id.to_string(),
        command: command.to_string(),
        protocol_version,
        agent_info_name: agent_info.as_ref().map(|info| info.name.clone()),
        agent_info_version: agent_info.as_ref().map(|info| info.version.clone()),
    }));

    if cancel.is_cancelled() {
        send_session_cancel(&connection, &session.session_id, tx);
        let _ = tx.send(AgentEvent::Cancelled);
        return Ok(());
    }

    let session_id = session.session_id.clone();
    let response = match prompt_with_cancel(
        &connection,
        PromptRequest::new(session.session_id, vec![ContentBlock::Text(TextContent::new(prompt))]),
        session_id,
        timeout,
        name,
        cancel,
        tx,
    )
    .await
    {
        Ok(response) => response,
        Err(TimedRequestError::Cancelled) => {
            let _ = tx.send(AgentEvent::Cancelled);
            return Ok(());
        }
        Err(TimedRequestError::Timeout(message)) => return Err(agent_client_protocol::util::internal_error(message)),
        Err(TimedRequestError::Protocol(error)) => return Err(error),
    };
    send_stop_reason(tx, response.stop_reason);
    Ok(())
}

enum TimedRequestError {
    Protocol(agent_client_protocol::Error),
    Timeout(String),
    Cancelled,
}

impl From<TimedRequestError> for agent_client_protocol::Error {
    fn from(val: TimedRequestError) -> Self {
        match val {
            TimedRequestError::Protocol(error) => error,
            TimedRequestError::Timeout(message) => agent_client_protocol::util::internal_error(message),
            TimedRequestError::Cancelled => {
                agent_client_protocol::util::internal_error("ACP request cancelled".to_string())
            }
        }
    }
}

fn timed_request_to_error(error: TimedRequestError) -> agent_client_protocol::Error {
    error.into()
}

async fn request_with_timeout<Req>(
    connection: &ConnectionTo<Agent>, request: Req, timeout: Duration, agent_name: &str, operation: &str,
    cancel: &CancelToken,
) -> Result<Req::Response, TimedRequestError>
where
    Req: JsonRpcRequest,
{
    let request = connection.send_request(request).block_task().boxed();
    let (watchdog, done) = watch_cancel_or_timeout(cancel.clone(), timeout);
    let result = match select(request, watchdog.boxed()).await {
        Either::Left((response, _)) => response.map_err(TimedRequestError::Protocol),
        Either::Right((WatchSignal::Cancelled, _)) => Err(TimedRequestError::Cancelled),
        Either::Right((WatchSignal::TimedOut, _)) => Err(TimedRequestError::Timeout(format!(
            "ACP agent `{agent_name}` {operation} timed out after {} seconds",
            timeout.as_secs()
        ))),
    };
    done.store(true, Ordering::SeqCst);
    result
}

async fn prompt_with_cancel(
    connection: &ConnectionTo<Agent>, request: PromptRequest, session_id: SessionId, timeout: Duration,
    agent_name: &str, cancel: &CancelToken, tx: &Sender<AgentEvent>,
) -> Result<<PromptRequest as JsonRpcRequest>::Response, TimedRequestError> {
    let request = connection.send_request(request).block_task().boxed();
    let (watchdog, done) = watch_cancel_or_timeout(cancel.clone(), timeout);
    let result = match select(request, watchdog.boxed()).await {
        Either::Left((response, _)) => response.map_err(TimedRequestError::Protocol),
        Either::Right((WatchSignal::Cancelled, _)) => {
            send_session_cancel(connection, &session_id, tx);
            Err(TimedRequestError::Cancelled)
        }
        Either::Right((WatchSignal::TimedOut, _)) => {
            send_session_cancel(connection, &session_id, tx);
            Err(TimedRequestError::Timeout(format!(
                "ACP agent `{agent_name}` prompt timed out after {} seconds",
                timeout.as_secs()
            )))
        }
    };
    done.store(true, Ordering::SeqCst);
    result
}

fn send_session_cancel(connection: &ConnectionTo<Agent>, session_id: &SessionId, tx: &Sender<AgentEvent>) {
    match connection.send_notification(CancelNotification::new(session_id.clone())) {
        Ok(()) => {
            let _ = tx.send(AgentEvent::Status(format!("acp: sent session/cancel for {session_id}")));
        }
        Err(error) => {
            let _ = tx.send(AgentEvent::Status(format!(
                "acp: failed to send session/cancel for {session_id}: {error}"
            )));
        }
    }
}

async fn watch_receiver(rx: oneshot::Receiver<WatchSignal>) -> WatchSignal {
    rx.await.unwrap_or(WatchSignal::Cancelled)
}

fn watch_cancel_or_timeout(
    cancel: CancelToken, timeout: Duration,
) -> (impl futures::Future<Output = WatchSignal>, Arc<AtomicBool>) {
    let (tx, rx) = oneshot::channel();
    let done = Arc::new(AtomicBool::new(false));
    let thread_done = done.clone();
    thread::spawn(move || {
        let deadline = Instant::now() + timeout;
        loop {
            if thread_done.load(Ordering::SeqCst) {
                return;
            }
            if cancel.is_cancelled() {
                let _ = tx.send(WatchSignal::Cancelled);
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                let _ = tx.send(WatchSignal::TimedOut);
                return;
            }
            thread::sleep(CANCEL_POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
        }
    });
    (watch_receiver(rx), done)
}

fn build_agent(agent: &AcpAgentConfig) -> Result<AcpAgent, String> {
    let mut args = agent
        .env
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    args.push(agent.command.clone());
    args.extend(agent.args.iter().cloned());
    AcpAgent::from_args(args).map_err(|err| format!("failed to build ACP agent command: {err}"))
}

fn listed_session(session: SessionInfo) -> ListedSession {
    ListedSession {
        session_id: session.session_id.to_string(),
        cwd: session.cwd,
        additional_directories: session.additional_directories,
        title: session.title,
        updated_at: session.updated_at,
    }
}

fn validate_admin_agent(name: &str, agent_config: &AcpAgentConfig) -> Result<(), String> {
    if !agent_config.enabled {
        return Err(format!("ACP agent `{name}` is disabled"));
    }
    Ok(())
}

fn send_stop_reason(tx: &Sender<AgentEvent>, stop_reason: StopReason) {
    let event = match stop_reason {
        StopReason::EndTurn => AgentEvent::Finished,
        StopReason::Cancelled => AgentEvent::Cancelled,
        StopReason::MaxTokens => AgentEvent::Failed("ACP agent stopped after reaching max tokens".to_string()),
        StopReason::MaxTurnRequests => {
            AgentEvent::Failed("ACP agent stopped after reaching max turn requests".to_string())
        }
        StopReason::Refusal => AgentEvent::Failed("ACP agent refused to continue".to_string()),
        _ => AgentEvent::Failed("ACP agent stopped for an unsupported reason".to_string()),
    };
    let _ = tx.send(event);
}

fn send(tx: &Sender<AgentEvent>, cancel: &CancelToken, event: AgentEvent) -> Option<()> {
    if cancel.is_cancelled() {
        let _ = tx.send(AgentEvent::Cancelled);
        return None;
    }
    tx.send(event).ok()
}
