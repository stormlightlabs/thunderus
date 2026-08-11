# Task List/To-Dos

## P1 — UI Foundation

### UI-1: Split `App` into cohesive state domains

Reduced field coupling while preserving one top-level update path.

### UI-2: Normalize input into semantic actions

Ensured raw Crossterm events do not directly implement product
behavior across large mode-specific branches.

### UI-3: Isolate update effects from state transitions

Made state transitions pure where practical and make every
terminal, filesystem, provider, process, clipboard, and session effect
explicit.

### UI-4: Introduce stable transcript blocks

Modeled transcript history as semantic, identifiable blocks whose
lifecycles update in place.

### UI-5: Consolidate bounded rendering on Ratatui

**Outcome:** Ratatui is the only bounded-screen renderer. The former iocraft
canvas-to-row path has been removed after parity coverage.

**Blocked by:** UI-1. UI-4 should land first for transcript surfaces;
independent focused surfaces may move earlier.

**Acceptance:**

- [x] Port one focused surface at a time from `IocraftSurfaceRenderer` to a
      direct Ratatui widget consuming the existing semantic projection.
- [x] Add only the characterization test needed for each surface before moving
      it; use existing snapshots and state-transition tests wherever they already
      protect the behavior.
- [x] Remove decorative borders and box-drawing chrome from the normal frame,
      composer, focused surfaces, pickers, permissions, help, and details. Use
      spacing, alignment, background, text attributes, selection, and accent
      glyphs for hierarchy and focus.
- [x] Preserve content, focus, accessibility labels, cursor, narrow/short
      behavior, and terminal cleanup. Treat the borderless presentation as an
      intentional parity exception.
- [x] Remove `renderer/adapter.rs`, its snapshots, and the `iocraft` dependency
      only after the final caller moves.
- [x] Keep Crossterm and focused Unicode/text utilities. Add no second general
      component or layout framework; justify any new utility with a concrete
      editor, wrapping, ANSI, or clipboard requirement.
- [x] Retain custom row/style/layout types only when a non-Ratatui consumer or a
      useful pure presentation boundary remains; otherwise use Ratatui primitives
      directly.
- [x] Rendering performs no filesystem, Git, provider, process, session, or
      clipboard I/O.
- [x] The alternate-screen driver continues to own complete dirty frames and
      restores the terminal on every exit path.

**Verify:** Semantic projection tests, Ratatui buffer snapshots for each moved
surface, borderless normal/narrow/monochrome full-frame `TestBackend` snapshots,
and `cargo tree -i iocraft` showing no application dependency before removal is
declared complete.

**Completed:** 2026-08-10. Focused surfaces now use the shared pure row
projection and Ratatui terminal backend. TestBackend snapshots cover each
surface and borderless full frames at normal, narrow, and style-independent
sizes.

### UI-6: Complete full-screen transcript navigation

**Outcome:** Restore the search, selection, copy, and scrollback affordances the
application takes over from the terminal.

**Blocked by:** UI-4 and UI-5.

**Acceptance:**

- [x] Search shows the query, current/total matches, next/previous navigation,
      no-match state, and safe cancellation.
- [x] Keyboard selection works across wrapped lines and block boundaries;
      mouse selection is enabled only where terminal behavior is reliable.
- [x] Copy uses an explicit action, preserves exact semantic text where
      possible, and reports unavailable clipboard support without losing the
      selection.
- [x] Scrolling away shows an anchored-away indicator; new activity never moves
      that viewport; returning to follow-latest is immediately visible.
- [x] Updating a live block preserves the user's semantic anchor rather than a
      fragile absolute row when wrapping changes.
- [x] Search and selection remain bounded on large transcripts and do not load
      hidden tool bodies unnecessarily.
- [x] Resize, suspend/resume, crash cleanup, mouse-off selection, narrow/short
      terminals, and Unicode are deterministic.

**Verify:** State/model tests, virtual-terminal navigation and selection tests,
Ratatui snapshots, and a real-terminal smoke on the supported terminal matrix.

**Completed:** 2026-08-10. Transcript search, keyboard selection, explicit copy,
and semantic scroll anchors now work in the alternate-screen interface. Search
and selection stay bounded and exclude hidden tool output.

### UI-7: Make queued input inspectable and editable

**Outcome:** Turn queued follow-ups and steering into durable, explicit state
rather than an opaque count.

**Blocked by:** UI-1, UI-2, and UI-3.

**Acceptance:**

- [x] Every queue item has a stable identifier, order, target, kind
      (follow-up/steer), bounded preview, created time, and audit/settlement state.
- [x] A focused queue surface supports inspect, edit, reorder, retarget, delete,
      send after current step, and send now for exactly one item.
- [x] Up/down composer history and queued-item editing have unambiguous focus
      and never silently overwrite draft text.
- [x] Interruption/cancellation preserves unrelated follow-ups and settles
      steering according to documented rules.
- [x] Audit or persistence failure does not lose queued input.
- [x] Queue text and attachment metadata remain redacted and bounded in logs,
      status, and child summaries.

**Verify:** Queue transition and persistence tests, input/focus tests, snapshots,
and an end-to-end streaming run with follow-up, steer, edit, and cancel.

**Completed:** 2026-08-10. Queue items now have stable IDs, persisted lifecycle
transitions, and a focused management surface. Editing preserves the composer
draft, cancellation settles steering without dropping follow-ups, and failed
audit writes retain the queued text without exposing it in status output.

### UI-8: Add a configurable status line

**Outcome:** Replace the fixed footer with one borderless, configurable status
line that shows immediate operational truth without becoming a diagnostics
panel.

**Blocked by:** UI-1 and UI-4.

**Acceptance:**

- [x] The status line distinguishes idle, thinking, named running tool, waiting
      for permission, compacting, cancelling, failed, and complete.
- [x] Configuration selects and orders known typed segments in left and right
      groups. It supports run state, active tool, model/provider route, authority,
      workspace, session, queue count, anchored-away state, and active child count.
- [x] Configuration does not execute commands or interpolate arbitrary
      templates. Invalid or unavailable segment names produce an actionable
      configuration error.
- [x] Every segment declares priority, minimum width, and truncation behavior.
      Narrow layouts drop optional segments before truncating eligible values and
      never wrap the status line.
- [x] Run state, permission waits, failures, and authority remain visible ahead
      of cosmetic context. The default configuration stays sparse.
- [x] Quota, token, account, and detailed diagnostics remain in `/status` or
      `/usage`, not the status line.
- [x] Unknown, unavailable, stale, and zero are visually and semantically
      distinct.
- [x] Tool failures include enough bounded transcript/log context to diagnose
      the failing operation without exposing secrets.

**Verify:** Configuration parse/validation tests, pure status projection tests,
normal/narrow/tiny/monochrome snapshots, and transitions driven by fake
provider/tool/permission events.

**Completed:** 2026-08-10. The footer is now a typed, configurable single-row
status projection with safe precedence, width-aware omission and truncation,
and explicit operational states. Its sparse default keeps authority and active
work visible while leaving usage and account diagnostics in their existing
detail surfaces.

### UI-9: Add structured review as a complete workflow

**Outcome:** Resolve one review target, run with read-only authority, and render
deterministic actionable findings or a clean result.

**Blocked by:** UI-4 and the existing resume/session picker workflow.

**Acceptance:**

- [x] The finding contract requires severity, evidence, and a tight valid
      location; it distinguishes actionable findings from a clean review.
- [x] Exactly one working-tree, revision, range, or session change set is
      resolved before the provider runs.
- [x] Review uses read-only tools and cannot modify the repository.
- [x] The review surface shows paths, bounded diffs, verification, failures,
      unresolved findings, and a clear clean-review outcome.
- [x] Human, JSONL, and ACP output share the semantic finding contract and
      deterministic ordering.
- [x] Invalid/out-of-range findings are rejected rather than rendered as fact.

**Verify:** Finding validation/serialization tests, deterministic fake-provider
review cases, clean/finding/error snapshots, and a bounded real-repository
smoke.

**Completed:** 2026-08-10. `thndrs review` resolves and bounds exactly one
working-tree, revision, range, or redacted session target before invoking the
provider with enforced read-only authority. Validated findings have stable
ordering and a shared serializable contract; human output distinguishes clean
reviews and reports paths, input bounds, verification, and failures.

### UI-10: Make search and file-discovery degradation explicit

**Outcome:** Preserve useful contained search when `fd` or `rg` is unavailable
without pretending the fallback is equivalent.

**Blocked by:** None.

**Acceptance:**

- [x] Prefer `fd` for file discovery and `rg --json` for content search.
- [x] Missing binaries use bounded fallbacks that preserve workspace
      containment, output caps, and generated/vendor exclusions.
- [x] Diagnostics and tool metadata name the selected implementation and mark
      degraded results.
- [x] Fallback behavior cannot escape allowed roots or turn unbounded output
      into transcript/session data.

**Verify:** Deterministic path-injection tests for native and missing-binary
cases, containment tests, cap tests, and transcript metadata snapshots.

**Completed:** 2026-08-11. File discovery prefers `fd`, then `rg --files`, and
content search prefers `rg --json`. Missing binaries use contained in-process
fallbacks with file, byte, depth, result, and line limits. Tool output and
`thndrs doctor` report the selected implementation and identify degraded
results.

### UI-11: Pass the daily-driver gate

**Outcome:** Demonstrate that the refactored TUI is a better daily driver before
instances or external capabilities add new surface area.

**Blocked by:** UI-4 through UI-10. UI-1 through UI-3 must be sufficiently
complete for the exercised flows.

**Acceptance:**

- [x] Re-run the recorded orientation/follow-up, implementation, diagnosis,
      review, verification, failure-recovery, cancellation, queue, and resume flows.
- [x] Sol completes the workflow repeatedly without transcript corruption,
      lost drafts/queues, unclear authority, or terminal cleanup failure.
- [x] Normal and constrained terminal fixtures pass deterministic checks and
      real-terminal review.
- [x] Reproduced harness failures become focused regression tests.
- [x] Long transcripts, streaming updates, resize, and long wrapped prompts
      remain responsive. Add a focused before/after benchmark only when a touched
      path shows risk or a measurable regression.

**Verify:** Focused flow results, current workspace checks, regressions added for
reproduced failures, and the real-terminal QA checklist.

**Completed:** 2026-08-11. The recorded workflows pass in the workspace suite,
including draft and queue preservation, cancellation, recovery, resize, long
prompt, streaming, and session-resume coverage. Rejected ChatGPT sessions and
OpenCode keys now open focused in-app sign-in recovery without losing the failed
prompt; environment overrides explain the required restart. Normal, narrow, and
short setup fixtures are deterministic, and the rebuilt binary passed a
real-terminal review in a dedicated Herdr tab. The touched renderer work remains
bounded to the setup accessory, so no performance benchmark was warranted.

## P2 — Dispatchable Instances

### INST-1: Define the instance contract

**Outcome:** Specify the validated values, lifecycle, authority, evidence, and
failure semantics shared by JSONL, ACP, and parent supervision.

**Blocked by:** None; settle before implementing instance UI or supervision.

**Acceptance:**

- [ ] An instance specification contains exact model route, absolute contained
      cwd, session policy, authority, prompt/task, timeout, evidence limits,
      executable, and protocol.
- [ ] Lifecycle covers starting, ready, running, waiting for permission,
      cancelling, succeeded, failed, and cancelled with valid transitions.
- [ ] Invalid transitions, traversal, implicit defaults, recursive delegation,
      and unbounded specifications are rejected locally.
- [ ] Results contain bounded semantic evidence and durable instance, session,
      and change handles rather than an unbounded transcript copy.
- [ ] ChatGPT Codex, OpenCode Zen, OpenCode Go, and unavailable routes are
      represented without leaking provider payloads into public library APIs.
- [ ] Capacity distinguishes provider-reported fresh/stale/unavailable data;
      unknown is never zero.

**Verify:** Pure validation/transition tests and serialization round trips for
valid, invalid, legacy, and unknown-provider/capacity cases.

### INST-2: Unify JSONL and ACP instance identity

**Outcome:** Map both dispatch surfaces to one local identity and settled
lifecycle without forcing their wire formats to be the same.

**Blocked by:** INST-1.

**Acceptance:**

- [ ] JSONL start and terminal events optionally identify instance, route,
      model, absolute cwd, session policy/ID, authority, and final state.
- [ ] ACP session metadata maps to the same local identity and lifecycle.
- [ ] Exact model identifiers survive configuration, child startup, events,
      session metadata, and result summaries.
- [ ] Stdout remains protocol-clean; safe bounded diagnostics use stderr.
- [ ] Unsupported model, missing credential, invalid cwd, protocol mismatch,
      startup failure, runtime failure, cancellation, and timeout remain distinct.
- [ ] Existing callers that ignore new optional metadata continue to work.

**Verify:** JSONL golden streams, ACP fake-client tests, compatibility fixtures,
and stderr/stdout separation tests.

### INST-3: Validate real ACP dispatch and packaging

**Outcome:** Prove the shipped `thndrs acp serve` command works with a real ACP
client and is discoverable as packaged.

**Blocked by:** INST-2.

**Acceptance:**

- [ ] Record one real client, client version, protocol version, and test date
      that prove initialization, streaming, tools, permission, cancellation, and
      session settlement.
- [ ] Each compatibility fix receives a deterministic fake-client regression.
- [ ] Registry/discovery material names the actual command and supported
      capabilities.
- [ ] Stdio remains the only transport until a concrete deployment cannot use
      it.
- [ ] Packaged execution resolves assets/config exactly as source execution does.

**Verify:** Fake-client suite, packaged command smoke, and one redacted real
client evidence record.

### INST-4: Prove every first-class provider route through every surface

**Outcome:** Ensure TUI, JSONL, ACP, and supervised children use the same route,
permission, workspace, and session semantics.

**Blocked by:** INST-2; supervised-child cases wait for INST-6.

**Acceptance:**

- [ ] Deterministic provider fakes cover ChatGPT Codex, OpenCode Zen, and
      OpenCode Go without network access.
- [ ] Provider setup/capacity failures stay distinct from harness lifecycle
      failures.
- [ ] Permissions and workspace containment do not vary by dispatch surface.
- [ ] Session events and terminal results settle identically for equivalent
      semantic runs.
- [ ] Bounded opt-in smokes cover one current ChatGPT Codex model and one model
      on each supported OpenCode route.

**Verify:** Cross-surface conformance tests and isolated live smokes with
redacted evidence.

### INST-5: Expose supported account capacity without scraping

**Outcome:** Show provider-reported remaining subscription/credit windows when
a supported account API can provide them, and say unavailable otherwise.

**Blocked by:** INST-1 and an evidenced supported API for each route. This task
does not block process supervision.

**Acceptance:**

- [ ] ChatGPT displays each returned rate-limit window with used/remaining,
      reset, observation time, and stale state when the supported route exposes it.
- [ ] OpenCode displays subscription/credit allowance and reset data only when
      its supported API returns them.
- [ ] `/usage` refreshes and shows detail; `/status` and orientation show only a
      compact redacted summary.
- [ ] ACP/JSONL metadata may expose an optional redacted snapshot.
- [ ] Missing fields and unsupported routes display `unavailable`; stale data
      displays `stale`; neither becomes zero.
- [ ] Raw account responses, email, token, account ID, and authorization URL are
      never persisted or rendered.

**Verify:** Provider-response fixtures for complete, partial, stale, malformed,
and unsupported cases; redaction tests; opt-in live smoke only for supported
APIs.

### INST-6: Supervise one read-only child process

**Outcome:** Let a foreground `thndrs` session dispatch and settle exactly one
explicit read-only child `thndrs` process.

**Blocked by:** INST-1 through INST-4. UI-7 is required before user steering is
added, but not for the first unsteerable child slice.

**Acceptance:**

- [ ] The child receives explicit executable, protocol, absolute cwd, exact
      model route, session policy, prompt, limits, timeout, and read-only authority.
- [ ] The parent owns pipes, process-group cleanup, cancellation, timeout, and
      terminal settlement; a child cannot outlive parent shutdown.
- [ ] Child context, transcript, queue, session, and process registry remain
      separate from the parent.
- [ ] The parent receives a bounded summary plus instance/session handles.
- [ ] Missing credentials, invalid cwd, startup failure, protocol corruption,
      timeout, cancellation, and task failure settle distinctly.
- [ ] Recursive delegation and write authority are disabled.
- [ ] Fresh supported capacity may reject a depleted route; unavailable
      capacity does not invent a decision.

**Verify:** Deterministic fake-child protocol tests, real local child smoke,
process cleanup tests, timeout/cancellation tests, and bounded-evidence tests.

### INST-7: Add bounded multi-instance supervision

**Outcome:** Run a small number of independent children without weakening
authority, lifecycle, or failure isolation.

**Blocked by:** INST-6.

**Acceptance:**

- [ ] Delegation requires direct user/project instruction and an independently
      useful bounded task.
- [ ] Concurrency, depth, total runtime, evidence, and account-capacity policy
      are validated before launch.
- [ ] Parent cancellation settles every owned child before the parent run can
      complete.
- [ ] One child failure does not hide another child's result or permission
      request.
- [ ] No child gains write authority or further delegation implicitly.
- [ ] Results are ordered and identified deterministically regardless of
      completion order.

**Verify:** Fake-process concurrency tests covering mixed success/failure,
capacity rejection, cancellation races, permission waits, and shutdown.

### INST-8: Expose sparse instance controls

**Outcome:** Inspect, steer, and stop a child without turning the TUI into a
pane manager.

**Blocked by:** INST-7 plus UI-6 through UI-8.

**Acceptance:**

- [ ] A compact surface shows stable ID, bounded task, route/model, cwd,
      lifecycle, elapsed time, authority, capacity state, and session/result handle.
- [ ] Inspect, steer, stop, and close actions resolve exactly one instance and
      are audited.
- [ ] Permission requests remain visible while another instance is focused.
- [ ] Closing a settled instance removes transient UI state but never deletes
      its durable session.
- [ ] Child transcript detail uses the same semantic block/progressive-
      disclosure model without merging histories.
- [ ] Default layout remains transcript + composer + restrained status; no
      permanent panes are added.

**Verify:** Instance-state transitions, focus/keymap tests, snapshots at normal
and narrow widths, and an end-to-end two-child smoke.

### INST-9: Pass the harness dogfood gate

**Outcome:** Prove `thndrs` is both a reliable foreground agent and a reliable
dispatchable child.

**Blocked by:** INST-4 and INST-6 through INST-8. INST-5 is required only for
routes that claim capacity support.

**Acceptance:**

- [ ] One ChatGPT Codex, OpenCode Zen, and OpenCode Go child is exercised where
      accounts and current routes are available; unavailable routes are recorded
      accurately.
- [ ] Foreground and child runs cover implementation, diagnosis, review,
      failure recovery, cancellation, queue steering, and resume.
- [ ] Herdr can host `thndrs`, Codex, and Pi panes without special integration
      or terminal corruption.
- [ ] Capacity is accurate or clearly unavailable; permissions remain visible;
      all child processes settle.
- [ ] Repeated harness failures become deterministic regression tests.

**Verify:** A redacted dogfood ledger, focused regression suite, provider smokes,
and real-terminal cleanup review.

## P3 — External Capabilities

Skills own instructions, reference knowledge, and straightforward CLI
workflows. MCP owns typed operations, resources, prompts, discovery, and server
lifecycle. Project trust, permissions, containment, redaction, auditing, and
transcript behavior stay in shared application policy.

### EXT-1: Apply project trust and permissions to MCP

**Outcome:** Prevent project MCP configuration and server operations from
gaining authority through discovery alone.

**Blocked by:** None.

**Acceptance:**

- [ ] Project MCP configuration remains inactive until the project is trusted
      for MCP, independently of ACP, skills, prompts, and commands.
- [ ] Trust decisions are explicit, inspectable, revocable, scoped, and durable.
- [ ] Server startup and tool or resource access cannot exceed the current run's
      authority and use the shared permission flow.
- [ ] Calls record the configured server, original capability name, requested
      authority, decision, result, and observed effects where available.
- [ ] Global and project configuration precedence is deterministic and visible.
- [ ] Documentation states when a server process is external to an enforcing
      sandbox.

**Verify:** Configuration-resolution tests, trust transitions, fake MCP client
permission cases, and semantic transcript/session projections.

### EXT-2: Add bounded MCP resources

**Outcome:** Let servers provide structured context without presenting every
read as a tool or injecting resource contents at startup.

**Blocked by:** EXT-1.

**Acceptance:**

- [ ] Negotiate and expose resources only when the server advertises the
      capability.
- [ ] List compact, namespaced resource metadata and fetch content explicitly.
- [ ] Enforce URI, item, byte, timeout, and serialization limits with visible
      truncation or omission.
- [ ] Preserve media type and distinguish text from opaque binary content.
- [ ] Apply project trust, permission, cancellation, redaction, and auditing to
      resource access.
- [ ] Resource failures do not remove unrelated servers or tools.

**Verify:** Fake-server negotiation, listing and read cases, limit and media-type
tests, permission failures, and transcript/context projections.

### EXT-3: Make MCP lifecycle and failures easy to diagnose

**Outcome:** Explain whether each configured server is disabled, blocked by
trust, starting, ready, degraded, failed, or stopped.

**Blocked by:** EXT-1.

**Acceptance:**

- [ ] `mcp list`, `mcp test`, startup diagnostics, and `/status` use consistent
      lifecycle terms and identify the configuration scope.
- [ ] Diagnostics distinguish configuration, environment, process startup,
      negotiation, capability-listing, timeout, cancellation, and shutdown
      failures.
- [ ] Stdio stderr and protocol diagnostics remain bounded and redact secrets.
- [ ] One failed server does not break unrelated startup or capability use.
- [ ] Cancellation and application shutdown settle child processes and report
      cleanup failures accurately.
- [ ] Diagnostics recommend only actions supported by the current CLI.

**Verify:** Lifecycle transition tests, fake process and MCP client failures,
output-cap/redaction tests, and command snapshots.

### EXT-4: Improve skill compatibility diagnostics

**Outcome:** Make an installed skill's compatibility limits and missing local
requirements visible without turning skills into executable authority.

**Blocked by:** Existing skill-loading safety tests.

**Acceptance:**

- [ ] `skills doctor` shows declared compatibility and unsupported required
      tools or local commands using a documented metadata convention.
- [ ] A missing or incompatible dependency produces a diagnostic and does not
      run installation or project code.
- [ ] Skill metadata cannot grant permissions, enable tools, or weaken the
      current run's authority.
- [ ] Unknown optional metadata remains preserved without being treated as
      trusted policy.
- [ ] Duplicate resolution and bounded progressive loading keep their current
      behavior.

**Verify:** Metadata parser and diagnostic fixtures, missing-command fakes,
permission invariants, and `skills doctor` snapshots.

## Later Vertical Slices

These are intentionally ordered after the foundation they need. Promote one to
a priority milestone only when it has an owner and its blocker is satisfied.

### SESSION-1: Fork a session from a settled turn

**Blocked by:** Stable semantic session events from UI-4.

- [ ] Select only replayable settled turn boundaries.
- [ ] Give the fork a new identifier and record source session, source turn,
      timestamp, and lineage.
- [ ] Store a self-contained replayable prefix.
- [ ] Never copy pending tools, permissions, queues, processes, or children.

### SESSION-2: Export sessions for human review

**Blocked by:** UI-4 and structured review semantics from UI-9.

- [ ] Export deterministic Markdown and self-contained HTML.
- [ ] Preserve messages, reasoning summaries, tools, status, errors, findings,
      session identity, and lineage with bounded redacted details.
- [ ] Require no external scripts or assets in HTML.

### INPUT-1: Attach images to native prompts

**Blocked by:** Composer state from UI-1 and provider capability metadata.

- [ ] Add, inspect, and remove paths before submission.
- [ ] Validate MIME type, size, dimensions, readability, and route support
      locally.
- [ ] Persist safe attachment metadata without duplicating arbitrary source
      files.
- [ ] Reject text-only routes before making a request.

### PROVIDER-1: Support the OpenAI Platform API

- [ ] Keep Platform and ChatGPT credentials/routes distinct.
- [ ] Normalize text, images, reasoning, tools, usage, retry, and errors through
      provider-neutral agent contracts.
- [ ] Setup, doctor, model selection, status, and sessions identify the route
      accurately.

### PROVIDER-2: Support the Anthropic API

- [ ] Add setup, doctor, model selection, recovery, and session identity.
- [ ] Normalize text, images, reasoning, tools, usage, retries, and errors.
- [ ] Reject unsupported controls locally through explicit capabilities.

### PROVIDER-3: Configure compatible provider endpoints

**Blocked by:** Two native provider adapters with stable capability contracts.

- [ ] Configuration declares protocol, base URL, credential source, model, and
      trust scope.
- [ ] Capability metadata covers tools, images, reasoning, context, and output
      limits.
- [ ] Invalid or incomplete capability declarations fail before a request.

### SAFETY-1: Gate project-owned runtime configuration on trust

- [ ] Cover project ACP, prompt templates, commands, and skills without one
      setting silently authorizing unrelated capabilities. EXT-1 owns MCP
      trust.
- [ ] Untrusted projects use global/user configuration and show what was
      ignored.
- [ ] Decisions are explicit, inspectable, revocable, scoped, and durable.
- [ ] Project files/resources cannot rewrite harness identity, direct
      instructions, tool schemas, provider boundaries, or safety policy.

### SAFETY-2: Define the sandbox execution boundary

**Blocked by:** SAFETY-1.

- [ ] Distinguish read-only, workspace-write, and external isolation.
- [ ] Treat filesystem and network authority as separate inputs.
- [ ] Make built-in shell, ACP terminals, MCP children, and supervised
      instances report the boundary they actually use.
- [ ] Claim no isolation when no enforcing backend exists.

### SAFETY-3: Implement the first OS sandbox backend

**Blocked by:** SAFETY-2.

- [ ] Enforce declared workspace reads/writes and network policy.
- [ ] Fail closed outside allowed roots and for disallowed network access.
- [ ] Protect repository-control and credential paths.
- [ ] Ensure descendants cannot outlive cancellation or shutdown.

### SAFETY-4: Ask for approval at authority boundaries

**Blocked by:** SAFETY-2; OS-enforced cases additionally require SAFETY-3.

- [ ] Describe the exact command, resource, effects, and requested authority.
- [ ] Audit allow, reject, cancel, timeout, and unavailable-interaction results.
- [ ] Never weaken the sandbox silently or claim enforcement that is absent.
- [ ] Apply skill-, MCP-, and child-specific permission constraints through the
      same policy model.

### CONTEXT-1: Inspect context at a provider request

- [ ] Choose a turn, request, and retry without exposing raw provider payloads.
- [ ] Show item kind, source, estimated size/tokens, visibility, lifecycle, and
      omission reason.
- [ ] Show budget totals, route/model, serialized request size, reported usage,
      and compaction boundary.
- [ ] Distinguish historical capture from the current next-request projection.
- [ ] Keep content-free historical records useful after resume and bounded on
      large sessions.

### CONTEXT-2: Compare context between requests

**Blocked by:** CONTEXT-1.

- [ ] Group additions, removals, visibility/lifecycle changes, replacements,
      and truncation/compaction.
- [ ] Compare budget, serialized size, estimated tokens, and reported usage.
- [ ] Use the same model across turns and forked sessions.
- [ ] Keep large diffs bounded/searchable without loading every body.

### CONTEXT-3: Browse session lineage

**Blocked by:** SESSION-1.

- [ ] Display pre-lineage sessions as roots.
- [ ] Show source turn, title, model, last activity, and lock/corruption state.
- [ ] Inspect, resume, fork, or export through existing workflows.
- [ ] Surface missing parents, malformed lineage, and cycles as diagnostics.

### CONTEXT-4: Capture opt-in request projections

**Blocked by:** CONTEXT-1 and an approved privacy/retention design.

- [ ] Default off and require an explicit per-run choice.
- [ ] Store normalized model-message projection, never credentials or raw wire
      payloads.
- [ ] Fail closed on size/retention limits and record omissions.
- [ ] Preserve existing redaction in inspect/export surfaces.

### EVAL-1: Run versioned coding-task evaluations

**Blocked by:** Stable headless semantics and structured review.

- [ ] Cover edits, diagnosis, review, tool failure, steering, compaction,
      cancellation, resume, Unicode, and containment.
- [ ] Separate harness failures from model task failures.
- [ ] Record model, route, revision, timing, usage, intervention, environment,
      and task version without secrets.
- [ ] Make reports deterministic enough to compare harness revisions.

### INSTANCE-10: Isolate writing children in worktrees

**Blocked by:** INST-9, SAFETY-3, and SESSION-1.

- [ ] Require an explicit bounded repository and write-capable task.
- [ ] Keep child writes inside its isolated worktree and authority boundary.
- [ ] Report changed files, verification, session, and cleanup state.
- [ ] Applying, committing, publishing, or deleting child work remains a
      separate explicit user action.

### PRODUCT-1: Add explicit plan/read-only mode

**Blocked by:** SAFETY-2 and UI-8.

- [ ] Mode has enforceable authority, visible status, local capability
      rejection, and a deliberate transition to write-capable work.
- [ ] TUI, JSONL, ACP, and child instances use the same semantics.

### PRODUCT-2: Add in-app task tracking only if sessions need it

**Blocked by:** Evidence from daily-driver and instance workflows.

- [ ] Model only durable user-visible work state not already represented by the
      transcript, queue, or child lifecycle.
- [ ] Avoid a second planning system or provider-specific todo payload.

### EXT-5: Expand skill distribution deliberately

**Blocked by:** EXT-4 and existing skill-loading safety tests.

- [ ] Add metadata validation, progressive-loading, remote-fetch safety,
      reference-depth, and self-knowledge snapshot coverage before distribution.
- [ ] Design marketplace/install/share behavior as a separate trust and supply-
      chain slice; do not imply that skills ship through a marketplace today.
- [ ] Prefer skills, slash commands, or MCP over a generic plugin framework
      unless a concrete capability cannot fit those boundaries.

## Trigger-Gated Horizons

These items are retained from the superseded feature plans but are not active
tasks. Convert one into a vertical slice only when its stated trigger exists.

### External-capability horizons

- **New integrations:** require repeated workflow evidence and a clear reason
  that an existing skill, CLI, or MCP server is insufficient. Define ownership,
  trust, permissions, versioning, and failure behavior before implementation.
- **Persistent indexes and stores:** define creation, invalidation, retention,
  inspection, and deletion before keeping application data outside sessions.
- **Contained MCP servers:** use the shared sandbox and permission semantics
  from SAFETY-2 and SAFETY-3; do not build an MCP-only isolation model.
- **Jobs, watch triggers, and a local daemon:** require a concrete durable
  workload that cannot be handled by an explicit foreground or headless run.
- **Distribution and comparison copy:** wait until an integration is real and
  reviewable, then describe implemented behavior only.

### Instance horizons

- **Write-capable children:** INSTANCE-10 is the first admissible design; shared
  writable checkouts remain out of scope.
- **Remote transports:** require a deployment that local stdio ACP or JSONL
  cannot serve.
- **Automatic model routing:** require repeated task evidence and reliable,
  supported capacity signals; unknown capacity is not a routing signal.
- **Permanent sidebar or cockpit:** require frequent switching that the compact
  list, picker, and detail overlay cannot handle.
- **Public instance SDK:** require a second external harness whose needs are not
  met by ACP and JSONL.
