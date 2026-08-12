# Session Context Tasks

## SESSION-1: Fork a session from a settled turn

- [x] Accept only replayable settled turn boundaries.
- [x] Record a new ID, parent session and turn, fork time, and lineage.
- [x] Persist a self-contained semantic prefix. Exclude pending tools,
      permissions, queues, processes, and other live runtime state.
- [x] Verify that the fork resumes independently and does not change its parent.

## SESSION-2: Export sessions for human review

- [x] Export deterministic Markdown and self-contained HTML.
- [x] Preserve semantic messages, reasoning summaries, tools, status, errors,
      findings, session identity, and lineage within the redaction and item
      limits.
- [x] Include recorded context transformations and request references.
- [x] Require no external scripts or assets in HTML.

## SESSION-3: Browse session lineage

**Blocked by:** SESSION-1.

- [x] Show source turn, title, model, activity, and lock or corruption state.
- [x] Provide inspect, resume, fork, and export actions through their existing
      workflows.
- [x] Report missing parents, malformed lineage, and cycles while leaving valid
      sessions accessible.

## SESSION-4: Support ephemeral runs

- [x] Accept `--ephemeral` for interactive and headless runs, with
      `--no-session` as an alias.
- [x] Keep the active run in memory without creating session JSONL, artifact
      bodies, a per-session log, or a shared daily log.
- [x] Reject resume and session naming in ephemeral mode while leaving shared
      settings and prompt history under their existing policies.
- [x] Make test and automation helpers ephemeral by default unless the test
      exercises persistence.

## SESSION-5: Inventory the session storage graph

- [x] Build one application-owned inventory of session JSONL, locks,
      per-session logs, artifact references and bodies, trash, and future
      session-owned state.
- [x] Track references across forks and retain an artifact while any live or
      archived session references it.
- [x] Report missing, malformed, multiply referenced, and unreferenced state
      without making valid sessions inaccessible.
- [x] Calculate counts, bytes, age, archive and pin state, and reclaimable bytes
      without loading artifact bodies.
- [x] Keep storage and retention policy out of `thndrs-agent` public APIs.

## SESSION-6: Add manual lifecycle controls

**Blocked by:** SESSION-5.

- [x] Add archive, unarchive, pin, and unpin without changing replayable session
      history.
- [x] Make delete show the exact session-owned state and shared artifacts it
      will preserve before confirmation.
- [x] Move deleted state to application trash and support restore during the
      configured grace period. Require an explicit option for permanent
      deletion.
- [x] Reject deletion of active or locked sessions. Allow explicit deletion of
      pinned sessions only after confirmation.
- [x] Apply moves atomically where practical and leave recoverable diagnostics
      after partial filesystem failures.

## SESSION-7: Add retention policy and prune previews

**Blocked by:** SESSION-5 and SESSION-6.

- [x] Configure enabled, maximum age, maximum live count, minimum age, and trash
      retention with defaults of 30 days, 200 sessions, one day, and seven days.
- [x] Select the oldest unpinned, unlocked sessions when either the age or count
      limit is exceeded. Derive age from recorded durable activity rather than
      filesystem modification time, and do not treat a title as a pin.
- [x] Add `session prune` overrides for older-than and keep-count, plus a dry run
      that reports IDs, titles, ages, sizes, and selection reasons.
- [x] Use one deterministic selector for previews, explicit pruning, automatic
      collection, and tests.
- [x] Cover disabled retention, recent-session bursts, clock boundaries, locked
      sessions, pins, archives, forks, and partial failures.

## SESSION-8: Collect expired and orphaned state

**Blocked by:** SESSION-7.

- [x] Run collection at startup or resume when the last successful pass is at
      least 24 hours old, without delaying or failing the agent run.
- [x] Apply the retention preview, expire trash, remove unreferenced artifacts
      and stale temporary state, and skip live locks. Preserve corrupt sessions
      and artifacts whose reachability cannot be proven.
- [x] Give shared daily logs independent age and size caps. Move a per-session
      log with its deleted session graph.
- [x] Record the policy version, last successful run, reclaimed bytes, skipped
      state, and bounded failure diagnostics.
- [x] Prove repeated collection is idempotent and cannot remove state reachable
      from a retained or restored session.

## SESSION-9: Expose storage and lifecycle in CLI and TUI

**Blocked by:** SESSION-6, SESSION-7, and SESSION-8.

- [x] Add `session storage` totals for live, archived, pinned, trash, artifact,
      and log state, including bytes reclaimable under the current policy.
- [x] Provide deterministic JSON for storage reports, prune previews, and
      lifecycle results used by non-interactive callers.
- [x] Add search, archive, pin, delete, restore, and storage details to the
      session browser using the same inventory and lifecycle operations as the
      CLI.
- [x] Keep the current session visible and protected, require confirmation for
      destructive actions, and refresh the picker after each operation.
- [x] Add a workspace-scoped purge preview and confirmation that uses the same
      ownership and shared-reference rules.
- [x] Test narrow layouts, large inventories, stale locks, corrupt sessions,
      cancellation, and partial cleanup failures.

## CONTEXT-1: Expand the live context surface

- [ ] Build the surface from the current `ContextLedger`. UI code must not
      recalculate accounting or read raw provider payloads.
- [ ] Show projected input used and available, percentage remaining, target and
      auto-compaction thresholds, estimate provenance, and model-limit source
      and confidence.
- [ ] Show selected and available totals for Harness, Instructions, Skills,
      Pinned, Conversation, Summaries, and Tool results.
- [ ] Show selected, omitted, recoverable, and protected counts. Include
      diagnostics for incomplete data and fallback limits.
- [ ] Limit `/context all` to 64 items. In `/context item <id>`, show origin,
      lifecycle, inclusion or omission reason, estimate, artifact handle,
      protection, and recovery details.
- [ ] Distinguish the next-request projection from historical snapshots.
- [ ] Test wide, narrow, empty, unknown-limit, and over-threshold states.

## CONTEXT-2: Persist a snapshot for every provider request

**Blocked by:** CONTEXT-1.

- [ ] Extend the ledger session record with a versioned `ContextSnapshot`
      instead of adding a parallel event store.
- [ ] Before dispatch, identify the snapshot by session, request, turn, attempt,
      model, and route.
- [ ] Record limit provenance, budget thresholds, candidate and selected counts,
      category totals, projected usage, transformations, and diagnostics.
- [ ] After completion, attach provider usage and its measurement provenance
      when the provider reports it.
- [ ] Store aggregates and stable item references or deltas. Do not copy item
      bodies into each snapshot.
- [ ] Record interrupted and failed attempts and keep retries distinct across
      resume.
- [ ] Serialize missing measurements as unknown, not zero.

## CONTEXT-3: Compare request context

**Blocked by:** CONTEXT-2.

- [ ] Make `/context changes` compare the latest two snapshots by default and
      accept two request IDs.
- [ ] Group additions, removals, lifecycle changes, replacements, reductions,
      compactions, recoveries, omissions, and instruction-scope changes by
      stable item ID.
- [ ] Compare candidate and selected totals, budget thresholds, projected size,
      provider input when measured, and measurement provenance.
- [ ] Record each compaction's before and after estimates, reclaimed amount,
      retained recent range, summary reference, source range, and recovery
      details.
- [ ] Use one diff algorithm across turns, retries, resumed sessions, and fork
      lineage.
- [ ] Cap diff output and retain item references and measurements after bodies
      expire or when content was never retained.

## CONTEXT-4: Show context transformations in the transcript

**Blocked by:** CONTEXT-2 and CONTEXT-3.

- [ ] Add context events for compaction, tool-output reduction, and recovery.
- [ ] Show before and after measurements, the affected request, and a link to
      the recorded details.
- [ ] Give context events less visual weight than conversation messages. Do not
      render them as tool calls or assistant content.
- [ ] Reconstruct the events after resume and include them in session exports.

## CONTEXT-5: Add an optional context status segment

**Blocked by:** CONTEXT-1.

- [ ] Add a configurable `context-remaining` segment. Omit it from the sparse
      default and render a plain value such as `ctx 73% left` when enabled.
- [ ] Calculate the value from the refreshed projection used by `/context`.
- [ ] Hide the segment when the value is unknown or space is tight. Do not show
      a gauge or raw token total.
- [ ] Warn temporarily when the target or auto-compaction threshold is crossed.
- [ ] Test updates after dispatch, completion, compaction, model changes, and
      resume.

## CONTEXT-6: Keep context, usage, and capacity distinct

- [ ] Use `/context` only for one request's projected pressure against its model
      limit.
- [ ] In `/usage`, show measured request and session input, output, reasoning,
      cache read, cache write, request count, and cost when reported.
- [ ] Label provider account capacity and refresh state separately from context
      pressure and cumulative consumption.
- [ ] Include estimate and measurement provenance in summaries, details,
      exports, and comparisons.
- [ ] When cache data is absent, leave fresh input unknown instead of deriving it
      from the provider's total.

## CONTEXT-7: Inspect one provider request

**Blocked by:** CONTEXT-2 and CONTEXT-6.

- [ ] Open request details by request and attempt ID.
- [ ] Show model, route, duration, time to first token, projected input, measured
      provider input, fresh and cached input, output, and reasoning when known.
- [ ] Show tool count and duration and link reductions, compactions, recoveries,
      and other context changes.
- [ ] Link the request to its turn, snapshot, provider operation, and transcript
      entries without exposing raw request bodies.
- [ ] Label unavailable timings and provider measurements as unknown.

## CONTEXT-8: Provide CLI and export parity

**Blocked by:** CONTEXT-3 and CONTEXT-7.

- [ ] Extend the versioned context export with snapshots, diffs, request
      accounting, transformations, diagnostics, and measurement provenance.
- [ ] Add `thndrs context`, `thndrs context --json`, `thndrs context changes`,
      `thndrs usage --json`, and `thndrs session inspect <id> --json`.
- [ ] Generate TUI, CLI, Markdown, JSON, and session JSONL output from the
      persisted semantic records instead of reconstructing provider requests.
- [ ] Serialize schema and policy versions, stable IDs, lineage, redaction
      state, artifact limits, and deterministic ordering.
- [ ] Export metadata when content was not retained. Reject unsupported capture
      options and requests over the configured size limit.

## CONTEXT-9: Define opt-in retained request content

**Blocked by:** CONTEXT-2, SESSION-6, SESSION-7, and SESSION-8.

- [ ] Retain metadata only by default. Add a per-run opt-in for normalized
      request content and artifact bodies.
- [ ] Remove credentials and secrets and exclude raw provider wire payloads in
      every capture mode.
- [ ] Approve access, redaction, retention, deletion, and on-disk size rules
      before enabling content capture.
- [ ] Do not write captured content when sanitization or limit enforcement
      fails.
- [ ] Apply the capture policy to inspect, compare, resume, fork, and export.

## CONTEXT-10: Export optional OpenTelemetry observations

**Blocked by:** CONTEXT-8.

- [ ] Read telemetry from persisted snapshots, diffs, request accounting, and
      transformation records.
- [ ] Emit request and tool timings, token measurements, counts, errors, and
      compaction or reduction before-and-after values with capped cardinality.
- [ ] Omit prompt, response, tool, and artifact content unless CONTEXT-9 permits
      capture for the run.
- [ ] Include estimate and measurement provenance and use provider-neutral
      export types.
- [ ] Ensure exporter failures do not block or change an agent request.
