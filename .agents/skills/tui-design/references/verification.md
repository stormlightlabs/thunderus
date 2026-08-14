# TUI verification

Use this reference to choose proportionate checks for a visual or interaction change. Test the changed behavior, then stop.

## Evidence layers

Each layer answers a different question:

| Layer              | Answers                                                                | Typical tool                                |
| ------------------ | ---------------------------------------------------------------------- | ------------------------------------------- |
| Pure behavior      | Are wrapping, truncation, projection, and state transitions correct?   | focused Rust unit test                      |
| Logical frame      | Are rows, semantic styles, cursor, and bounds correct?                 | `Frame` rendering and `insta`               |
| Ratatui buffer     | Do widgets paint the expected cells in the expected order?             | `TestBackend`                               |
| Terminal stream    | Do escape sequences, incremental frames, and cleanup behave correctly? | `vt100` or a fake backend/writer            |
| Product experience | Is the flow readable, stable, and predictable while used?              | real terminal and deterministic VHS fixture |

Do not use a broad end-to-end capture to replace a small state-transition test. Do not approve a text snapshot as proof that hierarchy, contrast, cursor motion, or animation feels right.

## Select the state matrix

Choose only rows affected by the change, including transitions into and out of them.

| Dimension  | Cases                                                                                                              |
| ---------- | ------------------------------------------------------------------------------------------------------------------ |
| Width      | changed breakpoint - 1, breakpoint, breakpoint + 1; one tiny width; one wide width when metadata expansion changes |
| Height     | composer-only clamp, short transcript, viewport overflow, resized shorter while focused                            |
| Composer   | empty, one line, multiline, wrapped long word, non-ASCII, queued/disabled/error where relevant                     |
| Transcript | following tail, scrolled away, new output while scrolled, tool output truncation, resize while streaming           |
| Focus      | composer, picker/modal, transcript scroll, dismissal, focus restoration                                            |
| Outcome    | pending, running, success, failure, cancelled, retryable                                                           |
| Theme      | project themes, no color, light/dark terminal background if default colors participate                             |
| Input      | press/repeat/release, bracketed paste, arrows/page keys, mouse on/off, terminal selection bypass                   |

Use actual project breakpoint constants and fixture widths. Avoid inventing a universal 80x24 contract.

## Snapshot review

For every changed snapshot:

1. Confirm the change belongs to the request.
2. Read line wrapping and truncation at both edges.
3. Inspect blank cells because their background style affects visible surfaces.
4. Confirm focus and selection have a non-color cue.
5. Check cursor coordinates and visibility.
6. Look for stale content after a shorter frame replaces a taller one.
7. Reject churn caused by timestamps, spinner phases, unordered data, or environment paths.

Keep fixtures deterministic. Freeze time, spinner phase, provider events, and filesystem values where needed.

## Real-terminal review

Treat the current documentation screenshot and VHS tapes as legacy until checked against the running product. Use a deterministic scenario for stable captures only after its content, dimensions, and product state have been refreshed. Interact manually when the change involves input timing or terminal protocols. When a user asks for verification in a background Herdr tab, use that tab for this hands-on product flow; run formatting, linting, and automated tests through the normal command workflow.

Inspect:

- no flicker or stale cells during streaming, resize, and popup dismissal;
- cursor shape, position, visibility, and restoration;
- terminal cleanup after success, error, panic, and `Ctrl+C`;
- paste behavior with multiline and large content;
- terminal text selection with mouse capture off;
- readable hierarchy on at least one light and one dark background;
- legible no-color and reduced-color output;
- SSH/tmux or Windows behavior when the code touches capability detection.

Update `docs/public/screenshot.png` and its VHS source only after the product state is approved. Keep the fixture representative and keep internal test language out of user-facing copy.

## Performance checks

Profile only when the change touches streaming, projection caches, highlighting, images, or animation. Look for:

- redraws while application state is unchanged;
- unbounded queues of frame requests;
- per-frame parsing, filesystem access, syntax loading, or full-transcript cloning;
- work proportional to complete history when only the viewport is visible;
- animation ticks continuing while hidden or complete.

Prefer invalidating a cached pure projection over introducing a second mutable rendering state. Ratatui already diffs buffers before terminal output.

## Completion note

Report:

- the user-visible state or transition changed;
- the focused tests or snapshots run;
- whether a real-terminal or VHS review was necessary and performed;
- any terminal, theme, or platform case that remains unverified.

Do not claim visual polish from passing tests alone.

## References

- [Ratatui snapshot testing with `insta`](https://ratatui.rs/recipes/testing/snapshots/)
- [Ratatui `TestBackend`](https://docs.rs/ratatui/0.30.0/ratatui/backend/struct.TestBackend.html)
- [`insta`](https://docs.rs/insta/latest/insta/)
- [`vt100`](https://docs.rs/vt100/0.16.2/vt100/)
