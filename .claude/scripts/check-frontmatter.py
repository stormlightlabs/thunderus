#!/usr/bin/env python3
"""Check that every file under internal/ opens with the frontmatter block.

    check-frontmatter.py                  # check internal/ next to this script
    check-frontmatter.py <dir>            # check some other tree
    check-frontmatter.py --since <ref>    # also check no identifier changed

The convention lives in internal/thunderstorm.md: a document opens with a name,
a date, and a ULID that never changes, and an issue cites the document it came
from by that identifier. A document missing one cannot be cited, and two
documents sharing one cite each other's work, so both are failures here.

--since compares each identifier against the same file at a git ref. Uniqueness
within one tree is not immutability across time: an identifier edited in place
leaves a tree that looks clean while every issue citing the old value points at
nothing, and that is the one failure no snapshot of the tree can see.

Only the top level of the block is read. Blank lines and comments are skipped,
an indented line belongs to the key above it and is not inspected, and a key may
carry any value the convention does not constrain. The three keys it does
constrain are top-level scalars, so nothing deeper has to be understood to know
whether they are right. A fully quoted scalar is unquoted first; escapes inside
one are not interpreted. This is not a YAML parser and does not try to be one.

The parsing is deliberate about what it rejects. A key repeated inside one block
is ambiguous rather than merely untidy, and a name that disagrees with the path
means a file was copied and half-edited, which is the failure this catches that
reading the files does not.

Exits non-zero listing every file that fails, so one run reports the whole tree.
A document that cannot be read is that document's failure and not the run's.
"""

from __future__ import annotations

import argparse
import datetime
import re
import subprocess
import sys
from pathlib import Path

FENCE = "---"
REQUIRED = ("name", "last_updated", "id")
FIELD = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*):\s*(.*?)\s*$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

# Crockford base32 without I, L, O, and U, which the alphabet omits so a
# transcribed identifier cannot turn into a different one.
ULID = re.compile(r"^[0-9A-HJKMNP-TV-Z]{26}$")

# The tree is markdown, and the extension is matched case-insensitively so that
# capitalising it cannot walk a document past every check below.
SUFFIX = ".md"

# Per-feature plans and task lists are held back from the naming rule until
# their scheme is decided: five files named "plan" and five named "tasks" would
# collide under the rule that a name matches its filename. Tracked in issue 9,
# which drops this list rather than narrowing it.
#
# The waiver is as narrow as it can be today. These files carry no block at all,
# so presence cannot be required of them until #9 gives them names to carry, and
# a file without one is passed over. A file that has a block is checked like any
# other except for its name: shape, date, and uniqueness all apply, so the day #9
# adds the blocks, a duplicate identifier is caught without anyone remembering to
# come back here.
#
# Each shape is matched against the whole relative path, component by component,
# so the exemption cannot be inherited by a features/ directory somewhere else
# in the tree.
UNNAMED = (("features", "*", "plan.md"), ("features", "*", "tasks.md"))


def check_tree(root: Path) -> tuple[int, list[str]]:
    """Check every document under root, returning how many and what failed."""
    failures: list[str] = []
    seen: dict[str, Path] = {}
    checked = 0

    for path in sorted(_documents(root)):
        relative = path.relative_to(root)

        if path.suffix != SUFFIX:
            failures.append(
                f"{relative}: extension is {path.suffix!r}, expected {SUFFIX!r}"
            )

        fields, problems = _read_frontmatter(path)
        unnamed = _is_unnamed(relative)
        if unnamed and not fields:
            continue

        checked += 1
        failures.extend(f"{relative}: {problem}" for problem in problems)

        name = fields.get("name")
        expected = _expected_name(relative, root)
        if name and not unnamed and name != expected:
            failures.append(f"{relative}: name is {name!r}, expected {expected!r}")

        identifier = fields.get("id")
        if identifier is not None and ULID.match(identifier):
            if identifier in seen:
                failures.append(f"{relative}: id is also on {seen[identifier]}")
            else:
                seen[identifier] = relative

    return checked, failures


def check_history(root: Path, ref: str) -> list[str]:
    """Return a failure for every identifier that differs from the one at ref."""
    repo = _repository(root)
    if repo is None:
        return [f"cannot compare against {ref}: {root} is not inside a git repository"]

    failures: list[str] = []
    for path in sorted(_documents(root)):
        relative = path.relative_to(root)
        tracked = path.resolve().relative_to(repo)

        before = _show(repo, f"{ref}:{tracked.as_posix()}")
        if before is None:
            continue

        was, _ = _parse_block(before.splitlines())
        now, _ = _read_frontmatter(path)
        old, new = was.get("id"), now.get("id")
        if old and new and old != new:
            failures.append(
                f"{relative}: id was {old} at {ref} and is now {new}; "
                "an identifier never changes once assigned"
            )

    return failures


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", nargs="?", help="tree to check; defaults to internal/")
    parser.add_argument("--since", metavar="REF", help="git ref to compare identifiers against")
    args = parser.parse_args(argv[1:])

    default = Path(__file__).resolve().parents[2] / "internal"
    root = Path(args.root) if args.root else default
    if not root.is_dir():
        print(f"{root} is not a directory", file=sys.stderr)
        return 2

    checked, failures = check_tree(root)
    if args.since:
        failures.extend(check_history(root, args.since))

    # Nothing found and everything correct produce the same empty list, and only
    # one of them means the check did its job.
    if not checked:
        failures.append(f"no documents found under {root}")

    for failure in failures:
        print(failure, file=sys.stderr)

    if failures:
        print(f"\n{len(failures)} failed", file=sys.stderr)
        return 1

    noun = "document" if checked == 1 else "documents"
    verb = "carries" if checked == 1 else "carry"
    print(f"{checked} {noun} under {root.name}/ {verb} their frontmatter")
    return 0


def _documents(root: Path) -> list[Path]:
    """Every markdown file in the tree, however its extension is capitalised."""
    return [
        path
        for path in root.rglob("*")
        if path.is_file() and path.suffix.lower() == SUFFIX
    ]


def _is_unnamed(relative: Path) -> bool:
    """Match the whole path against a shape, so the waiver is not a suffix."""
    parts = relative.parts
    return any(
        len(parts) == len(shape)
        and all(want == "*" or want == have for want, have in zip(shape, parts))
        for shape in UNNAMED
    )


def _read_frontmatter(path: Path) -> tuple[dict[str, str], list[str]]:
    """Parse the opening block. A file that cannot be read fails on its own line."""
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        return {}, [f"is not valid UTF-8 ({error.reason} at byte {error.start})"]
    except OSError as error:
        return {}, [f"cannot be read ({error.strerror})"]

    return _parse_block(text.splitlines())


def _parse_block(lines: list[str]) -> tuple[dict[str, str], list[str]]:
    """Read the top level of the opening block, and say what is wrong with it."""
    if not lines or lines[0] != FENCE:
        return {}, ["does not open with frontmatter"]

    try:
        end = lines.index(FENCE, 1)
    except ValueError:
        return {}, ["frontmatter is never closed"]

    fields: dict[str, str] = {}
    problems: list[str] = []
    for line in lines[1:end]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        # Indented, so it continues the key above: a list entry or a nested
        # mapping. The convention constrains no key that can hold one.
        if line[:1].isspace():
            continue

        match = FIELD.match(line)
        if not match:
            problems.append(f"cannot parse {stripped!r}")
            continue
        key, value = match.group(1), _scalar(match.group(2))
        if key in fields:
            problems.append(f"{key} appears twice")
        fields[key] = value

    for key in REQUIRED:
        if key not in fields:
            problems.append(f"no {key}")
        elif not fields[key]:
            problems.append(f"{key} is empty")

    problems.extend(_check_values(fields))
    return fields, problems


def _check_values(fields: dict[str, str]) -> list[str]:
    """Check the two keys whose form is constrained, once they are known present."""
    problems: list[str] = []

    date = fields.get("last_updated")
    if date and not DATE.match(date):
        problems.append(f"last_updated is {date!r}, expected YYYY-MM-DD")
    elif date:
        # The shape is right, which does not make it a date: a check that lets
        # 2026-13-45 through is not checking the thing it exists to check.
        try:
            datetime.date.fromisoformat(date)
        except ValueError:
            problems.append(f"last_updated is {date!r}, which is not a real date")

    identifier = fields.get("id")
    if identifier and not ULID.match(identifier):
        problems.append(f"id is {identifier!r}, expected a 26-character ULID")

    return problems


def _scalar(value: str) -> str:
    """The value a line carries: unquoted, with any trailing comment removed.

    A # opens a comment only when a space precedes it, and never inside quotes.
    Escapes within a quoted scalar are not interpreted, which the convention's
    three keys never need.
    """
    if value[:1] in ("'", '"'):
        closing = value.find(value[0], 1)
        # An unterminated quote is left whole so it fails loudly downstream
        # rather than being silently repaired into something plausible.
        return value[1:closing] if closing != -1 else value
    if value.startswith("#"):
        return ""
    return value.split(" #", 1)[0].rstrip()


def _expected_name(relative: Path, root: Path) -> str:
    """A README is named for the directory holding it, any other file for itself.

    The convention asks for kebab-case, so BUGS.md is named "bugs": the filename
    decides the name, its capitalisation does not.
    """
    if relative.stem == "README":
        stem = relative.parent.name or root.name
    else:
        stem = relative.stem
    return stem.lower().replace("_", "-")


def _repository(root: Path) -> Path | None:
    """The git work tree holding root, or None when there is not one."""
    found = _run(["git", "-C", str(root), "rev-parse", "--show-toplevel"])
    return Path(found.strip()) if found is not None else None


def _show(repo: Path, spec: str) -> str | None:
    """The content of a path at a ref, or None when it was not there."""
    return _run(["git", "-C", str(repo), "show", spec])


def _run(command: list[str]) -> str | None:
    """Run a git command, returning its output or None when it fails."""
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    return result.stdout if result.returncode == 0 else None


if __name__ == "__main__":
    sys.exit(main(sys.argv))
