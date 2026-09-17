---
name: transcript
last_updated: 2026-09-17
id: 01M2RP8M7WPM6D6SMA4TF6ZHKW
---

# Transcript

Every item under `## UI` in `../../BUGS.md` changes what a transcript row
means, and each one could invent its own words for it. This document fixes the
vocabulary they share: activity kinds, outcome data, detail levels, the
disclosure rule, and the closed set of row families. Issues cut from those
items use these names.

The research behind it is
`docs/src/content/docs/docs/notebook/ui.md` and `ui-patterns.md` in the same
directory. Their design lessons hold and are not re-decided here: row-first
rendering over native terminal scrollback, tools and diffs as first-class
events, detail in focused surfaces, and no sidebar until session navigation is
frequent enough to be slow through commands.

## Activity kinds

`ActivityKind` (`cli/renderer/transcript.rs:50`) has four variants today:
`Explore`, `Command`, `Edit`, and `Test`. `ActivitySummary` beside it counts
`reads` and `searches` in separate fields while both collapse into `Explore`,
so the distinction already lives in the data and not in the kind.

The kinds become `Read`, `Search`, `Edit`, `Test`, `Lint`, `Build`, and
`Command`. `Command` stays the fallback for a shell invocation none of the
others claims.

One site owns the mapping. `single_activity_summary`
(`cli/renderer/view/transcript.rs`) classifies by tool name and, for
`run_shell`, by argv through `verification_command`. Lint and build are argv
classifications of that same shape and belong in that function, not in a
second classifier elsewhere.

A kind carries its subject, its counts, its outcome, and its preview.
`ActivityImportance` stays the separate axis it is: a kind says what happened,
importance says whether the durable timeline shows it without disclosure.

## Outcomes are data, not text to read back

`core/tools/shell.rs:188` formats `$ {argv} [{} failed exit {exit_code}
{elapsed_ms}ms]` into the display lines, and `shell_result_metadata`
(`cli/renderer/view/transcript.rs:401-424`) parses the duration and the exit
code back out of those words. The format and the parse are a round trip
through prose that breaks whenever the wording changes.

`ToolOutput` (`thndrs-agent/src/contracts.rs:237`) carries the outcome as
typed fields instead: exit status and elapsed time for process-backed tools.
The renderer reads them, and the display lines stay prose for a person.

The session record carries the same fields at a raised `schema_version`.
`ToolFinished` (`core/session/records.rs:215`) persists `status` and `output`
and nothing else today, so without that change a resumed session shows no exit
code and no duration. That covers every fixture the capture set in
`../tui-verification/plan.md` (`01M2RNSY07A522766KBV2DPR41`) replays, which is
every frame past `startup`. The parse survives as a named fallback for records
written before the bump and applies to nothing written after it.

Test counts are the exception and stay a renderer projection. No tool knows it
ran a test, because `verification_command` classifies a shell command by its
argv, and the only source of a pass or fail count is the runner's own stdout.
That parse stays in one named place.

Failure diagnostics follow the same rule as far as the data reaches. A cause,
a source location, and a code excerpt come from a tool that can report them.
Where only output text exists, the heuristic is named as one and its failure
to match leaves the raw output visible.

## Detail levels and layout density

`Density` (`cli/renderer/layout.rs:17`) is derived from terminal width:
`Comfortable` at 120 columns or more, then `Compact`, then `Cramped`. It
suppresses secondary chrome, and the user does not choose it.

The compact, normal, and expanded levels `../../BUGS.md` asks for are a
different axis, so they get a different name: `Detail`, with the levels
`Compact`, `Normal`, and `Expanded`. `Density` keeps its name and meaning.

The two compose in one direction. Width may suppress chrome at any detail
level; it never changes the level the user chose. Persisting that level is out
of scope, so it is app state for the run.

## Disclosure

The transcript shows a bounded projection and a focused surface owns the rest.
`ActivityProjection` (`cli/renderer/transcript.rs:36`) is the mechanism, with
`Hidden`, `Summary`, and `DisclosedTool` coalescing routine work into one
group summary. It stays as it is.

`Ctrl+O` opens the targeted detail (`cli/app/input.rs:355`, hinted at
`input.rs:222`). Enter opens the same detail on the same target, so the two
keys never disagree about what is open. Neither grows the transcript to hold
full output.

## Row families are closed

`TranscriptRowKind` (`cli/renderer/view.rs:29`) has eleven variants: `User`,
`Assistant`, `Reasoning`, `Skill`, `Tool`, `Edit`, `Diff`, `Status`, `Error`,
`Notice`, and `Cancelled`. New semantics extend `ActivitySummary` and do not
add a family.

A family is a shape a reader learns, and the capture set in
`../tui-verification/plan.md` (`01M2RNSY07A522766KBV2DPR41`) is fixed at twelve
scenarios, so a new family either rides a scenario already in that set or
reopens it. Eleven covers the work `../../BUGS.md` lists.

## Diffs

`cli/renderer/tool_output.rs` projects a tool result into `ContentKind`, and
`cli/renderer/diff.rs` owns the unified-diff projection under three limits:
2,000 lines, 128 KiB, and 4 KiB for one line. One projection serves every tool
that emits a diff.

An adaptive, high-fidelity layout is a rendering decision inside that
projection. Past any of the three limits the transcript shows the summary and
the focused surface owns the content. A diff too large to project is not
rendered as generic tool output.

## What it does not cover

- The prompt surface and the orientation band. Both already carry the
  notebook's vocabulary in `PromptStatusView` and `OrientationBandView`, and
  neither is what the `## UI` items change.
- Themes, palettes, and visual identity. Row structure carries the
  distinction between families; color reinforces it.
- Session navigation, a sidebar, and multi-agent surfaces.
- Streaming cadence and anything else that only moves.
- How a change here is verified. That is `../tui-verification/plan.md`
  (`01M2RNSY07A522766KBV2DPR41`).
