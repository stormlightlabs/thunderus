#!/usr/bin/env python3
"""Check check-isolation.py against fixture trees.

    .claude/scripts/check-isolation-test.py

Every case in CASES builds a throwaway tree and runs the tool over it, so a
failure says which fixture produced it. The two cases after the table run the
tool differently: one on a root that does not exist, one bare against this
repository, because the default root is the only path CI uses and no fixture
reaches it.

The cases are the two ways the setting gets declared and the several ways a
check for it goes wrong: the word in prose, which is how the rule is written
down and must keep passing; the word in a value rather than a key, which every
`description` line in the tree carries; and a copy of the tree under
`worktrees/`, which would otherwise report findings against a path nobody
tracks.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path
from typing import NamedTuple

ROOT = Path(__file__).resolve().parent
TOOL = ROOT / "check-isolation.py"

CLEAN = """---
name: implementer
description: Work one issue to a pull request, with build isolation per tree.
tools: Bash, Read
---

Use the `implement` skill. The run gives you a directory; work there.
"""


class Case(NamedTuple):
    """One fixture tree and what the tool should say about it.

    `err` and `out` are substrings that must all appear in that stream, so a
    case asserting nothing about a stream leaves its tuple empty. `why` is the
    reason the case exists, printed when it fails, because a case that breaks
    is read by whoever broke it rather than by whoever wrote it.
    """

    label: str
    files: dict[str, str]
    code: int
    err: tuple[str, ...] = ()
    out: tuple[str, ...] = ()
    why: str = ""


CASES = (
    Case(
        label="a tree declaring nothing passes",
        files={"agents/implementer.md": CLEAN, "skills/worktree/SKILL.md": CLEAN},
        code=0,
        out=("2 definitions",),
    ),
    Case(
        label="the frontmatter key fails, naming the file and the key",
        files={
            "agents/implementer.md": (
                "---\nname: implementer\nisolation: worktree\n---\n\nBody.\n"
            )
        },
        code=1,
        err=("agents/implementer.md", "frontmatter declares isolation: worktree"),
    ),
    Case(
        label="the key with no value fails too",
        files={"agents/implementer.md": "---\nname: a\nisolation:\n---\n\nBody.\n"},
        code=1,
        err=("isolation: (empty)",),
    ),
    Case(
        label="an indented key fails",
        files={
            "agents/a.md": "---\nname: a\nagent:\n  isolation: worktree\n---\n\nB.\n"
        },
        code=1,
        err=("frontmatter declares isolation",),
        why=(
            "The harness reads the key whatever it is indented under. Requiring "
            "column zero would let one space through, and one space is what a "
            "hand-edited block has."
        ),
    ),
    Case(
        label="the setting in a fenced block fails",
        files={
            "agents/a.md": '---\nname: a\n---\n\n```json\n{"isolation": "x"}\n```\n'
        },
        code=1,
        err=("code block sets isolation",),
    ),
    Case(
        label="the setting as a flag in a shell block fails",
        files={
            "skills/s/SKILL.md": (
                "---\nname: s\n---\n\n```sh\nAgent --isolation=worktree\n```\n"
            )
        },
        code=1,
        err=("code block sets isolation",),
    ),
    Case(
        label="the word in prose passes",
        files={
            "skills/worktree/SKILL.md": (
                "---\nname: worktree\n---\n\n"
                "An `isolation` setting on a dispatch places the worktree inside\n"
                "the repository root, so this repository uses none.\n"
            )
        },
        code=0,
        why=(
            "The rule itself is a sentence containing the word. A check that "
            "fails the place the rule is written down cannot be the place the "
            "rule is written down."
        ),
    ),
    Case(
        label="the word inside a value passes",
        files={"agents/a.md": "---\nname: a\ndescription: Isolation.\n---\n\nB.\n"},
        code=0,
        why=(
            "Every definition has a description, and the worktree skill's says "
            "'build isolation'. A check keying on the word rather than the "
            "declaration fails the whole tree on the day it lands."
        ),
    ),
    Case(
        label="a worktree that landed under .claude/ is not reported",
        files={
            "agents/a.md": CLEAN,
            "worktrees/12/.claude/agents/a.md": "---\nisolation: worktree\n---\n",
        },
        code=0,
        out=("1 definition ",),
    ),
    Case(
        label="a nested fence does not close the block early",
        files={
            "skills/s/SKILL.md": (
                "---\nname: s\n---\n\n````md\n```yaml\nisolation: x\n```\n````\n"
            )
        },
        code=1,
        err=("code block sets isolation",),
        why=(
            "A fence inside a block is written with a longer fence. Closing on "
            "the inner one would leave the rest of the block read as prose."
        ),
    ),
    Case(
        label="an unclosed block is left to check-frontmatter.py",
        files={"agents/a.md": "---\nname: a\n\nisolation: worktree\n"},
        code=0,
    ),
    Case(
        label="a tree with no markdown fails rather than reporting success",
        files={"agents/notes.txt": "isolation: worktree\n"},
        code=1,
        err=("no definitions found",),
    ),
)

failures: list[str] = []


def run(files: dict[str, str]) -> subprocess.CompletedProcess[str]:
    """Write the fixture tree and check it."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for relative, text in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(TOOL), str(root)],
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


for case in CASES:
    result = run(case.files)
    absent = [text for text in case.err if text not in result.stderr]
    absent += [text for text in case.out if text not in result.stdout]
    check(
        case.label,
        result.returncode == case.code and not absent,
        " ".join(
            part
            for part in (
                f"exit={result.returncode}, wanted {case.code}",
                f"absent={absent}" if absent else "",
                f"err={result.stderr.strip()[:120]!r}",
                case.why,
            )
            if part
        ),
    )

missing = subprocess.run(
    [sys.executable, str(TOOL), str(ROOT / "no-such-tree")],
    capture_output=True,
    text=True,
    check=False,
)
check(
    "a root that is not a directory exits 2 rather than passing",
    missing.returncode == 2,
    f"exit={missing.returncode} stderr={missing.stderr.strip()[:90]}",
)

# The default root is what CI runs, and no fixture reaches it.
bare = subprocess.run(
    [sys.executable, str(TOOL)], capture_output=True, text=True, check=False
)
check(
    "this repository passes its own check",
    bare.returncode == 0,
    f"exit={bare.returncode} stderr={bare.stderr.strip()[:200]}",
)

print()
if failures:
    print(f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1)
print("all checks passed")
