#!/usr/bin/env python3
"""Deterministic ACP agent used by the landing-page TUI capture."""

import json
import sys
import time

SESSION_ID = "showcase-session"


def send(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def respond(request_id, result):
    send({"jsonrpc": "2.0", "id": request_id, "result": result})


def update(update_type, **fields):
    send(
        {
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": SESSION_ID,
                "update": {"sessionUpdate": update_type, **fields},
            },
        }
    )
    time.sleep(0.08)


def message(text, message_id):
    update(
        "agent_message_chunk",
        content={"type": "text", "text": text},
        messageId=message_id,
    )


def thought(text):
    update(
        "agent_thought_chunk",
        content={"type": "text", "text": text},
        messageId="showcase-thought",
    )


def tool(tool_call_id, title, kind, raw_input, output):
    update(
        "tool_call",
        toolCallId=tool_call_id,
        title=title,
        kind=kind,
        status="completed",
        rawInput=raw_input,
        content=[
            {
                "type": "content",
                "content": {"type": "text", "text": output},
            }
        ],
    )


def run_showcase(request_id):
    thought("I’ll inspect the router and its request tests before making the change.")
    tool(
        "showcase-read",
        "Read src/http.rs",
        "read",
        {"path": "src/http.rs", "line": 1, "limit": 220},
        "Found the router and its existing request tests.",
    )
    tool(
        "showcase-edit",
        "Edit src/http.rs",
        "edit",
        {"path": "src/http.rs", "change": "add GET /health and a request test"},
        "Added the health route and a focused test.",
    )
    tool(
        "showcase-test",
        "Run cargo test health_check",
        "execute",
        {"command": ["cargo", "test", "health_check"]},
        "test health_check ... ok",
    )
    message("Added `GET /health` and a request test. Tests pass.", "showcase-result")
    update("usage_update", used=1180, size=1324)
    respond(request_id, {"stopReason": "end_turn"})


def main():
    for line in sys.stdin:
        request = json.loads(line)
        request_id = request.get("id")
        method = request.get("method")

        if method == "initialize":
            respond(
                request_id,
                {
                    "protocolVersion": 1,
                    "agentCapabilities": {},
                    "authMethods": [],
                    "agentInfo": {"name": "thndrs-showcase", "version": "1.0.0"},
                },
            )
        elif method == "session/new":
            respond(request_id, {"sessionId": SESSION_ID})
        elif method == "session/prompt":
            run_showcase(request_id)
        elif method == "session/cancel":
            respond(request_id, {})
        else:
            send(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32601, "message": f"unsupported method: {method}"},
                }
            )


if __name__ == "__main__":
    main()
