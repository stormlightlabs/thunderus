//! ACP runner

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::acp::config;
use crate::agent::CancelToken;
use crate::app::AgentEvent;
use crate::config::AcpAgentConfig;

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
    thread::spawn(move || run(handle, &tx));
    rx
}

fn run(handle: RunHandle, tx: &Sender<AgentEvent>) {
    if send(tx, &handle.cancel, AgentEvent::Started).is_none() {
        return;
    }
    if handle.cancel.is_cancelled() {
        let _ = tx.send(AgentEvent::Cancelled);
        return;
    }

    let Some(agent) = handle.agent else {
        let _ = send(
            tx,
            &handle.cancel,
            AgentEvent::Failed(format!("ACP agent `{}` is not configured", handle.name)),
        );
        return;
    };

    if !agent.enabled {
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
            "acp: selected `{}` at {} using {} ({} prompt bytes)",
            handle.name,
            handle.root.display(),
            config::redacted_command_display(&agent),
            handle.prompt.len()
        )),
    )
    .is_none()
    {
        return;
    }

    let _ = send(
        tx,
        &handle.cancel,
        AgentEvent::Failed(format!(
            "ACP agent `{}` is routed, but the ACP connection lifecycle is scheduled for M4",
            handle.name
        )),
    );
}

fn send(tx: &Sender<AgentEvent>, cancel: &CancelToken, event: AgentEvent) -> Option<()> {
    if cancel.is_cancelled() {
        let _ = tx.send(AgentEvent::Cancelled);
        return None;
    }
    tx.send(event).ok()
}
