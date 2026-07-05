//! ACP server request and notification handlers.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, Implementation, InitializeRequest,
    InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionNotification,
    SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Dispatch, Error, Lines, Result};
use futures::channel::oneshot;
use futures::future::{Either, select};
use futures::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt, Sink, Stream, StreamExt};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::acp_server::events::SessionUpdateIntent;
use crate::acp_server::session::{AcpSessionStore, validate_and_normalize_cwd};
use crate::acp_server::{ServerConfig, config_options};
use crate::cli::WebSearchMode;

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
            .create_session(local_session_id, cwd, None)
            .map_err(|err| err.to_string())?;
        inner
            .sessions
            .update_session_metadata(
                &session_id,
                Some(self.config.model.clone()),
                websearch_mode(&self.config.websearch),
            )
            .map_err(|err| err.to_string())?;
        Ok(session_id)
    }

    /// Mark a known session cancelled.
    pub fn cancel_session(&self, session_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.cancelled.insert(session_id.to_string());
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
            async move |request: PromptRequest, responder, connection: ConnectionTo<Client>| match prompt(
                &prompt_state,
                request,
                &connection,
            ) {
                Ok(response) => responder.respond(response),
                Err(error) => responder.respond_with_error(Error::invalid_params().data(error)),
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

fn initialize(state: &ServerState, _request: &InitializeRequest) -> InitializeResponse {
    InitializeResponse::new(ProtocolVersion::V1)
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
    Ok(NewSessionResponse::new(session_id))
}

fn prompt(
    state: &ServerState, request: PromptRequest, connection: &ConnectionTo<Client>,
) -> Result<PromptResponse, String> {
    let session_id = request.session_id.0.to_string();
    if !state.has_session(&session_id) {
        return Err(format!("unknown ACP session id `{session_id}`"));
    }

    let prompt = text_prompt(&request.prompt)?;
    if state.is_cancelled(&session_id) {
        return Ok(PromptResponse::new(StopReason::Cancelled));
    }

    send_update(
        connection,
        request.session_id,
        SessionUpdateIntent::AssistantDelta(format!("thndrs ACP server accepted prompt: {prompt}")),
    )?;

    Ok(PromptResponse::new(StopReason::EndTurn))
}

fn capabilities(_state: &ServerState) -> AgentCapabilities {
    let _model = &_state.config.model;
    let _websearch = &_state.config.websearch;
    let _options = config_options::initial_config_option_ids();
    AgentCapabilities::new()
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
        SessionUpdateIntent::Status(text) => Some(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text)),
        ))),
        SessionUpdateIntent::Usage { .. }
        | SessionUpdateIntent::ToolStarted { .. }
        | SessionUpdateIntent::ToolFinished { .. }
        | SessionUpdateIntent::Failed(_)
        | SessionUpdateIntent::Cancelled
        | SessionUpdateIntent::Finished => None,
    }
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
    fn rejects_unsupported_prompt_blocks() {
        let err = text_prompt(&[ContentBlock::ResourceLink(
            agent_client_protocol::schema::v1::ResourceLink::new("file:///tmp/a", "text/plain"),
        )])
        .expect_err("unsupported block rejected");

        assert!(err.contains("unsupported ACP prompt content block"));
    }
}
