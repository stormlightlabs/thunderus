#!/usr/bin/env python3
"""Check check-isolation.py against fixture trees.

    .claude/scripts/check-isolation-test.py

Every case but the last builds a throwaway tree and runs the tool over it, so a
failure says which fixture produced it. The last runs the tool bare against this
repository, because the default root is the only path CI uses and nothing else
here exercises it.

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

ROOT = Path(__file__).resolve().parent
TOOL = ROOT / "check-isolation.py"

CLEAN = """---
name: implementer
description: Work one issue to a pull request, with build isolation per tree.
tools: Bash, Read
---

Use the `implement` skill. The run gives you a directory; work there.
"""

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


result = run({"agents/implementer.md": CLEAN, "skills/worktree/SKILL.md": CLEAN})
check(
    "a tree declaring nothing passes",
    result.returncode == 0 and "2 definitions" in result.stdout,
    f"exit={result.returncode} stderr={result.stderr.strip()[:90]}",
)

result = run(
    {
        "agents/implementer.md": (
            "---\nname: implementer\nisolation: worktree\n---\n\nBody.\n"
        )
    }
)
check(
    "the frontmatter key fails, naming the file and the key",
    result.returncode == 1
    and "agents/implementer.md" in result.stderr
    and "frontmatter declares isolation: worktree" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

result = run({"agents/implementer.md": "---\nname: a\nisolation:\n---\n\nBody.\n"})
check(
    "the key with no value fails too",
    result.returncode == 1 and "isolation: (empty)" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

# A key the harness reads whatever it is indented under. Requiring column zero
# would let one space through, and one space is what a hand-edited block has.
result = run(
    {"agents/a.md": "---\nname: a\nagent:\n  isolation: worktree\n---\n\nBody.\n"}
)
check(
    "an indented key fails",
    result.returncode == 1 and "frontmatter declares isolation" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

result = run(
    {"agents/a.md": '---\nname: a\n---\n\n```json\n{"isolation": "worktree"}\n```\n'}
)
check(
    "the setting in a fenced block fails",
    result.returncode == 1 and "code block sets isolation" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

result = run(
    {
        "skills/s/SKILL.md": (
            "---\nname: s\n---\n\n```sh\nAgent --isolation=worktree\n```\n"
        )
    }
)
check(
    "the setting as a flag in a shell block fails",
    result.returncode == 1 and "code block sets isolation" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

# The rule itself is a sentence containing the word. A check that fails the
# place the rule is written down cannot be the place the rule is written down.
result = run(
    {
        "skills/worktree/SKILL.md": (
            "---\nname: worktree\n---\n\n"
            "An `isolation` setting on a dispatch places the worktree inside the\n"
            "repository root, so this repository uses none.\n"
        )
    }
)
check(
    "the word in prose passes",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

# Every definition in the tree has a description, and the worktree skill's says
# "build isolation". A check keying on the word rather than the declaration
# fails the whole tree on the day it lands.
result = run(
    {"agents/a.md": "---\nname: a\ndescription: Build isolation.\n---\n\nB.\n"}
)
check(
    "the word inside a value passes",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

result = run(
    {
        "agents/a.md": CLEAN,
        "worktrees/12/.claude/agents/a.md": "---\nisolation: worktree\n---\n",
    }
)
check(
    "a worktree that landed under .claude/ is not reported",
    result.returncode == 0 and "1 definition " in result.stdout,
    f"exit={result.returncode} out={result.stdout.strip()[:90]}",
)

# A fence inside a block is written with a longer fence. Closing on the inner
# one would leave the rest of the block read as prose.
result = run(
    {
        "skills/s/SKILL.md": (
            "---\nname: s\n---\n\n````md\n```yaml\nisolation: worktree\n```\n````\n"
        )
    }
)
check(
    "a nested fence does not close the block early",
    result.returncode == 1 and "code block sets isolation" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

result = run({"agents/a.md": "---\nname: a\n\nisolation: worktree\n"})
check(
    "an unclosed block is left to check-frontmatter.py",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

result = run({"agents/notes.txt": "isolation: worktree\n"})
check(
    "a tree with no markdown fails rather than reporting success",
    result.returncode == 1 and "no definitions found" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
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
