//! Minimal UI-agnostic harness API for starting an agent turn.
//!
//! The harness starts a reusable agent run and exposes an event receiver
//! plus a cooperative cancellation handle.

use std::sync::mpsc::Receiver;

use crate::agent::{RunHandle, ToolExecutionHook, ToolPermissionHook};
use crate::app::AgentEvent;
use crate::providers::ProviderMessage;
use crate::tools::AgentRunConfig;

use thndrs_agent::run::AgentRunError;
use thndrs_agent::{AgentRun, CancelToken};

/// Handle returned when a harness turn has started.
#[derive(Debug)]
pub struct HarnessHandle {
    /// Stream of semantic agent events.
    pub events: AgentRun<AgentEvent>,
    /// Token that can be cancelled to stop the turn cooperatively.
    pub cancel: CancelToken,
}

impl HarnessHandle {
    /// Request cancellation for this turn.
    pub fn request_cancel(&self) {
        self.cancel.cancel();
    }

    /// Wait for the worker to settle and report a worker panic.
    pub fn wait(&mut self) -> Result<(), AgentRunError> {
        self.events.wait()
    }

    /// Cancel event delivery and wait for the worker to settle.
    pub fn cancel_and_wait(&mut self) -> Result<(), AgentRunError> {
        self.events.cancel_and_wait()
    }

    #[cfg(test)]
    pub(crate) fn from_test_receiver(events: Receiver<AgentEvent>, cancel: CancelToken) -> Self {
        let mut initial = Vec::new();
        let disconnected = match events.try_recv() {
            Ok(event) => {
                initial.push(event);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => true,
        };
        initial.extend(events.try_iter());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let mut run = AgentRun::spawn(cancel.clone(), move |sender, run_cancel| {
            for event in initial {
                let terminal = matches!(
                    event,
                    AgentEvent::Finished | AgentEvent::Cancelled | AgentEvent::Failed(_)
                );
                if sender.send(event).is_err() {
                    let _ = ready_tx.send(());
                    return;
                }
                if terminal {
                    let _ = ready_tx.send(());
                    return;
                }
            }
            let _ = ready_tx.send(());
            if disconnected {
                return;
            }
            loop {
                match events.recv_timeout(std::time::Duration::from_millis(10)) {
                    Ok(event) => {
                        let terminal = matches!(
                            event,
                            AgentEvent::Finished | AgentEvent::Cancelled | AgentEvent::Failed(_)
                        );
                        if sender.send(event).is_err() {
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) if run_cancel.is_cancelled() => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        ready_rx.recv().expect("test event bridge should start");
        if disconnected {
            run.wait().expect("test event bridge should settle");
        }
        Self { events: run, cancel }
    }
}

/// A single UI-independent harness turn description built from a
/// pre-constructed agent runtime handle.
#[derive(Debug)]
pub struct HarnessTurn {
    handle: RunHandle,
}

impl HarnessTurn {
    pub fn new(handle: RunHandle) -> Self {
        Self { handle }
    }

    /// Build a provider-backed harness turn with optional steering input.
    pub fn provider_with_steering(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
    ) -> Self {
        Self::new(RunHandle::provider_with_steering(
            config,
            messages,
            expects_write,
            steering,
        ))
    }

    /// Build a provider-backed harness turn with optional steering and tool permission review.
    pub fn provider_with_steering_and_permissions(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
        permission_hook: ToolPermissionHook,
    ) -> Self {
        Self::new(
            RunHandle::provider_with_steering(config, messages, expects_write, steering)
                .with_permission_hook(permission_hook),
        )
    }

    /// Build a provider-backed harness turn with permission review and custom tool execution.
    pub fn provider_with_steering_permissions_and_execution(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
        permission_hook: ToolPermissionHook, execution_hook: ToolExecutionHook,
    ) -> Self {
        Self::new(
            RunHandle::provider_with_steering(config, messages, expects_write, steering)
                .with_permission_hook(permission_hook)
                .with_execution_hook(execution_hook),
        )
    }

    /// Create and start the turn, returning the event stream and cancel handle.
    pub fn start(self) -> HarnessHandle {
        let cancel = self.handle.cancel.clone();
        let events = self.handle.spawn();
        HarnessHandle { events, cancel }
    }

    /// Create a deterministic fake-provider harness turn.
    #[cfg(test)]
    pub fn fake(config: AgentRunConfig, prompt: String) -> Self {
        Self::new(RunHandle::fake(config, prompt))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::cli::WebSearchMode;
    use crate::tools::AgentRunConfig;

    #[test]
    fn fake_turn_starts_and_finishes_without_app() {
        let config = AgentRunConfig::new(
            PathBuf::from("."),
            String::from("fake-agent"),
            WebSearchMode::DuckDuckGo,
        );
        let handle = HarnessTurn::fake(config, String::new()).start();
        let mut events = Vec::new();

        while let Ok(event) = handle.events.recv() {
            events.push(event);
        }

        assert_eq!(events.first(), Some(&AgentEvent::Started));
        assert_eq!(events.last(), Some(&AgentEvent::Finished));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::ReasoningDelta(_)))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::AssistantDelta(_)))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolStarted { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolFinished { .. }))
        );
    }
}
