#!/usr/bin/env python3
"""Check a commit message against the rules in .claude/skills/commits-and-prs.

Reads a message from a file, or several from `git log` when given a revision
range:

    check-commit-message.py .git/COMMIT_EDITMSG
    check-commit-message.py --range origin/edge..HEAD
    check-commit-message.py --range origin/edge..HEAD --warn

Without `--warn` a violation exits non-zero, which is what the commit-msg hook
wants: the message is still in the editor and costs nothing to fix. With
`--warn` the same violations are reported and the exit code stays zero, which
is what CI wants, because the only way to fix a message already pushed is to
rewrite history that someone may have pulled.

The limits below are the whole policy; change them here and the hook, CI, and
the skill stay in step.
"""

import os
import re
import subprocess
import sys

SUBJECT_LIMIT = 60  # "under 60 characters", so 59 is the longest allowed.
BODY_LIMIT = 72
TYPES = ("feat", "fix", "docs", "refactor", "test", "chore", "perf")

SUBJECT = re.compile(r"^(%s): (.+)$" % "|".join(TYPES))
TRAILER = re.compile(r"^[A-Za-z][A-Za-z-]*: .+$")


def comment_char() -> str:
    """The character git strips from a message being edited."""
    try:
        value = subprocess.run(
            ["git", "config", "--get", "core.commentChar"],
            capture_output=True,
            text=True,
            check=False,
        ).stdout.strip()
    except OSError:
        return "#"
    # "auto" asks git to pick one per message; it picks '#' unless the message
    # already starts a line with it, which a message we are about to reject for
    # its subject will not.
    return value[:1] if value and value != "auto" else "#"


def editable_lines(message: str) -> list[str]:
    """Strip what git itself removes from a message the author is editing.

    Comments and everything below the scissors line are deleted by git *after*
    this hook runs, so grading them rejects an author for a verbose diff that
    never becomes part of the message.
    """
    char = comment_char()
    lines = []
    for line in message.splitlines():
        if line.startswith(char):
            # "# ------------------------ >8 ------------------------"
            if " >8 " in line:
                break
            continue
        lines.append(line)
    return lines


def trailer_block(lines: list[str]) -> int:
    """Index of the first line of the trailing trailer paragraph, or len(lines).

    Only the final paragraph can hold trailers. Exempting every `Word: value`
    line anywhere in the body would let an ordinary sentence opening with
    "Note: " run to any width.
    """
    end = len(lines)
    start = end
    while start > 0 and lines[start - 1].strip():
        start -= 1
    if start == end:
        return end
    if all(TRAILER.match(line) for line in lines[start:end]):
        return start
    return end


def check(message: str, from_file: bool = True) -> list[str]:
    """Return one description per violation, empty when the message is clean."""
    # A stored message carries no comments, so stripping them there would drop
    # a real subject that happens to start with the comment character.
    lines = editable_lines(message) if from_file else message.splitlines()
    while lines and not lines[-1].strip():
        lines.pop()
    if not lines:
        return ["the message is empty"]

    problems = []
    subject = lines[0]

    match = SUBJECT.match(subject)
    if not match:
        problems.append(
            "subject must read '<type>: <what changed>', where type is one of "
            + ", ".join(TYPES)
        )
    else:
        rest = match.group(2)
        if rest[0].isupper():
            problems.append("subject starts with a capital after the type")
        if rest.endswith("."):
            problems.append("subject ends with a period")

    if len(subject) >= SUBJECT_LIMIT:
        problems.append(
            f"subject is {len(subject)} characters, over the {SUBJECT_LIMIT - 1} allowed"
        )

    if len(lines) > 1 and lines[1].strip():
        problems.append("no blank line between the subject and the body")

    # A fenced block holds output or commands that wrapping would corrupt, and a
    # line without spaces is a URL or a path that cannot be wrapped at all.
    trailers_from = trailer_block(lines)
    fenced = False
    fence_opened_at = 0
    for number, line in enumerate(lines[1:], start=2):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            if fenced:
                fence_opened_at = number
            continue
        if fenced or " " not in line.strip():
            continue
        if number - 1 >= trailers_from:
            continue
        if len(line) > BODY_LIMIT:
            problems.append(
                f"line {number} is {len(line)} characters, over the {BODY_LIMIT} allowed"
            )

    if fenced:
        problems.append(
            f"the code fence opened on line {fence_opened_at} is never closed, "
            "so everything below it skipped the column check"
        )

    return problems


def read_file(path: str) -> str:
    """Read a message, tolerating bytes that are not UTF-8.

    Git stores whatever bytes the author's editor wrote. Refusing to decode them
    would replace a verdict with a traceback, and in CI a traceback fails a job
    documented as never failing.
    """
    with open(path, "rb") as handle:
        return handle.read().decode("utf-8", errors="replace")


def read_range(revisions: str) -> tuple[list[str], str | None]:
    """Messages in a revision range, or an explanation of why they are missing."""
    # %x00 separates commits; a message may contain any other byte.
    result = subprocess.run(
        ["git", "log", "--format=%H%n%B%x00", revisions],
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip().splitlines()
        reason = detail[-1] if detail else f"git log exited {result.returncode}"
        return [], f"could not read {revisions}: {reason}"
    out = result.stdout.decode("utf-8", errors="replace")
    return [entry for entry in out.split("\0") if entry.strip()], None


def main() -> int:
    argv = sys.argv[1:]
    warn = "--warn" in argv
    argv = [argument for argument in argv if argument != "--warn"]
    if not argv:
        sys.exit("usage: check-commit-message.py <file> | --range <revisions> [--warn]")

    from_file = argv[0] != "--range"

    # Warnings are results rather than errors, so they belong on stdout where a
    # job summary or a pipe can pick them up.
    stream = sys.stdout if warn else sys.stderr
    annotate = warn and os.environ.get("GITHUB_ACTIONS") == "true"

    if from_file:
        entries = [read_file(argv[0])]
    else:
        if len(argv) != 2:
            sys.exit("--range takes one revision range")
        entries, error = read_range(argv[1])
        if error:
            print(error, file=stream)
            # Reporting is the whole job in warn mode, and a range this checkout
            # cannot resolve is not a verdict on anyone's message.
            return 0 if warn else 1

    failed = 0
    for entry in entries:
        if from_file:
            message, label = entry, ""
        else:
            commit, _, message = entry.strip("\n").partition("\n")
            label = f"{commit[:7]} "
        problems = check(message, from_file=from_file)
        if problems:
            failed += 1
            subject = next(iter(message.splitlines()), "")
            print(f"{label}{subject}", file=stream)
            for problem in problems:
                print(f"  {problem}", file=stream)
            if annotate:
                joined = "; ".join(problems)
                print(f"::warning title=Commit message::{label}{joined}")

    total = len(entries)
    if not failed:
        print(f"{total} commit message(s) checked, all clean.", file=stream)
        return 0

    verdict = "carry warnings" if warn else "rejected"
    print(
        f"\n{failed} of {total} commit message(s) {verdict}. The rules live in "
        ".claude/skills/commits-and-prs/SKILL.md.",
        file=stream,
    )
    return 0 if warn else 1


if __name__ == "__main__":
    sys.exit(main())
