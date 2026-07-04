//! ACP run lifecycle.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use futures::channel::oneshot;
use futures::future::{Either, FutureExt, select};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, ReadTextFileRequest,
    ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, StopReason, TextContent, WriteTextFileRequest,
    WriteTextFileResponse,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, JsonRpcRequest, LineDirection};

use crate::acp::{config, events, fs, permissions};
use crate::agent::CancelToken;
use crate::app::{AgentEvent, ToolStatus};
use crate::config::AcpAgentConfig;

const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

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

/// Spawn an ACP run and return the normal agent event receiver.
pub fn spawn_run(handle: RunHandle) -> Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>();
    thread::spawn(move || run(&handle, &tx));
    rx
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
    let prompt = handle.prompt.clone();
    let cancel = handle.cancel.clone();
    let permission_cancel = handle.cancel.clone();
    let name = handle.name.clone();
    let timeout_secs = agent_config.timeout_secs;

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
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            run_session(connection, &name, &root, &prompt, &cancel, timeout_secs, &tx).await
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

async fn run_session(
    connection: ConnectionTo<Agent>, name: &str, root: &Path, prompt: &str, cancel: &CancelToken, timeout_secs: u64,
    tx: &Sender<AgentEvent>,
) -> Result<(), agent_client_protocol::Error> {
    if cancel.is_cancelled() {
        let _ = tx.send(AgentEvent::Cancelled);
        return Ok(());
    }

    let timeout = Duration::from_secs(timeout_secs.max(1));
    let initialize = match request_with_timeout(
        &connection,
        InitializeRequest::new(ProtocolVersion::V1),
        timeout,
        name,
        "initialize",
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
    if initialize.protocol_version != ProtocolVersion::V1 {
        let _ = tx.send(AgentEvent::Failed(format!(
            "ACP agent `{name}` selected unsupported protocol version {:?}",
            initialize.protocol_version
        )));
        return Ok(());
    }
    if !initialize.auth_methods.is_empty() {
        let _ = tx.send(AgentEvent::Failed(format!(
            "ACP agent `{name}` requires authentication, which is not implemented yet"
        )));
        return Ok(());
    }
    if let Some(info) = initialize.agent_info {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatchSignal {
    Cancelled,
    TimedOut,
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
