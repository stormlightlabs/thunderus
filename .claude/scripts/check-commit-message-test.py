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

The second group covers `--pr`, which grades the title and body a squash merge
turns into the commit. That text is the only one that reaches the trunk, so it
is the only one with a length target.
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


def run_pr(title: str, body_text: str, *arguments: str) -> subprocess.CompletedProcess[str]:
    """Check a pull request's title and body, each held in its own file."""
    with tempfile.TemporaryDirectory() as directory:
        title_path = Path(directory) / "title.txt"
        body_path = Path(directory) / "body.txt"
        title_path.write_text(title, encoding="utf-8")
        body_path.write_text(body_text, encoding="utf-8")
        return subprocess.run(
            [
                sys.executable, str(TOOL), "--pr",
                "--title-file", str(title_path),
                "--body-file", str(body_path),
                *arguments,
            ],
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
    clean.returncode == 0 and "clean" in clean.stderr,
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

# A branch commit body is discarded by the squash, so it carries no target.
long_branch_body = run(f"fix: collapse the nested if\n\n{body(30)}\n")
check(
    "a long branch commit body says nothing, because the squash drops it",
    long_branch_body.returncode == 0 and "length:" not in long_branch_body.stderr,
    f"exit={long_branch_body.returncode} err={long_branch_body.stderr.strip()[:90]}",
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

# What reaches the trunk is the pull request title and body, joined the way git
# joins a subject and a body. Everything below grades that text.
TITLE = "fix: stop dropping skills over a name mismatch"

pr_clean = run_pr(TITLE, "Skill discovery keyed on the directory name.\n")
check(
    "a clean pull request passes",
    pr_clean.returncode == 0 and "clean" in pr_clean.stderr,
    f"exit={pr_clean.returncode} err={pr_clean.stderr.strip()[:90]}",
)

# The body is the commit body now, so its target applies here and nowhere else.
pr_long = run_pr(TITLE, body(30))
check(
    "a long pull request body advises and still exits 0",
    pr_long.returncode == 0 and "length:" in pr_long.stderr,
    f"exit={pr_long.returncode} err={pr_long.stderr.strip()[:90]}",
)
check(
    "the advice says the body is the commit body",
    "commit body" in pr_long.stderr,
    pr_long.stderr.strip()[:160],
)
check(
    "a long body is never called an error",
    "error:" not in pr_long.stderr,
    pr_long.stderr.strip()[:120],
)
check(
    "the advice says plainly that it fails nothing",
    "fails nothing" in pr_long.stderr,
    pr_long.stderr.strip()[-160:],
)

# The limit is inclusive.
pr_at_target = run_pr(TITLE, body(20))
check(
    "a body exactly at the target says nothing",
    pr_at_target.returncode == 0 and "length:" not in pr_at_target.stderr,
    pr_at_target.stderr.strip()[:120],
)

# GitHub appends " (#NN)" to the title. The author never sees it, so the budget
# has to name the merged width rather than the width they typed.
pr_title_budget = run_pr("docs: " + "w" * 50, "Why it changed.\n")
check(
    "a title over the squash budget is flagged before the merge",
    pr_title_budget.returncode == 0 and "length:" in pr_title_budget.stderr,
    f"exit={pr_title_budget.returncode} err={pr_title_budget.stderr.strip()[:90]}",
)
check(
    "the advice names the width the merged subject will reach",
    "62" in pr_title_budget.stderr,
    pr_title_budget.stderr.strip()[:160],
)

# Shape is shape wherever the text came from.
pr_shape = run_pr("Stop dropping skills", "Why it changed.\n")
check(
    "a pull request title with no type is rejected",
    pr_shape.returncode == 1 and "error:" in pr_shape.stderr,
    f"exit={pr_shape.returncode} err={pr_shape.stderr.strip()[:90]}",
)
check(
    "--warn reports a pull request shape error without failing",
    run_pr("Stop dropping skills", "Why.\n", "--warn").returncode == 0,
)

# A body wrapped past 72 columns lands in git log wrapped past 72 columns.
pr_wide = run_pr(TITLE, "w" * 40 + " " + "w" * 40 + "\n")
check(
    "a pull request body line over 72 columns is rejected",
    pr_wide.returncode == 1 and "error:" in pr_wide.stderr,
    f"exit={pr_wide.returncode} err={pr_wide.stderr.strip()[:90]}",
)

# A pull request with no body is a title alone, not a message glued to one.
pr_titleonly = run_pr(TITLE, "")
check(
    "a title with no body is clean",
    pr_titleonly.returncode == 0 and "error:" not in pr_titleonly.stderr,
    f"exit={pr_titleonly.returncode} err={pr_titleonly.stderr.strip()[:90]}",
)

# The flags are read from files precisely so a title never reaches a shell.
usage = subprocess.run(
    [sys.executable, str(TOOL), "--pr", "--title-file", "only.txt"],
    capture_output=True, text=True, check=False,
)
check(
    "--pr without both files exits with the usage line",
    usage.returncode != 0 and "--body-file" in usage.stderr,
    f"exit={usage.returncode} err={usage.stderr.strip()[:90]}",
)

print()
if failures:
    print(f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1)
print("all checks passed")
