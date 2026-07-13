# Tickets: v0.1 Release Candidate And Workbench UI

Implementation tickets for [the release-and-workbench specification](plan.md).
Work the frontier: any ticket whose blockers are complete can start.

## Ticket 1: Make Both Packages Independently Releasable

Gave `thndrs` and `thndrs-agent` independent v0 SemVer contracts, complete
crates.io discovery metadata, and public documentation that matches their
intended users.

## Ticket 2: Establish The App Composition Seam

Established a documented private composition boundary for the application
coordinator without changing user-visible behavior.

## Ticket 3: Extract Onboarding And Authentication Behavior

Extracted first-run recovery, provider setup, credential handling, and ChatGPT
OAuth interaction into a cohesive private application module while keeping the
existing behavior unchanged.

## Ticket 4: Extract Input, Pickers, And Command Routing

Extracted keyboard interaction, prompt accessories, picker state, and
slash-command dispatch into cohesive private modules while preserving every
existing input path.

## Ticket 5: Extract Context, Commands, And Agent Lifecycle Behavior

Moved context/compaction controls, command output projection,
agent event lifecycle, cancellation, and persistence helpers into behavior-owned
modules without changing their safety or session semantics.

## Ticket 6: Finish The Thin Coordinator And Renderer Boundary

Completed the app split and move presentation projection into
the renderer so application behavior exposes semantic state and events only.

## Ticket 7: Require Provider-Led Setup Before Coding

Replaced fixed-default-model startup with a required, keyboard-first provider/setup
gateway that makes an authenticated coding session the only successful first-run outcome.

**Scope note:** This gateway establishes provider choice and the no-default-model
rule; it does not prescribe ChatGPT Codex’s OAuth transport. Ticket 8 supersedes
the inherited device-code-first behavior with a browser-first provider workflow.

## Ticket 8: Make ChatGPT Codex A First-Class Browser-First Workflow

**What to build:** Finish and verify the ChatGPT Codex path from required setup
through browser-first OAuth, a coding turn, safe tool use, and session recovery.
Device code remains an explicit headless/remote alternative, not a prerequisite
or an automatic fallback.

**Blocked by:** Ticket 7: Require Provider-Led Setup Before Coding

**Acceptance criteria:**

- [x] Setup recognizes an existing valid ChatGPT Codex credential. When one is
      absent, login uses the supported ChatGPT OAuth route and never asks for
      or stores a ChatGPT API key.
- [x] Browser PKCE is preselected for browser-capable environments. It starts a
      short-lived loopback callback, launches or shows a copyable authorization
      URL, validates callback state, and lets the user paste the full redirect
      URL when the callback cannot reach the application.
- [x] Device code is a clearly labeled, user-selected headless/remote method.
      Its start, polling, slow-down, cancellation, expiry, auth failure, and
      credential write behavior have deterministic fake coverage and safe
      human-facing copy. Neither method silently falls through to the other.
- [x] Provider/model selection, request lowering, stream events, tool calls,
      cancellation, session recovery, and supported GPT-5.6 reasoning
      effort/summary lowering are covered at the application boundary.
- [x] Secrets, access tokens, refresh tokens, and account details are absent
      from logs, sessions, prompt inspection, snapshots, and diagnostics. The
      same boundary covers authorization codes, callback query strings, PKCE
      verifiers, and device-auth identifiers.
- [x] A release owner can perform the documented browser-default
      disposable-repository smoke: authenticate, make a bounded code change,
      approve tools, run verification, inspect the result, and resume the
      session. The headless/device-code smoke is recorded separately when the
      approved account and provider policy permit it.

**Verification:**

- `cargo test -p thndrs providers::codex`
- `cargo test -p thndrs cli::app`
- human browser-default ChatGPT Codex smoke using an explicitly approved account

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
single-column signal-rail language from the reviewed concepts, centralizing
bounded iocraft surfaces while keeping transcript history open in direct rows.

**Blocked by:** Tickets 6 and 7

**Acceptance criteria:**

- [ ] `renderer/adapter.rs` is the only source module that imports or calls
      iocraft; it receives semantic data and returns `Vec<Row>` with no
      terminal writes, render loop, or app state.
- [ ] Setup/authentication, permission, picker, help, and detail surfaces use
      consistent Unicode framing, title/status information, explicit focus,
      keyboard hints, and visible clipping state.
- [ ] The committed transcript has no persistent card-per-entry treatment; it
      remains native-scrollback-friendly with readable role, spacing, text
      hierarchy, and typed event marks. It has no persistent sidebar, fake
      terminal chrome, or second main panel.
- [ ] The live prompt/work region has compact orientation information without
      becoming a persistent dashboard.
- [ ] Permission and setup/recovery surfaces still outrank optional detail/help
      surfaces, and `Esc` preserves their established behavior.
- [ ] Normal, narrow, tiny-height, monochrome-equivalent, Unicode, long-line,
      and clipping snapshots demonstrate graceful fallbacks. Eldritch Minimal,
      Iceberg Dark, and Catppuccin Mocha use renderer palette roles rather than
      page-local colors.

**Verification:**

- `cargo test -p thndrs renderer::adapter`
- `cargo test -p thndrs renderer::view`
- `cargo test -p thndrs renderer::region`

## Ticket 11: Prepare Release Docs And Package Evidence

**What to build:** Complete the installed-user documentation and gather every
non-publishing artifact needed for the human release review.

**Blocked by:** Tickets 1, 8, 9, and 10

**Acceptance criteria:**

- [ ] README and public site have no release-facing placeholders, stale
      completion claims, or source-checkout instructions where installed-user
      commands belong.
- [ ] Public docs describe required setup, the browser-default and explicit
      headless ChatGPT Codex OAuth paths, first-class Umans workflow,
      model-specific reasoning controls, advanced provider status, sessions,
      diagnostics, tool safety, and the lack of a TUI sandbox.
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
