---
Title: Testing with insta snapshots
Source: https://ratatui.rs/recipes/testing/snapshots/
Author: Ratatui project
Date: 2026-06-28
Captured: 2026-06-28
Tags: [ratatui, testing, snapshots, insta, tui]
---

## Summary

Ratatui UIs can be tested by rendering into a fixed-size `TestBackend` and
snapshotting the backend buffer with `insta`.

## Key Ideas

- **Snapshot tests fit visual terminal output:** Instead of manually asserting
  every cell, render the UI once and compare future output against the saved
  terminal buffer.
- **Use deterministic terminal dimensions:** The recipe uses a fixed `80x20`
  `TestBackend`; stable sizes make snapshots reproducible.
- **Keep rendering testable:** The app or widget should expose a render path
  that can draw into a `Frame` without requiring a real terminal.
- **Review changes intentionally:** When output changes, use `cargo insta
review` to inspect and accept the new snapshot.

## Claims & Evidence

| Claim                                                   | Support                                                                                                           | Caveat / Confidence                                                              |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Snapshot tests reduce brittle manual visual assertions. | The recipe positions snapshots as a way to capture reference values once and compare future runs.                 | High. Still needs focused state setup so snapshots do not become huge and noisy. |
| `TestBackend` is the core Ratatui testing primitive.    | The example builds `Terminal::new(TestBackend::new(80, 20))`, draws the app, then snapshots `terminal.backend()`. | High.                                                                            |
| Snapshot files live under `snapshots/`.                 | The recipe shows generated `.snap` files containing the rendered terminal rows.                                   | High. Exact path depends on module/test names.                                   |
| Snapshot review is part of the workflow.                | The recipe recommends `cargo insta review` when UI changes are intentional.                                       | High.                                                                            |

## Important Terms

| Term          | Meaning                                                                   |
| ------------- | ------------------------------------------------------------------------- |
| `TestBackend` | Ratatui backend that stores rendered cells in memory for tests.           |
| `insta`       | Rust snapshot-testing crate used to compare rendered output.              |
| `cargo-insta` | CLI helper for reviewing and accepting snapshot changes.                  |
| Snapshot      | Saved expected output used as the comparison target for future test runs. |

## Open Questions

- Why should TUI snapshot tests use fixed terminal dimensions?
- What object should be passed to `assert_snapshot!` after drawing a Ratatui UI?
- When should `cargo insta review` be used?
- What parts of `thndrs` should be tested with pure state tests instead of snapshots?

## Connections

- Related ideas: Elm-style update tests, Ratatui `TestBackend`, render functions
  separated from terminal setup.
- Related sources: Ratatui Elm architecture notes, Ratatui terminal/event-handler
  recipe.
- Useful applications: normal-width chat layout, narrow layout with hidden
  sidebar, transcript entries for assistant/tool/reasoning states, provider
  error rendering.

## Open Questions

- Should `thndrs` snapshot whole screens or smaller render regions first?
- Do we want `insta` redactions for timestamps/session IDs once persistence
  exists?
- Should CI require committed snapshots immediately, or only after the layout
  stabilizes?

## Notable Quotes

> "Use a consistent terminal size"
