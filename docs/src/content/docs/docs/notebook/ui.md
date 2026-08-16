---
title: "Terminal Agent UI"
Author: Pi, Codex, OpenCode, Goose, Aider, Gridland, and public harness docs
Date: 2026-07-03
Captured: 2026-07-03
Tags: [tui, ui, coding-agent, harness, usability]
Sources:
  - https://github.com/earendil-works/pi
  - https://github.com/openai/codex
  - https://github.com/anomalyco/opencode
  - https://github.com/aaif-goose/goose
  - https://github.com/Aider-AI/aider
  - https://developers.openai.com/codex/cli/features
  - https://developers.openai.com/codex/concepts/sandboxing
  - https://opencode.ai/docs/agents
  - https://goose-docs.ai/docs/getting-started/using-extensions/
  - https://aider.chat/docs/usage.html
---

## Summary

Modern terminal coding-agent UIs converge on a chat transcript, bottom prompt,
visible execution state, commands, review loops, and temporary detail surfaces;
the main design tension is keeping the interface quiet while still making the
agent's state, boundaries, and recoverability obvious.

## Key Ideas

- **Transcript-first remains right:** Coding-agent work is mostly linear: prompt,
  reasoning, tool call, output, edit, review, follow-up. Native terminal
  scrollback is still a strong fit for the main history.
- **The prompt is a control surface:** Codex, Gridland, and Aider all treat the
  input area as more than a text box: it carries command discovery, history,
  queued input, file mentions, model/status hints, and stop/submit state.
- **Execution state must be visible:** The UI should show what is running, what
  is waiting, what failed, what was cancelled, and what is merely hidden or
  truncated. This matters more than ornamental styling.
- **Review is a first-class workflow:** Codex and Aider make diffs, command
  output, approval, undo, and copy/retry paths easy to find. Agent UIs should
  expose reviewable changes without making the transcript feel like a dashboard.
- **Permissions are part of orientation:** Codex and OpenCode make permissions
  visible through modes or policies. Pi keeps this simpler by documenting local
  execution. The UI should describe the current boundary honestly without
  turning permission display into permission theater.
- **Commands should be discoverable but sparse:** Slash commands and `!` shell
  commands are common across harnesses. The useful UI pattern is command
  discovery near the prompt, not a large permanent command panel.
- **Agent specialization can stay out of the first UI:** OpenCode and Codex
  expose agents, subagents, modes, and MCP. Those are useful references, but the
  interface should avoid a multi-agent cockpit until the runtime has real
  user-facing modes to switch between.
- **Aesthetic identity comes from information architecture:** Pi's minimalist
  look works because the runtime is small. A comparable product can stay quiet
  while creating its own identity through event clarity, typed transcript rows,
  restrained status bands, and purposeful detail panes rather than palette
  changes.

## Claims & Evidence

### Claim: Chat-first agents benefit from a durable transcript

Pi emphasizes append-to-scrollback chat, Codex interactive mode remains
conversation-centered, and Aider uses a terminal prompt/transcript workflow.

**Caveat / confidence:** High for chat-first coding agents. Lower confidence for
agents centered on long-running background jobs, dashboards, or task boards.

### Claim: The prompt surface carries more than raw text input

Gridland's `PromptInput` includes suggestions and status affordances. Codex
documents queued input and interactive controls. Aider exposes command workflows
from the chat prompt.

**Caveat / confidence:** High. Implementations can expose a smaller set first,
but the sources consistently treat the prompt as a workflow control surface.

### Claim: Permission and sandbox state need explicit orientation

Codex documents sandbox modes and approval policies. OpenCode exposes agent
permissions. Pi documents local-user execution instead of presenting the UI as a
security boundary.

**Caveat / confidence:** High. The UI can describe boundaries, but enforcement
belongs in the harness, process model, or external sandbox.

### Claim: Tool output needs progressive disclosure

Aider foregrounds diffs plus lint/test feedback. Codex exposes command logs and
review flows. Pi stores and truncates large output rather than rendering
everything inline.

**Caveat / confidence:** High. The full output still needs a durable record;
the transcript can show the useful slice and link to detail.

### Claim: A sidebar is useful but not mandatory

Gridland demonstrates a productive two-panel shell with a side navigation area.
Pi and Aider rely more on native scrollback and command-driven interaction.

**Caveat / confidence:** Medium-high. A sidebar becomes more compelling when
session switching, task lists, or background runs are frequent.

### Claim: Multi-agent and MCP surfaces expand the UI contract

OpenCode exposes agent/mode distinctions and tool permissions. Goose emphasizes
provider breadth and MCP extensions. Codex includes richer command, approval,
and MCP entry points.

**Caveat / confidence:** High. These features are valuable when the runtime
supports them, but they add configuration, status, and discoverability burden.

## Important Terms

| Term                   | Meaning                                                                                                                                                        |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Transcript             | The durable conversation and execution history shown to the user. In terminal agents, this often maps well to native terminal scrollback.                      |
| Prompt surface         | The bottom input area plus its local controls: draft text, history, command discovery, file mentions, queued input, model/status hints, and submit/stop state. |
| Tool output preview    | A bounded rendering of command or tool output in the transcript, distinct from the full stored output.                                                         |
| Focused detail surface | A temporary, keyboard-focused panel for bounded details such as help, command selection, file selection, diff detail, or long tool output.                     |
| Orientation band       | A compact status row that tells the user where they are, what is running, and where tool execution occurs.                                                      |
| Permission theater     | UI that implies safety or review guarantees that the underlying runtime or sandbox does not actually enforce.                                                  |
| Native scrollback      | The terminal emulator's own history, search, selection, and scrolling behavior, rather than an app-owned transcript scroller.                                  |

## Harness Comparison

### Pi

Useful UI lesson: keep the transcript minimal, expose an explicit event stream,
show local execution directly, and support queued steering or follow-up.

Risk to avoid: copying the same sparse visual language without adding
product-specific usability affordances.

### Codex CLI

Useful UI lesson: interactive review benefits from plan visibility, approvals,
queued follow-up, file search, slash commands, prompt editing, and extension
entry points.

Risk to avoid: importing a large mode or plugin surface before the core runtime
needs it.

### OpenCode

Useful UI lesson: primary/subagent distinctions, configurable permissions,
agent-specific tool access, and fast mode switching can make agent behavior more
legible.

Risk to avoid: turning mode configuration into a permanent UI burden.

### Goose

Useful UI lesson: desktop and CLI parity, provider breadth, MCP extensions, and
extension setup flows all need clear status and setup feedback.

Risk to avoid: making extension management central before an agent has a stable
extension contract.

### Aider

Useful UI lesson: repo orientation, concise commands, diffs, lint/test feedback,
git undo flows, and image/URL inputs are practical coding-agent affordances.

Risk to avoid: assuming git auto-commit or repo-map behavior belongs in every
terminal agent without a separate product decision.

### Gridland

Useful UI lesson: cell-first chat layout, role-aware messages, bottom prompt,
suggestions above input, and an optional sidebar transfer well to terminal-agent
interfaces.

Risk to avoid: recreating a component framework instead of borrowing layout
discipline.

## UI Patterns

The patterns below are synthesis across the sources, not direct claims from any
single project.

### Transcript Rows

Use stable row families rather than decorative cards:

- user prompt;
- assistant text;
- reasoning summary or live thinking state;
- tool start, live output, and settlement;
- file edit and diff summary;
- warning/error/cancelled status;
- compact session/system notice.

Each family should have a distinct prefix, shape, and wrapping rule. Color can
reinforce state, but the primary distinction should come from row structure so
the transcript remains scannable without becoming a theme exercise.

### Prompt Surface

The prompt should expose:

- input text and cursor;
- current model/provider when relevant;
- working/idle/error state;
- stop or submit affordance;
- queued follow-up count or summary;
- command suggestions when the draft begins with `/`, `@`, or `!`;
- a short hint row only when it helps the current state.

### Tool Output

Tool rows should show the command or operation, current status, elapsed time,
exit or error state, and a bounded output preview. Long output should make clear
that more exists in the session/transcript, not pretend the preview is the full
result.

### Focused Detail Surfaces

Use temporary, bounded surfaces for details that deserve keyboard focus:

- command picker;
- file mention picker;
- help;
- diff detail;
- long tool output detail;
- session picker, only after session navigation exists.

These surfaces can capture scroll input. The main transcript should keep native
terminal scrollback.

### Status And Orientation

Keep one compact status band with:

- workspace;
- model/provider;
- run state;
- trust/sandbox wording;
- session name or id, if it helps recovery;
- short network/search/tool indicators only when active.

The status band should not become a dashboard. It should answer "where am I,
what can happen, and is the agent currently doing anything?"

## Design Lessons

The reusable design lessons are:

- keep row-first transcript rendering and native terminal scrollback when the
  workflow is primarily linear;
- make the bottom prompt more useful before adding any sidebar;
- render tools, diffs, and queued input as first-class events;
- add focused detail panes only for command/file selection and long output;
- give each agent its own quiet visual identity through row structure, status
  language, and focused element treatment instead of copying another tool's
  exact spacing and tone;
- defer agent switching, plugin management, extension stores, and dashboards.

## Questions For Review

- Why do many terminal agents benefit from native terminal scrollback for the
  transcript?
  - Coding-agent work is linear enough that native search, scroll, copy, and
    terminal history are useful; focused panes can handle bounded lists.
- What can distinguish one minimal terminal agent from another visually?
  - More explicit orientation, review, queued-input, tool-status, and detail
    surfaces while keeping the overall UI sparse.
- When should a terminal agent add a sidebar?
  - When session switching, task lists, or background runs become frequent
    enough that command-driven access feels slow.
- What should the UI say about permissions?
  - It should show whether a process runs locally or in an ACP client, plus the
    approval state. Actual isolation belongs to the surrounding OS environment.

## Connections

- Related ideas: [ui-patterns](/docs/notebook/ui-patterns/), [pi](/docs/notebook/pi/),
  [ratatui](/docs/notebook/ratatui/), [harness-engineering](/docs/notebook/harness-engineering/),
  [sessions](/docs/notebook/sessions/).
- Related sources: Pi, Codex, OpenCode, Goose, Aider, and Gridland repositories
  and docs.
- Contradictions or tensions: Pi favors extreme minimalism; Codex/OpenCode/Goose
  show richer mode and extension systems. Designers should adopt interaction
  lessons without expanding the product surface prematurely.
- Useful applications: designing prompt surfaces, transcript row systems,
  tool-output previews, edit-review flows, and temporary detail panes.

## Open Questions

- Should queued input be editable in place, or should an implementation only
  show a compact queued summary at first?
- What is the smallest diff review surface that still makes edits trustworthy?
- Should model/provider selection live in slash commands, a picker, config, or
  all three eventually?
- How much permission state can be represented truthfully before a real sandbox
  boundary is implemented?

## Takeaways

- Borrow Pi's clarity and event discipline, not its exact aesthetic.
- Invest first in prompt, status, tool, diff, and detail surfaces because those
  improve everyday usability without adding a dashboard.
- Keep future extension, subagent, and sidebar surfaces behind real workflow
  need, not reference-project completeness.
