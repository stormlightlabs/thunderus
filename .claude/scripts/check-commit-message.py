#!/usr/bin/env python3
"""Check a commit message against the rules in .claude/skills/commits-and-prs.

Reads a message from a file, or several from `git log` when given a revision
range:

    check-commit-message.py .git/COMMIT_EDITMSG
    check-commit-message.py --range origin/edge..HEAD

Exits non-zero and names every violation. The limits below are the whole
policy; change them here and the hook, CI, and the skill stay in step.
"""

import re
import subprocess
import sys

SUBJECT_LIMIT = 60  # "under 60 characters", so 59 is the longest allowed.
BODY_LIMIT = 72
TYPES = ("feat", "fix", "docs", "refactor", "test", "chore", "perf")

SUBJECT = re.compile(r"^(%s): (.+)$" % "|".join(TYPES))
TRAILER = re.compile(r"^[A-Za-z][A-Za-z-]*: .+$")


def check(message: str) -> list[str]:
    """Return one description per violation, empty when the message is clean."""
    lines = [line for line in message.splitlines() if not line.startswith("#")]
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
    fenced = False
    for number, line in enumerate(lines[1:], start=2):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if fenced or TRAILER.match(line) or " " not in line.strip():
            continue
        if len(line) > BODY_LIMIT:
            problems.append(
                f"line {number} is {len(line)} characters, over the {BODY_LIMIT} allowed"
            )

    return problems


def main() -> int:
    argv = sys.argv[1:]
    if not argv:
        sys.exit("usage: check-commit-message.py <file> | --range <revisions>")

    if argv[0] == "--range":
        if len(argv) != 2:
            sys.exit("--range takes one revision range")
        # %x00 separates commits; a message may contain any other byte.
        out = subprocess.run(
            ["git", "log", "--format=%H%n%B%x00", argv[1]],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        entries = [entry for entry in out.split("\0") if entry.strip()]
    else:
        with open(argv[0], encoding="utf-8") as handle:
            entries = [handle.read()]

    failed = 0
    for entry in entries:
        if argv[0] == "--range":
            commit, _, message = entry.strip("\n").partition("\n")
            label = f"{commit[:7]} "
        else:
            message, label = entry, ""
        problems = check(message)
        if problems:
            failed += 1
            subject = next(
                (line for line in message.splitlines() if not line.startswith("#")),
                "",
            )
            print(f"{label}{subject}", file=sys.stderr)
            for problem in problems:
                print(f"  {problem}", file=sys.stderr)

    if failed:
        print(
            f"\n{failed} commit message(s) rejected. The rules live in "
            ".claude/skills/commits-and-prs/SKILL.md.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
