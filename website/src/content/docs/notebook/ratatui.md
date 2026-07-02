---
title: "Ratatui Application Patterns"
Author: Ratatui project
Date: 2026-06-28
Captured: 2026-06-28
Tags: [rust, ratatui, tui, elm-architecture, event-loop]
---

Source:

- https://ratatui.rs/concepts/application-patterns/the-elm-architecture/
- https://ratatui.rs/concepts/application-patterns/component-architecture/
- https://ratatui.rs/concepts/application-patterns/flux-architecture/
- https://ratatui.rs/recipes/apps/terminal-and-event-handler/

## Summary

Ratatui leaves application architecture to the app. The durable lesson is that
terminal agent apps stay simpler when state, messages, update logic, terminal
I/O, and rendering boundaries are explicit, even if the final renderer does not
use Ratatui widgets directly.

## Key Ideas

- **Model, message, update, view:** TEA maps well to Ratatui because terminal apps
  naturally redraw from state, convert input into messages, and mutate state through
  one update path.
- **Actions can chain:** Ratatui's TEA example lets `update` return another message,
  which keeps derived behavior inside the state machine instead of scattering follow-up
  calls through the event loop.
- **Terminal concerns deserve a wrapper:** The terminal/event-handler recipe separates
  raw mode, alternate screen, tick/render timing, resize, paste, mouse, and cleanup from app logic.
- **Components are useful when local state deserves ownership:** Component
  architecture co-locates init, event handling, update, and render per component,
  but it adds trait/object structure. Split when it makes state ownership clearer,
  not just because a framework pattern exists.
- **Flux is often too much for a small harness:** Dispatcher/store/view vocabulary
  is helpful for larger apps with multiple data sources, but a separate
  dispatcher can duplicate what a single `update(app, msg)` already provides.

## Claims & Evidence

| Claim                                                               | Support                                                                                                                                                                                       | Caveat / Confidence                                                                 |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| TEA is a good base pattern for a Rust terminal harness.             | The TEA page shows a loop that renders, maps terminal events into messages, and repeatedly calls `update` until no message remains.                                                           | High; complex side effects may still need clear async/event boundaries.             |
| A reusable TUI wrapper is worth building early.                     | The terminal/event-handler recipe handles enter/exit, raw mode, alternate screen, tick/render rates, crossterm events, cancellation, paste/mouse support, suspend/resume, and `Drop` cleanup. | High, but we can implement less than the full recipe initially.                     |
| Component traits should not be the starting point.                  | Component architecture is explicitly more trait/object oriented and gives each component its own event handlers and state.                                                                    | Medium; if input editing gets complex, a small `Prompt` component may be justified. |
| Flux is often overkill before multiple stores exist.                 | Flux adds dispatcher/store/action/view roles for predictable data flow in complex multi-view apps.                                                                                            | High; reconsider only when one update path becomes a real bottleneck.               |

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
  - Start with one update path because it keeps state transitions obvious until independent component state actually appears.
- Which terminal details should be hidden behind a wrapper so app logic stays
  testable?
  - Hide raw mode, screen setup, event polling, terminal size, cleanup, and backend writes behind a thin wrapper.
- How can `update` chain follow-up messages without making the event loop know about
  business logic?
  - Let `update` return follow-up messages and keep the event loop responsible only for delivery.
- When would this harness need a component boundary?
  - Add a component boundary when a subview owns enough state and behavior that keeping it inline makes the app harder to reason about.

## Connections

- Related ideas: Gridland's simple two-panel chat layout can be rendered from a single
  model; Pi's event stream maps naturally into app messages.
- Related sources: Ratatui terminal/event-handler recipe, Gridland AI chat block, Pi
  agent-core event flow.
- Contradictions or tensions: Ratatui examples often use alternate-screen full-screen UI;
  Pi argues coding-agent chat works better when preserving native scrollback.
- Conceptual use: borrow TEA structure for state, while deciding separately
  whether the final UX owns the full terminal viewport or preserves native
  scrollback.

## Open Questions

- When should component boundaries be introduced?
  - **Recommendation**: Use a single update/message path with a thin terminal wrapper until splitting components makes state ownership clearer rather than just more abstract.
- Which text editor/input crate should handle multi-line prompt editing, if any?
  - **Recommendation**: Keep input editing local until a crate solves a proven editing gap without taking over rendering.
- How much async infrastructure is needed before real model/tool streaming exists?
  - **Recommendation**: Add only enough async structure to deliver typed events into the update loop and defer broader task orchestration.
