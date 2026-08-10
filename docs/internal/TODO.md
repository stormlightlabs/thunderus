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
- [ ] OpenCode Zen and OpenCode Go credential entry, environment overrides,
      rejected-key recovery, and transient service failure are exercised
      without recording credentials.
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

Added a headless command that runs one coding prompt through the normal provider,
tool, context, and session paths.

## PL-3: Stream headless events as JSONL

Added a machine-readable mode for the headless command using
the provider-neutral event vocabulary.

## PL-4: Accept piped prompt input

Lets the headless command combine bounded stdin content with an explicit prompt.

## PL-5: Run without saving a session

Added `--ephemeral` (also available as `--no-session`) for interactive and
headless runs. Ephemeral runs keep their working state in memory and leave
sessions, session artifacts, and per-session logs untouched. Credential loading,
configuration, and shared prompt history continue to work; the TUI and JSONL
`started` event identify the mode.

## PL-6: Resume sessions from the TUI

**What to build:** Add a session picker that resumes a validated local session
without requiring its identifier or leaving the TUI. Reuse the restoration path
shared by startup `sessions resume` and `/resume`.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] The picker lists recent sessions with enough metadata to distinguish
      them.
- [ ] Selecting a session uses the existing validation and exclusive-lock
      rules.
- [ ] Cancellation and corrupt, missing, or already locked sessions leave the
      current session and draft unchanged.
- [ ] Successful selection restores the transcript, context control state,
      usage, and turn count before the next prompt can run.

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
- [ ] The fork has a new identifier and records its source session, source
      sequence, and turn in optional backward-compatible metadata.
- [ ] The new session contains a self-contained replayable prefix and remains
      usable if the source file is moved or removed.
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

## PL-27: Inspect context at a provider request

**What to build:** Add a TUI inspector for the context considered and selected
at each recorded provider request in the active or resumed session.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Users can choose a turn, request, and retry attempt without reading raw
      session JSONL.
- [ ] The inspector shows every context item's kind, source, estimated size,
      visibility, lifecycle, and inclusion or exclusion reason.
- [ ] Budget totals, provider/model identity, serialized request size, token
      estimate provenance, provider usage, and reduction receipts are visible.
- [ ] The current next-request projection is clearly distinguished from a
      request that was actually sent.
- [ ] Content-free historical records remain useful after resuming a session;
      missing records produce an explicit unavailable state.

**Verification:**

- State and renderer tests cover multiple requests in one turn, retries,
  compaction, pinned and dropped items, resume, and sessions without accounting
  records.

## PL-28: Compare context between requests

**What to build:** Let users diff two request snapshots to explain how the
model-visible working set and context pressure changed.

**Blocked by:** PL-27: Inspect context at a provider request

**Acceptance criteria:**

- [ ] The diff groups added, removed, visibility-changed, lifecycle-changed,
      and reduction-changed context items by stable identifier.
- [ ] Budget, serialized-size, estimated-token, and reported-usage changes are
      shown separately so estimates are not presented as provider facts.
- [ ] Comparing requests across turns and forked sessions uses the same
      projection and handles absent historical records explicitly.
- [ ] Large diffs remain bounded and searchable without loading context bodies.

**Verification:**

- Deterministic request-accounting fixtures cover unchanged, added, removed,
  compacted, retried, and cross-session comparisons.

## PL-29: Browse session lineage as a tree

**What to build:** Extend the session browser to show roots, forks, and the
selected fork point as a navigable tree.

**Blocked by:** PL-6: Resume sessions from the TUI; PL-8: Fork a session from a
completed turn

**Acceptance criteria:**

- [ ] Sessions created before lineage metadata are displayed as roots.
- [ ] Each child shows its source turn, title, model, last activity, and lock or
      corruption state without opening it for writing.
- [ ] Selecting a node can inspect, resume, or fork it through the existing
      validation paths.
- [ ] Missing parents, malformed lineage, and cycles are visible diagnostics
      and cannot crash or hide otherwise valid sessions.

**Verification:**

- Session graph and renderer fixtures cover multiple roots, deep and wide
  forks, legacy sessions, missing parents, cycles, locks, and corrupt records.

## PL-30: Capture opt-in request projections for debugging

**What to build:** Add an explicit diagnostic mode that preserves a bounded,
redacted model-message projection when context metadata cannot explain a
provider request.

**Blocked by:** PL-27: Inspect context at a provider request

**Acceptance criteria:**

- [ ] Capture is off by default, requires an explicit per-run choice, and the
      TUI states that prompt and tool content may be persisted.
- [ ] The durable record stores the normalized model-message projection, not
      provider wire payloads or authentication data.
- [ ] Size and retention limits fail closed, and inspection identifies omitted
      or redacted content.
- [ ] Inspect and export surfaces preserve existing redaction guarantees and
      clearly distinguish captured content from content-free accounting.

**Verification:**

- Session, inspection, and export tests cover default-off behavior, opt-in
  capture, redaction, truncation, retention, and resume.
