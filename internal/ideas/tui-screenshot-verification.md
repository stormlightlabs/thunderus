---
name: tui-screenshot-verification
last_updated: 2026-09-17
id: 01M2RH1Z3FCE0WBY5H6G9VAGS0
---

# Screenshot-driven TUI verification

## Problem

Nothing in the development loop runs the binary before merge. The three review
passes read the diff. Running behavior is confirmed at `status:verify`, after
the branch has already reached `edge`, and by one person. That queue has no
bound and no second reader.

The cost is visible the first time anyone starts the application. Launching
`target/debug/thndrs` in this session produced an `ATTENTION` row reading
`Skill skipped (session-start-hook): name "startup-hook-skill" must match
parent directory`. No diff review finds that, because nothing in the diff is
wrong.

The UI also needs substantial work that its owner is not doing personally. An
agent asked to improve a terminal interface cannot see the frame it produced.
Without a rendered capture it is editing layout code blind and reporting that
tests pass, which the archived verification reference already warns against:
"Do not claim visual polish from passing tests alone."

Three specific obstacles stand in the way of capturing frames today.

The documented QA procedure cannot see scrollback. `runtime/terminal.rs:170`
builds the terminal with `Viewport::Inline` and commits history through
`insert_before`, so finished rows leave the viewport and enter the terminal's
own history. `docs/src/content/docs/docs/development/tui-qa.md` calls
`capture-pane` without `-S`, which reads the visible pane only. The procedure
is blind to the part of the interface most in need of improvement.

That same procedure waits with fixed sleeps (`sleep 0.2`), which fail under
container load.

No screenshot fixtures exist. Every capture past the composer needs a live
model call, so captures cost money, vary run to run, and cannot be compared
across two versions of the same screen.

## Decisions

The loop is agent-run and never enters CI. A flaky terminal job in `ci.yml`
costs more trust than it returns, and the judgment the loop produces is not
expressible as a pass or fail.

Captures belong to the part of the loop that writes the pull request. The
`implement` skill produces them; the evidence goes in the `## Verification`
section that `commits-and-prs` already defines for pull request bodies; the
`review` passes read it. No new status label.

The captured ANSI text is the record and the image is derived from it. A
reviewer reads both.

Freeze is evidence and never an assertion. Tested at v0.2.2 in the Claude Code
cloud container, its ANSI lexer drops italic (`\e[3m`), dim (`\e[2m`), reverse
(`\e[7m`), and basic backgrounds (`\e[41m`). It renders bold, underline, all
foregrounds, 256-colour backgrounds, and truecolour. `cli/renderer/ratatui.rs`
sets `ITALIC` and `DIM`, so a regression in either is invisible in the image
that is supposed to catch it. A frame that renders correctly in the ANSI
capture and wrongly in freeze is a freeze defect; the application does not
change to suit the renderer.

Determinism comes from session fixtures, not from a new fake provider.
`/resume <session-id>` and `--session-dir` both exist
(`cli/app/commands.rs:80`). A committed fixture session directory gives a
populated transcript with no network call, byte-stable across runs, rendered by
the real renderer. The `ReplayEvaluator` stack in `thndrs-agent` is not this: it
scores agent runs and does not drive the interface.

Capture history, not the viewport. Use `capture-pane -S -<n>` with
`history-limit` raised on the session. Freeze accepts input of any height, so a
few hundred rows of transcript render as one tall image. Turn rhythm and
spacing across many turns are legible there and nowhere else, and they are what
the rubric's composition and rhythm dimensions measure.

Ratatui is the renderer under test. The OpenTUI frontend on `archive/x/lndrs`
is not returning for this purpose. Its `packages/lndrs/tests/fake-backend.ts`
was easy to write because a framed protocol sat between the Rust core and the
frontend, and the current in-process application has no equivalent seam.
Session fixtures substitute for that seam at far lower cost than a rewrite.

Rewrite the two archived skills rather than restore them. `tmux-tui-qa` and
`tui-design` on `archive/x/lndrs` already solve most of the mechanics: a
private tmux server through `-L <socket>`, poll-until-sentinel loops in place
of sleeps, `send-keys -l` for literal text, `capture-pane -S` for history,
freeze and VHS as optional renderers, and a cleanup block that confirms with
`has-session`. They assume a repository layout and a `Herdr` workflow that no
longer apply. Carry the mechanics across; discard the surrounding structure.

Keep the polish rubric from `tui-design/references/harness-patterns.md`
unchanged: hierarchy, composition, rhythm, restraint, legibility, state craft,
stability, and voice, each scored 0 to 2, where any zero blocks completion.
Rewriting a scoring scale that already works would only make two versions of it
exist.

Define one fixed scenario set and capture the same frames every time. Startup,
picker open, streaming mid-tool, truncated tool output, permission prompt,
error, 60-column narrow, 16-row short, no-colour, and one frame per theme. A
design change then produces a comparable before and after. Without a fixed set,
each pull request captures whatever flatters it and quality stops being
comparable across changes, which is the reason for having the loop at all.

Run reference harnesses through the same tmux geometry and the same freeze
invocation as thndrs. Published screenshots differ in font, width, theme, and
zoom, so comparing against them measures presentation rather than design. All
four named references are installable from npm in this container:
`opencode-ai@1.18.31`, `@openai/codex@0.154.0`, and
`@sourcegraph/amp@0.0.1789675234-g2899fe` resolve; `@factory-ai/cli` does not.
Grok Build is Rust source under `xai-org/grok-build` and is read rather than
run. The archived instruction still governs what gets copied: adopt patterns,
not screenshots.

Install freeze from `.claude/hooks/session-start.sh` at a pinned version, in
the report-and-continue style the hook already uses. Local machines have it.
The container does not, but carries Go 1.24.7, and `go install
github.com/charmbracelet/freeze@v0.2.2` completes in about 40 seconds. The
capture path degrades to ANSI text when freeze is missing, so the hook failing
never blocks a review.

## Open

Which `pi` and `grok` packages are meant. The registry holds `@badlogic/pi`
0.1.1, `pi-coder` 0.7.0, `@vibe-kit/grok-cli` 0.0.34, and `grok-cli` 1.0.5, and
these are different projects. Naming the intended two settles it.

Whether comparative captures run in the container or on a local machine. The
container renders through freeze with an embedded JetBrains Mono and no system
font dependency, which makes captures comparable but subject to the attribute
losses above. A local terminal screenshot is faithful and not reproducible.
Deciding which harness produces the reference set settles it.

How fixture session files get authored. Recording a real session and committing
the JSONL is the obvious route and needs a check that no credential, path, or
private transcript content survives. Hand-authoring the JSONL avoids that
entirely if the format is stable enough to write by hand. Reading the session
writer settles it.

Whether `status:verify` narrows once captures reach the pull request. It should
cover less, since part of what it waits for now arrives earlier. Saying which
part is left settles it.

Whether motion needs VHS. Freeze renders a still frame, so streaming, spinner
behaviour, and cursor motion stay unverified. Whether that matters depends on
how much of the intended redesign is animated.

## Sources

- `archive/x/lndrs` at `f6785b7`, files `.agents/skills/tmux-tui-qa/SKILL.md`
  and `.agents/skills/tui-design/references/{harness-patterns,verification}.md`:
  the tmux mechanics, the polish rubric, the evidence layers, and the reference
  harness list.
- `crates/thndrs/src/runtime/terminal.rs:170` and
  `crates/thndrs/src/cli/renderer/inline.rs`: inline viewport and
  `insert_before`, which put committed rows in terminal history.
- `crates/thndrs/src/cli/app/commands.rs:80-86`: `/resume <session-id>` and the
  session picker.
- `crates/thndrs/src/cli/renderer/ratatui.rs:79-90`: the modifiers the renderer
  sets, including `ITALIC` and `DIM`.
- [Freeze](https://github.com/charmbracelet/freeze): the renderer. Attribute
  losses recorded here were measured against v0.2.2 in this container, not
  taken from its documentation.
- [tmux manual](https://man.openbsd.org/tmux.1): `-L`, `capture-pane -e -N -S`,
  `send-keys -l`, `resize-window`.
