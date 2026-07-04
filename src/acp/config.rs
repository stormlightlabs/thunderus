//! ACP model-id and config helpers.

use crate::{
    agent,
    config::{AcpAgentConfig, validate_acp_agent_name},
};

/// Prefix used to route a model id to a configured ACP agent.
pub const MODEL_PREFIX: &str = "acp:";

/// Parse `acp:<name>` model ids and validate the embedded name.
pub fn parse_model_id(model: &str) -> Option<&str> {
    let name = model.strip_prefix(MODEL_PREFIX)?;
    validate_acp_agent_name(name).ok()?;
    Some(name)
}

/// Return a redacted command display for diagnostics and status rows.
pub fn redacted_command_display(agent: &AcpAgentConfig) -> String {
    let mut parts = Vec::with_capacity(agent.args.len() + 1);
    parts.push(agent.command.clone());
    parts.extend(agent.args.iter().cloned());
    parts.join(" ")
}

pub fn provider_label(model: &str) -> &'static str {
    match super::config::parse_model_id(model) {
        Some(_) => "acp",
        None => agent::ProviderKind::for_model(model).label(),
    }
}
