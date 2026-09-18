#!/usr/bin/env python3
"""Check a commit message against the rules in .claude/skills/commits-and-prs.

Reads a message from a file, or several from `git log` when given a revision
range:

    check-commit-message.py .git/COMMIT_EDITMSG
    check-commit-message.py --range origin/edge..HEAD
    check-commit-message.py --range origin/edge..HEAD --warn

Findings carry one of two severities, and only one of them can fail a run.

An *error* is a shape a reader cannot recover from: a missing type, a subject
that overflows the column `git log --oneline` gives it, a body glued to its
subject. Shape is not a judgement call, so without `--warn` an error exits
non-zero, which is what the commit-msg hook wants: the message is still in the
editor and costs nothing to fix.

*Advice* is length. A body over its target is usually padding, but sometimes a
change earns the room, and no script can tell those apart. Advice therefore
never fails a run in either mode. Rejecting a message for length would teach
authors to reach for `--no-verify`, which skips the shape errors too, so the
check that cannot be certain stays out of the way of the one that can. It names
what it finds and leaves the judgement with the author.

With `--warn` everything is reported and the exit code stays zero either way,
which is what CI wants, because the only way to fix a message already pushed is
to rewrite history that someone may have pulled.

The limits below are the whole policy; change them here and the hook, CI, and
the skill stay in step.
"""

import os
import re
import subprocess
import sys

SUBJECT_LIMIT = 60  # "under 60 characters", so 59 is the longest allowed.
BODY_LIMIT = 72
# The commit body target from writing-docs, counted in lines, because lines are
# what a reader scrolling `git log` spends rather than characters.
BODY_LINES = 15
# GitHub appends " (#NN)" to a squash subject server-side. Six characters for a
# two-digit issue, seven past #99, and the author never sees them: the title
# they wrote is the only part they control, so that is what the budget covers.
SQUASH_SUFFIX_WIDTH = 6
TITLE_BUDGET = SUBJECT_LIMIT - 1 - SQUASH_SUFFIX_WIDTH
TYPES = ("feat", "fix", "docs", "refactor", "test", "chore", "perf")

SUBJECT = re.compile(r"^(%s): (.+)$" % "|".join(TYPES))
TRAILER = re.compile(r"^[A-Za-z][A-Za-z-]*: .+$")
SQUASH_SUFFIX = re.compile(r" \(#\d+\)$")


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


def body_length(lines: list[str], trailers_from: int) -> int:
    """Lines of body a reader actually reads.

    The trailers the harness appends are excluded because the author does not
    write them and cannot shorten them. Blank lines between paragraphs are
    counted, because a reader scrolls past those too.
    """
    body = lines[1:trailers_from]
    while body and not body[0].strip():
        body.pop(0)
    while body and not body[-1].strip():
        body.pop()
    return len(body)


def message_lines(message: str, from_file: bool = True) -> list[str]:
    """The lines of a message that will survive into history."""
    # A stored message carries no comments, so stripping them there would drop
    # a real subject that happens to start with the comment character.
    lines = editable_lines(message) if from_file else message.splitlines()
    while lines and not lines[-1].strip():
        lines.pop()
    return lines


def measure(message: str, from_file: bool = True) -> int:
    """Body lines in one message, for the branch projection."""
    lines = message_lines(message, from_file)
    if not lines:
        return 0
    return body_length(lines, trailer_block(lines))


def check(message: str, from_file: bool = True) -> tuple[list[str], list[str]]:
    """Return (errors, advice); both empty when the message is clean.

    Errors are shape and fail the hook. Advice is length and never fails
    anything. See the module docstring for why the two are separated.
    """
    lines = message_lines(message, from_file)
    if not lines:
        return ["the message is empty"], []

    problems: list[str] = []
    advice: list[str] = []
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

    # Measure the title the author wrote, not the suffix GitHub bolted on. Every
    # overlong subject on this repository's trunk got there the second way, and
    # reporting those as the author's error sends them to shorten a title that
    # was already inside the limit.
    title = SQUASH_SUFFIX.sub("", subject)
    if len(title) >= SUBJECT_LIMIT:
        problems.append(
            f"subject is {len(title)} characters, over the {SUBJECT_LIMIT - 1} allowed"
        )
    elif len(subject) >= SUBJECT_LIMIT:
        advice.append(
            f"subject reaches {len(subject)} characters once GitHub's "
            f"'(#NN)' suffix is added. A pull request title has about "
            f"{TITLE_BUDGET} characters before the squash overflows"
        )
    # Only worth saying in the editor. Over a range, a branch commit's subject
    # is not the pull request title, so the budget does not apply to it.
    elif from_file and len(title) > TITLE_BUDGET:
        advice.append(
            f"subject is {len(title)} characters; a squash merge appends "
            f"'(#NN)', so a title over {TITLE_BUDGET} lands over the limit"
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

    length = body_length(lines, trailers_from)
    if length > BODY_LINES:
        advice.append(
            f"body is {length} lines against a {BODY_LINES}-line target. "
            "Cut what the diff already says; keep what it cannot say"
        )

    return problems, advice


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
    advised = 0
    for entry in entries:
        if from_file:
            message, label = entry, ""
        else:
            commit, _, message = entry.strip("\n").partition("\n")
            label = f"{commit[:7]} "
        problems, advice = check(message, from_file=from_file)
        if problems:
            failed += 1
        if advice:
            advised += 1
        if problems or advice:
            subject = next(iter(message.splitlines()), "")
            print(f"{label}{subject}", file=stream)
            for problem in problems:
                print(f"  error:  {problem}", file=stream)
            for note in advice:
                print(f"  length: {note}", file=stream)
            if annotate:
                for problem in problems:
                    print(f"::warning title=Commit message::{label}{problem}")
                for note in advice:
                    print(f"::notice title=Commit length::{label}{note}")

    total = len(entries)

    # What the hook grades is one message; what reaches the trunk is all of
    # them. GitHub squashes a branch by concatenating every commit body under a
    # "* subject" bullet, so a branch of short messages still merges long. The
    # projection is the only place that number is visible before the merge.
    if not from_file and total:
        projected = sum(
            measure(entry.partition("\n")[2], False) + 1 for entry in entries
        )
        if projected > BODY_LINES:
            print(
                f"\nProjected squash body: about {projected} lines from "
                f"{total} commit(s), against a {BODY_LINES}-line target. "
                "Edit the message in GitHub's merge box, or land fewer "
                "commits.",
                file=stream,
            )
            if annotate:
                print(
                    f"::notice title=Squash length::about {projected} lines "
                    f"from {total} commit(s); edit the squash message at merge"
                )

    if not failed and not advised:
        print(f"{total} commit message(s) checked, all clean.", file=stream)
        return 0

    verdict = "carry warnings" if warn else "rejected"
    summary = []
    if failed:
        summary.append(f"{failed} of {total} commit message(s) {verdict}")
    if advised:
        # Said plainly so nobody goes looking for the exit code that did not
        # happen. Length is reported to be read, not to stop anything.
        summary.append(
            f"{advised} of {total} carry length advice, which fails nothing"
        )
    print(
        "\n" + ". ".join(summary) + ". The rules live in "
        ".claude/skills/commits-and-prs/SKILL.md.",
        file=stream,
    )
    return 0 if warn or not failed else 1


if __name__ == "__main__":
    sys.exit(main())
