# Iced Desktop Cookbook

This is a practical guide for building `thndrs-desktop` with Iced 0.14.

The goal is to keep desktop screens consistent with Thunderus design references while preserving a strict Model-View-Update workflow.

## Mental model

### Everything flows through MVU

- **Model**: application state (`conversation`, input/editor state, backend status, UI section state).
- **Update**: a pure state transition per message (`ModelMessage`) that optionally returns effects.
- **View**: a pure projection of model -> widgets.
- **Effects**: side effects (provider calls, tool runs, filesystem/network) happen outside pure update logic.

Use this split for testability:

1. `update_model(&mut Model, Message) -> Vec<Effect>` stays deterministic.
2. App shell (`update`) executes effects and feeds backend events back as messages.

### Use `iced::application` builder

Iced 0.14 bootstraps with:

```rust
iced::application(boot, update, view)
    .title(title)
    .theme(theme)
    .subscription(subscription)
    .window(window_settings)
    .run()
```

- `boot` is `Fn() -> State` or `Fn() -> (State, Task<Message>)`.
- `update` returns `Task<Message>` (or anything convertible).

## Architecture patterns

### 1) Side effects via explicit `Effect` enum

Pattern:

- Update returns `Effect::DispatchPrompt(prompt)`.
- Shell sends prompt to a worker thread/channel.
- Worker emits `BackendEvent`s.
- UI receives them via polling subscription and re-enters update.

This keeps provider/tool orchestration out of view code and keeps unit tests fast.

### 2) Background worker for providers + tools

Recommended worker shape:

1. Spawn thread.
2. Build Tokio runtime in that thread.
3. Initialize `thndrs_core::Config`.
4. Initialize provider via `thndrs_providers::create_provider`.
5. Use `thndrs_providers::ConversationLoop` for tool orchestration (which routes tool calls through `thndrs_tools`).
6. Send narrow backend events to the GUI model.

### 3) Polling subscription for event channels

`std::sync::mpsc::Receiver` is easiest to integrate with:

- `iced::time::every(Duration::from_millis(16))`
- on each tick, drain receiver with `try_recv()`

This avoids custom async stream plumbing for foundation milestones.

## UI patterns (Thunderus desktop)

### Terminal shell composition

Mirror design references with a consistent shell:

1. Header row (traffic lights + title)
2. Scrollable transcript body
3. Input area with top divider

Use `Length::Fill` for body and fixed/min heights for chrome.

### Multi-line input

Use `widget::text_editor::Content` for multi-line editing and auto-growth:

- calculate visible line count from model text
- clamp line count (for example 2..10)
- derive widget height from line count

### Section contract for design fidelity

Encode layout tokens in one place:

- section order: `Intent`, `Actions`, `Result`, `Next`
- prompt symbol
- title
- canonical color hex tokens

Expose this as a static contract and use it in rendering + tests.

## Theming

Use `Theme::custom("Oxocarbon Dark", Palette { ... })` with explicit tokens.

At minimum set:

- `background`
- `text`
- `primary`
- `success`
- `warning`
- `danger`

Set default font with `Application::default_font`. Use JetBrains Mono family name with monospace fallback behavior.

## Testing strategy

### Unit tests should target update logic, not widget internals

Test:

1. submit message -> turn is created, streaming starts, effect emitted.
2. tool-calling/tool-completed events -> action list and statuses update correctly.
3. content delta + done -> assistant text is assembled and persisted to model conversation.
4. layout contract assertions -> design tokens and section order stay stable.

This catches regressions without needing screenshot or integration harnesses.

## Implementation checklist

- [ ] Keep imports at module top.
- [ ] Keep update deterministic (no network/IO directly in pure update function).
- [ ] Keep backend event types small and UI-focused.
- [ ] Keep design tokens centralized.
- [ ] Keep layout split into small rendering helpers per section.
- [ ] Add MVU regression tests whenever a new message/event variant is introduced.
