# Internals and Development Documentation

Update the pages in this order. Earlier tasks establish terminology and source
locations used by later pages.

## Priority 0: Establish the System Map

- [x] Update `internals/codebase.md`: verify the Workspace Map and Application
  Modules; complete Agent Library Modules, Find Code by Responsibility, Where
  Should New Code Live?, Dependency Boundaries, Source Map, and Related.
- [x] Update `internals/runtime.md`: complete Mental Model, Effects,
  Presentation Scheduling, Boundaries, Key Types, Source Map, and Related;
  verify the existing Responsibilities, State Transitions, Agent Events, and
  Invariants against the current code.

## Priority 1: Document the Execution Path

- [x] Update `internals/lifecycle.md`: complete Mental Model, Submitting a
  Request, Running the Agent, Handling a Tool Call, Updating the Interface,
  Cancellation and Failure, Boundaries, Key Types, Invariants, Source Map, and
  Related. Follow one request from input through final rendering.
- [x] Update `internals/context.md`: complete Mental Model, Responsibilities,
  Workspace Discovery, Instruction Precedence, Skill Selection, Conversation
  Context, Tool Exposure, Provider Projection, Boundaries, Key Types,
  Invariants, Source Map, and Related.

## Priority 2: Document Runtime Subsystems

- [x] Update `internals/providers.md`: complete Mental Model, Streaming Event
  Normalization, Authentication, Errors, Retries, and Cancellation, Boundaries,
  Key Types, Invariants, Source Map, and Related; verify Responsibilities and
  Request Conversion for every supported provider.
- [ ] Update `internals/tools.md`: complete Mental Model, Execution Results,
  Auditing and Side Effects, Boundaries, Key Types, Invariants, Source Map, and
  Related; verify the built-in and MCP paths against their registries and
  executors.
- [ ] Update `internals/sessions.md`: complete Mental Model, Responsibilities,
  Persisted Data, Ephemeral State, Resuming a Session, Tool Audits and Logs,
  Boundaries, Key Types, Invariants, Source Map, and Related. State explicitly
  what is not persisted.

## Priority 3: Document Frontends

- [ ] Update `internals/terminal-ui.md`: complete Responsibilities, Semantic
  Views, Terminal Lifecycle, Boundaries, Key Types, Source Map, and Related;
  verify the Mental Model, transcript/live-surface split, and Invariants.
- [x] Update `internals/acp.md`: complete Mental Model, Responsibilities, Shared
  Runtime, Transport and Session Handling, Request and Event Flow, Boundaries,
  Key Types, Invariants, Source Map, and Related. Distinguish shared runtime
  behavior from ACP-only transport behavior.

## Priority 4: Complete Contributor Guides

- [ ] Update `development/adding-a-provider.md`: complete Before You Start,
  Implement the Provider Boundary, Convert Requests and Tools, Normalize
  Streaming Events, Add Authentication and Configuration, Handle Errors and
  Cancellation, Register the Provider, Test the Provider, Update Public
  Documentation, and Run the Checks.
- [ ] Review `development/adding-a-tool.md`: verify every module, function, and
  registry path; add concrete focused-test commands; ensure side-effect and
  public-reference requirements match current behavior.
- [ ] Review `development/testing.md`: verify Unit Tests, Layout, Snapshots,
  Fixtures, Ignored Live Tests, Continuous Integration, Search, and cargo-insta
  Workflow against the current test tree and CI configuration.
- [ ] Review `development/workflow.md`: correct the command sequence, separate
  focused checks from full gates, and verify Formatting, Testing, Snapshots,
  Debugging, TUI and Release checks, and Multiplexer-Assisted Development.

## Priority 5: Finish Navigation and Verification

- [ ] Update `internals/index.md` after the subsystem pages are complete:
  reconcile System Overview, Major Subsystems, Architectural Boundaries, Where
  to Start, and Related with the final terminology and links.
- [ ] Review the Internals and Development entries in `docs/astro.config.mjs`:
  confirm labels, page order, slugs, and collapsed state match the completed
  documentation.
- [ ] Check every Internals page for one useful mental model, explicit ownership
  boundaries, current key types, verified source locations, and links instead
  of duplicated guarantees.
- [ ] Run `pnpm --dir docs build` and resolve content, route, and internal-link
  validation failures.
