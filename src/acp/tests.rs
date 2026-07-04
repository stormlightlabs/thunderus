use super::config::parse_model_id;
use super::runner::{RunHandle, spawn_run};
use crate::app::AgentEvent;
use crate::config::AcpAgentConfig;
use std::path::PathBuf;

fn collect(handle: RunHandle) -> Vec<AgentEvent> {
    spawn_run(handle).iter().collect()
}

#[test]
fn model_id_parser_accepts_valid_acp_names() {
    assert_eq!(parse_model_id("acp:claude"), Some("claude"));
    assert_eq!(parse_model_id("acp:zed-agent_1"), Some("zed-agent_1"));
    assert_eq!(parse_model_id("umans-coder"), None);
}

#[test]
fn model_id_parser_rejects_invalid_acp_names() {
    assert_eq!(parse_model_id("acp:"), None);
    assert_eq!(parse_model_id("acp:bad/name"), None);
    assert_eq!(parse_model_id("acp:bad name"), None);
}

#[test]
fn acp_runner_reports_missing_agent() {
    let events = collect(RunHandle::new(
        PathBuf::from("/repo"),
        "missing".to_string(),
        None,
        "hello".to_string(),
    ));

    assert_eq!(events.first(), Some(&AgentEvent::Started));
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Failed("ACP agent `missing` is not configured".to_string()))
    );
}

#[test]
fn acp_runner_reports_disabled_agent() {
    let agent = AcpAgentConfig { enabled: false, command: "agent".to_string(), ..AcpAgentConfig::default() };
    let events = collect(RunHandle::new(
        PathBuf::from("/repo"),
        "disabled".to_string(),
        Some(agent),
        "hello".to_string(),
    ));

    assert_eq!(events.first(), Some(&AgentEvent::Started));
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Failed("ACP agent `disabled` is disabled".to_string()))
    );
}
