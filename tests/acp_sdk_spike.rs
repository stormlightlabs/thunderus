//! ACP SDK spike tests.
//!
//! These tests intentionally exercise the official SDK against a real stdio
//! subprocess before the production ACP runner exists.

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest, SessionNotification,
    SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, LineDirection};

#[test]
fn acp_agent_runs_from_background_thread_with_block_on() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let fake_agent = temp.path().join("fake_acp_agent.py");
    std::fs::write(&fake_agent, fake_agent_script()).expect("write fake agent");

    let cwd = temp.path().to_path_buf();
    let debug_lines = Arc::new(Mutex::new(Vec::<(LineDirection, String)>::new()));
    let (update_tx, update_rx) = mpsc::channel::<String>();

    let thread_debug_lines = Arc::clone(&debug_lines);
    let handle = std::thread::spawn(move || {
        futures::executor::block_on(async move {
            let agent = AcpAgent::from_args(["python3".to_string(), fake_agent.display().to_string()])?.with_debug(
                move |line, direction| {
                    thread_debug_lines
                        .lock()
                        .expect("debug lock")
                        .push((direction, line.to_string()));
                },
            );

            Client
                .builder()
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        if let SessionUpdate::AgentMessageChunk(ContentChunk {
                            content: ContentBlock::Text(text),
                            ..
                        }) = notification.update
                        {
                            update_tx.send(text.text).expect("send update text");
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
                    let initialize = connection
                        .send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    assert_eq!(initialize.protocol_version, ProtocolVersion::V1);
                    assert!(initialize.agent_info.is_some());

                    let session = connection
                        .send_request(NewSessionRequest::new(cwd))
                        .block_task()
                        .await?;

                    let prompt = vec![ContentBlock::Text(TextContent::new("ping"))];
                    let response = connection
                        .send_request(PromptRequest::new(session.session_id, prompt))
                        .block_task()
                        .await?;
                    assert_eq!(response.stop_reason, StopReason::EndTurn);

                    Ok(())
                })
                .await
        })
    });

    handle
        .join()
        .expect("background thread should not panic")
        .expect("ACP SDK run should succeed");

    assert_eq!(
        update_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("session/update text"),
        "pong from fake ACP agent"
    );

    let debug_lines = debug_lines.lock().expect("debug lock");
    assert!(
        debug_lines
            .iter()
            .any(|(direction, line)| *direction == LineDirection::Stderr && line == "fake-agent stderr diagnostic"),
        "stderr should be captured by the debug callback"
    );
    assert!(
        debug_lines
            .iter()
            .filter(|(direction, _)| *direction == LineDirection::Stdout)
            .all(|(_, line)| line.starts_with('{')),
        "fake agent stdout should remain protocol-clean"
    );
}

fn fake_agent_script() -> &'static str {
    r#"#!/usr/bin/env python3
import json
import sys

SESSION_ID = "fake-session-1"

print("fake-agent stderr diagnostic", file=sys.stderr, flush=True)

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
