---
title: "iocraft"
Notes: iocraft
Source: https://docs.rs/iocraft/latest/iocraft/; https://github.com/ccbrown/iocraft
Author: iocraft maintainers
Date: captured 2026-07-04
Tags: [rust, tui, terminal-ui, components, hooks, flexbox]
---


## Summary

iocraft is a Rust terminal UI library that uses a declarative,
React-inspired component model with hooks, flexbox layout, terminal events, and
mock-terminal testing support.

## Key Ideas

- **Declarative UI syntax:** iocraft uses an `element!` macro to describe UI
  trees in a React/SwiftUI-like style.
- **Component model:** Built-in components include `View`, `Text`, and
  `TextInput`; custom components are defined with a `component` macro.
- **Hooks and futures:** Components can use hooks for state and asynchronous
  behavior, including future-driven updates.
- **Flexbox layout:** Layout is powered by Taffy, which makes it relevant to
  terminal layouts that need predictable sizing and alignment.
- **Testing surface:** The API exposes mock terminal configuration and canvas
  rendering concepts, making it useful as a reference for testing interactive
  terminal UI.

## Claims & Evidence

### iocraft targets both static text output and interactive terminal apps.

The crate documentation describes terminal/log output, styled layouts,
interactive elements, event handling, hooks, and fullscreen terminal
applications.

Confidence: high.

### iocraft is explicitly React-inspired.

The docs say developers familiar with React should feel at home and show a
component using state and a future.

Confidence: high.

### iocraft relies on flexbox layout via Taffy.

The docs list flexbox layouts powered by Taffy as a core feature and re-export
Taffy types.

Confidence: high.

## Important Terms

| Term        | Meaning                                                 |
| ----------- | ------------------------------------------------------- |
| `element!`  | Macro for declaratively building UI trees.              |
| `component` | Macro for defining a custom component.                  |
| Hooks       | Component behavior/state primitives.                    |
| Taffy       | Layout engine used for flexbox-style layout.            |
| Canvas      | Intermediate drawing surface before terminal rendering. |

## Questions for Review

- Which iocraft ideas remain useful now that `thndrs` uses a direct renderer?
- Can the component/hook model inform focused surfaces without adopting the
  library?
- Which testing ideas are reusable for row-model or mock-terminal snapshots?
- Where does declarative layout add clarity, and where does it hide terminal
  edge cases?

## Connections

- Related ideas: Ratatui testing, Yoga/Gridland layout, direct renderer row
  models, focused UI surfaces.
- Related sources: `ratatui.md`, `ratatui-testing.md`, `yoga.md`,
  `yoga-gridland.md`.
- Useful applications: UI plan and renderer tests.

## Open Questions

- Should `thndrs` borrow only testing/layout concepts, or revisit component
  composition for focused surfaces?
- How well does iocraft handle native terminal scrollback requirements?
- What examples best map to command picker, help, and settings-like surfaces?

## Takeaways

- iocraft is worth researching for declarative terminal UI concepts.
- Taffy-backed layout and mock-terminal testing are the most relevant pieces.
- The useful extraction is terminal UI structure and tests, not a broader app
  framework.
