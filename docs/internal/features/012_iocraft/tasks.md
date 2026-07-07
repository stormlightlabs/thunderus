# iocraft Surface Expansion Tasks

Status: Draft
Captured: 2026-07-07

## P0: Record Scope And Baseline

- [ ] Record that this feature expands bounded iocraft surfaces.
- [ ] Record that committed transcript rows stay direct-rendered.
- [ ] Record that the main prompt editor stays direct-rendered.
- [ ] Record that permission prompts and first-run recovery keep priority over
      optional focused surfaces.
- [ ] Review `docs/internal/archive/v0.1.md` Terminal UI decisions.
- [ ] Review `docs/src/content/docs/notebook/iocraft.md`.
- [ ] Review `src/cli/renderer/adapter.rs`.
- [ ] Review `src/cli/renderer/view.rs`.
- [ ] Review direct rows for model picker, skill picker, diff detail, tool
      detail, and recovery.

## P1: Adapter Cleanup

- [ ] Add module docs or comments for any new adapter helper types.
- [ ] Move palette lookup toward a caller-owned theme construction boundary.
- [ ] Keep `SurfaceRenderInput` small and renderer-owned.
- [ ] Preserve `Vec<Row>` as the only adapter output.
- [ ] Add helpers for title/body/footer surfaces if they remove real
      duplication.
- [ ] Add explicit clipped-content metadata support for scrollable surfaces.
- [ ] Add tests proving adapter row counts never exceed real content plus
      intentional reserved rows.
- [ ] Add tests for selected, muted, warning, error, diff added, and diff
      removed roles.

## P2: Model And Skill Pickers

- [ ] Extend semantic focused surface mapping for model picker.
- [ ] Extend semantic focused surface mapping for skill picker.
- [ ] Route model picker through the iocraft adapter.
- [ ] Route skill picker through the iocraft adapter.
- [ ] Preserve selection markers and selected-row background.
- [ ] Preserve long label/detail truncation behavior.
- [ ] Preserve empty-state behavior.
- [ ] Preserve tiny-height behavior.
- [ ] Add normal-width snapshots.
- [ ] Add narrow-width snapshots.
- [ ] Add no-match snapshots.
- [ ] Add region tests proving detail panes still replace optional pickers.

## P3: Structured Tables

- [ ] Inventory existing table-producing paths.
- [ ] List which paths already have `TableView` data.
- [ ] Route eligible semantic tables through the adapter.
- [ ] Preserve fixed, percent, and flexible width policies.
- [ ] Preserve left, center, and right alignment.
- [ ] Preserve selected-row styling.
- [ ] Preserve narrow fallback behavior.
- [ ] Add Unicode table cell coverage.
- [ ] Add long-cell truncation coverage.
- [ ] Add snapshots for normal, narrow, and tiny widths.

## P4: Diff Detail

- [ ] Compare direct diff detail rows against adapter diff rows.
- [ ] Preserve unified diff header behavior.
- [ ] Preserve added and removed line styling.
- [ ] Add clipped-above and clipped-below indicators.
- [ ] Preserve tiny-height behavior.
- [ ] Add parity tests, then route diff detail through the adapter.
- [ ] Add snapshots for multi-file diffs.
- [ ] Add snapshots for narrow diffs.
- [ ] Add snapshots for empty or summary-only diffs.

## P5: Tool Detail

- [ ] Document current direct tool detail behavior before changing it.
- [ ] Preserve title/status row styling.
- [ ] Preserve wrapped output row scrolling.
- [ ] Preserve clipped-above and clipped-below indicators.
- [ ] Preserve failed, cancelled, running, and succeeded status treatment.
- [ ] Preserve full stored output access versus transcript preview.
- [ ] Add parity tests, then route tool detail through the adapter.
- [ ] Add snapshots for failed compiler/test output.
- [ ] Add snapshots for long unbroken lines.
- [ ] Add snapshots for scrolled output.

## P6: Setup And Recovery Forms

- [ ] Expand `SetupFormView` with provider and stage copy.
- [ ] Represent recovery action rows semantically.
- [ ] Represent selected recovery action semantically.
- [ ] Represent ChatGPT OAuth URL, user code, and polling status
      semantically.
- [ ] Preserve hidden secret input rendering.
- [ ] Preserve validation and cancellation behavior.
- [ ] Preserve project/global credential action labels.
- [ ] Add semantic parity coverage, then route one recovery stage through the
      adapter.
- [ ] Add snapshots for API-key recovery.
- [ ] Add snapshots for ChatGPT OAuth polling.
- [ ] Add snapshots for tiny-height recovery.

## P7: Interaction And Priority

- [ ] Prove pending permissions beat help, pickers, and detail surfaces.
- [ ] Prove first-run recovery beats help, pickers, and detail surfaces.
- [ ] Prove detail pane replacement behavior remains intact.
- [ ] Prove `Esc` still closes focused optional surfaces.
- [ ] Prove selection keys still update app state, not adapter state.
- [ ] Add resize tests for every migrated surface.
- [ ] Add cursor visibility tests where prompt chrome is nearby.

## P8: Documentation

- [ ] Update public TUI docs if visible behavior changes.
- [ ] Update keybinding docs if any labels or detail behavior changes.
- [ ] Update internal archive at feature completion.
- [ ] Update `CHANGELOG.md` with release-facing bullets.
- [ ] Run `pnpm --dir docs build` if public docs change.

## P9: Verification

- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy --fix --allow-dirty --allow-staged`.
- [ ] Run `cargo clippy`.
- [ ] Run `cargo test`.
- [ ] Run focused adapter snapshots.
- [ ] Run focused live/region tests for priority and clipping.
- [ ] Manually review snapshots for clarity and overall feel.
- [ ] Verify no iocraft fullscreen/render-loop APIs are called from the TUI.

## Review Checkpoints

- [ ] After P1, review adapter complexity before migrating more surfaces.
- [ ] After P2, review picker snapshots for clarity and row-budget behavior.
- [ ] After P4/P5, review detail snapshots for lost output or clipping cues.
- [ ] After P6, review recovery snapshots with provider-specific expectations.
- [ ] Before completion, verify the feature improved clarity rather than only
      increasing abstraction.
