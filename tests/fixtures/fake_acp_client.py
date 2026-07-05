#!/usr/bin/env python3
"""Small ACP client fixture used by tests to drive `thndrs-acp-server`."""

import argparse
import json
import select
import subprocess
import sys
import time


def send(process, method, method_id, params):
    process.stdin.write(
        json.dumps({"jsonrpc": "2.0", "id": method_id, "method": method, "params": params}) + "\n"
    )
    process.stdin.flush()


def send_response(process, request_id, result):
    process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}) + "\n")
    process.stdin.flush()


def send_notification(process, method, params):
    process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
    process.stdin.flush()


def read_message(process, deadline):
    remaining = deadline - time.time()
    if remaining <= 0:
        return None
    readable, _, _ = select.select([process.stdout], [], [], remaining)
    if not readable:
        return None
    message = process.stdout.readline()
    if not message:
        return None
    return json.loads(message)


def read_until(process, wanted_id, timeout_secs):
    deadline = time.time() + timeout_secs
    while time.time() < deadline:
        message = read_message(process, deadline)
        if message is None:
            break
        if message.get("id") == wanted_id:
            return message
    raise RuntimeError(f"timed out waiting for response id={wanted_id}")


def permission_result(option_id):
    return {"outcome": {"outcome": "selected", "optionId": option_id}}


def read_prompt(process, wanted_id, timeout_secs, session_id, cancel_after_update=False):
    updates = []
    permission_requests = []
    cancelled = False
    deadline = time.time() + timeout_secs
    while time.time() < deadline:
        message = read_message(process, deadline)
        if message is None:
            break
        if message.get("method") == "session/update":
            updates.append(message)
            if cancel_after_update and not cancelled:
                send_notification(process, "session/cancel", {"sessionId": session_id})
                cancelled = True
            continue
        if message.get("method") == "session/request_permission":
            permission_requests.append(message)
            option = next(
                (
                    option["optionId"]
                    for option in message.get("params", {}).get("options", [])
                    if option.get("optionId", "").startswith("allow")
                ),
                "allow_once",
            )
            send_response(process, message["id"], permission_result(option))
            continue
        if message.get("id") == wanted_id:
            return updates, permission_requests, message
    raise RuntimeError(f"timed out waiting for completion of response id={wanted_id}")


def launch_server(args, model):
    process = subprocess.Popen(
        [args.server, "--cwd", args.cwd, "--model", model, "--websearch", "none"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None
    return process


def initialize(process, protocol_version):
    send(
        process,
        "initialize",
        "initialize",
        {
            "protocolVersion": protocol_version,
            "clientCapabilities": {},
            "clientInfo": {"name": "fake-client", "version": "0.1.0"},
        },
    )
    response = read_until(process, "initialize", 5.0)
    if "result" not in response:
        raise RuntimeError("initialize did not return a result")
    return response


def new_session(process, cwd):
    send(
        process,
        "session/new",
        "new-session",
        {"cwd": cwd, "additionalDirectories": [], "mcpServers": []},
    )
    response = read_until(process, "new-session", 5.0)
    if "result" not in response:
        raise RuntimeError("session/new did not return a result")
    session_id = response["result"]["sessionId"]
    if not session_id:
        raise RuntimeError("session/new returned empty sessionId")
    return session_id


def prompt_blocks(rich_content):
    prompt = [{"type": "text", "text": "smoke test from fake client"}]
    if rich_content:
        prompt.extend(
            [
                {"type": "image", "data": "aGVsbG8=", "mimeType": "image/png"},
                {"type": "resource_link", "name": "notes.md", "uri": "file:///tmp/notes.md"},
            ]
        )
    return prompt


def run_prompt_scenario(process, session_id, rich_content, cancel_after_update):
    send(process, "session/prompt", "prompt", {"sessionId": session_id, "prompt": prompt_blocks(rich_content)})
    updates, permissions, response = read_prompt(
        process,
        "prompt",
        10.0,
        session_id,
        cancel_after_update=cancel_after_update,
    )
    if "result" not in response:
        raise RuntimeError("prompt did not return a result")
    return updates, permissions, response


def run_malformed_scenario(process, cwd):
    send(
        process,
        "session/new",
        "malformed",
        {"cwd": "relative/path", "additionalDirectories": [], "mcpServers": []},
    )
    response = read_until(process, "malformed", 5.0)
    if "error" not in response:
        raise RuntimeError(f"malformed request unexpectedly succeeded: {response}")
    return response


def close_process(process):
    try:
        if process.stdin and not process.stdin.closed:
            process.stdin.close()
        returncode = process.wait(timeout=5)
        if returncode not in (0, 130):
            raise RuntimeError(f"server exited with {returncode}")
    finally:
        if process.stdout:
            process.stdout.close()
        if process.stderr:
            process.stderr.close()
        process.terminate()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", required=True)
    parser.add_argument("--protocol-version", type=int, default=1)
    parser.add_argument("--cwd", required=True)
    parser.add_argument("--rich-content", action="store_true")
    parser.add_argument(
        "--scenario",
        choices=["prompt", "permission", "cancel", "malformed"],
        default="prompt",
    )
    args = parser.parse_args()

    model = {
        "prompt": "fake-agent",
        "permission": "fake-agent-shell",
        "cancel": "fake-agent-slow",
        "malformed": "fake-agent",
    }[args.scenario]
    process = launch_server(args, model)

    try:
        initialize_response = initialize(process, args.protocol_version)
        protocol_version = initialize_response["result"]["protocolVersion"]

        if args.scenario == "malformed":
            malformed = run_malformed_scenario(process, args.cwd)
            summary = {
                "protocolVersion": protocol_version,
                "malformedError": "error" in malformed,
                "errorCode": malformed["error"].get("code"),
            }
        else:
            session_id = new_session(process, args.cwd)
            updates, permissions, prompt_response = run_prompt_scenario(
                process,
                session_id,
                args.rich_content,
                cancel_after_update=args.scenario == "cancel",
            )
            stop_reason = prompt_response["result"]["stopReason"]
            summary = {
                "protocolVersion": protocol_version,
                "sessionId": session_id,
                "stopReason": stop_reason,
                "updated": len(updates) > 0,
                "permissionRequests": len(permissions),
                "promptCapabilities": initialize_response.get("result", {})
                .get("agentCapabilities", {})
                .get("promptCapabilities", {}),
                "textPromptCapable": isinstance(
                    initialize_response.get("result", {})
                    .get("agentCapabilities", {})
                    .get("promptCapabilities", {}),
                    dict,
                ),
                "richContent": args.rich_content,
            }

        close_process(process)
        print(json.dumps(summary, sort_keys=True))
    finally:
        if process.poll() is None:
            process.terminate()


if __name__ == "__main__":
    try:
        main()
    except Exception as err:
        print(json.dumps({"error": str(err)}))
        sys.exit(1)
