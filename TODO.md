# TODO

## Backlog

### Refactor

- [ ] Move Anthropic-compatible stream accumulation out of `src/agent.rs` and
      into provider/protocol code that returns provider-neutral turn output.
- [ ] Move OpenAI-compatible chat stream accumulation out of `src/agent.rs` and
      into provider/protocol code that returns provider-neutral turn output.
- [ ] Keep retry, cancellation, tool-loop budgeting, steering, and tool
      execution orchestration in the agent loop.
- [ ] Add focused tests for provider stream normalization using existing SSE
      fixtures.
- [ ] Introduce a tool executor/registry shape where each tool module owns its
      definition, input parsing, execution, and output mapping.
- [ ] Generate the model-visible tool catalog from the tool registry.
- [ ] Replace the large tool dispatch match with registry lookup and executor
      calls.
- [ ] Add tests proving every registered tool has a stable schema and dispatch
      path.
- [ ] Introduce a small runtime/run controller for active agent slot,
      cancellation, steering sender, run spawning, event draining, and
      lifecycle logging.
- [ ] Keep terminal event polling/rendering in `lib.rs` and move agent
      lifecycle glue into the runtime/run controller.
- [ ] Define a stable semantic turn-event layer between `AgentEvent`,
      transcript `Entry`, and append-only session records.
- [ ] Project semantic turn events into transcript entries in one tested path.
- [ ] Project semantic turn events into session records in one tested path.
- [ ] Precompute renderer view geometry and row groups before terminal output.
- [ ] Add focused renderer snapshots for computed transcript, prompt,
      accessory, and status regions.
- [ ] Introduce input/app command enums so raw terminal keys are translated
      before mutating `App`.
- [ ] Move mode-specific key handling toward command translation tests.

### Inspect And Export

- [ ] Add non-TUI session inspect/export command.
- [ ] Keep inspect/export output JSON or JSONL.
- [ ] Include loaded `AGENTS.md` files, scopes, hashes, and truncation state in
      inspect/export output.
- [ ] Include renderer-independent message metadata needed for later rendering.

### LSP And Code Intelligence

- [ ] Define read-only LSP tool names, inputs, outputs, and fallback behavior.
- [ ] Support document symbols.
- [ ] Support workspace symbols.
- [ ] Support go to definition.
- [ ] Support find references.
- [ ] Support hover.
- [ ] Support find implementations where the language server supports it.
- [ ] Degrade clearly when no language server is available.
- [ ] Keep LSP startup and indexing bounded with visible diagnostics.
- [ ] Record LSP calls as structured transcript/tool events.
- [ ] Preserve plain file search as the fallback path.
- [ ] Unit-test LSP fixture responses.
- [ ] Unit-test no-server fallback behavior.
- [ ] Add snapshots for LSP transcript entries.

## Parking Lot

- [ ] Tool call failures should have debuggable logs and more information about
      why in the transcript.
- [ ] Plan mode?
- [ ] In-app task management
- [ ] Subagents or multi-agent orchestration
- [ ] Custom terminal multiplexer?
- [ ] LSP code actions or automatic refactors?
- [ ] Long-lived LSP server process management?
- [ ] Skill marketplace, installer, sharing, or publishing?
- [ ] Skill-specific tool permission enforcement?
- [ ] Plugin framework for self-description?
- [ ] Provider-private state introspection?
- [ ] Project files, skills, or remote resources rewriting harness identity,
      direct instructions, tool schemas, or safety boundaries?

### Bugs
