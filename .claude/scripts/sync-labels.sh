#!/usr/bin/env bash
# Apply .github/labels.yml to the repository and remove GitHub's stock labels.
#
#   .claude/scripts/sync-labels.sh            # show what would change
#   .claude/scripts/sync-labels.sh --apply    # make the changes
#
# Creating a label is idempotent: an existing label is updated in place.
# Deleting a stock label removes it from every issue that carries it. This
# script refuses to delete a label that is still in use unless --force is given.

set -euo pipefail

MANIFEST="${MANIFEST:-.github/labels.yml}"
APPLY=0
FORCE=0

for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=1 ;;
    --force) FORCE=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

command -v gh >/dev/null || { echo "gh is required" >&2; exit 1; }
test -f "$MANIFEST" || { echo "missing $MANIFEST" >&2; exit 1; }

# Stock labels GitHub creates with every repository.
STOCK=(
  "bug" "documentation" "duplicate" "enhancement" "good first issue"
  "help wanted" "invalid" "question" "wontfix"
)

# Commands run inside read loops get stdin from /dev/null so they cannot
# consume the loop's input.
run() {
  if [ "$APPLY" -eq 1 ]; then
    "$@" </dev/null
  else
    printf '  would run:'; printf ' %q' "$@"; printf '\n'
  fi
}

labels_present() {
  gh label list --limit 200 --json name --jq '.[].name' </dev/null
}

echo "creating labels from $MANIFEST"
# The manifest has one fixed shape: a group key, then a list of records with
# name, color, and description. Parsed here directly so the script needs no
# YAML library.
parse_manifest() {
  python3 - "$1" <<'PY'
import re
import sys

FIELD = re.compile(r'^\s*(-\s*)?(name|color|description):\s*(.*?)\s*$')


def unquote(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        return value[1:-1]
    return value


records, current = [], {}
for line in open(sys.argv[1], encoding="utf-8"):
    if not line.strip() or line.lstrip().startswith("#"):
        continue
    match = FIELD.match(line)
    if not match:
        continue
    dash, key, value = match.groups()
    if dash and key == "name":
        if current:
            records.append(current)
        current = {}
    current[key] = unquote(value)
if current:
    records.append(current)

for record in records:
    missing = {"name", "color", "description"} - record.keys()
    if missing:
        sys.exit(f"incomplete label record {record}: missing {sorted(missing)}")
    print("\t".join((record["name"], record["color"], record["description"])))
PY
}

parse_manifest "$MANIFEST" | while IFS=$'\t' read -r name color description; do
  run gh label create "$name" --color "$color" --description "$description" --force
done

echo
echo "removing stock labels"
present="$(labels_present)"
for label in "${STOCK[@]}"; do
  if ! grep -qxF "$label" <<<"$present"; then
    continue
  fi
  count=$(gh issue list --state all --label "$label" --limit 1 --json number --jq 'length' </dev/null)
  if [ "$count" -gt 0 ] && [ "$FORCE" -eq 0 ]; then
    echo "  skipping '$label': still applied to at least one issue (use --force)"
    continue
  fi
  run gh label delete "$label" --yes
done

if [ "$APPLY" -eq 0 ]; then
  echo
  echo "dry run. re-run with --apply to make these changes."
  exit 0
fi

# Verify rather than assume. A skipped create or delete is silent otherwise.
echo
echo "verifying"
final="$(labels_present)"
failed=0

while IFS=$'\t' read -r name _ _; do
  grep -qxF "$name" <<<"$final" || { echo "  missing: $name"; failed=1; }
done < <(parse_manifest "$MANIFEST")

for label in "${STOCK[@]}"; do
  if grep -qxF "$label" <<<"$final"; then
    count=$(gh issue list --state all --label "$label" --limit 1 --json number --jq 'length' </dev/null)
    if [ "$count" -eq 0 ]; then
      echo "  still present: $label"
      failed=1
    fi
  fi
done

if [ "$failed" -eq 1 ]; then
  echo "  label sync incomplete. re-run."
  exit 1
fi
echo "  labels match the manifest."
