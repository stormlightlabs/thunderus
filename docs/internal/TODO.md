# Parking Lot

## RR-1: Make workspace writes atomic

Made `create_file`, `replace_range`, and every `write_patch` operation preserve
the previous target when validation or writing fails. The implemented behavior
must match the public unchanged-on-failure guarantee.

## RR-2: Implement real background process ownership

Made `run_shell` return promptly for a background command and keep the actual
running child under application ownership until it exits or is cancelled.

## RR-3: Make ACP field caps UTF-8 safe

Capped ACP tool input, output, and content without slicing a string between
UTF-8 code units.

## RR-4: Align clean-install diagnostics with onboarding

Made `doctor` describe an unset model as incomplete setup instead of silently
diagnosing Umans, and direct users to a working support URL.

## RR-5: Fix the application crate archive

Maade the `thndrs` crate archive complete and ready for the two-stage crates.io
publication flow.

## RR-6: Make strict lint and API documentation checks green

Removed the current all-target Clippy warnings and Rustdoc errors so strict
checks can be used as release gates.

## RR-7: Add continuous release checks

Added CI that runs the checks required to keep the alpha installable and catches
regressions in both crates before merge.

## RR-8: Finalize the `thndrs-agent` 0.1 release contract

Finished the provider-neutral library's public release contract and record the
evidence needed for a separate publication decision.

## RR-9: Prove the registry-to-clean-install path

**What to build:** Exercise the exact publication order and prove that a user can
install the application from crates.io without a source checkout.

**Blocked by:** RR-5: Fix the application crate archive; RR-7: Add continuous
release checks; RR-8: Finalize the `thndrs-agent` 0.1 release contract; explicit
owner approval to publish `thndrs-agent 0.1.0`.

**Acceptance criteria:**

- [ ] Publish `thndrs-agent 0.1.0` only after direct approval and verify registry
      availability and docs.rs output.
- [ ] Package `thndrs` against the registry dependency and review its final
      archive before requesting application publication approval.
- [ ] Install `thndrs` with `cargo install --locked thndrs` under a clean `HOME`.
- [ ] `thndrs --version`, first-run provider choice, CLI setup, `doctor`, config
      inspection, and empty session listing work from the installed binary.
- [ ] The clean first run does not assume a provider or model and does not write
      credential material before authentication succeeds.
- [ ] Record versions, revision, platform, architecture, Rust/Cargo versions,
      commands, and redacted results in the release evidence.
- [ ] Publishing `thndrs` and creating a tag remain separate approval steps.

**Verification:**

- `cargo install --locked thndrs`
- `thndrs --version`
- Clean-`HOME` first-run and CLI smoke from a disposable workspace.

## RR-10: Execute real-provider and terminal release smokes

**What to build:** Complete the human checks that deterministic tests cannot
cover: current provider authentication, one bounded coding task per first-class
provider, session recovery, and real-terminal behavior.

**Blocked by:** RR-1: Make workspace writes atomic; RR-2: Implement real
background process ownership; RR-3: Make ACP field caps UTF-8 safe; RR-4: Align
clean-install diagnostics with onboarding; RR-9: Prove the registry-to-clean-install
path.

**Acceptance criteria:**

- [ ] ChatGPT Codex browser OAuth, explicit device-code OAuth, cancellation,
      expired/revoked credential recovery, and transient service failure are
      exercised without recording tokens, account identifiers, or OAuth URLs.
- [ ] Umans hidden credential entry, environment override behavior, rejected-key
      recovery, and transient service failure are exercised without recording
      credentials.
- [ ] Each provider completes a bounded edit, uses local tools, runs verification,
      exposes inspectable output, and resumes the resulting session.
- [ ] Session inspection, export, logs, diagnostics, and prompt inspection are
      reviewed for secret leakage.
- [ ] Normal, narrow, short, Unicode, CJK, emoji, combining-mark, long-path,
      monochrome, setup, picker, permission, help, and detail surfaces are
      reviewed in a real terminal.
- [ ] Known provider or terminal limitations are added to public documentation
      before approval.

**Verification:**

- Complete and sign off the applicable sections of `docs/internal/qa/README.md`
  and its channel checklists.
- Run the ignored live tests individually only with the required account and
  privacy prerequisites.

## RR-11: Approve or reject the public alpha candidate

**What to build:** Produce one complete release evidence packet and make an
explicit go/no-go decision for `thndrs 0.1.0`.

**Blocked by:** RR-1 through RR-10

**Acceptance criteria:**

- [ ] Every preceding ticket has passing verification evidence or a documented,
      owner-approved alpha limitation that is accurate in public documentation.
- [ ] The release checklist contains the candidate revision, environment,
      archive reviews, clean install, provider smokes, terminal review, and
      redacted results.
- [ ] The changelog describes the shipped application, `thndrs-agent` contract,
      known limitations, and migration expectations without stale provider or
      default-model claims.
- [ ] The owner separately approves application publication and tagging.
- [ ] The evidence packet and repository contain no credential, token, account
      identifier, authorization URL, callback URL, or registry secret.

**Verification:**

- Re-run the complete command list in `docs/internal/qa/README.md` from the approved
  candidate revision.
- Review the final crates.io pages, docs site, repository links, and release
  notes after publication.

## Parking Lot

Quiver owns toolchain extensibility. Do not add a separate plugin runtime.

## PL-1: Own agent-run completion

Made `AgentRun` retain its event receiver, cancellation token, and worker.
Dropping a run now cancels, disconnects, and joins it; explicit completion
reports worker panic, and the TUI, ACP, and server paths retain the owner.

## PL-2: Run one prompt without the TUI

**What to build:** Add a headless command that runs one coding prompt through
the normal provider, tool, context, and session paths.

**Blocked by:** PL-1: Own agent-run completion

**Acceptance criteria:**

- [ ] The command accepts a prompt, streams useful text to stdout, and exits
      with stable success, failure, setup, policy, and cancellation codes.
- [ ] Tool use, retries, context control, and session audit match interactive
      behavior.
- [ ] Diagnostics never corrupt stdout intended for the result.

**Verification:**

- No-network provider fixtures cover success, tool use, failure, and
  cancellation.

## PL-3: Stream headless events as JSONL

**What to build:** Add a machine-readable mode for the headless command using
the provider-neutral event vocabulary.

**Blocked by:** PL-2: Run one prompt without the TUI

**Acceptance criteria:**

- [ ] Every stdout line is one versioned JSON object.
- [ ] Text, reasoning, usage, retries, tools, completion, cancellation, and
      failure have stable event shapes.
- [ ] Human diagnostics remain on stderr.

**Verification:**

- Golden JSONL fixtures cover a complete tool-using run and each terminal
  outcome.

## PL-4: Accept piped prompt input

**What to build:** Let the headless command combine bounded stdin content with
an explicit prompt.

**Blocked by:** PL-2: Run one prompt without the TUI

**Acceptance criteria:**

- [ ] Piped input works with and without an explicit prompt.
- [ ] Interactive terminals are not read until EOF accidentally.
- [ ] Oversized or invalid UTF-8 input fails with an actionable error.

**Verification:**

- CLI tests cover pipes, empty stdin, invalid input, and the configured size
  limit.

## PL-5: Run without saving a session

**What to build:** Add an explicit ephemeral mode for headless and interactive
runs.

**Blocked by:** PL-3: Stream headless events as JSONL

**Acceptance criteria:**

- [ ] Ephemeral runs create no session or per-session log.
- [ ] Credential, config, and shared prompt-history behavior remains unchanged.
- [ ] The UI and JSONL stream identify the run as ephemeral.

**Verification:**

- Tests run against an empty session directory and prove it remains empty.

## PL-6: Resume sessions from the TUI

**What to build:** Add a session picker that resumes a validated local session
without leaving the TUI.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] The picker lists recent sessions with enough metadata to distinguish
      them.
- [ ] Selecting a session uses the existing validation and exclusive-lock
      rules.
- [ ] Corrupt, missing, or already locked sessions fail without losing the
      current draft.

**Verification:**

- State and renderer tests cover selection, cancellation, locking, and corrupt
  records.

## PL-7: Name local sessions

**What to build:** Let users assign and change a short display name without
rewriting append-only history.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Names appear in list, show, inspect, and export surfaces.
- [ ] Renaming appends durable metadata and preserves the session identifier.
- [ ] Empty, oversized, and control-character names are rejected.

**Verification:**

- Session round-trip and CLI/TUI projection tests.

## PL-8: Fork a session from a completed turn

**What to build:** Create a new append-only session from a selected completed
turn while preserving provenance to the source session.

**Blocked by:** PL-6: Resume sessions from the TUI

**Acceptance criteria:**

- [ ] Users can select only replayable, settled turn boundaries.
- [ ] The fork has a new identifier and records its source session and sequence.
- [ ] Pending tools, permissions, queues, and processes are never copied.

**Verification:**

- Deterministic session fixtures cover valid forks and rejected live or corrupt
  boundaries.

## PL-9: Export sessions for human review

**What to build:** Export a redacted session as readable Markdown and
standalone HTML.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Both formats preserve message, reasoning-summary, tool, status, and error
      order.
- [ ] Tool details remain bounded and secrets stay redacted.
- [ ] The HTML export requires no external assets or scripts.

**Verification:**

- Snapshot both formats from the same representative session fixture.

## PL-10: Attach images to native prompts

**What to build:** Let TUI and headless users attach local images to providers
that advertise image input.

**Blocked by:** PL-2: Run one prompt without the TUI

**Acceptance criteria:**

- [ ] Users can add, inspect, and remove image paths before submission.
- [ ] MIME type, size, dimensions, and provider support are validated locally.
- [ ] Session records preserve safe attachment metadata without duplicating
      image bytes.
- [ ] Text-only providers fail before making a request.

**Verification:**

- Provider request fixtures and TUI/headless tests cover supported,
  unsupported, missing, and oversized images.

## PL-11: Support the OpenAI Platform API

**What to build:** Add first-class API-key authentication and model discovery
for OpenAI Platform models without using ChatGPT OAuth credentials.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Platform and ChatGPT credentials remain separate and are never used as
      fallbacks for each other.
- [ ] Text, image, reasoning, tool, usage, retry, and error behavior normalize
      through existing contracts.
- [ ] Setup, doctor, model selection, and session metadata identify the route
      accurately.

**Verification:**

- No-network request/stream fixtures plus an ignored live smoke test.

## PL-12: Support the Anthropic API

**What to build:** Add first-class Anthropic API-key authentication and model
discovery through the existing Anthropic-compatible adapter boundary.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Setup, doctor, model selection, and recovery support Anthropic.
- [ ] Text, image, reasoning, tools, usage, retries, and errors normalize
      without exposing provider payloads publicly.
- [ ] Provider-specific capabilities reject unsupported controls locally.

**Verification:**

- No-network request/stream fixtures plus an ignored live smoke test.

## PL-13: Configure compatible provider endpoints

**What to build:** Let users register custom OpenAI- or Anthropic-compatible
endpoints and a data-driven model catalogue.

**Blocked by:** PL-11: Support the OpenAI Platform API; PL-12: Support the
Anthropic API

**Acceptance criteria:**

- [ ] Configuration declares protocol, base URL, credential source, and model
      capabilities without embedding secrets.
- [ ] Model metadata covers tools, images, reasoning, context, and output
      limits.
- [ ] Invalid or incomplete capabilities fail before a provider request.

**Verification:**

- Local fake servers cover both protocols, model loading, and rejected config.

## PL-14: Run stable checks on macOS

**What to build:** Add macOS CI for formatting-independent Rust checks and
platform-sensitive process, filesystem, session, and terminal behavior.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] CI runs workspace tests and strict Clippy on the supported macOS target.
- [ ] macOS-specific process-tree, file-permission, and atomic-write behavior
      is exercised.

**Verification:**

- A green macOS CI run on the candidate revision.

## PL-15: Run stable checks on Windows

**What to build:** Add Windows CI for Rust checks and platform-sensitive
process, filesystem, session, and terminal behavior.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] CI runs workspace tests and strict Clippy on the supported Windows target.
- [ ] Windows process-tree, path, replacement, and terminal fallbacks are
      exercised.

**Verification:**

- A green Windows CI run on the candidate revision.

## PL-16: Gate project-owned runtime configuration on trust

**What to build:** Ask before loading project-owned configuration that can
start processes or change runtime behavior.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Trust covers project MCP, ACP, prompt templates, and skills without
      treating `AGENTS.md` as executable authority.
- [ ] Untrusted projects use global/user configuration and show what was
      skipped.
- [ ] Trust decisions are explicit, inspectable, revocable, and scoped to a
      canonical project root.

**Verification:**

- Clean-home tests cover trusted, untrusted, moved, and replaced project roots.

## PL-17: Define the sandbox execution boundary

**What to build:** Route local commands through one application-owned sandbox
adapter while preserving an explicit external/no-sandbox mode.

**Blocked by:** PL-1: Own agent-run completion

**Acceptance criteria:**

- [ ] Policy distinguishes read-only, workspace-write, and external isolation.
- [ ] Filesystem and network authority are separate inputs.
- [ ] Built-in shell, ACP terminals, and MCP children report which boundary
      applies.
- [ ] The adapter adds no in-process claim of isolation when no backend exists.

**Verification:**

- Deterministic policy and routing tests with a fake sandbox backend.

## PL-18: Implement the first OS sandbox backend

**What to build:** Enforce read-only and workspace-write command execution on
one supported operating system.

**Blocked by:** PL-17: Define the sandbox execution boundary

**Acceptance criteria:**

- [ ] Workspace reads and policy-approved writes behave as declared.
- [ ] Writes outside allowed roots and disallowed network access fail closed.
- [ ] Protected repository and credential paths remain unavailable.
- [ ] Child processes cannot outlive cancellation or application shutdown.

**Verification:**

- Platform integration tests attempt allowed and denied filesystem, network,
  and process operations.

## PL-19: Ask for approval at sandbox boundaries

**What to build:** Surface approval only when an operation requests authority
outside the active sandbox policy.

**Blocked by:** PL-18: Implement the first OS sandbox backend

**Acceptance criteria:**

- [ ] Approval describes the exact command, resource, and requested authority.
- [ ] Allow, reject, cancel, and unavailable-interaction outcomes are audited.
- [ ] Approval never weakens the sandbox silently or claims that command
      classification is isolation.

**Verification:**

- Policy, TUI, headless, ACP, and session tests cover every outcome.

## PL-20: Define structured code-review findings

**What to build:** Define a stable review result with severity, summary,
evidence, and the smallest useful code location.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] The contract distinguishes actionable findings from a clean review.
- [ ] Findings require evidence and reject invalid or out-of-range locations.
- [ ] Rendering is deterministic in human and JSON forms.

**Verification:**

- Parser, validation, ordering, and rendering tests use fixed review fixtures.

## PL-21: Add a first-class review command

**What to build:** Review uncommitted changes, a base branch, or one commit
through an explicit read-only workflow.

**Blocked by:** PL-20: Define structured code-review findings

**Acceptance criteria:**

- [ ] Exactly one review target is resolved before the provider runs.
- [ ] Review uses read-only tools and does not modify the repository.
- [ ] Human and JSON output use the structured finding contract.
- [ ] A clean review exits successfully with no invented findings.

**Verification:**

- Git fixtures cover every target, invalid combinations, findings, and a clean
  result.

## PL-22: Run versioned coding-task evaluations

**What to build:** Add a deterministic runner for versioned repository tasks
with explicit setup, expected behavior, verification, and scoring.

**Blocked by:** PL-2: Run one prompt without the TUI

**Acceptance criteria:**

- [ ] Tasks cover edits, diagnosis, review, tool failure, steering, compaction,
      hostile instructions, and interrupted resume.
- [ ] Reports separate harness failures from model task failures.
- [ ] Results record model, provider, revision, timing, usage, intervention,
      changed files, and verification outcome.

**Verification:**

- A fake provider run produces stable JSON and Markdown reports.

## PL-24: Supervise read-only subagents

**What to build:** Let a parent run delegate explicit independent tasks to
bounded, read-only child runs and collect their summaries.

**Blocked by:** PL-1: Own agent-run completion

**Acceptance criteria:**

- [ ] Delegation requires direct user or project guidance and obeys a
      concurrency limit.
- [ ] Each child has isolated context, its own transcript, and inherited
      sandbox limits.
- [ ] Parent cancellation settles every child before completion.
- [ ] The parent receives summaries rather than unbounded child transcripts.

**Verification:**

- Deterministic fake-agent tests cover success, failure, cancellation, and the
  concurrency bound.

## PL-25: Inspect and steer subagents

**What to build:** Let users inspect, steer, stop, and close supervised child
runs without losing the parent conversation.

**Blocked by:** PL-24: Supervise read-only subagents

**Acceptance criteria:**

- [ ] Active and completed children have stable identifiers and visible state.
- [ ] Steering and stop requests target exactly one child and are audited.
- [ ] Child approval requests remain visible while another thread is focused.

**Verification:**

- State-machine and renderer tests cover concurrent child updates and actions.

## PL-26: Isolate writing subagents in worktrees

**What to build:** Give an explicitly authorized writing child its own Git
worktree and return an inspectable change summary to the parent.

**Blocked by:** PL-19: Ask for approval at sandbox boundaries; PL-24: Supervise
read-only subagents

**Acceptance criteria:**

- [ ] Worktree creation is explicit, bounded to one repository, and never
      mutates the user's current checkout.
- [ ] Child writes stay inside its worktree.
- [ ] Completion reports changed files, verification, and cleanup state.
- [ ] Applying, committing, or deleting child work remains a separate user
      action.

**Verification:**

- Temporary-repository tests cover success, cancellation, dirty state, and
  cleanup failure.
