#!/usr/bin/env python3
"""Scriptable stdio ACP agent fixture for tests."""

import json
import sys
import time

SESSION_ID = "fake-session-1"
AUTHENTICATED = False


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


def initialize(request_id, script):
    auth_methods = []
    agent_capabilities = {}
    if script.startswith("auth"):
        auth_methods = [
            {
                "id": "agent-login",
                "name": "Agent login",
                "description": "Fake agent-owned login",
            }
        ]
        agent_capabilities = {"auth": {"logout": {}}}
    response(
        request_id,
        {
            "protocolVersion": 1,
            "agentCapabilities": agent_capabilities,
            "authMethods": auth_methods,
            "agentInfo": {"name": "fake-acp-agent", "version": "0.0.0"},
        },
    )


def session_new(request_id, message):
    if script_requires_auth(sys.argv[1] if len(sys.argv) > 1 else "lifecycle"):
        global AUTHENTICATED
        if not AUTHENTICATED:
            error(request_id, "auth_required")
            return "."
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


def terminal(request_id, cwd):
    request(
        "term-create-1",
        "terminal/create",
        {
            "sessionId": SESSION_ID,
            "command": "python3",
            "args": ["-c", "print('terminal ok')"],
            "cwd": cwd,
            "outputByteLimit": 4096,
        },
    )
    message = read_until_response("term-create-1")
    terminal_id = message.get("result", {}).get("terminalId", "") if message else ""
    request(
        "term-wait-1",
        "terminal/wait_for_exit",
        {"sessionId": SESSION_ID, "terminalId": terminal_id},
    )
    read_until_response("term-wait-1")
    request(
        "term-output-1",
        "terminal/output",
        {"sessionId": SESSION_ID, "terminalId": terminal_id},
    )
    message = read_until_response("term-output-1")
    output = message.get("result", {}).get("output", "") if message else ""
    request(
        "term-release-1",
        "terminal/release",
        {"sessionId": SESSION_ID, "terminalId": terminal_id},
    )
    read_until_response("term-release-1")
    text_update(f"terminal: {output}")
    response(request_id, {"stopReason": "end_turn"})


def timeout_prompt(_request_id, _cwd):
    while True:
        time.sleep(1)


def script_requires_auth(script):
    return script in {"auth-success", "auth-failure"}


def main():
    global AUTHENTICATED
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
            initialize(request_id, script)
        elif method == "authenticate":
            if script == "auth-failure":
                error(request_id, "auth rejected")
            else:
                AUTHENTICATED = True
                response(request_id, {})
        elif method == "logout":
            AUTHENTICATED = False
            response(request_id, {})
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
                "auth-success": lifecycle,
                "auth-failure": lifecycle,
                "terminal": terminal,
                "timeout-prompt": timeout_prompt,
            }
            scripts.get(script, lifecycle)(request_id, cwd)
        else:
            error(request_id, f"unsupported method: {method}")


if __name__ == "__main__":
    main()
