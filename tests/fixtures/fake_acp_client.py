#!/usr/bin/env python3
"""Small ACP client fixture used by unit tests to drive `thndrs-acp-server`."""

import argparse
import json
import os
import subprocess
import sys
import time


def send(process, method, method_id, params):
    process.stdin.write(
        json.dumps({"jsonrpc": "2.0", "id": method_id, "method": method, "params": params}) + "\n"
    )
    process.stdin.flush()


def send_notification(process, method, params):
    process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
    process.stdin.flush()


def read_message(process):
    message = process.stdout.readline()
    if not message:
        return None
    return json.loads(message)


def read_until(process, wanted_id, timeout_secs):
    deadline = time.time() + timeout_secs
    while time.time() < deadline:
        message = read_message(process)
        if message is None:
            break
        if message.get("id") == wanted_id:
            return message
    raise RuntimeError(f"timed out waiting for response id={wanted_id}")


def read_updates(process, wanted_id, timeout_secs):
    updates = []
    deadline = time.time() + timeout_secs
    while time.time() < deadline:
        message = read_message(process)
        if message is None:
            break
        if message.get("method") == "session/update":
            updates.append(message)
            continue
        if message.get("id") == wanted_id:
            return updates, message
    raise RuntimeError(f"timed out waiting for completion of response id={wanted_id}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", required=True)
    parser.add_argument("--protocol-version", type=int, default=1)
    parser.add_argument("--cwd", required=True)
    parser.add_argument("--rich-content", action="store_true")
    args = parser.parse_args()

    process = subprocess.Popen(
        [args.server, "--cwd", args.cwd],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None

    try:
        send(
            process,
            "initialize",
            "initialize",
            {
                "protocolVersion": args.protocol_version,
                "clientCapabilities": {},
                "clientInfo": {"name": "fake-client", "version": "0.1.0"},
            },
        )
        initialize = read_until(process, "initialize", 5.0)
        if "result" not in initialize:
            raise RuntimeError("initialize did not return a result")
        protocol_version = initialize["result"]["protocolVersion"]

        send(
            process,
            "session/new",
            "new-session",
            {"cwd": args.cwd, "additionalDirectories": [], "mcpServers": []},
        )
        new_session = read_until(process, "new-session", 5.0)
        session_id = new_session["result"]["sessionId"]
        if not session_id:
            raise RuntimeError("session/new returned empty sessionId")

        prompt = [{"type": "text", "text": "smoke test from fake client"}]
        if args.rich_content:
            prompt.extend(
                [
                    {"type": "image", "data": "aGVsbG8=", "mimeType": "image/png"},
                    {"type": "resource_link", "name": "notes.md", "uri": "file:///tmp/notes.md"},
                ]
            )

        send(
            process,
            "session/prompt",
            "prompt",
            {
                "sessionId": session_id,
                "prompt": prompt,
            },
        )
        updates, prompt_response = read_updates(process, "prompt", 5.0)
        if "result" not in prompt_response:
            raise RuntimeError("prompt did not return a result")
        stop_reason = prompt_response["result"]["stopReason"]

        send_notification(
            process,
            "session/cancel",
            {"sessionId": session_id},
        )
        process.stdin.close()
        returncode = process.wait(timeout=5)
        if returncode not in (0, 130):
            raise RuntimeError(f"server exited with {returncode}")

        has_text_prompt = (
            isinstance(initialize.get("result", {}).get("agentCapabilities", {}).get("promptCapabilities", {}), dict)
        )
        summary = {
            "protocolVersion": protocol_version,
            "sessionId": session_id,
            "stopReason": stop_reason,
            "updated": len(updates) > 0,
            "promptCapabilities": initialize.get("result", {}).get("agentCapabilities", {}).get("promptCapabilities", {}),
            "textPromptCapable": has_text_prompt,
            "richContent": args.rich_content,
        }
        print(json.dumps(summary, sort_keys=True))
    finally:
        if process.stdout:
            process.stdout.close()
        if process.stderr:
            process.stderr.close()
        process.terminate()


if __name__ == "__main__":
    try:
        main()
    except Exception as err:
        print(json.dumps({"error": str(err)}))
        sys.exit(1)
