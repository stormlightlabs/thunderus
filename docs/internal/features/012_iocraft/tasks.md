# Tickets: iocraft Surface Hardening And Expansion

These tickets implement the harden-first plan in
`docs/internal/features/012_iocraft/plan.md`. Work the frontier: any ticket
whose blockers are complete.

Completed baseline:

- Source inspection confirms iocraft calls are isolated to
  `src/cli/renderer/adapter.rs`.
- The adapter boundary returns `Vec<Row>` through `SurfaceRenderer`.
- The adapter does not call iocraft fullscreen/render-loop APIs.
- The adapter does not write to stdout or stderr.
- Focus, selection, scroll, form state, and app state are projected in
  `src/cli/renderer/view.rs` before rendering.
- Current adapter-rendered surfaces include command picker, file picker, help,
  tool detail, diff detail, transcript lens, setup form, and structured table.

## Ticket 1: Add Row-Budget Tests For Every Adapter Surface

**What to build:** Prove every existing iocraft-rendered focused surface stays
within its intended row budget.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Command picker row counts are bounded.
- [ ] File picker row counts are bounded.
- [ ] Help row counts are bounded.
- [ ] Tool detail row counts are bounded.
- [ ] Diff detail row counts are bounded.
- [ ] Transcript lens row counts are bounded.
- [ ] Setup form row counts are bounded.
- [ ] Structured table row counts are bounded.
- [ ] Tiny-height cases do not panic.
- [ ] Row-budget helpers are pure functions or otherwise directly unit-testable.

**Verification:**

- `cargo test renderer::adapter`

## Ticket 2: Add Clipping Metadata And Indicators

**What to build:** Make scrollable focused surfaces visibly communicate clipped
content while preserving row budgets.

**Blocked by:** Ticket 1: Add Row-Budget Tests For Every Adapter Surface

**Acceptance criteria:**

- [ ] Tool detail shows clipped-above and clipped-below indicators.
- [ ] Diff detail shows clipped-above and clipped-below indicators.
- [ ] Transcript lens shows clipped-above and clipped-below indicators.
- [ ] Clipping is represented with typed state, not sentinel display strings.
- [ ] Setup/recovery overflow shows clear clipping without hiding validation
      errors.
- [ ] Indicators fit normal, narrow, and tiny-height cases.
- [ ] Full stored output remains accessible even when previews are clipped.

**Verification:**

- `cargo test renderer::adapter`
- focused snapshots for above-only, below-only, both, and no-clipping cases.

## Ticket 3: Make Theme Role Input Explicit

**What to build:** Keep palette lookup and theme role resolution at a clear
renderer boundary so adapter tests do not rely on global theme mutation.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Adapter helpers consume explicit semantic theme roles.
- [ ] Tests can pass deterministic role data.
- [ ] Selected, muted, warning, error, diff added, and diff removed styles are
      covered.
- [ ] Palette lookup remains at a narrow renderer boundary.
- [ ] Existing theme behavior is preserved for the live TUI.
- [ ] No unrelated global theme mutation is needed for adapter tests.

**Verification:**

- `cargo test renderer::adapter`
- snapshot diff review for selected, warning, error, and diff styles.

## Ticket 4: Add Unicode And Long-Line Surface Coverage

**What to build:** Cover the adapter with realistic terminal text cases that
commonly break row layout.

**Blocked by:** Ticket 1: Add Row-Budget Tests For Every Adapter Surface

**Acceptance criteria:**

- [ ] CJK text appears in picker, table, and detail snapshots.
- [ ] Emoji and combining marks appear in focused surface snapshots.
- [ ] Long unbroken paths truncate deterministically.
- [ ] Long command output lines do not resize or shift the surface.
- [ ] Narrow and tiny-width fallbacks remain readable.
- [ ] The direct renderer still owns prompt cursor and wrapping behavior.
- [ ] No new wrapping/layout dependency is added without fixture-backed
      evidence that local helpers are insufficient.

**Verification:**

- `cargo test renderer::adapter`
- `cargo test renderer::region`

## Ticket 5: Prove Blocking Surface Priority

**What to build:** Add app/view/region coverage that optional focused surfaces
cannot hide permission or setup/recovery states.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Pending permission prompts outrank help.
- [ ] Pending permission prompts outrank pickers.
- [ ] Pending permission prompts outrank tool/diff detail surfaces.
- [ ] Setup/recovery outranks help, pickers, and optional details.
- [ ] Detail pane replacement behavior remains intentional.
- [ ] `Esc` closes optional focused surfaces without cancelling blocking work.

**Verification:**

- `cargo test app`
- `cargo test renderer::view`
- `cargo test renderer::region`

## Ticket 6: Harden Setup And Recovery Surface Semantics

**What to build:** Ensure setup/recovery rendering preserves security and
validation semantics before any richer form expansion.

**Blocked by:** Ticket 2: Add Clipping Metadata And Indicators; Ticket 5: Prove
Blocking Surface Priority

**Acceptance criteria:**

- [ ] Secret fields render hidden values only.
- [ ] Validation errors are visible and styled as errors.
- [ ] Global/project credential choices are represented semantically.
- [ ] Cancellation and confirmation labels are preserved.
- [ ] Provider/model/reasoning readiness copy is represented as semantic data,
      not hardcoded adapter text.
- [ ] Security-sensitive form state is represented in typed view data before
      reaching the adapter.
- [ ] Prompt drafts survive setup/recovery success, cancellation, and failure.

**Verification:**

- `cargo test app`
- `cargo test renderer::adapter`
- setup/recovery snapshots for API-key, validation-error, and tiny-height
  states.

## Ticket 7: Harden Tool And Diff Detail Parity

**What to build:** Prove iocraft-rendered tool and diff details preserve the
same useful information as the direct renderer path they replaced.

**Blocked by:** Ticket 2: Add Clipping Metadata And Indicators; Ticket 4: Add
Unicode And Long-Line Surface Coverage

**Acceptance criteria:**

- [ ] Tool detail preserves running, succeeded, failed, and cancelled status.
- [ ] Tool detail preserves wrapped or truncated output cues.
- [ ] Diff detail preserves file headers, additions, removals, and summary
      counts.
- [ ] Multi-file diffs remain readable.
- [ ] Long compiler/test output remains inspectable through scroll state.
- [ ] Empty or summary-only details render clearly.

**Verification:**

- `cargo test renderer::adapter`
- parity snapshots for failed compiler output, long unbroken lines, scrolled
  output, multi-file diff, narrow diff, and empty diff.

## Ticket 8: Establish The Expansion Gate

**What to build:** Add a checklist or test grouping that makes future iocraft
surface migrations conditional on hardening evidence.

**Blocked by:** Tickets 1, 2, 3, 4, 5, 6, and 7

**Acceptance criteria:**

- [ ] The repo has an explicit list of existing adapter surfaces and required
      hardening checks.
- [ ] New iocraft surface migrations have a documented review checklist.
- [ ] The checklist requires row-budget, clipping, priority, Unicode/narrow,
      and snapshot coverage.
- [ ] The checklist asks what complexity iocraft removes for the new surface.
- [ ] The checklist says prompt editor, committed transcript, terminal I/O, and
      app state remain off limits.
- [ ] The checklist requires typed state and pure row-budget helpers where
      practical.

**Verification:**

- human review of the checklist and associated tests.

## Ticket 9: Expand One Surface After The Gate

**What to build:** Migrate or enrich one additional bounded focused surface only
after the expansion gate is satisfied.

**Blocked by:** Ticket 8: Establish The Expansion Gate

**Acceptance criteria:**

- [ ] The chosen surface has a clear bounded layout problem.
- [ ] Existing behavior is documented before migration.
- [ ] App state remains outside iocraft.
- [ ] The surface passes row-budget, clipping, priority, Unicode/narrow, and
      snapshot checks.
- [ ] The migration demonstrably removes duplication or layout complexity.

**Verification:**

- `cargo test renderer::adapter`
- `cargo test renderer::region`
- manual snapshot review.

## Ticket 10: Public/Internal Docs Update If Behavior Changes

**What to build:** Update docs only for visible behavior changes or contributor
invariants that become stable.

**Blocked by:** Ticket 8: Establish The Expansion Gate

**Acceptance criteria:**

- [ ] Public TUI docs change only if user-visible behavior changed.
- [ ] Development renderer docs explain stable adapter invariants if they are
      worth promoting.
- [ ] Notebook research remains research, not the source of truth.
- [ ] Internal archive is updated when the feature completes.

**Verification:**

- `pnpm --dir docs build` if public docs changed.

## Ticket 11: Final Verification

**What to build:** Run the full verification checklist for the harden-first
iocraft feature.

**Blocked by:** Tickets 8, 9, and 10

**Acceptance criteria:**

- [ ] Adapter boundary audit passes.
- [ ] Row-budget tests cover every adapter surface.
- [ ] Clipping indicators are covered.
- [ ] Priority tests cover blocking surfaces.
- [ ] Unicode, narrow, and tiny-height snapshots are reviewed.
- [ ] No iocraft fullscreen/render-loop APIs are called from the TUI.
- [ ] Expansion evidence shows iocraft improved clarity rather than only adding
      abstraction.

**Verification:**

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test`

## Frontier

Tickets that can start immediately:

- Ticket 1: Add Row-Budget Tests For Every Adapter Surface
- Ticket 3: Make Theme Role Input Explicit
- Ticket 5: Prove Blocking Surface Priority
