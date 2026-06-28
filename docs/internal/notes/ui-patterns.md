# Notes: Gridland AI Chat UI Patterns

Source:

- https://www.gridland.io/docs/blocks/ai-chat-interface
- https://github.com/thoughtfulllc/gridland
- https://www.gridland.io/docs/core-concepts/cells-and-layout
- https://www.gridland.io/docs/components/message
- https://www.gridland.io/docs/components/prompt-input
  Author: Thoughtful LLC / Gridland project
  Date: Not specified on docs pages
  Captured: 2026-06-28
  Tags: tui, chat-ui, layout, gridland, ai-agent

## Summary

Gridland's AI chat interface is a compact two-panel terminal-style app: fixed
conversation sidebar, flexible message stream, prompt input with command/file
suggestions, model/status display, and keyboard-first focus handling.

## Key Ideas

- **Two-panel shell:** `SideNav` uses a fixed-width sidebar (`22` columns in the demo)
  and a flexible main panel. For `thndrs`, this maps to a left conversation/session list
  and a right chat/task transcript.
- **Main panel is vertical:** The chat panel is a column: scrollable message area grows,
  prompt input stays pinned at the bottom.
- **Messages are role-styled layout shells:** Gridland's `Message` component owns
  alignment, background, and context; content, markdown, reasoning, tool calls, and sources are composed separately.
- **Prompt input is command-aware:** `PromptInput` supports slash commands, file mentions,
  history, model label, submit/stop status, and an optional command registry.
- **Focus is explicit:** `SideNav` exposes whether the main panel is selected for
  interaction; prompt and modal controls register stable focus IDs.
- **Cells, not pixels:** Layout sizes are character-cell counts or percentages.
  This is directly transferable to Ratatui constraints.

## Claims & Evidence

| Claim                                                                            | Support                                                                                                                            | Caveat / Confidence                                                      |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| The layout should be sidebar + transcript + bottom prompt.                       | Gridland's AI chat demo composes `SideNav`, a scrollbox message area, and `PromptInput` with dividers/model label.                 | High; this is the requested visual reference.                            |
| Messages should stay SDK/runtime agnostic.                                       | Gridland's docs say the consumer maps Vercel AI SDK parts into message subcomponents; `Message` itself does not depend on the SDK. | High; Rust harness should model transcript parts itself and render them. |
| Tool/reasoning UI should be sibling content, not hidden inside a message bubble. | Gridland removed `Message.Reasoning`/`Message.ToolCall` and recommends composing reasoning blocks separately.                      | Medium-high; terminal width may force simpler rendering.                 |
| Prompt should own stop/error/submitted/streaming state.                          | `PromptInput` status controls disabled state, Escape stop handling, and status icon/hint.                                          | High; this maps to a simple Rust `RunStatus` enum.                       |
| Command registry is optional in v0.                                              | Gridland uses `CommandProvider` for `/model` and `/clear`, but `PromptInput` also accepts commands directly.                       | High; start with a static command match in `update`.                     |

## Important Terms

| Term           | Meaning                                                                                         |
| -------------- | ----------------------------------------------------------------------------------------------- |
| SideNav        | Fixed-width keyboard-navigable sidebar with active item and main-panel interaction state.       |
| Message        | Role-aware chat row; user aligns right, assistant aligns left.                                  |
| PromptInput    | Bottom input region with suggestions, history, submit/stop, model label, and optional commands. |
| ChainOfThought | Expandable reasoning block shown separately from normal text content.                           |
| Cell           | Terminal character position; all layout widths, padding, borders, and gaps are integer cells.   |

## Layout Notes for `thndrs`

- Sidebar: fixed `22-28` columns, title optional, rows for `new`, active session,
  recent sessions.
- Transcript: full remaining width, vertical scroll state, newest content near bottom,
  one-row gaps between entries.
- Prompt: bottom block with top divider, prompt marker, editable text, optional
  suggestions, model/status line.
- Status/footer: one line for cwd, model, tokens/cost later, and current mode/status.
- Modal later: model picker or session switcher centered over main area; not needed for
  first playable harness.

## Questions for Review

- Which parts of Gridland's prompt input are essential for v0: slash commands, file
  mentions, history, model label, or stop?
- Should user messages align right in a narrow terminal, or should all messages align
  left for readability?
- Do we need a sidebar before sessions exist, or can it start as a static placeholder?
- How should tool calls render: compact status rows, expandable blocks, or full
  transcript entries?

## Connections

- Related ideas: Ratatui constraints already work in cells, so Gridland's cell-first
  layout transfers cleanly.
- Related sources: Gridland demo file `packages/demo/demos/ai-chat-interface.tsx`, docs
  block `packages/docs/content/docs/blocks/ai-chat-interface.mdx`.
- Contradictions or tensions: Gridland uses React/OpenTUI focus/provider abstractions;
  `thndrs` should avoid recreating those until needed.
- Useful applications: Copy the screen structure, not the framework architecture.

## Open Questions

- Should the sidebar be hidden below a minimum width?
- How much scrollback should be kept in memory before persistence exists?
- Which command set should exist on day one: `/clear`, `/model`, `/quit`, `/help`,
  maybe `/run`?

## Takeaways

- Build the actual first screen as a workbench, not a landing page: sidebar, transcript,
  prompt, footer.
- Keep message rendering composable: normal text, reasoning, tool call, and error entries
  should be separate variants.
- Implement static command suggestions only after basic text input and submit work.
