---
Title: Gridland AI Chat UI Patterns
Author: Thoughtful LLC / Gridland project
Date: 2026-06-28
Captured: 2026-06-28
Tags: [tui, chat-ui, layout, gridland, ai-agent]
Sources:
  - https://www.gridland.io/docs/blocks/ai-chat-interface
  - https://github.com/thoughtfulllc/gridland
  - https://www.gridland.io/docs/core-concepts/cells-and-layout
  - https://www.gridland.io/docs/components/message
  - https://www.gridland.io/docs/components/prompt-input
---

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
- **Bottom-sticky transcript:** The AI chat demo uses a scrollbox with bottom sticky
  behavior, `paddingX={1}`, `paddingBottom={1}`, and a one-cell gap between messages.
  The transferable lesson is to keep newest content pinned while preserving
  intentional breathing room.
- **Role shells, not labels only:** `Message` aligns user content to the right and
  assistant content to the left, with message content bounded to about `85%` width and
  padded inside the bubble. Tool/reasoning rows remain siblings rather than being hidden
  inside message bubbles.
- **Prompt is a compound surface:** `PromptInput` separates divider, suggestions,
  textarea, status text, submit/status icon, and model label. Dividers intentionally
  extend into gutter space so the prompt reads as one full-width control.
- **Sidebar has focus and shortcut semantics:** `SideNav` distinguishes active/focused
  item from selected/interacting main-panel state and renders a compact shortcut/status
  bar (`↑↓ navigate`, `enter select`, `esc back`).

## Claims & Evidence

| Claim                                                                            | Support                                                                                                                            | Caveat / Confidence                                                   |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Sidebar + transcript + bottom prompt is a proven chat shell.                     | Gridland's AI chat demo composes `SideNav`, a scrollbox message area, and `PromptInput` with dividers/model label.                 | High as a reference shape; specific products may hide the sidebar.    |
| Messages should stay SDK/runtime agnostic.                                       | Gridland's docs say the consumer maps Vercel AI SDK parts into message subcomponents; `Message` itself does not depend on the SDK. | High; the transcript model should own semantic parts and render them. |
| Tool/reasoning UI should be sibling content, not hidden inside a message bubble. | Gridland removed `Message.Reasoning`/`Message.ToolCall` and recommends composing reasoning blocks separately.                      | Medium-high; terminal width may force simpler rendering.              |
| Prompt should own stop/error/submitted/streaming state.                          | `PromptInput` status controls disabled state, Escape stop handling, and status icon/hint.                                          | High; this maps cleanly to a small run-status state.                  |
| A command registry is optional until commands become user-extensible.            | Gridland uses `CommandProvider` for `/model` and `/clear`, but `PromptInput` also accepts commands directly.                       | High; direct command matching stays simpler for a small command set.  |
| Transcript spacing should be deliberate, not accidental.                         | Gridland's message area uses one-cell horizontal padding, bottom padding, and one-cell vertical gaps between message groups.       | High; spacing changes should be captured intentionally in snapshots.  |
| Prompt dividers can carry focus/status.                                          | Gridland's prompt divider accepts color and dashed/solid style, with status-specific prompt icons.                                 | Medium-high; useful for streaming/error focus states in terminal UIs. |

## Source-Level Details To Borrow Conceptually

- Message rows should be grouped by semantic block: user/assistant message, reasoning,
  tool result, source/result metadata. Within a group, content can wrap, but the group's
  left edge and gutter should not shift between streaming and completed states.
- User message content can be visually distinct without forcing every transcript row into
  the same fixed role-label grid. A bounded right-aligned or indented user block may scan
  better than `User (name)` plus a long fixed spacer.
- Reasoning should stay a sibling block with its own header/status (`Thinking`, `Thought
for`, running/done), not a normal assistant message row.
- Prompt suggestions should appear above the prompt text, not in the transcript, and should
  support slash commands plus file mentions if those features are exposed.
- The prompt should preserve input on async submit failure so the user can retry.
- The sidebar should expose keyboard intent: navigation, selecting the main panel for
  interaction, and returning to the sidebar.
- Sidebar session rows should reserve space for suffixes/badges such as dirty, running,
  failed, or unread state.
- Layout should treat borders as part of the cell budget; every fixed width should include
  border and padding in its accounting.

## Important Terms

| Term           | Meaning                                                                                         |
| -------------- | ----------------------------------------------------------------------------------------------- |
| SideNav        | Fixed-width keyboard-navigable sidebar with active item and main-panel interaction state.       |
| Message        | Role-aware chat row; user aligns right, assistant aligns left.                                  |
| PromptInput    | Bottom input region with suggestions, history, submit/stop, model label, and optional commands. |
| ChainOfThought | Expandable reasoning block shown separately from normal text content.                           |
| Cell           | Terminal character position; all layout widths, padding, borders, and gaps are integer cells.   |

## Open Questions

- Which parts of Gridland's prompt input are essential for the active product:
  slash commands, file mentions, history, model label, or stop?
  - **Recommendation**: Copy Gridland's cell-budget discipline and prompt/transcript structure first, then add sidebar, commands, and richer prompt states only when active workflows need them.
- Should user messages align right in a narrow terminal, or should all messages align
  left for readability?
  - **Recommendation**: Prefer left alignment in narrow terminals and reserve right alignment for widths where it remains readable.
- Does the product need a sidebar, or is native scrollback plus session commands
  enough for the current workflow?
  - **Recommendation**: Defer a sidebar until session navigation becomes frequent enough that commands or native scrollback feel slow.
- How should tool calls render: compact status rows, expandable blocks, or full
  transcript entries?
  - **Recommendation**: Render tool calls as compact status rows with access to details when output is long, failed, or user-relevant.

## Connections

- Related ideas: Ratatui constraints already work in cells, so Gridland's cell-first
  layout transfers cleanly.
- Related sources: Gridland demo file `packages/demo/demos/ai-chat-interface.tsx`, docs
  block `packages/docs/content/docs/blocks/ai-chat-interface.mdx`.
- Contradictions or tensions: Gridland uses React/OpenTUI focus/provider
  abstractions; terminal harnesses should avoid recreating those until needed.
- Conceptual use: copy the screen structure and cell-budget discipline, not the
  framework architecture.
