#!/usr/bin/env python3
"""Check that every file under internal/ opens with the frontmatter block.

    check-frontmatter.py            # check internal/ next to this script
    check-frontmatter.py <dir>      # check some other tree

The convention lives in internal/thunderstorm.md: a document opens with a name,
a date, and a ULID that never changes, and an issue cites the document it came
from by that identifier. A document missing one cannot be cited, and two
documents sharing one cite each other's work, so both are failures here.

The parsing is deliberate about what it rejects. A key repeated inside one block
is ambiguous rather than merely untidy, and a name that disagrees with the path
means a file was copied and half-edited, which is the failure this catches that
reading the files does not.

Exits non-zero listing every file that fails, so one run reports the whole tree.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

FENCE = "---"
REQUIRED = ("name", "last_updated", "id")
FIELD = re.compile(r"^([A-Za-z][A-Za-z0-9_]*):\s*(.*?)\s*$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

# Crockford base32 without I, L, O, and U, which the alphabet omits so a
# transcribed identifier cannot turn into a different one.
ULID = re.compile(r"^[0-9A-HJKMNP-TV-Z]{26}$")

# Per-feature plans and task lists are held back from the convention until their
# naming scheme is decided: five files named "plan" and five named "tasks" would
# collide under the rule that a name matches its filename. Tracked in issue 9,
# which drops this list rather than narrowing it.
EXEMPT = ("features/*/plan.md", "features/*/tasks.md")


def check_tree(root: Path) -> list[str]:
    """Return one message per failure across the tree, empty when it is clean."""
    failures: list[str] = []
    seen: dict[str, Path] = {}

    for path in sorted(root.rglob("*.md")):
        relative = path.relative_to(root)
        if any(relative.match(pattern) for pattern in EXEMPT):
            continue

        fields, problems = _read_frontmatter(path)
        failures.extend(f"{relative}: {problem}" for problem in problems)

        name = fields.get("name")
        expected = _expected_name(relative)
        if name and name != expected:
            failures.append(f"{relative}: name is {name!r}, expected {expected!r}")

        identifier = fields.get("id")
        if identifier is not None and ULID.match(identifier):
            if identifier in seen:
                failures.append(f"{relative}: id is also on {seen[identifier]}")
            else:
                seen[identifier] = relative

    return failures


def _read_frontmatter(path: Path) -> tuple[dict[str, str], list[str]]:
    """Parse the opening block, returning its fields and everything wrong with it."""
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != FENCE:
        return {}, ["does not open with frontmatter"]

    try:
        end = lines.index(FENCE, 1)
    except ValueError:
        return {}, ["frontmatter is never closed"]

    fields: dict[str, str] = {}
    problems: list[str] = []
    for line in lines[1:end]:
        match = FIELD.match(line)
        if not match:
            problems.append(f"cannot parse {line.strip()!r}")
            continue
        key, value = match.group(1), match.group(2)
        if key in fields:
            problems.append(f"{key} appears twice")
        fields[key] = value

    for key in REQUIRED:
        if key not in fields:
            problems.append(f"no {key}")
        elif not fields[key]:
            problems.append(f"{key} is empty")

    date = fields.get("last_updated")
    if date and not DATE.match(date):
        problems.append(f"last_updated is {date!r}, expected YYYY-MM-DD")

    identifier = fields.get("id")
    if identifier and not ULID.match(identifier):
        problems.append(f"id is {identifier!r}, expected a 26-character ULID")

    return fields, problems


def _expected_name(relative: Path) -> str:
    """A README is named for the directory holding it, any other file for itself.

    The convention asks for kebab-case, so BUGS.md is named "bugs": the filename
    decides the name, its capitalisation does not.
    """
    if relative.stem == "README":
        stem = relative.parent.name or "internal"
    else:
        stem = relative.stem
    return stem.lower().replace("_", "-")


def main(argv: list[str]) -> int:
    default = Path(__file__).resolve().parents[2] / "internal"
    root = Path(argv[1]) if len(argv) > 1 else default
    if not root.is_dir():
        print(f"{root} is not a directory", file=sys.stderr)
        return 2

    failures = check_tree(root)
    for failure in failures:
        print(failure, file=sys.stderr)

    if failures:
        print(f"\n{len(failures)} failed", file=sys.stderr)
        return 1

    print(f"every document under {root.name}/ carries its frontmatter")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
