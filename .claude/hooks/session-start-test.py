#!/usr/bin/env python3
"""Check session-start.sh against stub toolchains.

    .claude/hooks/session-start-test.py

The hook warms caches and installs a renderer, and every step of it is written
to report a failure and carry on. That rule is invisible in the diff of any one
line: dropping a `||` leaves a script that still reads correctly and now fails
a session whose container happened to be offline. These cases hold it in place.

Each case runs the hook against a directory of stub `rustup`, `cargo`, `pnpm`,
and `go` commands, so nothing here downloads anything or depends on what the
runner already has. The stubs record their arguments, which is how the case for
the pinned version checks that the version the hook installs is the version it
then looks for, rather than two constants that agree until one is edited.

Stdout is asserted empty throughout: a SessionStart hook's stdout lands in the
session's context, and a package list there costs tokens on every session.
"""

from __future__ import annotations

import re
import subprocess
import tempfile
from pathlib import Path

HOOKS = Path(__file__).resolve().parent
HOOK = HOOKS / "session-start.sh"
REPOSITORY = HOOKS.parent.parent

TOOLS = ("rustup", "cargo", "pnpm", "go")

failures: list[str] = []


def write_stubs(directory: Path, status: int, freeze_version: str | None) -> Path:
    """Write a stub for every command the hook calls and return their directory."""
    binaries = directory / "bin"
    binaries.mkdir()
    for tool in TOOLS:
        stub = binaries / tool
        stub.write_text(
            f'#!/bin/sh\nprintf "%s\\n" "$*" >> "$STUB_LOG"\necho "{tool}: stub" \nexit {status}\n'
        )
        stub.chmod(0o755)
    if freeze_version is not None:
        stub = binaries / "freeze"
        stub.write_text(f'#!/bin/sh\necho "freeze version {freeze_version}"\n')
        stub.chmod(0o755)
    return binaries


def run(
    *,
    remote: bool = True,
    status: int = 1,
    freeze_version: str | None = None,
    home: bool = True,
) -> tuple[subprocess.CompletedProcess[str], str]:
    """Run the hook against fresh stubs and return it with what they recorded."""
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        binaries = write_stubs(directory, status, freeze_version)
        log = directory / "calls.txt"
        log.touch()
        environment = {
            "PATH": f"{binaries}:/usr/bin:/bin",
            "STUB_LOG": str(log),
            "CLAUDE_PROJECT_DIR": str(REPOSITORY),
        }
        if remote:
            environment["CLAUDE_CODE_REMOTE"] = "true"
        if home:
            environment["HOME"] = str(directory / "home")
        completed = subprocess.run(
            ["bash", str(HOOK)], env=environment, capture_output=True, text=True, check=False
        )
        return completed, log.read_text()


def check(case: str, condition: bool, detail: str) -> None:
    if not condition:
        failures.append(f"{case}: {detail}")


outside, _ = run(remote=False)
check("outside the cloud", outside.returncode == 0, f"exited {outside.returncode}")
check("outside the cloud", outside.stdout == "", f"wrote {outside.stdout!r} to stdout")
check("outside the cloud", outside.stderr == "", f"wrote {outside.stderr!r} to stderr")

broken, calls = run(status=1)
check("every step fails", broken.returncode == 0, f"exited {broken.returncode}")
check("every step fails", broken.stdout == "", f"wrote {broken.stdout!r} to stdout")
for message in (
    "updating the stable toolchain failed",
    "warming the cargo registry failed",
    "installing docs dependencies failed",
    "installing freeze",
    "is not on PATH (found nothing)",
):
    check("every step fails", message in broken.stderr, f"did not report {message!r}")

pin = re.search(r"freeze@(v\S+)", calls)
check("the pin reaches go install", pin is not None, f"go was never asked for freeze: {calls!r}")

if pin is not None:
    version = pin.group(1)

    matched, _ = run(status=0, freeze_version=version)
    check("freeze at the pin", matched.returncode == 0, f"exited {matched.returncode}")
    check("freeze at the pin", matched.stdout == "", f"wrote {matched.stdout!r} to stdout")
    check(
        "freeze at the pin",
        "is not on PATH" not in matched.stderr,
        f"reported the pinned version missing: {matched.stderr!r}",
    )

    stale, _ = run(status=0, freeze_version="v0.0.0-stale")
    check("freeze at another version", stale.returncode == 0, f"exited {stale.returncode}")
    check(
        "freeze at another version",
        f"freeze {version} is not on PATH (found freeze version v0.0.0-stale)" in stale.stderr,
        f"did not report the mismatch: {stale.stderr!r}",
    )

homeless, homeless_calls = run(status=0, home=False)
check("no HOME", homeless.returncode == 0, f"exited {homeless.returncode}")
check("no HOME", "HOME is unset" in homeless.stderr, f"did not report it: {homeless.stderr!r}")
check("no HOME", "freeze@" not in homeless_calls, "installed to a guessed directory anyway")

if failures:
    for failure in failures:
        print(failure)
    raise SystemExit(f"{len(failures)} case(s) failed")

print("session-start.sh reports every failure and exits zero in all of them")
