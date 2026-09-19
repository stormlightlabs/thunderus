#!/usr/bin/env python3
"""Check that nothing under .claude/ asks the harness for a worktree.

    check-isolation.py                    # check .claude/ next to this script
    check-isolation.py <dir>              # check some other tree

The `worktree` skill makes every worktree here, outside the repository root so
Cargo cannot reach the parent `.cargo/config.toml` and build into the parent
`target/`. The harness makes them inside the root, and takes the instruction two
ways:

- An `isolation` key in a definition's frontmatter, which the harness reads
  before any skill, so the key wins over every sentence written under it.
- An `isolation` setting on a dispatch, as far as a file can carry one. The
  argument is a tool call, so what is checkable is the text it gets copied
  from: a fenced code block naming the setting.

The key is rejected everywhere rather than allowed for named files. No agent
here could justify an allowlist entry: every worktree this repository wants is
outside the root, which is the one place the harness will not put it.

What no file can hold, the `worktree` skill's **Who gets one** covers.

Exits non-zero listing every file that fails, so one run reports the whole tree.
A file that cannot be read is that file's failure and not the run's.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

KEY = "isolation"
FENCE = "---"

# A key line in the opening block, at any depth.
FIELD = re.compile(r"^(\s*)([A-Za-z][A-Za-z0-9_-]*)\s*:\s*(.*?)\s*$")

# The setting as a dispatch carries it. The bare word is prose and is left alone.
SETTING = re.compile(r"""(\bisolation\b\s*[:=]|["']isolation["'])""")

# ``` or ~~~, three or more, because a longer fence is how a block nests one.
CODE_FENCE = re.compile(r"^\s*(`{3,}|~{3,})")

SUFFIX = ".md"

# A worktree landing here carries a second copy of this tree. Its findings
# would be duplicates, against paths nothing tracks.
SKIP = ("worktrees",)


def check_tree(root: Path) -> tuple[int, list[str]]:
    """Check every definition under root, returning how many and what failed."""
    failures: list[str] = []
    checked = 0

    for path in sorted(_definitions(root)):
        relative = path.relative_to(root)
        checked += 1

        try:
            # utf-8-sig: a BOM would otherwise sit in front of the fence.
            text = path.read_text(encoding="utf-8-sig")
        except UnicodeDecodeError as error:
            failures.append(
                f"{relative}: is not valid UTF-8 "
                f"({error.reason} at byte {error.start})"
            )
            continue
        except OSError as error:
            failures.append(f"{relative}: cannot be read ({error.strerror})")
            continue

        failures.extend(f"{relative}: {problem}" for problem in _problems(text))

    return checked, failures


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", nargs="?", help="tree to check; defaults to .claude/")
    args = parser.parse_args(argv[1:])

    default = Path(__file__).resolve().parents[1]
    root = Path(args.root) if args.root else default
    if not root.is_dir():
        print(f"{root} is not a directory", file=sys.stderr)
        return 2

    checked, failures = check_tree(root)

    if not checked:
        failures.append(f"no definitions found under {root}")

    for failure in failures:
        print(failure, file=sys.stderr)

    if failures:
        print(f"\n{len(failures)} failed", file=sys.stderr)
        return 1

    noun = "definition" if checked == 1 else "definitions"
    verb = "asks" if checked == 1 else "ask"
    print(f"{checked} {noun} under {root.name}/ {verb} the harness for no worktree")
    return 0


def _definitions(root: Path) -> list[Path]:
    """Every markdown file in the tree, however its extension is capitalised."""
    return [
        path
        for path in root.rglob("*")
        if path.is_file()
        and path.suffix.lower() == SUFFIX
        and not _skipped(path.relative_to(root))
    ]


def _skipped(relative: Path) -> bool:
    """Whether the path sits under a directory this check does not own."""
    return any(part in SKIP for part in relative.parts[:-1])


def _problems(text: str) -> list[str]:
    """Every declaration of the setting in one file, with the line that carries it."""
    lines = text.splitlines()
    return _frontmatter_problems(lines) + _code_problems(lines)


def _frontmatter_problems(lines: list[str]) -> list[str]:
    """Read the opening block, if there is one, and report the key.

    An unclosed block is left to `check-frontmatter.py`, which owns block shape.
    The block also has to open the file, as the harness requires of it: reading
    past either would make a `---` in the body look like frontmatter.
    """
    if not lines or lines[0] != FENCE:
        return []
    try:
        end = lines.index(FENCE, 1)
    except ValueError:
        return []

    problems: list[str] = []
    for number, line in enumerate(lines[1:end], start=2):
        match = FIELD.match(line)
        if match and match.group(2) == KEY:
            problems.append(_declared(number, match.group(3) or "(empty)"))
        # A flow mapping, or the key nested in another key's value. The line's
        # own key is ruled out above, so prose in a value does not reach here.
        elif SETTING.search(line):
            problems.append(_declared(number, "(inside this line)"))

    return problems


def _declared(number: int, value: str) -> str:
    """The one message every frontmatter declaration reports, however it is written."""
    return (
        f"line {number}: frontmatter declares {KEY}: {value}. "
        "The harness provisions from this key before any skill is read; "
        "the `worktree` skill makes the worktree instead."
    )


def _code_problems(lines: list[str]) -> list[str]:
    """Report the setting inside a fenced block, which is text somebody runs.

    A fence closes on a run of the same character at least as long as the one
    that opened it, so a block quoting a fence does not end the block early.
    """
    problems: list[str] = []
    opening: str | None = None

    for number, line in enumerate(lines, start=1):
        match = CODE_FENCE.match(line)
        if opening is None:
            if match:
                opening = match.group(1)
            continue

        closing = match.group(1) if match else ""
        if closing and closing[0] == opening[0] and len(closing) >= len(opening):
            opening = None
            continue

        if SETTING.search(line):
            problems.append(
                f"line {number}: code block sets {KEY}. "
                "A dispatch carrying it gets a worktree inside the repository "
                "root; make one through the `worktree` skill instead."
            )

    return problems


if __name__ == "__main__":
    sys.exit(main(sys.argv))
