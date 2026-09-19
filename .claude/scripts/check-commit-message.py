#!/usr/bin/env python3
"""Check a commit message against the rules in .claude/skills/commits-and-prs.

Reads a message from a file, or builds one from a pull request's title and
body:

    check-commit-message.py .git/COMMIT_EDITMSG
    check-commit-message.py --pr --title-file t.txt --body-file b.txt
    check-commit-message.py --pr --title-file t.txt --body-file b.txt --warn

The title and body arrive as files rather than arguments. Both are written by
whoever opened the pull request, and a workflow that interpolates a title into
a shell command runs that title.

The pull request is the one that matters. This repository merges with
squash_merge_commit_title=PR_TITLE and squash_merge_commit_message=PR_BODY, so
the title and body reach `edge` verbatim and the branch's own messages are
discarded. Checking the branch would grade text nobody reads.

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
which is what CI wants: a pull request's title and body are editable until the
merge, so naming a problem is worth more than blocking on it.

The limits below are the whole policy; change them here and the hook, CI, and
the skill stay in step.
"""

import os
import re
import subprocess
import sys

SUBJECT_LIMIT = 60  # "under 60 characters", so 59 is the longest allowed.
BODY_LIMIT = 72
# The pull request body becomes the squash commit body verbatim, so it is
# counted in lines at 72 columns like any other commit body. Twenty lines at
# that width is about 150 words, which is the figure
# internal/ideas/commit-and-pr-length.md cites for a focused change.
#
# A branch commit body has no target. The squash discards it, so grading it
# would spend a reader's attention on text that never reaches anyone.
PR_BODY_LINES = 20
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


def check(
    message: str, from_file: bool = True, from_pull_request: bool = False
) -> tuple[list[str], list[str]]:
    """Return (errors, advice); both empty when the message is clean.

    Errors are shape and fail the hook. Advice is length and never fails
    anything. See the module docstring for why the two are separated.

    `from_file` says whether to strip the comment lines an editor adds.
    `from_pull_request` says the subject is a pull request title, which is the
    one subject the ` (#NN)` budget is certain to apply to.
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
    elif from_pull_request and len(title) > TITLE_BUDGET:
        advice.append(
            f"title is {len(title)} characters; the merged subject will be "
            f"{len(title) + SQUASH_SUFFIX_WIDTH} once GitHub appends '(#NN)', "
            f"over the {SUBJECT_LIMIT - 1} allowed"
        )
    # In the editor nothing knows whether this subject becomes a title, so the
    # budget is advice rather than the verdict it is on a pull request.
    elif from_file and len(title) > TITLE_BUDGET:
        advice.append(
            f"subject is {len(title)} characters; if it becomes a pull request "
            f"title, the squash appends '(#NN)' and lands over the limit"
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

    if from_pull_request:
        length = body_length(lines, trailers_from)
        if length > PR_BODY_LINES:
            advice.append(
                f"body is {length} lines against a {PR_BODY_LINES}-line "
                "target, and it is the commit body. Cut what the diff already "
                "says; keep what it cannot say"
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


def read_pull_request(title_path: str, body_path: str) -> str:
    """The commit message a squash merge will build from a pull request.

    This repository merges with squash_merge_commit_title=PR_TITLE and
    squash_merge_commit_message=PR_BODY, so the two files join exactly as git
    joins a subject and a body. GitHub appends " (#NN)" to the subject at merge
    time; it is not added here, because the budget covers the part the author
    controls.
    """
    title = read_file(title_path).strip()
    body = read_file(body_path).strip("\n")
    return f"{title}\n\n{body}" if body else title


def parse(argv: list[str]) -> tuple[str, bool]:
    """Return (message, from_file), or exit with the usage line."""
    usage = (
        "usage: check-commit-message.py <file> [--warn]\n"
        "       check-commit-message.py --pr --title-file <f> --body-file <f> "
        "[--warn]"
    )
    if not argv:
        sys.exit(usage)
    if argv[0] != "--pr":
        if len(argv) != 1:
            sys.exit(usage)
        return read_file(argv[0]), True

    paths: dict[str, str] = {}
    rest = argv[1:]
    while rest:
        flag = rest[0]
        if flag not in ("--title-file", "--body-file") or len(rest) < 2:
            sys.exit(usage)
        paths[flag] = rest[1]
        rest = rest[2:]
    if len(paths) != 2:
        sys.exit(usage)
    return read_pull_request(paths["--title-file"], paths["--body-file"]), False


def main() -> int:
    argv = sys.argv[1:]
    warn = "--warn" in argv
    message, from_file = parse([a for a in argv if a != "--warn"])
    from_pull_request = not from_file

    # Warnings are results rather than errors, so they belong on stdout where a
    # job summary or a pipe can pick them up.
    stream = sys.stdout if warn else sys.stderr
    annotate = warn and os.environ.get("GITHUB_ACTIONS") == "true"

    problems, advice = check(
        message, from_file=from_file, from_pull_request=from_pull_request
    )
    subject = next(iter(message.splitlines()), "")
    what = "pull request text" if from_pull_request else "commit message"

    if not problems and not advice:
        print(f"{what} checked, clean: {subject}", file=stream)
        return 0

    print(subject, file=stream)
    for problem in problems:
        print(f"  error:  {problem}", file=stream)
    for note in advice:
        print(f"  length: {note}", file=stream)
    if annotate:
        for problem in problems:
            print(f"::warning title=Commit message::{problem}")
        for note in advice:
            print(f"::notice title=Commit length::{note}")

    if problems:
        print(
            f"\nThe {what} needs a shape fix. The rules live in "
            ".claude/skills/commits-and-prs/SKILL.md.",
            file=stream,
        )
    if advice:
        # Said plainly so nobody goes looking for the exit code that did not
        # happen. Length is reported to be read, not to stop anything.
        print(
            "\nLength advice fails nothing. A pull request's title and body "
            "stay editable until the merge, so this is worth reading rather "
            "than worth blocking on.",
            file=stream,
        )
    return 0 if warn or not problems else 1


if __name__ == "__main__":
    sys.exit(main())
