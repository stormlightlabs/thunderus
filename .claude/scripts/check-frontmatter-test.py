#!/usr/bin/env python3
"""Check check-frontmatter.py against fixture trees.

    .claude/scripts/check-frontmatter-test.py

Almost every case builds a throwaway tree in a temporary directory and runs the
tool over it, so a failure says which fixture produced it. The exception is the
last case, which runs the tool bare against this repository, because the default
root is the only path CI uses and nothing else here exercises it.

The cases are the ways a hand-written block goes wrong: a file that never got
one, a key that got copied from a neighbour and not edited, and an identifier
that reads like a ULID without being one. The later ones are the ways a document
walks past the check entirely, which is the worse failure and the quieter one.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
TOOL = ROOT / "check-frontmatter.py"

GOOD = "01M2RFP6G4NBXT94SAAYZ1D1FH"
ALSO_GOOD = "01M2RFP6G4GR61PWC6AR0WVSTR"

failures: list[str] = []


def block(name: str, identifier: str = GOOD, date: str = "2026-09-17") -> str:
    return f"---\nname: {name}\nlast_updated: {date}\nid: {identifier}\n---\n\n# Body\n"


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


def git_tree(files: dict[str, str]) -> tuple[Path, tempfile.TemporaryDirectory]:
    """A committed git repository holding the fixture, for the --since cases."""
    holder = tempfile.TemporaryDirectory()
    root = Path(holder.name) / "internal"
    root.mkdir()
    for relative, text in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    git = ["git", "-C", holder.name, "-c", "user.email=t@t", "-c", "user.name=t"]
    for command in (["init", "-q"], ["add", "-A"], ["commit", "-qm", "fixture"]):
        subprocess.run(git + command, capture_output=True, text=True, check=True)
    return root, holder


def check(label: str, passed: bool, detail: str = "") -> None:
    print(f"{'ok  ' if passed else 'FAIL'} {label}")
    if not passed:
        failures.append(label)
        if detail:
            print(f"     {detail}")


result = run({"models.md": block("models"), "qa/README.md": block("qa", ALSO_GOOD)})
check(
    "a tree carrying frontmatter everywhere passes",
    result.returncode == 0 and "2 documents" in result.stdout,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": "# Body\n"})
check(
    "a file with no frontmatter fails",
    result.returncode == 1 and "does not open with frontmatter" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": "---\nname: models\n\n# Body\n"})
check(
    "an unclosed block fails rather than reading the whole file",
    result.returncode == 1 and "never closed" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": f"---\nname: models\nid: {GOOD}\n---\n"})
check(
    "a missing key is named",
    result.returncode == 1 and "no last_updated" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": f"---\nname:\nlast_updated: 2026-09-17\nid: {GOOD}\n---\n"})
check(
    "a key present but empty fails",
    result.returncode == 1 and "name is empty" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models", date="17-09-2026")})
check(
    "a date that is not YYYY-MM-DD fails",
    result.returncode == 1 and "expected YYYY-MM-DD" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

# I, L, O, and U are outside the alphabet, so this is 26 characters of the wrong
# thing rather than a ULID.
result = run({"models.md": block("models", identifier="01M2RFP6G4NBXT94SAAYZ1D1FI")})
check(
    "an identifier using a letter the alphabet omits fails",
    result.returncode == 1 and "expected a 26-character ULID" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models", identifier=GOOD[:-1])})
check(
    "an identifier of the wrong length fails",
    result.returncode == 1 and "expected a 26-character ULID" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models"), "qa/brew.md": block("brew")})
check(
    "two documents sharing an identifier fail",
    result.returncode == 1 and "id is also on" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("mdoels")})
check(
    "a name that disagrees with its filename fails",
    result.returncode == 1 and "expected 'models'" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"qa/README.md": block("readme")})
check(
    "a README named for itself rather than its directory fails",
    result.returncode == 1 and "expected 'qa'" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"BUGS.md": block("bugs")})
check(
    "an upper-case filename takes its kebab-case name",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

repeated = f"---\nname: models\nname: models\nlast_updated: 2026-09-17\nid: {GOOD}\n---\n"
result = run({"models.md": repeated})
check(
    "a key repeated inside one block fails",
    result.returncode == 1 and "name appears twice" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models"), "features/mcp/plan.md": "# MCP\n"})
check(
    "a per-feature plan is exempt while its naming scheme is undecided",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models"), "features/mcp/tasks.md": "# MCP\n"})
check(
    "a per-feature task list is exempt too",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

# The exemption names internal/features, not any directory called features.
result = run({"models.md": block("models"), "archive/features/old/plan.md": "# Old\n"})
check(
    "a features directory elsewhere in the tree inherits no exemption",
    result.returncode == 1 and "does not open with frontmatter" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"a.md": "# A\n", "b.md": "# B\n", "c.md": "# C\n"})
check(
    "one run reports every failing file, not just the first",
    result.returncode == 1 and result.stderr.count("does not open with frontmatter") == 3,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

missing = subprocess.run(
    [sys.executable, str(TOOL), str(ROOT / "nowhere")],
    capture_output=True,
    text=True,
    check=False,
)
check(
    "a directory that is not there is a usage error, not a clean tree",
    missing.returncode == 2,
    f"exit={missing.returncode}",
)

listed = f"""---
name: models
last_updated: 2026-09-17
id: {GOOD}
tags:
  - one
  - two
---

# Body
"""
result = run({"models.md": listed})
check(
    "a list under a key the convention does not constrain is not a parse error",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

commented = f"""---
# the identifier never changes once assigned
name: models
last_updated: 2026-09-17
id: {GOOD}
feature-id: mcp
---

# Body
"""
result = run({"models.md": commented})
check(
    "a comment line and a hyphenated key are both allowed",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models", identifier=f"{GOOD} # assigned in #8")})
check(
    "a trailing comment is not part of the value",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({"models.md": block("models", date="2026-13-45")})
check(
    "a date of the right shape that is not a real date fails",
    result.returncode == 1 and "not a real date" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

with tempfile.TemporaryDirectory() as parent:
    named = Path(parent) / "fixtures"
    named.mkdir()
    (named / "README.md").write_text(block("fixtures"), encoding="utf-8")
    rooted = subprocess.run(
        [sys.executable, str(TOOL), str(named)],
        capture_output=True,
        text=True,
        check=False,
    )
check(
    "a README at the root is named for the directory being checked",
    rooted.returncode == 0,
    f"exit={rooted.returncode} stderr={rooted.stderr.strip()[:80]}",
)

# Everything below is a way a document walks past the check rather than a way it
# is written wrong. A false rejection is loud; these are silent.

result = run({"models.md": block("models"), "NOTES.MD": "# no block at all\n"})
check(
    "capitalising the extension does not walk a document past the check",
    result.returncode == 1
    and "extension is '.MD'" in result.stderr
    and "does not open with frontmatter" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

exempt_with_block = f"""---
name: anything
last_updated: not-a-date
id: {GOOD}
---

# Plan
"""
result = run({"models.md": block("models"), "features/mcp/plan.md": exempt_with_block})
check(
    "a waived file that has a block still answers for its id and its date",
    result.returncode == 1
    and "id is also on" in result.stderr
    and "expected YYYY-MM-DD" in result.stderr
    and "name is" not in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:120]}",
)

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    (root / "good.md").write_text(block("good"), encoding="utf-8")
    (root / "binary.md").write_bytes(b"---\nname: binary\n---\n\ncaf\xe9\n")
    (root / "zzz.md").write_text("# no block\n", encoding="utf-8")
    undecodable = subprocess.run(
        [sys.executable, str(TOOL), str(root)],
        capture_output=True,
        text=True,
        check=False,
    )
check(
    "a file that is not UTF-8 fails on its own line without ending the run",
    undecodable.returncode == 1
    and "not valid UTF-8" in undecodable.stderr
    and "zzz.md" in undecodable.stderr
    and "Traceback" not in undecodable.stderr,
    f"exit={undecodable.returncode} stderr={undecodable.stderr.strip()[:120]}",
)

quoted = f"""---
name: "models"
last_updated: "2026-09-17"
id: "{GOOD}"
---

# Body
"""
result = run({"models.md": quoted})
check(
    "a quoted scalar means what the unquoted one means",
    result.returncode == 0,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

result = run({})
check(
    "a tree with no documents is a failure, not a clean run",
    result.returncode == 1 and "no documents found" in result.stderr,
    f"exit={result.returncode} stderr={result.stderr.strip()[:80]}",
)

# Uniqueness within one tree is not immutability across time, so the --since
# cases mutate a committed identifier rather than duplicating a live one.
root, holder = git_tree({"models.md": block("models")})
unchanged = subprocess.run(
    [sys.executable, str(TOOL), str(root), "--since", "HEAD"],
    capture_output=True,
    text=True,
    check=False,
)
(root / "models.md").write_text(block("models", identifier=ALSO_GOOD), encoding="utf-8")
changed = subprocess.run(
    [sys.executable, str(TOOL), str(root), "--since", "HEAD"],
    capture_output=True,
    text=True,
    check=False,
)
tree_only = subprocess.run(
    [sys.executable, str(TOOL), str(root)],
    capture_output=True,
    text=True,
    check=False,
)
holder.cleanup()
check(
    "an identifier that did not change passes --since",
    unchanged.returncode == 0,
    f"exit={unchanged.returncode} stderr={unchanged.stderr.strip()[:80]}",
)
check(
    "an identifier edited in place fails --since",
    changed.returncode == 1 and "never changes once assigned" in changed.stderr,
    f"exit={changed.returncode} stderr={changed.stderr.strip()[:80]}",
)
check(
    "the same edit passes without --since, which is why --since exists",
    tree_only.returncode == 0,
    f"exit={tree_only.returncode} stderr={tree_only.stderr.strip()[:80]}",
)

# The default root is the only one CI's bare invocation uses.
bare = subprocess.run(
    [sys.executable, str(TOOL)],
    capture_output=True,
    text=True,
    check=False,
)
check(
    "a bare run finds this repository's internal/ and passes",
    bare.returncode == 0 and "carry their frontmatter" in bare.stdout,
    f"exit={bare.returncode} stderr={bare.stderr.strip()[:120]}",
)

# A plan a milestone tracks names it, so either side of the link is reachable
# from the other. The value is optional and checked only when present.
MS = "https://github.com/stormlightlabs/thunderus/milestone/1"


def with_milestone(value: str) -> str:
    return (
        f"---\nname: tracked\nlast_updated: 2026-09-17\n"
        f"id: {GOOD}\nmilestone: {value}\n---\n\n# Body\n"
    )


ok_ms = run({"internal/tracked.md": with_milestone(MS)})
check(
    "a milestone URL passes",
    ok_ms.returncode == 0,
    f"exit={ok_ms.returncode} err={ok_ms.stderr.strip()[:110]}",
)

bad_ms = run({"internal/tracked.md": with_milestone("UI Polish")})
check(
    "a milestone that is not a URL is rejected",
    bad_ms.returncode != 0 and "milestone" in bad_ms.stderr,
    f"exit={bad_ms.returncode} err={bad_ms.stderr.strip()[:110]}",
)

no_ms = run({"internal/tracked.md": block("tracked")})
check(
    "a document with no milestone still passes",
    no_ms.returncode == 0,
    f"exit={no_ms.returncode} err={no_ms.stderr.strip()[:110]}",
)

print()
if failures:
    print(f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1)
print("all checks passed")
