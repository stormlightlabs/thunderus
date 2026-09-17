# Lifecycle Hook Tasks

## HOOK-1: Define the provider-neutral hook API

**What to build:** Add typed lifecycle inputs and outcomes, an ordered
dispatcher, and one application-owned execution boundary to `thndrs-agent`.
Disambiguate the existing permission and execution callbacks before using
"hook" for lifecycle interception.

**Acceptance criteria:**

- [ ] The ten initial session, turn, model, tool, compaction, and stop points
      expose only the context and outcomes defined in the plan.
- [ ] Handlers run sequentially in effective configuration order; later
      handlers see earlier valid transformations and denial ends the chain.
- [ ] Outcomes that leave a lifecycle boundary short-circuit the remaining
      handlers at that point.
- [ ] Hook code cannot mutate provider wire payloads, run authority, tool
      identity, captured evidence, actual tool status, or display output.
- [ ] Hook context and stop continuation are bounded and recorded in the
      context ledger with provenance.
- [ ] `turn_end` is finalization-only and cannot change a settled terminal
      outcome or durable session record.
- [ ] Cancellation and typed hook failures settle without panics or leaked
      tasks.
- [ ] `ToolPermissionHook` and `ToolExecutionHook` become clearly named
      application adapters, with compatibility aliases if required.

**Verification:**

- Focused `thndrs-agent` tests for ordering, chained transformations, denial,
  invalid point/outcome pairs, limits, cancellation, finalizer isolation, and
  the stop continuation budget.
- Compile tests or an equivalent public-API test for an application-provided
  hook handler.

## HOOK-2: Integrate hooks into the shared run loop

**What to build:** Route turn, model, tool, and stop hooks through one execution
path used by real providers, the fake provider, TUI, JSONL, and ACP surfaces.

**Blocked by:** HOOK-1.

**Acceptance criteria:**

- [ ] `turn_start` runs once after user input is accepted and before any
      compaction, model, or tool work. Rejection settles the turn as failed.
- [ ] `pre_model` runs before every provider lowering and may therefore run
      several times in one turn.
- [ ] `stop` runs only when normal completion checks propose finishing, after
      queued steering has been read. Continuation stays in the current turn.
- [ ] `pre_tool_use` runs after normalization but before permission and
      execution; rewritten arguments are reparsed and revalidated against the
      active schema and authority.
- [ ] Permission evaluates the effective request, and no hook outcome can
      grant permission or bypass the application execution adapter.
- [ ] A denied tool call becomes an attributed failed result without reporting
      that execution occurred.
- [ ] `post_tool_use` may change only the bounded model projection. Agent
      events, evidence, display output, and session records retain the actual
      outcome.
- [ ] Hook invocation and outcome events are bounded, redacted, and do not
      trigger hooks recursively.
- [ ] `turn_end` runs exactly once after the result and any required session
      write settle on every finished, failed, and cancelled path, before the
      matching terminal event.
- [ ] Every matching `turn_end` finalizer runs. A finalizer failure is reported
      without reopening the turn, changing its outcome, or skipping later
      finalizers.
- [ ] Provider, fake-provider, permission rejection, cancellation, and
      post-execution failure paths settle consistently.

**Verification:**

- Focused run-loop tests for each point, repeated model requests, tool rewrite
  and denial, permission ordering, preserved evidence, stop continuation, and
  exactly-once finalization across every terminal path.
- Adapter tests showing equivalent lifecycle behavior through the direct,
  JSONL, and ACP entry points without exposing provider-native payloads.

## HOOK-3: Integrate session and compaction hooks

**What to build:** Invoke lifecycle hooks from the application-owned session
activation, explicit close, manual compaction, and automatic compaction paths.

**Blocked by:** HOOK-1.

**Acceptance criteria:**

- [ ] `session_start` runs once after durable context is loaded and before a new
      or resumed session accepts work.
- [ ] `session_end` runs before explicit close. Rejection leaves the close
      uncommitted, while process shutdown still settles resources and records
      any hook failure.
- [ ] Manual and automatic compaction use the same `pre_compact` and
      `post_compact` dispatch path.
- [ ] `pre_compact` additions cannot change the source range, protected facts,
      recovery handle, selected model, or review policy.
- [ ] A replaced post-compaction candidate passes the normal schema,
      source-range, protected-fact, and review checks before commit.
- [ ] Denial, rejection, failure, or cancellation preserves the active context
      and pending user turn.

**Verification:**

- Focused tests for new and resumed sessions, explicit close and shutdown,
  manual and automatic compaction, replacement revalidation, rejection, stale
  recovery handles, and pending-turn preservation.

## HOOK-4: Add global command-backed hooks

**What to build:** Load global `[[hooks]]` entries from the existing TOML
configuration and execute them through a bounded, versioned JSON protocol.

**Blocked by:** HOOK-2 and HOOK-3.

**Acceptance criteria:**

- [ ] Strict configuration validates unique names, supported points, direct
      executable and argument vectors, positive timeouts, and exact tool-name
      matchers where applicable.
- [ ] Commands receive versioned, point-specific JSON on stdin and return one
      schema-checked, size-limited JSON outcome on stdout.
- [ ] Execution has bounded stdout, stderr, runtime, cancellation, and process
      settlement. Diagnostics identify the hook and phase without exposing
      payloads or secrets.
- [ ] The command uses no implicit shell and inherits only the documented
      environment. Provider credentials are not forwarded automatically.
- [ ] A global executable cannot resolve relative to or be shadowed by the
      active project.
- [ ] Status makes configuration order and process authority clear and does
      not claim sandbox isolation.
- [ ] Public documentation defines the configuration, protocol version,
      failure behavior, matching, ordering, data exposure, and security model.

**Verification:**

- Focused configuration and process tests for precedence, matching, malformed
  input and output, non-zero exit, output limits, timeout, cancellation,
  redaction, and executable resolution.
- End-to-end fixtures for turn start and end, a context addition, tool denial,
  argument rewrite, model-result rewrite, compaction and session decisions, and
  stop continuation.
- `pnpm --dir docs build` after the public documentation is added.

## HOOK-5: Activate trusted project hooks

**What to build:** Include project hooks in the shared project trust model and
merge active definitions after global hooks without allowing project code to
activate during discovery.

**Blocked by:** HOOK-4.

**Acceptance criteria:**

- [ ] Untrusted or stale project hooks are parsed for bounded status reporting
      but never spawned.
- [ ] Approval applies only to the displayed project configuration hash;
      inspection and revocation use the shared trust surfaces.
- [ ] Effective order is global declaration order followed by trusted project
      declaration order, with scope retained in diagnostics and events.
- [ ] A project hook cannot replace or suppress a global hook or expand the
      current run authority.
- [ ] Editing trusted configuration blocks the changed hooks until the new
      hash is approved.
- [ ] Status states plainly that an active command hook runs with thndrs
      process permissions when no enforcing sandbox is present.

**Verification:**

- Focused trust-state tests for absent, approved, stale, revoked, and malformed
  project configuration.
- A process-spawn test proving discovery and blocked status do not execute a
  project command, plus ordering tests with global and trusted project hooks.
