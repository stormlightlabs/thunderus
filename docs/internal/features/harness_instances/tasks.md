---
title: Tickets - Harness Instances And Daily Driver
status: Draft
captured: 2026-08-05
---

# Tickets: Harness Instances And Daily Driver

Implementation tickets for [the harness-instance and daily-driver plan](plan.md).
The pair consolidates the unfinished ACP expansion, provider/workbench gate, and
process-supervision parking lot. Token optimization and Quiver remain separate.

## Milestone 1: Establish The Product Baseline

**Exit criterion:** A ranked friction ledger and typed instance contract explain
what to build without committing to a broad UI rewrite.

### Ticket 1: Run The Daily-Driver Workflow Study

**What to build:** Exercise the same five coding workflows in current `thndrs`,
Codex, and Pi, then record reproducible friction and a ranked set of product
gaps.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] The study covers orientation/follow-up, edit/review/verify,
      failure/steering, interrupt/resume, and read-only delegation.
- [ ] Findings record commands, terminal dimensions, model, cwd, and session
      policy without storing sensitive prompts or credentials.
- [ ] Every finding names lost time, lost evidence, unclear state, or failed
      control rather than visual preference alone.
- [ ] The ledger evaluates native selection/search/scrollback, queued-input
      management, change review, session recovery, usage visibility, and
      process cleanup.
- [ ] The owner ranks the small set of gaps that prevent daily use.

**Verification:**

- Repeat the top three findings and confirm they are reproducible.

### Ticket 2: Define The Harness Instance Contract

**What to build:** Add provider-neutral types for instance specification,
identity, lifecycle, authority, bounds, account-capacity snapshot, status, and
settled result.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] A specification includes exact model, absolute cwd, session policy,
      reasoning/search settings, authority, parent ID, depth, and concurrency
      budget.
- [ ] Lifecycle transitions cover starting, ready, running,
      waiting-permission, stopping, completed, failed, and cancelled.
- [ ] Invalid transitions, unbounded specifications, traversal, and implicit
      write authority are rejected.
- [ ] Results contain bounded semantic evidence and session/change handles,
      never credentials or an unbounded transcript.
- [ ] Account capacity distinguishes provider-reported, stale, and unavailable
      fields from per-request token consumption.
- [ ] The contract describes ChatGPT Codex, OpenCode Zen, OpenCode Go, and
      configured ACP model selections without provider wire types.

**Verification:**

- Pure state-machine, validation, serialization, ordering, cap, provenance,
  freshness, and redaction tests.

## Milestone 2: Make ChatGPT And OpenCode First-Class

**Exit criterion:** A fresh user can choose ChatGPT Codex or OpenCode, see
remaining account capacity, and complete a verified coding session. No active
product workflow depends on Umans.

### Ticket 3: Retire Umans And Promote OpenCode

**What to build:** Remove Umans from supported setup and release paths, then
bring OpenCode Zen and OpenCode Go to the same setup, recovery, model-selection,
and session-readiness bar as ChatGPT Codex.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] First-run setup presents ChatGPT Codex, OpenCode Zen, and OpenCode Go;
      Umans is absent.
- [ ] Existing Umans configuration fails with one actionable unsupported-route
      message and does not silently select another provider.
- [ ] Stored Umans credentials are not read for runs, displayed, migrated, or
      deleted automatically.
- [ ] OpenCode credential setup, validation, cancellation, rejected-key
      recovery, model discovery, and reasoning controls have deterministic
      coverage.
- [ ] Public documentation and release smokes name ChatGPT Codex and OpenCode as
      the supported provider routes.
- [ ] Dead Umans adapter/auth/model code is removed after all callers and docs
      migrate, without changing provider-neutral library APIs.

**Verification:**

- Fresh-HOME setup tests, migration diagnostics, provider fixtures, session
  tests, model-picker snapshots, and public documentation build.

### Ticket 4: Show Remaining Subscription And Credit Capacity

**What to build:** Fetch and render provider-reported remaining usage for the
active ChatGPT Codex or OpenCode account through a typed, cached capacity
snapshot.

**Blocked by:** Ticket 2: Define The Harness Instance Contract; Ticket 3:
Retire Umans And Promote OpenCode

**Acceptance criteria:**

- [ ] ChatGPT capacity shows every returned rate-limit window, used/remaining
      percentage, reset time, limit state, and optional plan/credit data.
- [ ] OpenCode Go shows returned subscription allowance and reset data;
      OpenCode Zen shows returned credit balance and monthly-limit data.
- [ ] Only supported provider account APIs are used. Missing provider fields
      render as unavailable with a console/dashboard handoff.
- [ ] `/usage` refreshes and shows full detail; `/status` and the orientation
      surface show compact active-provider state.
- [ ] ACP and JSONL instance metadata can expose a redacted snapshot so a
      supervisor can avoid a depleted route.
- [ ] Values include observation time and stale state. Unknown is not zero, and
      request tokens are not presented as remaining subscription capacity.
- [ ] No raw account response, email, token, account ID, or authorization URL is
      persisted.

**Verification:**

- Provider response fixtures cover multiple windows, balances, resets, limit
  reached, missing fields, stale cache, auth failure, service failure, and
  redaction. Live checks require separate approval.

## Milestone 3: Make `thndrs` A Dispatchable Instance

**Exit criterion:** Another harness can launch a configured `thndrs` process,
control a long-lived session over ACP, or consume a one-shot JSONL run.

### Ticket 5: Unify JSONL And ACP Instance Identity

**What to build:** Project Ticket 2 identity, lifecycle, and safe capacity
metadata through the existing headless JSONL and ACP server modes without
turning JSONL into a second interactive protocol.

**Blocked by:** Ticket 2: Define The Harness Instance Contract

**Acceptance criteria:**

- [ ] JSONL start/terminal events identify instance, model, cwd, and session
      policy with a versioned compatible change.
- [ ] ACP session metadata maps to the same local instance identity and settled
      outcome.
- [ ] stdout remains protocol-clean; safe diagnostics stay on stderr.
- [ ] Unsupported model, missing credential, invalid cwd, protocol mismatch,
      cancellation, and child exit have distinct outcomes.
- [ ] Existing callers that ignore new optional metadata continue to work.

**Verification:**

- Black-box executable tests cover JSONL and ACP startup, prompt, cancellation,
  failure, and clean shutdown.

### Ticket 6: Validate Real ACP Dispatch And Packaging

**What to build:** Fold in the ACP expansion feature by validating the packaged
server with a real client/harness and preparing accurate discovery metadata.

**Blocked by:** Ticket 5: Unify JSONL And ACP Instance Identity

**Acceptance criteria:**

- [ ] One real client, version, and date prove initialization, streaming,
      cancellation, permissions, capacity metadata, and session operations.
- [ ] Every compatibility fix has a fake-client regression test.
- [ ] Registry/discovery material names the actual command and capabilities;
      checks run without publishing.
- [ ] Stdio remains the only transport until a concrete deployment cannot use
      a local process.

**Verification:**

- `cargo test -p thndrs --test acp_server_smoke`
- documented real-client smoke with protocol-clean stdout

### Ticket 7: Prove Supported Provider Routes Through Instances

**What to build:** Verify that an instance can select ChatGPT Codex, OpenCode
Zen, OpenCode Go, or a configured `acp:<name>` route with consistent session,
permission, event, and capacity behavior.

**Blocked by:** Tickets 4 through 6

**Acceptance criteria:**

- [ ] Deterministic provider fakes cover every route without network access.
- [ ] Exact model IDs survive configuration, child startup, events, session
      metadata, and final results.
- [ ] Provider setup and capacity failures stay distinct from harness lifecycle
      and model-task failures.
- [ ] Permissions and workspace containment do not vary by dispatch surface.
- [ ] Bounded opt-in smokes cover one current ChatGPT Codex model and one model
      from each OpenCode route without retaining secrets or raw payloads.

**Verification:**

- Provider request/stream fixtures, black-box instance tests, and separately
  approved live smokes.

## Milestone 4: Let `thndrs` Dispatch `thndrs`

**Exit criterion:** A foreground parent supervises bounded read-only child
instances through ACP without embedding another agent loop.

### Ticket 8: Supervise One Read-Only Child Process

**What to build:** Spawn one child `thndrs ... acp serve`, negotiate ACP,
delegate a read-only task, collect bounded updates, and return a typed result.

**Blocked by:** Ticket 6: Validate Real ACP Dispatch And Packaging

**Acceptance criteria:**

- [ ] The child receives explicit executable, cwd, model, session, and read-only
      authority.
- [ ] The parent owns pipes, process-group cleanup, cancellation, timeout, and
      unexpected-exit handling.
- [ ] Child context, transcript, and session remain separate from the parent.
- [ ] The parent receives a bounded summary plus instance/session handles.
- [ ] Startup can reject a depleted provider route using fresh capacity data;
      stale or unavailable capacity warns but does not silently reroute.
- [ ] Recursive delegation is disabled in this slice.

**Verification:**

- Fake-child tests cover completion, provider failure, depleted capacity,
  malformed protocol, timeout, cancellation, permission, EOF, and panic.

### Ticket 9: Add Bounded Multi-Instance Supervision

**What to build:** Run explicitly requested independent read-only tasks in
several child instances with hard concurrency and depth limits.

**Blocked by:** Ticket 8: Supervise One Read-Only Child Process

**Acceptance criteria:**

- [ ] Delegation requires direct user or project instruction and an independent
      bounded task per child.
- [ ] Concurrency/depth and account-capacity policy are enforced before launch
      and recorded in session audit.
- [ ] Parent cancellation settles every owned child before completion.
- [ ] One child failure does not hide other child outcomes.
- [ ] No child gains write authority or further delegation implicitly.

**Verification:**

- Fake-process tests cover bounds, partial failure, cancellation, ordering,
  capacity changes, and attempted recursion.

### Ticket 10: Expose Sparse Instance Controls

**What to build:** Let users list, inspect, steer, stop, and close one child
instance from transcript-oriented detail surfaces.

**Blocked by:** Ticket 8: Supervise One Read-Only Child Process

**Acceptance criteria:**

- [ ] Children show stable ID, task, model, cwd, lifecycle, elapsed time,
      capacity state, and latest bounded status.
- [ ] Steering and stop actions resolve exactly one instance and are audited.
- [ ] Permission requests remain visible while another instance is focused.
- [ ] Closing a settled instance does not delete its durable session.
- [ ] The TUI remains a transcript, not a pane manager or dashboard.

**Verification:**

- State-machine and renderer snapshots cover concurrent updates, focus,
  steering, permissions, stopping, settlement, and removal.

## Milestone 5: Close The Daily-Driver Gaps

**Exit criterion:** The owner completes the study workflows without a remaining
high-severity friction item.

### Ticket 11: Fix Transcript Ownership And Terminal Navigation

**What to build:** Implement the smallest terminal-ownership change that closes
the study's scrollback, selection, search, copy, and resize findings.

**Blocked by:** Ticket 1: Run The Daily-Driver Workflow Study

**Acceptance criteria:**

- [ ] The design is justified by reproduced study evidence.
- [ ] Completed rows are stable and searchable/selectable through the chosen
      terminal workflow.
- [ ] Prompt, streaming rows, focused surfaces, and cursor redraw without
      corrupting committed transcript output.
- [ ] Resize, suspend/resume, crash cleanup, mouse-off selection, and narrow
      terminals behave predictably.

**Verification:**

- Semantic frame tests, bounded PTY tests, and repetition of affected workflows.

### Ticket 12: Make Queued Input Inspectable And Editable

**What to build:** Add a focused queue surface for inspect, edit, remove,
reorder, and steering/follow-up target changes.

**Blocked by:** Ticket 1: Run The Daily-Driver Workflow Study

**Acceptance criteria:**

- [ ] Queue order, target, and bounded preview are visible without exposing text
      to the model early.
- [ ] Actions affect exactly one item and are audited.
- [ ] Cancellation preserves follow-ups and settles steering through the
      existing lifecycle contract.
- [ ] Audit failure does not lose queued input.

**Verification:**

- Pure queue, session, renderer, and active-run keyboard tests.

### Ticket 13: Make Review And Resume Complete Workflows

**What to build:** Connect diff/tool detail to explicit read-only review and add
a recent-session picker that preserves the current draft.

**Blocked by:** Ticket 1: Run The Daily-Driver Workflow Study

**Acceptance criteria:**

- [ ] Change review shows paths, bounded diffs, verification, failures, and
      recoverable evidence without writing or mutating Git.
- [ ] Structured findings require severity, evidence, and tight locations; a
      clean review is explicit.
- [ ] The session picker distinguishes recent sessions and uses existing
      validation and exclusive-lock rules.
- [ ] Corrupt, missing, incompatible, or locked sessions fail without losing
      the current transcript or draft.

**Verification:**

- Temporary repositories plus review, session, lock, renderer, and PTY tests.

## Milestone 6: Approve The Daily Driver

**Exit criterion:** The owner decides whether `thndrs` is ready to replace
Codex/Pi for normal foreground work, based on repeated tasks.

### Ticket 14: Run The Dogfood Gate

**What to build:** Repeat the five study workflows with Sol as the normal model,
ChatGPT/OpenCode alternatives, visible capacity, and external/self-dispatched
instances.

**Blocked by:** Tickets 4 and 7 through 13

**Acceptance criteria:**

- [ ] Every high-severity Ticket 1 item is closed or explicitly accepted.
- [ ] Sol completes implementation, diagnosis, review, failure recovery,
      cancellation/resume, and child dispatch with objective verification.
- [ ] At least one ChatGPT Codex, OpenCode Zen, and OpenCode Go child is
      dispatched through the same contract.
- [ ] Remaining subscription capacity is accurate or clearly unavailable
      throughout foreground and supervised work.
- [ ] Herdr hosts `thndrs`, Codex, and Pi panes without special integration or
      process/session ambiguity.
- [ ] Repeated harness failures become deterministic regression tests.

**Verification:**

- Review the redacted dogfood ledger, capacity snapshots, instance traces,
  deterministic checks, and accepted limitations.

## Deferred Ticket: Isolate Writing Children

Give an explicitly authorized writing child a separate workspace or worktree
and return an inspectable change summary. Applying, committing, and cleanup
remain separate user actions.

**Blocked by:** Ticket 9; explicit approval for the isolation design and every
Git operation

## Frontier

Tickets that can start immediately:

- Ticket 1: Run The Daily-Driver Workflow Study
- Ticket 2: Define The Harness Instance Contract
- Ticket 3: Retire Umans And Promote OpenCode

Ticket 1 unlocks evidence-based UX work. Tickets 2 and 3 unlock remaining usage
and process dispatch. Read-only self-dispatch starts only after real ACP client
validation.
