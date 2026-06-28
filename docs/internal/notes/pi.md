# Notes: Pi Coding Agent Harness Lessons

Source:

- https://pi.dev
- https://pi.dev/docs
- https://mariozechner.at/posts/2025-11-30-pi-coding-agent/
- https://github.com/earendil-works/pi
- https://github.com/earendil-works/pi/blob/main/packages/coding-agent/README.md
- https://github.com/earendil-works/pi/blob/main/packages/agent/README.md
- https://github.com/earendil-works/pi/blob/main/packages/tui/README.md
  Author: Mario Zechner / Earendil Works
  Date: 2025-11-30 for the article; docs current as fetched on 2026-06-28
  Captured: 2026-06-28
  Tags: coding-agent, harness, terminal-ui, agent-loop, minimalism

## Summary

Pi argues that a coding harness can stay small and inspectable by exposing simple tools,
explicit context, event-streamed agent state, file-based planning, and terminal-native
workflows instead of baking in heavyweight modes and hidden orchestration.

## Key Ideas

- **Minimal core, extensible edges:** Pi splits LLM API, agent loop, TUI, and CLI harness
  into separate packages. The CLI adds sessions, tools, themes, context files, and customization.
- **Context control matters:** The article stresses that hidden prompt/tool/context
  injection makes model behavior harder to predict.
  Pi keeps prompt/tool definitions small and surfaces what gets loaded.
- **Four default tools can be enough:** Pi's default coding surface is read, write, edit,
  and bash, with separate read-only tools available for restricted runs.
- **Events are the UI contract:** `pi-agent-core` emits agent, turn, message, stream
  update, and tool execution events; a UI can subscribe and render incrementally.
- **Queueing is a first-class interaction:** Pi supports steering messages during work
  and follow-up messages after work, rather than forcing the user to wait silently.
- **Use the terminal instead of rebuilding it:** The Pi article chooses an
  append-to-scrollback TUI for coding-agent chat so native terminal search/scrolling keep
  working.
- **No built-in plan mode/todos/sub-agents/MCP by default:** Pi prefers files, CLI tools
  with READMEs, tmux, and explicit separate sessions over hidden state and heavyweight tool surfaces.
- **Sandbox outside the harness:** Pi does not pretend permission prompts solve the core
  security problem; its docs recommend containers, micro-VM routing, or policy sandboxes when stronger boundaries are needed.

## Claims & Evidence

| Claim                                                                                       | Support                                                                                                                             | Caveat / Confidence                                                                                            |
| ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| A useful coding agent does not need a huge system prompt.                                   | Pi reports its prompt plus tool definitions are under 1000 tokens and relies on project `AGENTS.md` for customization.              | Medium; this is the author's experience and benchmark framing, not a universal proof.                          |
| Agent runtime should expose event flow instead of hiding it.                                | `pi-agent-core` documents `agent_start`, `turn_start`, `message_start/update/end`, `tool_execution_*`, `turn_end`, and `agent_end`. | High; event streams map cleanly to any UI.                                                                     |
| Tool result data should separate model content from UI detail.                              | The article calls out separate tool result blocks for LLM content and UI rendering detail as a useful abstraction.                  | High for future design; not needed in first stub.                                                              |
| Native scrollback is a better fit for linear coding-agent chats than full-screen ownership. | The article argues coding agents are mostly linear chat plus tool output, so terminal scrolling/search are valuable.                | Medium for `thndrs`; Ratatui defaults push us toward alternate screen unless we deliberately choose otherwise. |
| Built-in background process management can be avoided.                                      | Pi recommends tmux for long-running servers/debuggers and keeping bash synchronous.                                                 | Medium-high; good for simplicity, but our harness may eventually need supervised tool streaming.               |
| Plan mode and todos can be files.                                                           | Pi recommends `PLAN.md`/TODO files for persistent, visible planning state.                                                          | High; this aligns with this repo's simplicity rule.                                                            |

## Important Terms

| Term                     | Meaning                                                                                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent loop               | The repeated process of sending context to a model, streaming a response, executing tool calls, feeding results back, and stopping when no tool calls remain. |
| Steering message         | User input queued while the agent is working, delivered after the current turn/tool batch.                                                                    |
| Follow-up message        | User input queued to run after the agent finishes current work.                                                                                               |
| Context file             | Project/user instructions loaded into the agent context, such as `AGENTS.md`.                                                                                 |
| Tool preflight           | A hook before tool execution that can validate or block a call.                                                                                               |
| Append-to-scrollback TUI | TUI model that writes mostly linearly to terminal scrollback and only redraws a small active region.                                                          |

## Lessons for `thndrs`

- Keep the first harness local and inspectable: no hidden planner, no hidden sub-agent,
  no MCP surface.
- Start with a typed event stream even if the first agent is fake.
  UI should render `User`, `AssistantDelta`, `ToolStart`, `ToolOutput`, `ToolEnd`, `Error`, and `Done`.
- Represent tools as explicit Rust structs/functions with clear input/output;
  defer provider abstraction until one provider works.
- Store sessions as append-only JSONL eventually. For v0, in-memory transcript is enough.
- Prefer CLI/file workflows for planning and context gathering. If users want a plan,
  write/read a Markdown file.
- Do not build permission theater into the UI. If safety matters, design a real sandbox
  boundary later.
- Consider whether Ratatui full-screen mode conflicts with the Pi scrollback lesson.
  If we keep alternate screen for v0, document that it is a tactical choice, not a
  philosophical one.

## Questions for Review

- Should this Rust harness preserve native scrollback like Pi, or use Ratatui's normal
  alternate-screen model first?
- What is the minimum event enum that supports streaming model text and tool calls?
- Do we need real tool execution in v0, or is a scripted fake agent enough to validate
  layout and input?
- Where should project context be read from: just `AGENTS.md`, or also `README.md`/selected files?

## Connections

- Related ideas: Ratatui TEA gives the Rust UI state model;
  Gridland gives the target layout;
  Pi gives harness philosophy and agent event semantics.
- Related sources: Pi README package split, `pi-agent-core` event flow, `pi-tui`
  retained-mode rendering, Pi containerization docs.
- Contradictions or tensions: Pi's TUI is append-to-scrollback and TypeScript; this
  project targets Rust + Ratatui, which normally redraws a full viewport.
- Useful applications: Build a minimal local coding harness whose complexity lives in
  explicit event/data types, not hidden modes.

## Open Questions

- Which model/provider should be first when real inference is added?
- Should tool execution be synchronous only, with tmux recommended for long-running
  processes?
- How should abort/stop propagate through async model streams and tools?
- What session format will be stable enough to inspect and replay?

## Takeaways

- Preserve observability: every context source, model event, and tool call should be visible.
- Keep v0 fake or narrow: prove the UI and event loop before adding provider complexity.
- Let files and shell conventions do work before inventing app modes.
