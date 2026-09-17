---
name: tui-verification
last_updated: 2026-09-17
id: 01M2RNSY07A522766KBV2DPR41
---

# TUI verification

A pull request that changes the interface regenerates a fixed set of terminal
captures and posts them to the pull request as a comment. That comment is the
first point in the loop where a reviewer sees the frame a change produced
instead of the code that produced it, and it is where the feedback on that frame
goes.

No capture is committed. The frames are evidence attached to one pull request,
read while that pull request is open, and they have no life after it merges. A
tree of `.ansi` files would be an artifact the repository carries, regenerates,
and argues about, and none of that is what a reviewer needs in order to say the
spacing is wrong.

The idea this comes from is `01M2RH1Z3FCE0WBY5H6G9VAGS0`, in
`internal/ideas/tui-screenshot-verification.md`. It decided that captures are
agent-run, that the ANSI text is the record, and that the image is derived from
it. It left three questions open: where comparative captures run, how fixture
sessions get authored, and whether still frames are enough. This document
answers those and fixes the vocabulary the issues use.

## What a capture is

A capture is one named scenario, rendered at one stated geometry, taken from the
pane as the text `tmux capture-pane` produced. A capture set is every scenario
below, regenerated in one pass. Nothing is captured outside the set, so a pull
request cannot choose the frame that flatters it.

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
that enum adds a capture; the scenario list the harness reads and the enum are
checked against each other rather than kept in step by memory.

### Which of them are posted

Nine of the twelve are posted. `Theme` selects a color palette and nothing else:
it is documented as a color theme (`cli/mod.rs:27`), `renderer_palette` maps
each variant to a `Palette` (`cli/renderer/style.rs:215-219`), and the bold,
italic, underline, and dim modifiers are set per span at the call site rather
than per theme. Strip the escapes and the three theme frames are identical text
over the same fixture at the same geometry, so posting them fills the comment
with three copies of one frame and shows a palette regression to nobody.
`no-color` is the same case for a further reason: no renderer path reads
`NO_COLOR` today, and until #52 lands the frame is the default palette under
another name.

Those four are captured and kept in scratch, where they hold their escapes and
a reader can `cat` them or render them with freeze. The comment carries the
nine scenarios in the table above, which differ in structure and so differ in
text. A palette regression is caught by the snapshot tests and by the human
confirmation at `status:verify`, neither of which reads a stripped frame.

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

A frame is evidence about the interface only if the transcript behind it is
fixed. A capture over a transcript that varies run to run shows a reviewer the
transcript, and every disagreement about spacing turns into an argument about
whether the two frames were even of the same thing. So every scenario past
`startup` loads a session fixture and no scenario sends a prompt.
`--session-dir` is a top-level flag rather than a global one
(`crates/thndrs/src/cli/mod.rs:278`), so it goes before any subcommand, and
`/resume <session-id>` already exists
(`crates/thndrs/src/cli/app/commands.rs:80`), so a scratch session directory
holding the fixture, plus a resume, reaches a populated transcript with no model
call. That directory is the `<scratch>` the launch line below passes.

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
thndrs --cwd <workspace> --session-dir <scratch> --model fake-agent --tick-rate-ms 100
```

That line carries no `--ephemeral`, and adding it breaks eleven of the twelve
captures: `resume_session` rejects an ephemeral run outright
(`crates/thndrs/src/cli/app.rs:1613`), so `/resume` cannot reach a fixture. The
scratch session directory the generator writes into is what keeps a capture run
out of the real session store instead.

No new route was needed, and the fake route cannot stand in for a real one.
`ProviderKind::for_model` maps it to `ProviderKind::Fake`
(`crates/thndrs/src/core/agent.rs:65`), which emits scripted events in process.
Tests in `crates/thndrs/src/cli/app/tests/setup.rs` and
`crates/thndrs/src/cli/mod.rs` hold that under an empty `HOME` with no provider
environment variable set, and with the workspace and the home directory kept
apart so the project and global stores are separate files. One reaches the
surface with both stores absent; the other seeds a credential a real provider
would load, and asserts the turn dispatches nothing, leaves that store
byte-identical, and writes nothing to the other.

## Evidence

The evidence is a comment on the pull request. A pull request that touches the
renderer, the app, or the runtime terminal runs the harness and posts one
comment holding the nine frames above; one that touches none of them posts
nothing. The reviewer reads the frames there and replies there. A remark about
spacing, rhythm, or a wrong state belongs next to the frame that shows it, on
the pull request that would change it.

### What the comment holds

The comment carries plain text. Each scenario is a fenced block inside a
collapsed `<details>` whose summary names the scenario and its geometry, so a
comment holding nine frames stays navigable. ANSI escapes are stripped before
posting. `capture-pane -e` writes real ESC bytes, and a code fence renders those
as invisible or replacement characters rather than as color, so leaving them in
costs legibility and buys nothing.

A posted frame carries at most its last 60 rows. A GitHub comment body stops at
65,536 characters, and nine frames 100 columns wide reach that at about 66 rows
each, so an unbudgeted set fails to post the first time it runs against a
populated transcript. Sixty rows across nine frames is roughly 55,000
characters, which leaves room for the markup around them.

The full-height capture stays in scratch and the budget applies only to the
posted copy. This is the one place where the comment is worse than the file it
came from: turn rhythm over a long transcript is exactly what height was for,
and 60 rows is two screens of it. Take it up by splitting the set across two
comments if two screens turns out to be too few.

### Who posts it

The harness does not talk to GitHub. It takes a pull request number, writes the
frames to scratch, and writes the comment body to a file; posting that file is
the caller's, through whichever transport the run is already using. A script
that shelled out to `gh` would break in a cloud session, which has none, and
that is where captures are taken. `github-board` decides the transport, and it
decides it once for the whole run.

The comment is replaced rather than appended. A push that changes the interface
regenerates the body and edits the existing capture comment in place, so the
pull request holds the frames its current head produced and a reviewer never
scrolls past four stale sets to reach them. Review replies stay on their own
threads and survive the edit.

The body opens with `<!-- thndrs-captures -->`, which is how a later run finds
the comment to edit. Matching on the author and a title prefix would find the
review passes' comments too, since those post to the same thread under the same
account.

### What the comment cannot carry

Losing color in the pull request is the accepted cost. GitHub renders no ANSI in
a comment, and there is no route by which an agent uploads an image to one, so
the alternatives are a CI job this track rejects or a committed artifact this
track exists to avoid. Structure, spacing, alignment, truncation, and rhythm all
survive in plain text, and those are what a capture is read for. A defect that
is only visible in color is not covered here and stays with the human
confirmation at `status:verify`.

The harness writes its `.ansi` files to ignored scratch space, and they are what
the posted text is stripped from. They stay on disk after a run for whoever
wants the colored frame: `cat` shows it in a terminal, and freeze renders it to
an image locally. Freeze is a local convenience here
rather than a step in the loop, installed from `.claude/hooks/session-start.sh`
at a pinned version in the report-and-continue style that hook already uses. A
capture run that does not find it posts its comment as usual.

At v0.2.2 freeze drops `\e[3m` italic, `\e[2m` dim, `\e[7m` reverse, and basic
backgrounds, while rendering bold, underline, every foreground, 256-color
backgrounds, and truecolor. `ratatui_style` sets `ITALIC` and `DIM`
(`crates/thndrs/src/cli/renderer/ratatui.rs:77-93`), so a regression in either
leaves a freeze image unchanged. A frame correct in the `.ansi` file and wrong
in the image is a freeze defect. The application does not change to suit the
renderer.

## Who runs it

Captures are agent-run. There is no CI job: a terminal job that flakes costs
more trust than it returns, and a rubric score is a judgment rather than a pass
or a fail. There is no new status label either. `implement` captures and posts,
the review passes read the comment, and `status:verify` keeps whatever the
captures do not cover.

Scoring uses the rubric carried over from `tui-design`: hierarchy, composition,
rhythm, restraint, legibility, state craft, stability, and voice, each scored 0
to 2, any zero blocking completion. The rubric is applied by whoever reads the
captures, and its score belongs in a review comment rather than in a file.

The repository owner is a reader of that comment too, not only the review
passes. A capture set exists so that someone who will not run the binary can
still say the spacing is wrong, on the pull request, while it is still open.

The skills from `archive/x/lndrs` are rewritten rather than restored. What
carries across is the mechanism: a private tmux server through `-L <socket>`,
poll-until-sentinel in place of sleeps, `send-keys -l` for literal text,
`capture-pane -S` for history, and a cleanup block that confirms with
`has-session`. The repository layout and workflow around them do not apply here.
`docs/src/content/docs/docs/development/tui-qa.md` is rewritten to match, since
it currently teaches the two mistakes above.

## Reference harnesses

Comparative captures of other agents run in the container, through the same tmux
geometry as thndrs. A local terminal screenshot is faithful and not
reproducible, and a reference set whose frames differ in font, width, theme, and
zoom compares nothing.

Those captures are not committed either, and they are not posted. They are read
once by whoever writes the comparison. What gets committed is the written
comparison under `internal/qa/`, naming which harness each observation came from
and at what version. The instruction from the archive stands: adopt patterns,
not screenshots.

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

Regression detection. Nothing is stored, so nothing compares this pull
request's frames against the last one's mechanically. A reviewer judges the
frames in front of them against what the interface should look like. Catching a
frame that silently got worse over several pull requests is what the snapshot
tests and `status:verify` are for.

What the frames should show. A capture records what the interface did, and
`../transcript/plan.md` (`01M2RP8M7WPM6D6SMA4TF6ZHKW`) decides the vocabulary a
reviewer reads it against.
