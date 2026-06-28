# Notes: Ratatui Application Patterns

Source:

- https://ratatui.rs/concepts/application-patterns/the-elm-architecture/
- https://ratatui.rs/concepts/application-patterns/component-architecture/
- https://ratatui.rs/concepts/application-patterns/flux-architecture/
- https://ratatui.rs/recipes/apps/terminal-and-event-handler/
  Author: Ratatui project
  Date: Not specified on pages
  Captured: 2026-06-28
  Tags: rust, ratatui, tui, elm-architecture, event-loop

## Summary

Ratatui leaves application architecture to the app; for this project, the smallest
durable shape is Elm-style state plus messages, with a thin terminal/event wrapper and
only later splitting into components if local state proves necessary.

## Key Ideas

- **Model, message, update, view:** TEA maps well to Ratatui because terminal apps
  naturally redraw from state, convert input into messages, and mutate state through
  one update path.
- **Actions can chain:** Ratatui's TEA example lets `update` return another message,
  which keeps derived behavior inside the state machine instead of scattering follow-up
  calls through the event loop.
- **Terminal concerns deserve a wrapper:** The terminal/event-handler recipe separates
  raw mode, alternate screen, tick/render timing, resize, paste, mouse, and cleanup from app logic.
- **Components are useful later:** Component architecture co-locates init, event handling,
  update, and render per component, but it adds trait/object structure. For a tiny harness,
  start with functions and split only when the file stops being readable.
- **Flux is probably too much for v0:** Dispatcher/store/view vocabulary is helpful for larger
  apps with multiple data sources, but a separate dispatcher duplicates what a single
  `update(app, msg)` already gives us.

## Claims & Evidence

| Claim                                                               | Support                                                                                                                                                                                       | Caveat / Confidence                                                                 |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| TEA is the best initial pattern for a minimal Rust/Ratatui harness. | The TEA page shows a loop that renders, maps terminal events into messages, and repeatedly calls `update` until no message remains.                                                           | High for v0; less clear once tool execution and session branching grow.             |
| A reusable TUI wrapper is worth building early.                     | The terminal/event-handler recipe handles enter/exit, raw mode, alternate screen, tick/render rates, crossterm events, cancellation, paste/mouse support, suspend/resume, and `Drop` cleanup. | High, but we can implement less than the full recipe initially.                     |
| Component traits should not be the starting point.                  | Component architecture is explicitly more trait/object oriented and gives each component its own event handlers and state.                                                                    | Medium; if input editing gets complex, a small `Prompt` component may be justified. |
| Flux is overkill for the first implementation.                      | Flux adds dispatcher/store/action/view roles for predictable data flow in complex multi-view apps.                                                                                            | High for this repository's current three-file state.                                |

## Important Terms

| Term    | Meaning                                                                                          |
| ------- | ------------------------------------------------------------------------------------------------ |
| Model   | The full UI/application state used to draw the screen.                                           |
| Message | A typed event or intent, usually produced from terminal input, timer events, or background work. |
| Update  | The only place that mutates app state in response to a message.                                  |
| View    | A pure-ish render function from state to Ratatui widgets.                                        |
| Tick    | Periodic event for timers, spinners, queued work, or polling.                                    |
| Render  | A draw request, often separated from tick/input to avoid redrawing on every loop iteration.      |

## Questions for Review

- Why is a single `update` function a better first fit than a component trait hierarchy?
- Which terminal details should be hidden behind a wrapper before app logic is written?
- How can `update` chain follow-up messages without making the event loop know about
  business logic?
- When would this harness need a component boundary?

## Connections

- Related ideas: Gridland's simple two-panel chat layout can be rendered from a single
  model; Pi's event stream maps naturally into app messages.
- Related sources: Ratatui terminal/event-handler recipe, Gridland AI chat block, Pi
  agent-core event flow.
- Contradictions or tensions: Ratatui examples often use alternate-screen full-screen UI;
  Pi argues coding-agent chat works better when preserving native scrollback.
- Useful applications: Use Ratatui's TEA structure for state, but decide separately whether
  the final UX should own the full terminal viewport.

## Open Questions

- Should `thndrs` use alternate screen from the start, or follow Pi's
  append-to-scrollback approach even though Ratatui is usually full-screen?
- Which text editor/input crate should handle multi-line prompt editing, if any?
- How much async infrastructure is needed before real model/tool streaming exists?

## Takeaways

- Start with `App`, `Msg`, `update`, `view`, and a small `run` loop.
- Isolate terminal setup, event polling, cleanup, and tick/render scheduling.
- Defer component traits and Flux-style dispatcher/store structure until the app has
  enough complexity to pay for them.
