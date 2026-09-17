#!/usr/bin/env python3
"""Apply .github/labels.yml to the repository, and remove the labels it retires.

    sync-labels.py                  # show what would change
    sync-labels.py --apply          # make the changes
    sync-labels.py --apply --force  # also delete labels still in use

Creating a label is idempotent: an existing label is updated in place. Deleting
one removes it from every issue and pull request that carries it, which applies
to a retired name as much as to a stock one, so a label still in use is kept and
reported unless --force is given. Retiring a label that issues still carry means
relabelling them first and syncing after.

Needs a token that may write labels, which is why it runs in two places: a local
checkout, or a runner through .github/workflows/labels.yml. The GitHub MCP
surface keeps label writes in a toolset that is off by default, so a cloud
session dispatches that workflow rather than calling this directly.

The manifest is parsed here rather than with a YAML library so the script has no
dependency beyond gh, and in one pass so that every record is validated in one
place. An earlier version split parsing across a mode argument and lost a parse
failure between the halves.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass

GROUP = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*):\s*$")
FIELD = re.compile(r"^\s*(-\s*)?(name|color|description):\s*(.*?)\s*$")

# Stock labels GitHub creates with every repository.
STOCK = (
    "bug",
    "documentation",
    "duplicate",
    "enhancement",
    "good first issue",
    "help wanted",
    "invalid",
    "question",
    "wontfix",
)

# A label name reaches the REST issues endpoint as one entry of a
# comma-separated list, so a comma inside a name would silently mean "carrying
# both halves" and report a use count for something else. Leading or trailing
# whitespace is just as invisible in a picker and in a diff.
FORBIDDEN = {",": "a comma", "\t": "a tab", "\n": "a newline"}


class ManifestError(Exception):
    """The manifest cannot be trusted, so nothing is written."""


@dataclass(frozen=True)
class Label:
    name: str
    color: str
    description: str


def check_name(name: str, where: str) -> None:
    if not name:
        raise ManifestError(f"{where}: a label name is empty")
    if name != name.strip():
        raise ManifestError(f"{where}: {name!r} has leading or trailing whitespace")
    for character, described in FORBIDDEN.items():
        if character in name:
            raise ManifestError(f"{where}: {name!r} contains {described}")


def parse_manifest(path: str) -> tuple[list[Label], list[str]]:
    """Return the labels to create and the names to remove.

    One pass, so a failure anywhere stops the whole run rather than leaving a
    caller to decide which half it applies to.
    """

    def unquote(value: str) -> str:
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            return value[1:-1]
        return value

    records: list[tuple[str | None, dict[str, str]]] = []
    current: dict[str, str] = {}
    group: str | None = None

    def flush() -> None:
        if current:
            records.append((group, dict(current)))
            current.clear()

    with open(path, "rb") as handle:
        text = handle.read().decode("utf-8", errors="replace")

    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        header = GROUP.match(line)
        if header:
            flush()
            group = header.group(1)
            continue
        match = FIELD.match(line)
        if not match:
            continue
        dash, key, value = match.groups()
        if dash and key == "name":
            flush()
            current["line"] = str(number)
        current[key] = unquote(value)
    flush()

    active: list[Label] = []
    retired: list[str] = []
    for group, record in records:
        where = f"{path}:{record.get('line', '?')}"
        if group is None:
            raise ManifestError(f"{where}: a record appears before any group heading")
        name = record.get("name", "")
        check_name(name, where)
        if group == "retired":
            # A retired entry needs only a name; its description says what
            # replaced it, for whoever reads the manifest later.
            retired.append(name)
            continue
        missing = {"name", "color", "description"} - record.keys()
        if missing:
            raise ManifestError(f"{where}: {name!r} is missing {sorted(missing)}")
        active.append(Label(name, record["color"], record["description"]))

    seen: set[str] = set()
    for label in active:
        if label.name in seen:
            raise ManifestError(f"{path}: {label.name!r} is defined twice")
        seen.add(label.name)

    both = seen & set(retired)
    if both:
        raise ManifestError(
            f"{path}: {sorted(both)} appear as both active and retired, so the "
            "sync would create them and then delete them"
        )

    return active, retired


def gh(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["gh", *args],
        capture_output=True,
        text=True,
        check=False,
        stdin=subprocess.DEVNULL,
    )


def labels_present() -> set[str]:
    result = gh("label", "list", "--limit", "200", "--json", "name", "--jq", ".[].name")
    if result.returncode != 0:
        raise ManifestError(f"could not list labels: {result.stderr.strip()}")
    return {line for line in result.stdout.splitlines() if line}


def uses_of(name: str) -> int | None:
    """How many issues and pull requests carry a label, or None if unreadable.

    `gh issue list` omits pull requests, which carry labels just as issues do,
    so the REST issues endpoint is used instead. An answer that is missing or
    not a number must not read as "nothing uses this": deleting a label strips
    it from everything carrying it, so an unreadable count is a refusal.
    """
    result = gh(
        "api",
        "-X",
        "GET",
        "repos/{owner}/{repo}/issues",
        "-f",
        "state=all",
        "-f",
        f"labels={name}",
        "-F",
        "per_page=1",
        "--jq",
        "length",
    )
    if result.returncode != 0:
        return None
    answer = result.stdout.strip()
    return int(answer) if answer.isdigit() else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="make the changes")
    parser.add_argument(
        "--force", action="store_true", help="delete labels that are still in use"
    )
    parser.add_argument(
        "--manifest", default=os.environ.get("MANIFEST", ".github/labels.yml")
    )
    options = parser.parse_args()

    try:
        active, retired = parse_manifest(options.manifest)
    except (ManifestError, OSError) as error:
        print(error, file=sys.stderr)
        return 1

    def run(*args: str) -> bool:
        if not options.apply:
            print(f"  would run: gh {' '.join(args)}")
            return True
        result = gh(*args)
        if result.returncode != 0:
            print(f"  failed: gh {' '.join(args)}: {result.stderr.strip()}", file=sys.stderr)
        return result.returncode == 0

    failed = False

    print(f"creating labels from {options.manifest}")
    for label in active:
        if not run(
            "label",
            "create",
            label.name,
            "--color",
            label.color,
            "--description",
            label.description,
            "--force",
        ):
            failed = True

    print()
    print("removing stock and retired labels")
    # Deduplicated, because a name in both lists would be visited twice and the
    # second delete would fail against a label that is already gone.
    removable = list(dict.fromkeys([*STOCK, *retired]))
    kept: list[str] = []
    try:
        present = labels_present()
    except ManifestError as error:
        print(error, file=sys.stderr)
        return 1

    for name in removable:
        if name not in present:
            continue
        count = uses_of(name)
        if count is None:
            print(f"  skipping {name!r}: could not read what carries it")
            kept.append(f"{name} (unreadable)")
            continue
        if count > 0 and not options.force:
            print(f"  skipping {name!r}: still applied to an issue or pull request (use --force)")
            kept.append(f"{name} (in use)")
            continue
        if not run("label", "delete", name, "--yes"):
            failed = True

    if not options.apply:
        print()
        print("dry run. re-run with --apply to make these changes.")
        return 1 if failed else 0

    # Verify rather than assume. A skipped create or delete is silent otherwise.
    print()
    print("verifying")
    try:
        final = labels_present()
    except ManifestError as error:
        print(error, file=sys.stderr)
        return 1

    for label in active:
        if label.name not in final:
            print(f"  missing: {label.name}")
            failed = True

    kept_names = {entry.split(" (")[0] for entry in kept}
    for name in removable:
        if name in final and name not in kept_names:
            print(f"  still present: {name}")
            failed = True

    if failed:
        print("  label sync incomplete. re-run.")
        return 1

    if kept:
        print(f"  labels match the manifest; kept {len(kept)} label(s):")
        for entry in kept:
            print(f"    {entry}")
        return 0

    print("  labels match the manifest, and no stock or retired label remains.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
