---
name: tui-verification
last_updated: 2026-09-17
id: 01M2RNSY07A522766KBV2DPR41
---

# TUI verification

A pull request that changes the interface regenerates a fixed set of terminal
captures and commits them. The review passes read the diff between that set and
the set on `edge`. That diff is the first point in the loop where a reviewer
sees the frame a change produced instead of the code that produced it.

The idea this comes from is `01M2RH1Z3FCE0WBY5H6G9VAGS0`, in
`internal/ideas/tui-screenshot-verification.md`. It decided that captures are
agent-run, that the ANSI text is the record, and that the image is derived from
it. It left three questions open: where comparative captures run, how fixture
sessions get authored, and whether still frames are enough. This document
answers those and fixes the vocabulary the issues use.

## What a capture is

A capture is one named scenario, rendered at one stated geometry, stored as the
ANSI text `tmux capture-pane` produced. A capture set is every scenario below,
regenerated in one pass. Nothing is captured outside the set, so a pull request
cannot choose the frame that flatters it.

| Scenario                | Frame                                            |
| ----------------------- | ------------------------------------------------ |
| `startup`               | The first frame, before any input.               |
| `picker-open`           | The session picker over a populated transcript.  |
| `streaming-mid-tool`    | Assistant text arriving while a tool still runs. |
| `tool-output-truncated` | A tool group whose output passed its budget.     |
| `permission-prompt`     | A tool waiting on approval.                      |
| `error`                 | A failed turn and the recovery it offers.        |
| `narrow-60-cols`        | The populated transcript at 60 columns.          |
| `short-16-rows`         | The populated transcript at 16 rows.             |
| `no-color`              | The populated transcript under `NO_COLOR`.       |

One further capture per built-in theme covers `eldritch-minimal`,
`iceberg-dark`, and `catppuccin-mocha`, the three variants of `Theme` in
`crates/thndrs/src/cli/mod.rs:31`. Twelve captures in total. A theme added to
that enum adds a capture; the set and the enum are checked against each other
rather than kept in step by memory.

Geometry is 100 by 30 unless the scenario names another, matching the session
the existing QA page opens. `narrow-60-cols` is 60 by 30 and `short-16-rows` is
100 by 16.

Captures read terminal history, not the visible pane. The renderer draws through
`Viewport::Inline` and moves settled rows out with `insert_before`
(`crates/thndrs/src/runtime/terminal.rs:170`), so a capture without `-S` loses
every turn but the last. Each capture raises `history-limit` and passes
`capture-pane -e -N -S -<n>`. Height is what makes turn rhythm and spacing
across many turns legible, so a tall capture is the point rather than a cost.

The harness waits by polling the pane until a sentinel appears. It never sleeps
for a fixed interval. A `sleep 0.2` passes on an idle laptop and fails under
container load, which produces a capture of a half-drawn frame that reads as a
layout defect.

## Determinism

A capture set is comparable to the one before it only if the transcript behind
it is identical, so every scenario past `startup` loads a session fixture and no
scenario sends a prompt. `--session-dir` is global
(`crates/thndrs/src/cli/mod.rs:278`) and `/resume <session-id>` already exists
(`crates/thndrs/src/cli/app/commands.rs:80`), so a fixture directory plus a
resume reaches a populated transcript with no model call.

Fixtures are generated from typed records, not recorded and not hand-written.
`SessionRecord` is a serde-tagged enum whose every variant carries
`schema_version`, a `seq` that must strictly increase, and an ISO 8601 `time`.
`SessionReader::read_validated_records` rejects a corrupt line, an out-of-order
sequence, or a first record whose `session_id` disagrees with the one being
opened. Hand-written JSONL satisfies those rules until the enum changes, and
then fails at capture time with no compiler warning first. A recorded session
carries whatever was in the real transcript, which needs a redaction check for
credentials, absolute paths, and private content on every refresh.

A generator that serializes `SessionRecord` values avoids both. It compiles
against the enum, so a schema change breaks the build rather than the capture,
and it invents its content, so there is nothing to redact. It writes the fixture
directory at capture time into ignored scratch space. No `.jsonl` file is
committed, which is why the redaction question does not need an answer.

The generator must not add a user-visible command. Whether it lives as an
example, a development binary, or a test helper is an implementation choice.

The harness also has to reach the main surface without a valid credential, since
a capture never sends a request and the container holds no key. First-run setup
opens when the selected provider has no credential
(`crates/thndrs/src/cli/app/onboarding.rs`), and `--model fake-agent` clears
that gate. `selected_provider_missing` returns `None` for any model starting
with `fake-agent` (`onboarding.rs:425`) and `model_uses_unsupported_route`
exempts the same prefix (`crates/thndrs/src/cli/commands/setup.rs:30`), so
configuration loads and no recovery overlay opens. Every capture launches from:

```sh
thndrs --cwd <workspace> --model fake-agent --ephemeral --tick-rate-ms 100
```

Scenario flags extend that line; the `--session-dir` and `/resume` above are
what the populated scenarios add to it.

No new route was needed, and the fake route cannot stand in for a real one.
`ProviderKind::for_model` maps it to `ProviderKind::Fake`
(`crates/thndrs/src/core/agent.rs:65`), which emits scripted events in process,
so a turn taken on it reads no credential, writes none, and sends no provider
request. Tests in `crates/thndrs/src/cli/app/tests/setup.rs` and
`crates/thndrs/src/cli/mod.rs` hold all three: under an empty `HOME` with no
provider environment variable set, startup leaves the setup overlay closed, both
credential stores and `auth.json` stay absent, every supported provider stays
unauthenticated, and the configuration check accepts the model.

## Evidence

The committed captures are a baseline, replaced in place, under
`internal/qa/captures/<scenario>.ansi`. A pull request that touches the
renderer, the app, or the runtime terminal regenerates all twelve and commits
them; one that touches neither commits nothing. The `## Verification` section
that `commits-and-prs` defines names the scenarios whose capture changed and
what the change was, and a review pass that disagrees with that reading has the
diff to argue from.

Replacing in place is what keeps this affordable. Per-pull-request attachments
would grow without bound and could not be diffed, and an image cannot be
reviewed as a diff at all.

Images stay derived. Freeze renders a committed `.ansi` file to a picture for
whoever wants to look at one, and no image is committed. Freeze installs from
`.claude/hooks/session-start.sh` at a pinned version in the report-and-continue
style that hook already uses, and a capture run falls back to ANSI text alone
when it is missing.

At v0.2.2 freeze drops `\e[3m` italic, `\e[2m` dim, `\e[7m` reverse, and basic
backgrounds, while rendering bold, underline, every foreground, 256-color
backgrounds, and truecolor. `ratatui_style` sets `ITALIC` and `DIM`
(`crates/thndrs/src/cli/renderer/ratatui.rs:77-93`), so a regression in either
leaves the image unchanged and shows in the ANSI diff. A frame correct in the
ANSI capture and wrong in the image is a freeze defect. The application does not
change to suit the renderer.

## Who runs it

Captures are agent-run. There is no CI job: a terminal job that flakes costs
more trust than it returns, and a rubric score is a judgment rather than a pass
or a fail. There is no new status label either. `implement` captures, the
review passes read, and `status:verify` keeps whatever the captures do not
cover.

Scoring uses the rubric carried over from `tui-design`: hierarchy, composition,
rhythm, restraint, legibility, state craft, stability, and voice, each scored 0
to 2, any zero blocking completion. The rubric is applied by whoever reads the
captures, and its score belongs in a review comment rather than in a file.

The skills from `archive/x/lndrs` are rewritten rather than restored. What
carries across is the mechanism: a private tmux server through `-L <socket>`,
poll-until-sentinel in place of sleeps, `send-keys -l` for literal text,
`capture-pane -S` for history, and a cleanup block that confirms with
`has-session`. The repository layout and workflow around them do not apply here.
`docs/src/content/docs/docs/development/tui-qa.md` is rewritten to match, since
it currently teaches the two mistakes above.

## Reference harnesses

Comparative captures of other agents run in the container, through the same tmux
geometry and the same freeze invocation as thndrs. A local terminal screenshot
is faithful and not reproducible, and a reference set whose frames differ in
font, width, theme, and zoom compares nothing. The attribute losses above apply
to every harness equally, which is what makes the comparison hold.

Those captures are not committed. What gets committed is the written comparison
under `internal/qa/`, naming which harness each observation came from and at
what version. The instruction from the archive stands: adopt patterns, not
screenshots.

Four harnesses install from npm in this container: `opencode-ai@1.18.31`,
`@openai/codex@0.154.0`, `@sourcegraph/amp@0.0.1789675234-g2899fe`, and
`@earendil-works/pi-coding-agent@0.85.1`. `@factory-ai/cli` does not resolve and
Grok Build ships as Rust source, so both are read rather than run. Read
`@earendil-works/pi-tui` beside its harness, which publishes its terminal UI as
a separate library and so shows its layout decisions in source.

## What this does not cover

Motion. Freeze renders a still frame, so streaming cadence, spinner behavior,
and cursor movement stay unverified by this track and remain with the human
confirmation at `status:verify`. A second renderer for motion is a separate
decision, taken only if the redesign turns out to animate enough to need one.

The OpenTUI frontend on `archive/x/lndrs` stays archived. Ratatui is the
renderer under test. That frontend's `fake-backend.ts` was cheap because a
framed protocol sat between the core and the frontend; the in-process
application has no such seam, and session fixtures substitute for one without a
rewrite.

Snapshot tests, the release checklist in `internal/qa/README.md`, and the
provider smoke tests are all unchanged. A capture set is evidence for a review,
and none of those three is a review.

What the frames should show. A capture records what the interface did, and
`../transcript/plan.md` (`01M2RP8M7WPM6D6SMA4TF6ZHKW`) decides the vocabulary a
reviewer reads it against.
