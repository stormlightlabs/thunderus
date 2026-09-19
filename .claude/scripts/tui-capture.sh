#!/usr/bin/env bash
# Capture thndrs frames from a private tmux server and render them with freeze.
#
#   .claude/scripts/tui-capture.sh              # every scenario
#   .claude/scripts/tui-capture.sh startup      # one of them
#   .claude/scripts/tui-capture.sh --list       # the set, for another process
#
# Writes <name>.ansi to target/tui-captures/, and <name>.svg beside it where
# freeze is installed. /target is ignored, so nothing here is committed.
#
# The application needs a terminal: run outside one it exits with "No such
# device or address". Every capture therefore goes through a pane.
set -uo pipefail

root=$(git rev-parse --show-toplevel)
binary="$root/target/debug/thndrs"
table="$root/.claude/scripts/tui-scenarios.tsv"
out="$root/target/tui-captures"

# A private server. The contributor's own tmux keeps its sessions, its
# history-limit and its base-index, none of which a capture should touch or
# depend on.
socket="thndrs-capture"

# Settled rows leave the viewport: the renderer draws through Viewport::Inline
# and moves them out with insert_before, so a capture of the visible pane holds
# the last turn alone. History is where the rest is, and it has to be raised
# before the session starts.
history=5000

tmuxc() { tmux -L "$socket" "$@"; }

scenarios() { grep -v '^#' "$table" | awk -F'\t' 'NF > 1 { print }'; }

if [ "${1:-}" = "--list" ]; then
  scenarios
  exit 0
fi

if [ ! -x "$binary" ]; then
  echo "no binary at $binary. Run: cargo build -p thndrs" >&2
  exit 1
fi
mkdir -p "$out"

# Workaround for issue 105. A session writer removes its lock on Drop, which a
# killed pane skips, and the lock is an empty file so nothing can tell a dead
# writer from a live one. Captures kill panes by design, so every run would
# otherwise poison the next one: 26 locks accumulated here in one afternoon and
# every resume scenario captured an error frame instead.
#
# Safe because this directory is generated scratch under /target. Delete this
# block once a lock can be checked for liveness.
fixtures="$root/target/tui-fixtures/sessions"
if [ -d "$fixtures" ]; then
  find "$fixtures" -name '*.lock' -delete 2>/dev/null || true
fi

# Poll until the pane stops changing. A fixed wait passes on an idle laptop and
# fails under container load, and a capture of a half-drawn frame reads as a
# layout defect rather than as the timing it is.
settle() {
  local target="$1" previous="" current="" stable=0 i visible
  for i in $(seq 1 100); do
    visible=$(tmuxc capture-pane -p -t "$target" 2>/dev/null | tr -d '[:space:]' | wc -c)
    # An empty pane holds still too. Three identical blank frames once passed
    # for settled and wrote a capture holding one escape character.
    if [ "$visible" -lt 20 ]; then
      stable=0
      previous=""
      command sleep 0.1
      continue
    fi
    current=$(tmuxc capture-pane -p -t "$target" 2>/dev/null | cksum)
    if [ "$current" = "$previous" ]; then
      stable=$((stable + 1))
      [ "$stable" -ge 3 ] && return 0
    else
      stable=0
    fi
    previous="$current"
    command sleep 0.1
  done
  return 1
}

capture_one() {
  local name="$1" cols="$2" rows="$3" env="$4" args="$5" keys="$6"
  local session="cap-$name"
  local ansi="$out/$name.ansi" image="$out/$name.svg"

  tmuxc kill-session -t "$session" 2>/dev/null
  tmuxc set-option -g history-limit "$history" 2>/dev/null

  local prefix=""
  [ "$env" != "-" ] && prefix="env $env "

  # A pane whose command exits takes the pane with it, and tmux then answers
  # "can't find window" instead of saying why the binary stopped.
  tmuxc new-session -d -x "$cols" -y "$rows" -s "$session" \
    "sh -c '${prefix}\"$binary\" $args 2>&1; echo EXIT=\$?; exec cat'" 2>/dev/null

  if ! settle "$session"; then
    echo "  $name: pane never settled" >&2
    tmuxc kill-session -t "$session" 2>/dev/null
    return 1
  fi

  if [ "$keys" != "-" ]; then
    # A checkout with no provider credential opens setup over the transcript,
    # and that overlay takes the keystrokes. Escape closes it. Sending Escape
    # where no overlay is open costs one keystroke and changes nothing.
    tmuxc send-keys -t "$session" Escape
    settle "$session" || true
    # -l sends the text literally, so a leading slash is not read as a flag.
    tmuxc send-keys -t "$session" -l "$keys"
    tmuxc send-keys -t "$session" Enter
    settle "$session" || true
  fi

  # -e keeps color, -N keeps trailing styled spaces so a painted background
  # that runs to the edge still measures full width, and -S reaches history.
  if ! tmuxc capture-pane -p -e -N -S -"$history" -t "$session" > "$ansi" 2>/dev/null; then
    echo "  $name: capture failed" >&2
    tmuxc kill-session -t "$session" 2>/dev/null
    return 1
  fi

  tmuxc kill-session -t "$session" 2>/dev/null
  if tmuxc has-session -t "$session" 2>/dev/null; then
    echo "  $name: session survived teardown" >&2
    return 1
  fi

  # -S asks for more history than exists, and tmux answers with blank rows above
  # the frame; trailing blanks pad it to the pane height below. Neither is
  # something the application drew, and both render as dead space in an image.
  python3 - "$ansi" <<'PY'
import re, sys
path = sys.argv[1]
lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
bare = re.compile(r"\x1b\[[0-9;]*m")
blank = lambda line: not bare.sub("", line).strip()
while lines and blank(lines[-1]):
    lines.pop()
while lines and blank(lines[0]):
    lines.pop(0)
open(path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
PY

  local drawn
  drawn=$(wc -l < "$ansi")
  printf "  %-24s %s rows\n" "$name" "$drawn"

  # SVG, because freeze v0.2.2 segfaults rasterizing PNG in the cloud
  # container on input with no ANSI at all. Rendering is a convenience: a run
  # without freeze still succeeds and the .ansi survives.
  if command -v freeze >/dev/null 2>&1; then
    freeze "$ansi" -o "$image" >/dev/null 2>&1 || echo "  $name: freeze failed" >&2
  fi
  return 0
}

wanted="${1:-}"
found=0
failed=0
# Read the whole table first. A command inside a `while read` loop that takes
# stdin eats the rest of the table, and the run then captures one scenario and
# reports success.
rows_read=$(scenarios)
while IFS=$'\t' read -r name cols rows env args keys; do
  [ -z "$name" ] && continue
  [ -n "$wanted" ] && [ "$wanted" != "$name" ] && continue
  found=1
  capture_one "$name" "$cols" "$rows" "$env" "$args" "$keys" < /dev/null \
    || failed=$((failed + 1))
done <<< "$rows_read"

if [ "$found" = 0 ]; then
  echo "no scenario named '$wanted'. Run --list for the set." >&2
  exit 2
fi
[ "$failed" -gt 0 ] && exit 1
exit 0
