#!/usr/bin/env python3
"""Scriptable stdio ACP agent fixture for tests."""

import json
import sys
import time

SESSION_ID = "fake-session-1"


def send(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def response(request_id, result):
    send({"jsonrpc": "2.0", "id": request_id, "result": result})


def error(request_id, message):
    send(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32601, "message": message},
        }
    )


def update(kind, **fields):
    send(
        {
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": SESSION_ID,
                "update": {"sessionUpdate": kind, **fields},
            },
        }
    )


def text_update(text):
    update(
        "agent_message_chunk",
        content={"type": "text", "text": text},
        messageId="fake-message-1",
    )


def request(request_id, method, params):
    send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})


def read_message():
    line = sys.stdin.readline()
    if not line:
        return None
    return json.loads(line)


def read_until_response(request_id):
    while True:
        message = read_message()
        if message is None:
            return None
        if message.get("id") == request_id:
            return message


def initialize(request_id):
    response(
        request_id,
        {
            "protocolVersion": 1,
            "agentCapabilities": {},
            "authMethods": [],
            "agentInfo": {"name": "fake-acp-agent", "version": "0.0.0"},
        },
    )


def session_new(request_id, message):
    cwd = message.get("params", {}).get("cwd", ".")
    response(request_id, {"sessionId": SESSION_ID})
    return cwd


def lifecycle(request_id, _cwd):
    text_update("pong from fake ACP agent")
    response(request_id, {"stopReason": "end_turn"})


def cancel(request_id, _cwd):
    text_update("waiting")
    while True:
        message = read_message()
        if message is None:
            return
        if message.get("method") == "session/cancel":
            response(request_id, {"stopReason": "cancelled"})
            return


def permission(request_id, _cwd):
    request(
        "perm-1",
        "session/request_permission",
        {
            "sessionId": SESSION_ID,
            "toolCall": {"toolCallId": "tool-1", "title": "Write file"},
            "options": [
                {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                {"optionId": "reject", "name": "Reject", "kind": "reject_once"},
            ],
        },
    )
    read_until_response("perm-1")
    response(request_id, {"stopReason": "cancelled"})


def fs_read(request_id, cwd):
    path = f"{cwd}/readme.txt"
    request(
        "read-1",
        "fs/read_text_file",
        {"sessionId": SESSION_ID, "path": path, "line": 1, "limit": 5},
    )
    message = read_until_response("read-1")
    content = message.get("result", {}).get("content", "") if message else ""
    text_update(f"read: {content}")
    response(request_id, {"stopReason": "end_turn"})


def fs_write(request_id, cwd):
    path = f"{cwd}/acp-write.txt"
    request(
        "write-1",
        "fs/write_text_file",
        {"sessionId": SESSION_ID, "path": path, "content": "written by fake ACP\n"},
    )
    read_until_response("write-1")
    text_update("write ok")
    response(request_id, {"stopReason": "end_turn"})


def unknown_update(request_id, _cwd):
    update("available_commands_update", availableCommands=[])
    response(request_id, {"stopReason": "end_turn"})


def timeout_prompt(_request_id, _cwd):
    while True:
        time.sleep(1)


def main():
    script = sys.argv[1] if len(sys.argv) > 1 else "lifecycle"
    print("fake-agent stderr diagnostic", file=sys.stderr, flush=True)
    cwd = "."

    if script == "timeout-initialize":
        while True:
            time.sleep(1)

    for message in iter(read_message, None):
        method = message.get("method")
        request_id = message.get("id")

        if method == "initialize":
            initialize(request_id)
        elif method == "session/new":
            if script == "timeout-session":
                while True:
                    time.sleep(1)
            cwd = session_new(request_id, message)
        elif method == "session/prompt":
            scripts = {
                "lifecycle": lifecycle,
                "cancel": cancel,
                "permission": permission,
                "fs-read": fs_read,
                "fs-write": fs_write,
                "unknown-update": unknown_update,
                "timeout-prompt": timeout_prompt,
            }
            scripts.get(script, lifecycle)(request_id, cwd)
        else:
            error(request_id, f"unsupported method: {method}")


if __name__ == "__main__":
    main()
