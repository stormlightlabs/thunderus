---
title: "Interactive TUI QA"
---

`thndrs` needs a terminal: run outside one it exits with `No such device or
address`. So every hands-on check goes through a tmux pane, and the capture
from that pane is the record.

## Capturing

```sh
cargo build -p thndrs
cargo run -p thndrs --features dev-fixtures --example session_fixtures
.claude/scripts/tui-capture.sh
```

That writes one `.ansi` per scenario to `target/tui-captures/`, and a `.svg`
beside it where [freeze](https://github.com/charmbracelet/freeze) is installed.
`/target` is ignored, so nothing generated is committed. Pass a scenario name to
capture one, or `--list` to print the set for another process to read.

## The scenarios

`.claude/scripts/tui-scenarios.tsv` holds them, one row each, with the geometry
and environment a row names. Twelve:

| Scenario | Geometry | For |
| --- | --- | --- |
| `startup` | 100x30 | The first frame, with no history behind it. |
| `picker-open` | 100x30 | The session picker. |
| `streaming-mid-tool` | 100x30 | A turn in flight. |
| `tool-output-truncated` | 100x30 | Output past the cap. |
| `permission-prompt` | 100x30 | A prompt waiting on an answer. |
| `error` | 100x30 | A failed turn. |
| `narrow-60-cols` | 60x30 | Reflow. |
| `short-16-rows` | 100x16 | Vertical crowding. |
| `no-color` | 100x30 | `NO_COLOR=1`. |
| `theme-*` | 100x30 | One per `Theme` variant. |

The three `theme-*` rows and `no-color` are identical once escapes are
stripped, and different once rendered, which is why the set is twelve rather
than nine. Issue 33 checks the theme rows against the `Theme` enum, so they
stay in the table rather than in a case statement.

## Driving it by hand

The script captures a scenario. Checking something the table does not name
means driving a pane yourself:

```sh
socket=thndrs-qa
tmux -L "$socket" set-option -g history-limit 5000
tmux -L "$socket" new-session -d -x 100 -y 30 -s probe \
  "sh -c './target/debug/thndrs --ephemeral 2>&1; echo EXIT=$?; exec cat'"
tmux -L "$socket" send-keys -t probe -l '/model'
tmux -L "$socket" send-keys -t probe Enter
tmux -L "$socket" capture-pane -p -e -N -S -5000 -t probe > frame.ansi
tmux -L "$socket" kill-session -t probe
tmux -L "$socket" has-session -t probe 2>/dev/null && echo "teardown failed"
```

Five details have each cost a run.

Use a private server through `-L`. Your own tmux keeps its sessions, its
`history-limit` and its `base-index`, and a capture should neither touch them
nor depend on them.

Raise `history-limit` before the session starts. The renderer draws through
`Viewport::Inline` and moves settled rows out with `insert_before`, so a
capture of the visible pane holds the last turn alone. The rest is in history,
which is why every `capture-pane` here passes `-S`.

Target the session, never `probe:0.0`. Window and pane indexes start at 1 under
`base-index 1`, so a hard-coded `0` fails with `can't find window: 0` on those
machines and nowhere else. A bare session name means its current pane.

Run the binary under `sh -c '...; exec cat'`. A pane whose command exits takes
the pane with it, and tmux then reports `can't find window` instead of saying
why the binary stopped.

Wait by polling until the pane stops changing, which is what the script does. A
fixed interval passes on an idle laptop and fails under load, and a capture of
a half-drawn frame reads as a layout defect rather than as the timing it is.

`-e` keeps color and `-N` keeps trailing styled spaces, so a painted background
that runs to the edge still measures its real width. `send-keys -l` sends text
literally, so a leading slash is not read as a flag.

## Rendering

```sh
freeze frame.ansi -o frame.svg
```

Freeze needs no `--language` for a `.ansi` file and keeps its color, so there
is no strip step. Render to SVG: v0.2.2 segfaults rasterizing PNG in the cloud
container, on plain-text input with no ANSI at all, while SVG succeeds on the
same input.

Rendering is a convenience. A run without freeze on `PATH` still succeeds and
the `.ansi` survives, so a contributor can `cat` it.

**Show the frames.** A capture nobody looks at has checked nothing. Render the
scenarios a change touches and put the images in front of the maintainer in the
session that produced them, before reporting the change as verified. GitHub
renders no ANSI in a comment and takes no upload from an agent, so the pull
request thread carries the stripped text and the image is shown in chat.

Show them even when they look right. Three of the defects on the board were
found by a person looking at a frame, not by the person who captured it: the
row drawn twice at 60 columns, the tense mismatch on a running tool, and the
doubled `$`. None of them failed a test.

Render PNG to show a frame locally, since an image is what a person reads. The
harness writes SVG because v0.2.2 segfaults rasterizing PNG in the cloud
container; locally, `freeze frame.ansi -o frame.png` works and is easier to
look at.

Capture trailing blank rows and they render as dead space, so the script strips
them. Strip them by hand before rendering, or the image shows padding the
application never drew.

## Checking a visual claim

An image invites claims the text can settle, and the text is the one that is
right. Before reporting that two bands are ragged or an element is misaligned,
measure it:

```sh
python3 - frame.ansi <<'PY'
import re, sys
bare = re.compile(r"\x1b\[[0-9;]*m")
for i, line in enumerate(open(sys.argv[1]).read().splitlines(), 1):
    if "48;2;" in line:                      # a painted background
        print(i, len(bare.sub("", line)), bare.sub("", line).strip()[:40])
PY
```

Three rows painting to width 100 and one to 50 is not a ragged band. It is one
element that is half-width, which is a different finding and may be deliberate.

At v0.2.2 freeze drops `\e[3m` italic, `\e[2m` dim, and `\e[7m` reverse, while
rendering bold, underline, every foreground, and truecolor. `ratatui_style`
sets `ITALIC` and `DIM`, so a regression in either leaves the image unchanged
and shows only in the ANSI. A frame correct in the `.ansi` and wrong in the
image is a freeze defect. The application does not change to suit the renderer.

## Session fixtures

Scenarios past `startup` resume a generated fixture, so the same check twice
shows the same frame twice. The generator constructs `SessionRecord` values and
serializes them, which means a change to the record enum breaks the build
rather than a QA run.

The generator is behind the `dev-fixtures` feature, so it stays out of a
released build. With no argument it writes to the workspace root's
`target/tui-fixtures/sessions`, found by walking up to the manifest carrying
`[workspace]`.

Regenerate before each pass. Resuming a session appends to it, and a run also
writes its own session into the directory it was pointed at, so a fixture
directory carried over from the previous pass is no longer the transcript that
pass captured. `--ephemeral` is not the way around that: it refuses `/resume`.

That run's own session is the one thing a fixture cannot settle. The picker
lists it above every fixture under a wall-clock id and activity time, so the
picker frame still differs between two runs. [Issue 50][picker-capture] carries
that.

[picker-capture]: https://github.com/stormlightlabs/thunderus/issues/50
