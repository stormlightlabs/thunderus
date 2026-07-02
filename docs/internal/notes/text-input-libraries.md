---
title: Text Input Library Lessons
Sources:
  - https://github.com/unicode-rs/unicode-segmentation
  - https://docs.rs/unicode-segmentation/latest/unicode_segmentation/
  - https://github.com/cessen/ropey
  - https://docs.rs/ropey/latest/ropey/
  - https://github.com/mgeisler/textwrap
  - https://docs.rs/textwrap/latest/textwrap/
Author: unicode-rs maintainers, Ropey maintainers, Textwrap maintainers
Date: 2026-07-02
Captured: 2026-07-02
Tags: [rust, terminal-ui, text-editing, unicode, wrapping, editor]
---

## Summary

These crates fit different layers of text handling: `unicode-segmentation`
teaches correct edit boundaries, `ropey` teaches when a real editor buffer is
worth its cost, and `textwrap` teaches better wrapping choices, but none should
replace the whole prompt model by default.

## Fit For This Project

| Library                | Proposed use              | Fit                                                                      | Why                                                                                                                                                                                                                                          |
| ---------------------- | ------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unicode-segmentation` | Cell measurement          | Partial                                                                  | It does not measure terminal cells. It finds grapheme, word, and sentence boundaries. Pair it with `unicode-width` so cursor movement/deletion respects user-perceived characters while width remains column-based.                          |
| `ropey`                | Prompt/transcript backing | Evaluate before adopting                                                 | Agent prompts can be long enough that a rope may help, but Ropey only solves backing-buffer mutation and indexing. The renderer still has to wrap, measure, style, and place cursors. Prototype with long prompts before changing the model. |
| `textwrap`             | Reflow                    | Maybe for plain transcript prose; not a drop-in for prompt cursor layout | Textwrap has Unicode-aware word wrapping, indentation, refilling, and optional optimal-fit wrapping. Our renderer also needs styled spans, stable cursor coordinates, hard terminal-cell budgets, and predictable snapshots.                 |

## Key Ideas

- **Boundary and width are separate concepts:** Grapheme clusters answer "what
  should move/delete as one user-visible unit?" Display width answers "how many
  terminal cells does it occupy?" We need both.
- **Char indices are better than byte indices but still incomplete:** The current
  `PromptInput` avoids invalid UTF-8 by using char indices. Combining marks,
  emoji sequences, and CRLF-like boundaries still want grapheme-aware editing.
- **A rope pays off when edits are large, scattered, or line-indexed:** Ropey
  tracks char and line positions efficiently across large mutable texts. Long
  agent prompts can hit this shape, especially when users paste plans, logs,
  code, or structured context and then edit near the middle.
- **A rope is not a full editor:** Ropey does not decide grapheme movement,
  terminal cell width, wrapping, styling, cursor coordinates, history, file
  mention semantics, or provider-visible prompt text. Those remain local
  responsibilities.
- **Line semantics are part of the buffer contract:** Ropey treats line breaks
  as part of lines and recognizes several Unicode line endings. This is useful
  as a design lesson even if we keep `String`.
- **Wrapping quality has knobs:** Textwrap distinguishes first-fit wrapping from
  whole-paragraph optimal-fit wrapping. Terminal UI code should choose deliberately:
  stable and local for live input, nicer paragraph reflow for completed prose.
- **Feature flags are part of the dependency decision:** Textwrap can include
  Unicode line breaking, Unicode width, optimal-fit wrapping, terminal-size
  detection, and hyphenation. Pull in only what we need.

## Claims & Evidence

| Claim                                                                       | Support                                                                                                                                                                  | Caveat / Confidence                                                                           |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `unicode-segmentation` is about Unicode text boundaries, not display width. | Its docs describe iterators for grapheme, word, and sentence boundaries according to UAX #29 and expose grapheme/word boundary APIs.                                     | High. Keep `unicode-width` for cell counts.                                                   |
| Grapheme-aware editing would improve prompt correctness.                    | The crate's examples show combining marks grouped into single grapheme clusters, and it exposes `GraphemeCursor` for cursor-based segmentation.                          | High for non-ASCII input; current ASCII/common multibyte tests are not enough.                |
| Ropey is optimized for editor-style buffers.                                | Ropey's README and docs describe it as a UTF-8 rope for text-editor backing buffers, frequent edits, large texts, char-indexed operations, and line indexing.            | High. This may fit long prompt drafts, not only file editors.                                 |
| Ropey is not ideal for tiny prompts.                                        | Ropey's own guidance says small texts pay unnecessary chunk allocation overhead, even though they still work.                                                            | High. This does not rule it out for long agent prompts.                                       |
| Ropey may be useful for long prompt drafts.                                 | Agent prompts can contain pasted plans, logs, code, and context. Ropey provides efficient insertion, removal, slicing, and line lookup for large editable text.          | Medium-high. Needs local benchmark because wrapping/redraw may dominate.                      |
| Ropey is not a good transcript source of truth today.                       | Transcript history is structured entries persisted as JSONL, not one editable text document.                                                                             | High. A rope can be a derived render/search buffer later, not the canonical transcript model. |
| Textwrap can improve prose wrapping.                                        | Textwrap wraps/fills text for terminal output, uses display width by default, supports Unicode line breaking, indentation, refilling, and optional optimal-fit wrapping. | High for plain text; lower for styled spans.                                                  |
| Textwrap is not enough for the live prompt by itself.                       | Our live prompt needs styled mention spans, prefix indentation, exact cursor coordinates, explicit newline rows, and snapshot-stable row construction.                   | High based on local renderer constraints.                                                     |

## Important Terms

| Term                 | Meaning                                                                                                                                  |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Grapheme cluster     | A user-perceived character made from one or more Unicode scalar values, such as a base letter plus combining marks or an emoji sequence. |
| Display width        | The number of terminal cells occupied by text. This is the `unicode-width` layer, not the segmentation layer.                            |
| Char index           | An index in Unicode scalar values. Safer than byte offsets for UTF-8, but not always the same as a user-visible cursor step.             |
| Rope                 | A tree-like text buffer optimized for large text and frequent edits without moving one contiguous string on every change.                |
| Rope slice           | A cheap immutable view into part of a rope.                                                                                              |
| First-fit wrapping   | Greedy line breaking as text is scanned from left to right. Stable and local.                                                            |
| Optimal-fit wrapping | Paragraph-level line breaking that looks ahead to reduce ragged line endings.                                                            |
| Refill               | Reflow already-wrapped text to a new width.                                                                                              |

## Local Application

### Prompt Input

Keep the current `PromptInput` API shape until a benchmark shows Ropey is worth
the extra model, but treat Ropey as a serious candidate for long drafts:

- continue storing prompt text as `String`;
- keep one internal cursor model and one renderer cursor calculation path;
- add grapheme-boundary helpers before adding a rope;
- keep cell width calculation centralized in `renderer::layout`;
- test combining marks, emoji sequences, CJK, zero-width characters, and wide
  characters across insert, delete, backspace, left/right, word movement, wrap,
  and cursor placement.

The smallest useful next step is a boundary abstraction that does not care
whether storage is a `String` or a rope:

```text
text storage: String now, Ropey only if benchmarked
edit cursor: grapheme boundary or char index with validated boundary helpers
cell measurement: unicode-width
visual rows: renderer-owned row builder
```

Ropey should be adopted for prompt input only if a prototype shows a clear win
for realistic long drafts, such as 10 KB, 100 KB, and 1 MB prompts with edits at
the start, middle, and end. Measure:

- insert/delete latency;
- cursor left/right and word movement;
- line up/down movement;
- conversion between cursor storage and byte/char offsets;
- wrapping and cursor-coordinate calculation after edits;
- render tick cost while the prompt is unchanged.

If wrapping and cell measurement dominate, Ropey alone will not fix the user
experience. In that case the better design is cached visual lines or incremental
layout over the existing buffer abstraction.

### Transcript And Session History

Do not make Ropey the transcript backing store right now.

The transcript is semantically a sequence of entries: user, assistant, status,
tool, reasoning, and error. Session persistence is structured JSON lines. A rope
would make sense only for a future feature that edits or searches one large
rendered transcript as text. Until then, use entry records as the source of truth
and generate rows from them.

### Reflow

Textwrap is worth evaluating for completed plain prose rows, especially assistant
text and long status blocks. It is less attractive for the live prompt and styled
rows because our renderer already couples wrapping to spans, backgrounds,
padding, and cursor coordinates.

If we try it, isolate it behind a small function:

```text
wrap_plain_text(text, width) -> Vec<String>
```

Do not let Textwrap become the row model. The row model still owns styles,
padding, truncation, cursor coordinates, and deterministic snapshots.

## Questions For Review

- Which prompt operations should move by grapheme rather than char?
- Should word movement use Unicode word boundaries instead of whitespace-only
  scanning?
- Where do we need display width, and where do we need text boundaries?
- What prompt size and edit pattern would justify a rope instead of `String`?
- What transcript feature would justify a rope-derived view instead of
  structured entries alone?
- Which wrapping mode matters more for assistant prose: stable first-fit or
  nicer optimal-fit paragraphs?
- Can Textwrap handle our desired CJK and emoji line-break behavior better than
  the current custom wrapper without destabilizing snapshots?

## Connections

- Related ideas: Unicode Standard Annex #29, Unicode line breaking, terminal
  cell measurement, editor buffer design, native scrollback, row snapshots.
- Related sources: `docs/internal/specs/v0.md`, `src/input.rs`,
  `src/renderer/layout.rs`, `src/renderer/cursor.rs`.
- Contradictions or tensions: A library can improve correctness while still
  making the code harder to reason about. The first adoption target should be a
  small boundary or wrapping helper, not a replacement editor.
- Useful applications: grapheme-safe editing, Unicode word movement, optional
  plain-prose wrapping, tests that separate storage offsets from display cells.

## Open Questions

- Should the prompt cursor be stored as a grapheme index, a byte index on
  validated grapheme boundaries, or continue as char index with helper methods?
- Should `unicode-segmentation` become a direct dependency, or should we first
  add tests that expose the current char-index limitations?
- Should Textwrap's Unicode line-breaking behavior replace only `wrap_text`, or
  should styled `wrap_spans` stay fully custom?
- Do we need paragraph refilling on terminal resize for already-committed prose,
  or is current re-rendering of semantic entries enough?

## Takeaways

- Use `unicode-segmentation` concepts for edit boundaries, not cell measurement.
- Evaluate Ropey seriously for long prompt drafts, but adopt it only after a
  prototype shows buffer mutation/indexing is the bottleneck.
- Consider Textwrap only behind a narrow plain-text wrapping boundary; keep the
  renderer's row model local and deterministic.
