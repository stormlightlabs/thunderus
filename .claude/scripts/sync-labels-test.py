#!/usr/bin/env python3
"""Check sync-labels.py against a stubbed gh and fixture manifests.

    .claude/scripts/sync-labels-test.py

Needs no token and touches no repository: gh is replaced by a stub whose label
set lives in a temporary file, so an apply is checked by what the stub holds
afterwards rather than by what the script says it did.

The cases here are the ones that have actually gone wrong. A malformed manifest
once left the run reporting success while the label it should have removed was
still there, because a parse failure went to stderr and the caller ignored it.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
TOOL = ROOT / "sync-labels.py"

STUB = '''#!/usr/bin/env python3
"""A gh that keeps its label set in a file."""
import os
import sys
from pathlib import Path

state = Path(os.environ["STUB_STATE"])
uses = dict(
    pair.split("=", 1) for pair in os.environ.get("STUB_USES", "").split(";") if pair
)
unreadable = os.environ.get("STUB_UNREADABLE", "")
args = sys.argv[1:]


def labels() -> list[str]:
    return [line for line in state.read_text().splitlines() if line]


if args[:2] == ["label", "list"]:
    print("\\n".join(labels()))
elif args[:2] == ["label", "create"]:
    name = args[2]
    if name not in labels():
        state.write_text("\\n".join([*labels(), name]) + "\\n")
elif args[:2] == ["label", "delete"]:
    name = args[2]
    remaining = [line for line in labels() if line != name]
    if len(remaining) == len(labels()):
        sys.exit(1)  # gh fails when the label is already gone
    state.write_text("\\n".join(remaining) + "\\n")
elif args[:1] == ["api"]:
    name = next(a.split("=", 1)[1] for a in args if a.startswith("labels="))
    if name == unreadable:
        sys.exit(3)
    print(uses.get(name, "0"))
'''

MANIFEST = """\
status:
  - name: "status:queued"
    color: "c5d9ed"
    description: "Ready to work. No owner."

kind:
  - name: "kind:epic"
    color: "1f2933"
    description: "A goal and its stop rule."

retired:
  - name: "run"
    description: "Renamed to kind:epic."
"""

failures: list[str] = []


def run(manifest: str, present: list[str], *flags: str, **stub_env: str):
    """Run the tool against a fresh stub and return (result, labels afterwards)."""
    workspace = Path(tempfile.mkdtemp())
    (workspace / "bin").mkdir()
    stub = workspace / "bin" / "gh"
    stub.write_text(STUB)
    stub.chmod(0o755)
    state = workspace / "labels.txt"
    state.write_text("\n".join(present) + ("\n" if present else ""))
    manifest_path = workspace / "labels.yml"
    manifest_path.write_text(manifest)

    environment = {
        **os.environ,
        "PATH": f"{workspace / 'bin'}:{os.environ['PATH']}",
        "STUB_STATE": str(state),
        **stub_env,
    }
    result = subprocess.run(
        [sys.executable, str(TOOL), "--manifest", str(manifest_path), *flags],
        capture_output=True,
        text=True,
        env=environment,
    )
    after = [line for line in state.read_text().splitlines() if line]
    return result, after


def check(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name}{': ' + detail if detail else ''}")
        failures.append(name)


print("sync-labels")

result, after = run(MANIFEST, ["run"], "--apply")
check(
    "applies the manifest and removes a retired label",
    result.returncode == 0 and sorted(after) == ["kind:epic", "status:queued"],
    f"exit={result.returncode} labels={sorted(after)}",
)

result, after = run(MANIFEST, ["run"])
check(
    "a dry run changes nothing",
    result.returncode == 0 and after == ["run"] and "would run" in result.stdout,
    f"exit={result.returncode} labels={after}",
)

# The failure that prompted this file: a parse error must stop the run, not
# leave it reporting success with the retired label still in place.
broken = MANIFEST.replace('  - name: "run"\n', "  - color: \"ffffff\"\n")
result, after = run(broken, ["run"], "--apply")
check(
    "a malformed record fails the run",
    result.returncode != 0 and after == ["run"],
    f"exit={result.returncode} labels={after}",
)

both = MANIFEST.replace('  - name: "run"', '  - name: "kind:epic"')
result, after = run(both, [], "--apply")
check(
    "a name that is active and retired is rejected",
    result.returncode != 0 and "both active and retired" in result.stderr,
    result.stderr.strip()[:80],
)

for description, bad in (
    ("an empty name", MANIFEST.replace('"run"', '""')),
    ("a name with a comma", MANIFEST.replace('"run"', '"one,two"')),
    ("a name with surrounding space", MANIFEST.replace('"run"', '" run "')),
    ("a record before any group", "  - name: \"loose\"\n" + MANIFEST),
):
    result, _ = run(bad, [], "--apply")
    check(f"{description} is rejected", result.returncode != 0, result.stderr.strip()[:80])

result, after = run(MANIFEST, ["run"], "--apply", STUB_USES="run=2")
check(
    "a retired label still in use is kept",
    result.returncode == 0 and "run" in after and "kept 1 label" in result.stdout,
    f"exit={result.returncode} labels={after}",
)

result, after = run(MANIFEST, ["run"], "--apply", "--force", STUB_USES="run=2")
check(
    "--force deletes a label that is in use",
    result.returncode == 0 and "run" not in after,
    f"exit={result.returncode} labels={after}",
)

result, after = run(MANIFEST, ["run"], "--apply", STUB_UNREADABLE="run")
check(
    "an unreadable use count refuses the delete",
    result.returncode == 0 and "run" in after and "could not read" in result.stdout,
    f"exit={result.returncode} labels={after}",
)

# "bug" is both a stock label and, here, a retired one. Visiting it twice would
# delete it and then fail against a label that is already gone.
duplicated = MANIFEST.replace('  - name: "run"', '  - name: "bug"')
result, after = run(duplicated, ["bug"], "--apply")
check(
    "a name in both the stock and retired lists is deleted once",
    result.returncode == 0 and "bug" not in after,
    f"exit={result.returncode} stderr={result.stderr.strip()[:60]}",
)

result, after = run(MANIFEST, ["status:queued", "kind:epic"], "--apply")
check(
    "a repository already in step reports nothing kept",
    result.returncode == 0 and "no stock or retired label remains" in result.stdout,
    f"exit={result.returncode}",
)

print()
if failures:
    print(f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1)
print("all checks passed")
