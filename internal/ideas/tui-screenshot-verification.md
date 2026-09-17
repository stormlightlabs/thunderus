---
name: tui-screenshot-verification
last_updated: 2026-09-17
id: 01M2RH1Z3FCE0WBY5H6G9VAGS0
---

# Screenshot-driven TUI verification

## Problem

No review pass runs the binary. All three read the diff, and running behavior
is confirmed at `status:verify`, after the branch reaches `edge`, by one
person.

Starting the application once during this session printed `Skill skipped
(session-start-hook): name "startup-hook-skill" must match parent directory`.
Nothing in the diff that introduced it was wrong, so no reading of that diff
would have found it.

The interface needs work that its owner will not do personally, and an agent
editing layout code cannot see the frame it produced. It reports that tests
pass, which the archived verification reference already rules out: "Do not
claim visual polish from passing tests alone."

Capturing frames today runs into three obstacles.
`docs/src/content/docs/docs/development/tui-qa.md` calls `capture-pane` with no
`-S`, which reads the visible pane, while `runtime/terminal.rs:170` renders
through `Viewport::Inline` and moves finished rows into terminal history with
`insert_before`. That document also waits with `sleep 0.2`, which fails under
container load. No session fixtures exist either, so a capture past the
composer costs a live model call and differs from the one before it.

## Decisions

### Where it runs

- Agent-run only. A flaky terminal job in `ci.yml` costs more trust than it
  returns, and a rubric score is not a pass or a fail.
- The `implement` skill captures; the evidence fills the `## Verification`
  section that `commits-and-prs` already defines; the `review` passes read it.
- No new status label. `status:verify` keeps whatever the captures do not
  cover.

### What counts as evidence

- The ANSI capture is the record. The image is derived from it, and a reviewer
  reads both.
- Freeze renders evidence, not assertions. At v0.2.2 it drops `\e[3m` italic,
  `\e[2m` dim, `\e[7m` reverse, and `\e[41m` basic backgrounds, while rendering
  bold, underline, every foreground, 256-color backgrounds, and truecolor.
  `cli/renderer/ratatui.rs:79-90` sets `ITALIC` and `DIM`, so a regression in
  either leaves the image unchanged.
- A frame correct in the ANSI capture and wrong in freeze is a freeze defect.
  The application does not change to suit the renderer.
- Freeze installs from `.claude/hooks/session-start.sh` at a pinned version, in
  the report-and-continue style the hook already uses. `go install
  github.com/charmbracelet/freeze@v0.2.2` takes about 40 seconds against the
  Go 1.24.7 the container ships. Captures fall back to ANSI text when it is
  missing.

### How captures stay comparable

- Session fixtures supply determinism. `/resume <session-id>` and
  `--session-dir` already exist (`cli/app/commands.rs:80`), so a committed
  fixture directory gives a populated transcript with no network call. The
  `ReplayEvaluator` stack in `thndrs-agent` scores agent runs and does not
  drive the interface.
- Capture with `capture-pane -S -<n>` and a raised `history-limit`. Freeze
  accepts input of any height, so a few hundred rows render as one tall image,
  which is where turn rhythm and spacing across many turns become legible.
- One fixed scenario set, captured every time: startup, picker open, streaming
  mid-tool, truncated tool output, permission prompt, error, 60-column narrow,
  16-row short, no-color, and one frame per theme. Otherwise each pull request
  captures whatever flatters it.
- Reference harnesses run through the same tmux geometry and the same freeze
  invocation as thndrs, because published screenshots differ in font, width,
  theme, and zoom. Four install from npm in this container:
  `opencode-ai@1.18.31`, `@openai/codex@0.154.0`,
  `@sourcegraph/amp@0.0.1789675234-g2899fe`, and
  `@earendil-works/pi-coding-agent@0.85.1`. `@factory-ai/cli` does not resolve,
  and Grok Build ships as Rust source under `xai-org/grok-build`, so both get
  read rather than run.
- Read `@earendil-works/pi-tui` alongside its harness. Pi publishes its
  terminal UI as a separate library with differential rendering, which makes
  its layout decisions legible in source rather than inferred from a frame.
  The archived instruction holds: adopt patterns, not screenshots.

### What the interface is

- Ratatui is the renderer under test. The OpenTUI frontend on
  `archive/x/lndrs` stays archived and will belong to something else later.
- Its `packages/lndrs/tests/fake-backend.ts` was cheap to write because a
  framed protocol sat between the Rust core and the frontend. The in-process
  application has no equivalent seam, and session fixtures substitute for one
  without a rewrite.
- Rewrite `tmux-tui-qa` and `tui-design` rather than restore them. Carry across
  the private tmux server through `-L <socket>`, poll-until-sentinel loops in
  place of sleeps, `send-keys -l` for literal text, `capture-pane -S` for
  history, and the cleanup block that confirms with `has-session`. Their
  repository layout and `Herdr` workflow no longer apply.
- Keep the polish rubric from `tui-design/references/harness-patterns.md` as
  it stands: hierarchy, composition, rhythm, restraint, legibility, state
  craft, stability, and voice, each scored 0 to 2, any zero blocking
  completion.

## Open questions, now decided

All three are answered in `../features/tui-verification/plan.md`
(`01M2RNSY07A522766KBV2DPR41`). They stay here as the record of what that spec
had to settle.

Whether comparative captures run in the container or on a local machine. Freeze
embeds JetBrains Mono and depends on no system font, so container captures are
comparable to each other but carry the attribute losses above. A local terminal
screenshot is faithful and not reproducible. Pick which one produces the
reference set.

How fixture session files get authored. Recording a real session and committing
the JSONL needs a check that no credential, path, or private transcript content
survives. Hand-authoring the JSONL avoids that if the format is stable enough
to write by hand. Read the session writer to find out.

Whether motion needs VHS. Freeze renders a still frame, leaving streaming,
spinner behavior, and cursor movement unverified. How much that matters depends
on how much of the redesign animates.

## Sources

- `archive/x/lndrs` at `f6785b7`, files `.agents/skills/tmux-tui-qa/SKILL.md`
  and `.agents/skills/tui-design/references/{harness-patterns,verification}.md`:
  the tmux mechanics, the polish rubric, the evidence layers, and the reference
  harness list.
- `crates/thndrs/src/runtime/terminal.rs:170` and
  `crates/thndrs/src/cli/renderer/inline.rs`: the inline viewport and
  `insert_before`, which put finished rows in terminal history.
- `crates/thndrs/src/cli/app/commands.rs:80-86`: `/resume <session-id>` and the
  session picker.
- `crates/thndrs/src/cli/renderer/ratatui.rs:79-90`: the modifiers the renderer
  sets, including `ITALIC` and `DIM`.
- [Freeze](https://github.com/charmbracelet/freeze): the renderer. The
  attribute losses recorded here were measured against v0.2.2 in this
  container, not read from its documentation.
- [tmux manual](https://man.openbsd.org/tmux.1): `-L`, `capture-pane -e -N -S`,
  `send-keys -l`, and `resize-window`.
- [Pi](https://github.com/earendil-works/pi) and
  [Grok Build](https://github.com/xai-org/grok-build): the two reference
  harnesses named on the pull request. Pi publishes `pi-coding-agent` and
  `pi-tui` to npm; Grok Build is source only.
