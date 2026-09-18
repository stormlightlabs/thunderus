#!/usr/bin/env python3
"""Check check-commit-message.py against messages written to be wrong.

    .claude/scripts/check-commit-message-test.py

Each case writes one message to a temporary file and runs the tool over it, so
a failure names the message that produced it.

The cases that matter most are the ones about severity. A shape error has to
fail the hook and a length finding has to not, and the two are one `if` apart
in the tool. If that `if` ever inverts, a long body starts rejecting commits,
authors learn `--no-verify`, and the shape checks stop running at all. The
exit-code assertions below are what stands between that and the repository.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
TOOL = ROOT / "check-commit-message.py"

failures: list[str] = []


def run(message: str, *arguments: str) -> subprocess.CompletedProcess[str]:
    """Check one message held in a file."""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "COMMIT_EDITMSG"
        path.write_text(message, encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(TOOL), str(path), *arguments],
            capture_output=True,
            text=True,
            check=False,
        )


def check(label: str, passed: bool, detail: str = "") -> None:
    print(f"{'ok  ' if passed else 'FAIL'} {label}")
    if not passed:
        failures.append(label)
        if detail:
            print(f"     {detail}")


def body(lines: int) -> str:
    return "\n".join(f"Sentence {number} of the body." for number in range(lines))


TRAILERS = (
    "Co-Authored-By: Claude <noreply@anthropic.com>\n"
    "Claude-Session: https://claude.ai/code/session_0123456789"
)

# A message with nothing wrong reports nothing and exits clean.
clean = run("fix: collapse nested if in percentage check\n\nClippy fires here.\n")
check(
    "a clean message passes silently",
    clean.returncode == 0 and "all clean" in clean.stderr,
    f"exit={clean.returncode} err={clean.stderr.strip()[:90]}",
)

# Shape is not a judgement call, so it fails the hook.
for label, message in (
    ("a subject with no type is rejected", "Fix the thing\n"),
    ("a capitalised subject is rejected", "fix: Collapse the nested if\n"),
    ("a subject ending in a period is rejected", "fix: collapse the nested if.\n"),
    ("a body glued to its subject is rejected", "fix: collapse it\nWhy it changed\n"),
    (
        "a body line over 72 columns is rejected",
        "fix: collapse it\n\n" + "w" * 40 + " " + "w" * 40 + "\n",
    ),
    (
        "an unclosed fence is rejected",
        "fix: collapse it\n\n```text\nsome output\n",
    ),
):
    result = run(message)
    check(
        label,
        result.returncode == 1 and "error:" in result.stderr,
        f"exit={result.returncode} err={result.stderr.strip()[:90]}",
    )

# The rule this change exists for: length is reported and fails nothing.
long_body = run(f"fix: collapse the nested if\n\n{body(30)}\n")
check(
    "a body over the target advises and still exits 0",
    long_body.returncode == 0 and "length:" in long_body.stderr,
    f"exit={long_body.returncode} err={long_body.stderr.strip()[:90]}",
)
check(
    "the advice says plainly that it fails nothing",
    "fails nothing" in long_body.stderr,
    long_body.stderr.strip()[:120],
)
check(
    "a long body is never called an error",
    "error:" not in long_body.stderr,
    long_body.stderr.strip()[:120],
)

# A body at the target is not advice; the limit is inclusive.
at_target = run(f"fix: collapse the nested if\n\n{body(15)}\n")
check(
    "a body exactly at the target says nothing",
    at_target.returncode == 0 and "length:" not in at_target.stderr,
    at_target.stderr.strip()[:120],
)

# The trailers are the harness's, not the author's, and cannot be shortened.
with_trailers = run(f"fix: collapse the nested if\n\n{body(14)}\n\n{TRAILERS}\n")
check(
    "trailers do not count toward the body target",
    "length:" not in with_trailers.stderr,
    with_trailers.stderr.strip()[:120],
)

# Every overlong subject on this repository's trunk arrived this way: a title
# inside the limit, plus the suffix GitHub appends. Blaming the author for it
# sends them to cut a title that was already short enough.
suffixed = run("docs: sharpen review passes and add writing length targets (#23)\n")
check(
    "a suffix-induced overflow is advice, not an error",
    suffixed.returncode == 0 and "length:" in suffixed.stderr,
    f"exit={suffixed.returncode} err={suffixed.stderr.strip()[:90]}",
)
check(
    "the advice names the pull request title budget",
    "53" in suffixed.stderr and "(#NN)" in suffixed.stderr,
    suffixed.stderr.strip()[:140],
)

# A title that is genuinely too long is still the author's to fix.
overlong = run("docs: " + "w" * 60 + "\n")
check(
    "a title over the limit on its own is still rejected",
    overlong.returncode == 1 and "error:" in overlong.stderr,
    f"exit={overlong.returncode} err={overlong.stderr.strip()[:90]}",
)

# A title heading for overflow is worth saying before the merge, not after.
near = run("docs: " + "w" * 50 + "\n")
check(
    "a title past the squash budget is flagged early",
    near.returncode == 0 and "squash" in near.stderr,
    f"exit={near.returncode} err={near.stderr.strip()[:90]}",
)

# CI reports and never fails, whatever it finds.
warned = run("Fix the thing\n", "--warn")
check(
    "--warn reports a shape error without failing",
    warned.returncode == 0 and "error:" in warned.stdout,
    f"exit={warned.returncode} out={warned.stdout.strip()[:90]}",
)

# git deletes comments and the scissors block after this hook runs, so grading
# them rejects an author for a diff that never becomes part of the message.
verbose = run(
    "fix: collapse the nested if\n\nWhy it changed.\n\n"
    "# ------------------------ >8 ------------------------\n"
    "diff --git a/very/long/path/that/would/blow/the/column/limit.rs b/x.rs\n"
    + "\n".join(f"+    line {number}" for number in range(40))
    + "\n"
)
check(
    "the diff under a scissors line is not graded",
    verbose.returncode == 0 and verbose.stderr.count("length:") == 0,
    f"exit={verbose.returncode} err={verbose.stderr.strip()[:120]}",
)

# A branch of short messages still merges long, because GitHub concatenates
# them. Only the range mode can see that, so only the range mode reports it.
with tempfile.TemporaryDirectory() as directory:
    git = ["git", "-C", directory, "-c", "user.email=t@t", "-c", "user.name=t"]
    subprocess.run(git + ["init", "-q", "-b", "trunk"], check=True, capture_output=True)
    subprocess.run(git + ["commit", "-q", "--allow-empty", "-m", "chore: base"],
                   check=True, capture_output=True)
    for number in range(4):
        subprocess.run(
            git + ["commit", "-q", "--allow-empty",
                   "-m", f"fix: change number {number}\n\n{body(10)}"],
            check=True,
            capture_output=True,
        )
    ranged = subprocess.run(
        [sys.executable, str(TOOL), "--range", "trunk~4..trunk", "--warn"],
        capture_output=True,
        text=True,
        check=False,
        cwd=directory,
    )

check(
    "four short bodies project a long squash",
    "Projected squash body" in ranged.stdout and ranged.returncode == 0,
    f"exit={ranged.returncode} out={ranged.stdout.strip()[-140:]}",
)
check(
    "no single message in that branch was over the target",
    "body is" not in ranged.stdout,
    ranged.stdout.strip()[:160],
)
check(
    "the projection says where to fix it",
    "merge box" in ranged.stdout,
    ranged.stdout.strip()[-140:],
)

# A range is what CI passes; a bad one is not a verdict on anyone's message.
missing = subprocess.run(
    [sys.executable, str(TOOL), "--range", "no-such-ref..HEAD", "--warn"],
    capture_output=True,
    text=True,
    check=False,
    cwd=ROOT,
)
check(
    "an unreadable range warns and does not fail CI",
    missing.returncode == 0,
    f"exit={missing.returncode} out={missing.stdout.strip()[:90]}",
)

print()
if failures:
    print(f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1)
print("all checks passed")
