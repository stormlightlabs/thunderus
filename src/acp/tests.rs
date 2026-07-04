use super::config::parse_model_id;
use super::events::map_session_update;
use super::runner::{RunHandle, spawn_run};
use crate::app::{AgentEvent, ToolStatus};
use crate::config::AcpAgentConfig;
use agent_client_protocol::schema::v1::{
    Content, ContentBlock, ContentChunk, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, SessionUpdate,
    TextContent, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, UsageUpdate,
};
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

#[test]
fn maps_assistant_and_reasoning_chunks() {
    assert_eq!(
        map_session_update(SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("hello")
        )))),
        vec![AgentEvent::AssistantDelta("hello".to_string())]
    );
    assert_eq!(
        map_session_update(SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("thinking")
        )))),
        vec![AgentEvent::ReasoningDelta("thinking".to_string())]
    );
}

#[test]
fn maps_plan_and_usage_updates() {
    let plan = Plan::new(vec![PlanEntry::new(
        "do the thing",
        PlanEntryPriority::High,
        PlanEntryStatus::InProgress,
    )]);
    assert!(matches!(
        map_session_update(SessionUpdate::Plan(plan)).as_slice(),
        [AgentEvent::ReasoningDelta(text)] if text.contains("do the thing")
    ));

    assert_eq!(
        map_session_update(SessionUpdate::UsageUpdate(UsageUpdate::new(40, 100))),
        vec![AgentEvent::Usage { input_tokens: 40, output_tokens: 60 }]
    );
}

#[test]
fn maps_tool_start_and_completion_with_redaction() {
    let started = map_session_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-1", "run command").raw_input(Some(serde_json::json!({"token": "sk-secret"}))),
    ));
    assert_eq!(
        started,
        vec![AgentEvent::ToolStarted {
            id: "tool-1".to_string(),
            name: "run command".to_string(),
            arguments: "{\"token\":\"sk-[redacted]-secret\"}".to_string(),
        }]
    );

    let completed = map_session_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .content(Some(vec![ToolCallContent::Content(Content::new(ContentBlock::Text(
                TextContent::new("done"),
            )))])),
    )));
    assert_eq!(
        completed,
        vec![AgentEvent::ToolFinished {
            id: "tool-1".to_string(),
            output: vec!["done".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        }]
    );
}

#[test]
fn maps_unknown_like_metadata_updates_to_stable_status() {
    assert_eq!(
        map_session_update(SessionUpdate::AvailableCommandsUpdate(
            agent_client_protocol::schema::v1::AvailableCommandsUpdate::new(vec![])
        )),
        vec![AgentEvent::Status("acp: available commands updated".to_string())]
    );
}

#[test]
fn acp_runner_completes_fake_agent_lifecycle() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let fake_agent = temp.path().join("fake_acp_agent.py");
    std::fs::write(&fake_agent, fake_agent_script()).expect("write fake agent");
    let agent = AcpAgentConfig {
        command: "python3".to_string(),
        args: vec![fake_agent.display().to_string()],
        ..AcpAgentConfig::default()
    };

    let events = collect(RunHandle::new(
        temp.path().to_path_buf(),
        "fake".to_string(),
        Some(agent),
        "ping".to_string(),
    ));

    assert!(events.contains(&AgentEvent::Started));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Status(text) if text.contains("fake-acp-agent")))
    );
    assert!(events.contains(&AgentEvent::AssistantDelta("pong from fake ACP agent".to_string())));
    assert_eq!(events.last(), Some(&AgentEvent::Finished));
}

fn fake_agent_script() -> &'static str {
    r#"#!/usr/bin/env python3
import json
import sys

SESSION_ID = "fake-session-1"

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    request_id = message.get("id")

    if method == "initialize":
        result = {
            "protocolVersion": 1,
            "agentCapabilities": {},
            "authMethods": [],
            "agentInfo": {
                "name": "fake-acp-agent",
                "version": "0.0.0"
            }
        }
    elif method == "session/new":
        result = {"sessionId": SESSION_ID}
    elif method == "session/prompt":
        update = {
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": SESSION_ID,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": "pong from fake ACP agent"
                    },
                    "messageId": "fake-message-1"
                }
            }
        }
        print(json.dumps(update, separators=(",", ":")), flush=True)
        result = {"stopReason": "end_turn"}
    else:
        error = {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {
                "code": -32601,
                "message": f"unsupported method: {method}"
            }
        }
        print(json.dumps(error, separators=(",", ":")), flush=True)
        continue

    response = {"jsonrpc": "2.0", "id": request_id, "result": result}
    print(json.dumps(response, separators=(",", ":")), flush=True)
"#
}
