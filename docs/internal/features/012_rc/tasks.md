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

## Ticket 8: Make ChatGPT Codex A First-Class Browser-First Workflow

Finished and verifed the ChatGPT Codex path from required setup
through browser-first OAuth, a coding turn, safe tool use, and session recovery.

## Ticket 9: Make Umans A First-Class Workflow

Finished and verify the Umans path from required setup through
credential entry, a coding turn, safe tool use, and session recovery.

## Ticket 10: Build The Restrained Workbench UI

Implemented two-lane renderer architecture and the single-column signal-rail language
from the reviewed concepts, centralizing bounded iocraft surfaces while keeping transcript
history open in direct rows.

## Ticket 11: Prepare Release Docs And Package Evidence

Completed the installed-user documentation and non-publishing release evidence.

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

- Ticket 12: Execute The Human Release Gate
