---
Title: Herdr Agent Multiplexer
Author: Ogulcan Celik / Herdr project
Date: 2026-06-28
Captured: 2026-06-28
Tags: [rust, ratatui, terminal-multiplexer, agents, panes, tui]
---

Source:

- https://herdr.dev/docs/
- https://github.com/ogulcancelik/herdr
- https://herdr.dev/docs/concepts/
- https://herdr.dev/docs/session-state/
- https://herdr.dev/docs/socket-api/
- https://herdr.dev/docs/keyboard/

## Summary

Herdr is a Rust/Ratatui terminal workspace manager for coding agents: it keeps real terminal
processes in panes, adds workspace/tab structure, tracks agent state, and exposes a CLI/socket
control surface for both humans and agents.

## Key Ideas

- **Real terminal panes:** Herdr does not reinterpret agents into a custom chat view.
  A pane is a real terminal process with input, output, cwd, scrollback, and lifecycle.
- **Server/client split:** The server owns pane processes and state; clients attach, detach, and
  reattach.
  This is a major persistence boundary, not just a UI trick.
- **Workspace/tab/pane hierarchy:** Workspaces organize projects or tasks, tabs group layouts, and
  panes hold terminals.
  Agent state rolls up so the sidebar can show which workspace needs attention.
- **Modes protect terminal input:** Terminal mode sends keys to the focused pane;
  prefix mode captures the next key for Herdr;
  navigator/copy/resize/modal modes own input temporarily.
- **Agent state is semantic:** Herdr tracks `blocked`, `working`, `done`, `idle`, and `unknown`.
  These states drive sidebar markers, waits, notifications, and rollups.
- **View geometry is precomputed:** Herdr computes layout rectangles and hit areas into view state
  before rendering, then rendering reads that state. This keeps draw code less stateful and makes
  UI behavior testable.
- **Layout logic is pure and tested:** Its BSP pane layout supports split, focus, swap, resize,
  and directional navigation with focused tests over `Rect` output.
- **Automation has layers:** Herdr offers an agent skill, CLI wrappers, and raw newline-delimited
  JSON socket API.
  Most automation should use CLI wrappers before raw protocol.

## Claims & Evidence

| Claim                                                       | Support                                                                                                                              | Caveat / Confidence                                                                 |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| Prefix mode is the right default for terminal multiplexing. | Herdr docs explain a multiplexer must avoid stealing keys from shells/editors; `prefix+key` reserves one key instead of many.        | High for multiplexers; `thndrs` may not need prefix mode until it embeds terminals. |
| Agent state should be small and semantic.                   | Herdr docs define five states and use them for rollups, waits, notifications, and sidebar display.                                   | High; maps well to a harness transcript/status model.                               |
| Precomputing view geometry improves Ratatui design.         | Herdr has `compute_view` that writes sidebar, tab, terminal, pane, split, toast, and hit-area rectangles before `render`.            | High; useful even in a much smaller app.                                            |
| BSP layout is a good pane model when split panes exist.     | Herdr's `TileLayout` is a tree of `Pane` and `Split` nodes with ratio, focus, split, resize, swap, remove, and directional search.   | Medium for v0; unnecessary until `thndrs` actually has multiple panes.              |
| Server/client persistence is out of scope for v0.           | Herdr's docs distinguish live detach, snapshot restore, pane history replay, native agent restore, and live handoff.                 | High; valuable later, too much for a first harness.                                 |
| Raw socket APIs should be deferred.                         | Herdr docs say most automation should start with CLI wrappers and use raw sockets only for direct request/response or subscriptions. | High; `thndrs` should not start with an IPC protocol.                               |

## Important Terms

| Term             | Meaning                                                                                            |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| Workspace        | Top-level project/task container that owns tabs and panes.                                         |
| Tab              | A layout inside a workspace, often used for agents, logs, server, or review.                       |
| Pane             | A real terminal process rendered and controlled by Herdr.                                          |
| Agent state      | Semantic status: blocked, working, done, idle, unknown.                                            |
| Prefix mode      | A temporary mode where the next key is interpreted by the multiplexer instead of the pane process. |
| BSP layout       | Binary space partition tree used to represent split panes and ratios.                              |
| Snapshot restore | Recreate workspace/tab/pane shape after server restart, without preserving original processes.     |
| Live handoff     | Experimental transfer of live panes from an old server to a new server.                            |

## Open Questions

- Does `thndrs` need true terminal panes, or only a chat transcript and tool-output blocks?
- Should the first key model be direct app keys, or should we start with a prefix-style command
  layer?
- Which agent states are enough for our UI: idle/working/done/error, or do we need blocked
  separately?
- What view geometry should be precomputed in v0: sidebar, transcript, prompt, footer, and hit
  areas?

## Connections

- Related ideas: Ratatui TEA gives the state/update loop; Gridland gives chat layout; Pi gives
  minimal harness philosophy; Herdr gives robust terminal/multiplexer patterns.
- Related sources: Herdr `src/layout.rs`, `src/ui.rs`, docs for concepts/session-state/socket-api/keyboard.
- Contradictions or tensions: Herdr is an agent multiplexer around real terminal processes;
  `thndrs` is intended as a coding harness and should not start by becoming a multiplexer.
- Useful applications: State rollups, modes, geometry precompute, and pure layout tests transfer
  cleanly without inheriting Herdr's full scope.

## Open Questions

- Should future `thndrs` use Herdr externally for multi-agent work instead of building multiplexing internally?
- If tool execution becomes long-running, should it appear as transcript blocks or real terminal
  panes?
- Should session restore be transcript-first like Pi or process-first like Herdr?
