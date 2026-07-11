# Tickets: v0.1 Release Candidate And Workbench UI

Implementation tickets for [the release-and-workbench specification](plan.md).
Work the frontier: any ticket whose blockers are complete can start.

## Ticket 1: Make Both Packages Independently Releasable

**What to build:** Give `thndrs` and `thndrs-agent` independent v0 SemVer
contracts, complete crates.io discovery metadata, and public documentation that
matches their intended users.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Both packages start at `0.1.0` but declare their own package version
      rather than inheriting one shared workspace version.
- [ ] `thndrs` uses the intended compatible `thndrs-agent` release line and
      local development continues to work through the path dependency.
- [ ] `thndrs-agent` has homepage, documentation, keywords, categories, and
      complete README/rustdoc guidance for external application authors.
- [ ] The public library docs state that every public module is experimental,
      supported for external use, provider-neutral, and on a path to stability.
- [ ] Library examples compile and do not imply that applications should import
      provider wire types, filesystem policy, terminal I/O, or session storage.
- [ ] Both package manifests and archive contents are intentionally reviewed;
      no package is published or tagged.

**Verification:**

- `cargo test -p thndrs-agent --doc`
- `cargo package -p thndrs-agent --allow-dirty`
- `cargo package -p thndrs --allow-dirty` after the release owner makes the
  published dependency version available
- human metadata and README review

## Ticket 2: Establish The App Composition Seam

**What to build:** Turn the oversized application module into a documented
composition boundary that can host cohesive child modules without changing any
user-visible behavior.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] The root app module retains the shared public state/message vocabulary
      and the single `update(&mut App, Msg) -> Option<Msg>` mutation route.
- [ ] It declares an intentional child-module layout for onboarding, input,
      commands, context, and agent lifecycle behavior.
- [ ] Existing tests compile through the new module topology without broad
      visibility leaks, new traits, or a second effect system.
- [ ] No command behavior, setup behavior, keybinding, persisted session data,
      renderer output, or public application behavior changes in this ticket.
- [ ] The coordinator’s module docs explain ownership and the extraction order.

**Verification:**

- `cargo test -p thndrs cli::app`
- `cargo test -p thndrs renderer::view`
- `cargo fmt --check`

## Ticket 3: Extract Onboarding And Authentication Behavior

**What to build:** Move first-run recovery, provider setup, credential handling,
and ChatGPT OAuth interaction into a cohesive application module while keeping
the existing behavior unchanged.

**Blocked by:** Ticket 2: Establish The App Composition Seam

**Acceptance criteria:**

- [ ] Recovery/setup state, provider authentication checks, OAuth polling, and
      credential-store actions live outside the root coordinator.
- [ ] Secret input remains hidden and absent from transcript/session/prompt
      surfaces; provider credentials retain their existing ownership and scope.
- [ ] Setup cancellation, OAuth cancellation, failed authentication, and model
      switching retain the prompt draft.
- [ ] Existing model-specific reasoning setup/picker state and config writes
      remain behaviorally unchanged during the extraction.
- [ ] The root app module contains routing and shared state only, not the
      extracted feature’s private workflow helpers.
- [ ] Existing setup and recovery snapshots/tests pass without intentional UI
      change.

**Verification:**

- `cargo test -p thndrs cli::app::tests`
- focused setup/recovery snapshot review

## Ticket 4: Extract Input, Pickers, And Command Routing

**What to build:** Move keyboard interaction, prompt accessories, picker state,
and slash-command dispatch into cohesive modules while preserving every existing
input path.

**Blocked by:** Ticket 2: Establish The App Composition Seam

**Acceptance criteria:**

- [ ] Prompt editing, command mode, file/model/skill/reasoning pickers,
      detail-surface navigation, and input history no longer crowd the root
      coordinator.
- [ ] Keyboard handling still reaches all mutations through `update` and does
      not parse application behavior from rendered display strings.
- [ ] Prompt draft retention, queued steering input, picker selection,
      `Esc` behavior, and command suggestion behavior remain covered.
- [ ] Public keybinding behavior and existing command names are unchanged.
- [ ] The extraction adds no renderer or iocraft dependency to app behavior.

**Verification:**

- `cargo test -p thndrs cli::app::tests::input`
- `cargo test -p thndrs cli::app::tests::movement`
- `cargo test -p thndrs cli::app::tests::slash`

## Ticket 5: Extract Context, Commands, And Agent Lifecycle Behavior

**What to build:** Move context/compaction controls, command output projection,
agent event lifecycle, cancellation, and persistence helpers into behavior-owned
modules without changing their safety or session semantics.

**Blocked by:** Ticket 2: Establish The App Composition Seam

**Acceptance criteria:**

- [ ] Context inspection, pins, compaction review, and context audit behavior
      stay deterministic, bounded, redacted, and independent of rendering.
- [ ] Agent events, permission/cancellation handling, session persistence,
      input history, MCP audit, and stream finalization retain their ordering
      and failure behavior.
- [ ] Slash-command output remains redacted and preserves existing commands,
      exit/error behavior, and session metadata guarantees.
- [ ] Extracted modules depend on domain data and message routing rather than
      renderer cell/style types.
- [ ] Existing context, session, command, cancellation, and permission tests
      remain green.

**Verification:**

- `cargo test -p thndrs core::context`
- `cargo test -p thndrs core::session`
- `cargo test -p thndrs cli::app::tests::commands`
- `cargo test -p thndrs cli::app::tests::prompts`

## Ticket 6: Finish The Thin Coordinator And Renderer Boundary

**What to build:** Complete the app split and move presentation projection into
the renderer so application behavior exposes semantic state and events only.

**Blocked by:** Tickets 3, 4, and 5

**Acceptance criteria:**

- [ ] The root coordinator is small, documented, and limited to shared state,
      message routing, and explicit feature composition.
- [ ] The app layer no longer constructs renderer view cells or imports
      presentation-only types to express domain behavior.
- [ ] Renderer view projection owns semantic table/detail/orientation data and
      remains pure enough for deterministic tests.
- [ ] The direct-renderer transcript, prompt cursor, terminal I/O, scrollback,
      and resize behavior are unchanged.
- [ ] Existing normal, narrow, and tiny-height renderer snapshots are reviewed
      for unintended differences.

**Verification:**

- `cargo test -p thndrs renderer`
- `cargo test -p thndrs cli::app`
- `cargo test -p thndrs`

## Ticket 7: Require Provider-Led Setup Before Coding

**What to build:** Replace fixed-default-model startup with a required,
keyboard-first provider/setup gateway that makes an authenticated coding session
the only successful first-run outcome.

**Blocked by:** Tickets 3 and 6

**Acceptance criteria:**

- [ ] A clean HOME/workspace has no implied default model and cannot submit a
      prompt before provider setup/authentication reaches a ready state.
- [ ] The setup UI has explicit provider choice, provider-specific
      authentication copy, cancellation, failure recovery, and next-step
      guidance; it retains an equivalent CLI route with useful
      non-interactive failure instructions.
- [ ] After authentication and model selection, existing supported
      `reasoning_effort`/`reasoning_summary` controls remain available without
      being required for first-run success; unsupported models remain
      conservative.
- [ ] Setup does not write secrets to TOML, sessions, logs, prompt inspection,
      renderer view state, or snapshots.
- [ ] The prompt draft survives every setup success, cancellation, and failure
      path.
- [ ] Existing advanced provider and ACP configuration stays available without
      becoming the first-run default.

**Verification:**

- setup/recovery app and renderer snapshots at normal, narrow, and tiny height
- fresh-HOME CLI/TUI tests with no credentials
- `cargo test -p thndrs cli::app`

## Ticket 8: Make ChatGPT Codex A First-Class Workflow

**What to build:** Finish and verify the ChatGPT Codex path from required setup
through OAuth, a coding turn, safe tool use, and session recovery.

**Blocked by:** Ticket 7: Require Provider-Led Setup Before Coding

**Acceptance criteria:**

- [ ] Setup and login consistently use the supported ChatGPT OAuth flow and
      never ask for or store a ChatGPT API key.
- [ ] Device-code start, polling, cancellation, expiry, auth failure, and
      credential write behavior have deterministic fake coverage and safe
      human-facing copy.
- [ ] Provider/model selection, request lowering, stream events, tool calls,
      cancellation, session recovery, and supported GPT-5.6 reasoning
      effort/summary lowering are covered at the application boundary.
- [ ] Secrets, access tokens, refresh tokens, and account details are absent
      from logs, sessions, prompt inspection, snapshots, and diagnostics.
- [ ] A release owner can perform the documented disposable-repository smoke:
      authenticate, make a bounded code change, approve tools, run verification,
      inspect the result, and resume the session.

**Verification:**

- `cargo test -p thndrs providers::codex`
- `cargo test -p thndrs cli::app`
- human ChatGPT Codex smoke using an explicitly approved account

## Ticket 9: Make Umans A First-Class Workflow

**What to build:** Finish and verify the Umans path from required setup through
credential entry, a coding turn, safe tool use, and session recovery.

**Blocked by:** Ticket 7: Require Provider-Led Setup Before Coding

**Acceptance criteria:**

- [ ] Umans setup has a clear provider-specific key flow, safe credential
      scope choice, cancellation/failure recovery, and no default-model claim.
- [ ] Model discovery/selection, request lowering, stream events, tool calls,
      cancellation, session recovery, and its supported thinking-toggle
      lowering are covered at the application boundary.
- [ ] The credential is stored only at the existing safe boundary and is
      excluded from TOML, logs, sessions, prompt inspection, snapshots, and
      diagnostics.
- [ ] Provider failure messages are actionable and retain the prompt draft.
- [ ] A release owner can perform the documented disposable-repository smoke:
      authenticate, make a bounded code change, approve tools, run verification,
      inspect the result, and resume the session.

**Verification:**

- `cargo test -p thndrs providers::umans`
- `cargo test -p thndrs cli::app`
- human Umans smoke using an explicitly approved account

## Ticket 10: Build The Restrained Workbench UI

**What to build:** Implement the two-lane renderer architecture and the
restrained workbench language from the reviewed concepts, centralizing bounded
iocraft surfaces while keeping transcript history open in direct rows.

**Blocked by:** Tickets 6 and 7

**Acceptance criteria:**

- [ ] `renderer/adapter.rs` is the only source module that imports or calls
      iocraft; it receives semantic data and returns `Vec<Row>` with no
      terminal writes, render loop, or app state.
- [ ] Setup/authentication, permission, picker, help, and detail surfaces use
      consistent Unicode framing, title/status information, explicit focus,
      keyboard hints, and visible clipping state.
- [ ] The committed transcript has no persistent card-per-entry treatment; it
      remains native-scrollback-friendly with readable role, spacing, and text
      hierarchy.
- [ ] The live prompt/work region has compact orientation information without
      becoming a persistent dashboard.
- [ ] Permission and setup/recovery surfaces still outrank optional detail/help
      surfaces, and `Esc` preserves their established behavior.
- [ ] Normal, narrow, tiny-height, monochrome-equivalent, Unicode, long-line,
      and clipping snapshots demonstrate graceful fallbacks.

**Verification:**

- `cargo test -p thndrs renderer::adapter`
- `cargo test -p thndrs renderer::view`
- `cargo test -p thndrs renderer::region`
- manual real-terminal review against `.sandbox/concepts/`

## Ticket 11: Prepare Release Docs And Package Evidence

**What to build:** Complete the installed-user documentation and gather every
non-publishing artifact needed for the human release review.

**Blocked by:** Tickets 1, 8, 9, and 10

**Acceptance criteria:**

- [ ] README and public site have no release-facing placeholders, stale
      completion claims, or source-checkout instructions where installed-user
      commands belong.
- [ ] Public docs describe required setup, first-class ChatGPT Codex/Umans
      workflows, model-specific reasoning controls, advanced provider status,
      sessions, diagnostics, tool safety, and the lack of a TUI sandbox.
- [ ] The changelog summarizes visible release behavior and records the v0 API
      compatibility policy/migration expectations for `thndrs-agent`.
- [ ] Package archives are inspected for README, license, intended sources,
      test fixtures, and unwanted generated artifacts.
- [ ] The release QA checklist contains reproducible clean-install,
      real-provider, package-order, and human-terminal evidence fields.
- [ ] Public documentation build and internal Markdown/diff review pass.

**Verification:**

- `pnpm --dir docs build`
- `cargo package -p thndrs-agent --allow-dirty`
- `git diff --check`
- human archive and QA-checklist review

## Ticket 12: Execute The Human Release Gate

**What to build:** Produce a complete, human-reviewed release evidence packet
and publication sequence. This ticket prepares and verifies a release; it does
not publish or tag without separate direct approval.

**Blocked by:** Ticket 11: Prepare Release Docs And Package Evidence

**Acceptance criteria:**

- [ ] Formatting, Clippy, workspace tests, docs build, package checks, and
      clean-install smoke have recorded passing evidence from the release
      candidate.
- [ ] ChatGPT Codex and Umans real-account coding smokes have recorded
      redacted evidence from disposable repositories.
- [ ] The owner has reviewed package contents, docs/screenshots, safety wording,
      Unicode/narrow-terminal behavior, known limitations, and changelog.
- [ ] The evidence packet describes the exact approved order: publish
      `thndrs-agent 0.1.0`; wait for registry availability; package/install-test
      `thndrs` against it; obtain separate approval to publish `thndrs 0.1.0`;
      tag only after publication succeeds.
- [ ] No registry token, credential, account identifier, or secret appears in
      the evidence packet or repository.

**Verification:**

- `cargo fmt`
- `cargo clippy --workspace --fix --allow-dirty --allow-staged`
- `cargo clippy --workspace`
- `cargo test --workspace`
- `pnpm --dir docs build`
- human clean-install and provider-smoke review

## Frontier

Tickets that can start immediately:

- Ticket 1: Make Both Packages Independently Releasable
- Ticket 2: Establish The App Composition Seam
