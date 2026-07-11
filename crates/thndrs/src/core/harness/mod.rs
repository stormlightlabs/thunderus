//! Minimal UI-agnostic harness API for starting an agent turn.
//!
//! The harness starts a reusable agent run and exposes an event receiver plus a
//! cooperative cancellation handle. It intentionally owns no renderer or
//! UI-specific dependencies.

use std::sync::mpsc::Receiver;

use crate::agent::{RunHandle, ToolExecutionHook, ToolPermissionHook};
use crate::app::AgentEvent;
use crate::providers::ProviderMessage;
use crate::tools::AgentRunConfig;
use thndrs_agent::CancelToken;

/// Handle returned when a harness turn has started.
#[derive(Debug)]
pub struct HarnessHandle {
    /// Stream of semantic agent events.
    pub events: Receiver<AgentEvent>,
    /// Token that can be cancelled to stop the turn cooperatively.
    pub cancel: CancelToken,
}

/// A single UI-independent harness turn description.
#[derive(Debug)]
pub struct HarnessTurn {
    handle: RunHandle,
}

impl HarnessTurn {
    /// Build a harness turn from a pre-constructed agent runtime handle.
    pub fn from_run_handle(handle: RunHandle) -> Self {
        Self { handle }
    }

    /// Build a provider-backed harness turn with optional steering input.
    pub fn provider_with_steering(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
    ) -> Self {
        Self::from_run_handle(RunHandle::provider_with_steering(
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
        Self::from_run_handle(
            RunHandle::provider_with_steering(config, messages, expects_write, steering)
                .with_permission_hook(permission_hook),
        )
    }

    /// Build a provider-backed harness turn with permission review and custom tool execution.
    pub fn provider_with_steering_permissions_and_execution(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
        permission_hook: ToolPermissionHook, execution_hook: ToolExecutionHook,
    ) -> Self {
        Self::from_run_handle(
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
        Self::from_run_handle(RunHandle::fake(config, prompt))
    }
}

impl HarnessHandle {
    /// Request cancellation for this turn.
    pub fn request_cancel(&self) {
        self.cancel.cancel();
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
        let config = AgentRunConfig::new(PathBuf::from("."), String::from("fake-agent"), WebSearchMode::Native);
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
