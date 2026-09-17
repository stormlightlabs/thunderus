#!/usr/bin/env python3
"""Check check-frontmatter.py against fixture trees.

    .claude/scripts/check-frontmatter-test.py

Every case builds a throwaway tree in a temporary directory and runs the tool
over it, so nothing here reads the repository and a failure says which fixture
produced it. The cases are the ways a hand-written block goes wrong: a file that
never got one, a key that got copied from a neighbour and not edited, and an
identifier that reads like a ULID without being one.
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


def check(label: str, passed: bool, detail: str = "") -> None:
    print(f"{'ok  ' if passed else 'FAIL'} {label}")
    if not passed:
        failures.append(label)
        if detail:
            print(f"     {detail}")


result = run({"models.md": block("models"), "qa/README.md": block("qa", ALSO_GOOD)})
check(
    "a tree carrying frontmatter everywhere passes",
    result.returncode == 0 and "carries its frontmatter" in result.stdout,
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

print()
if failures:
    print(f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1)
print("all checks passed")
