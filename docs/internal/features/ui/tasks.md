# UI Polish Tasks

## UI-1: Establish shared geometry and density

- [x] Add one pure layout projection for comfortable, compact, and cramped
      density, shared content rails, readable prose width, and wide technical
      surfaces.
- [x] Apply it to startup, transcript, live composer, status, and focused
      details without creating a second rendering path.
- [x] Let code, diffs, tables, logs, and details use extra width while ordinary
      prose stays within roughly 100–120 columns.
- [x] Hide or shorten low-value metadata before truncating primary content or
      controls.
- [x] Cover rail, wrapping, and density decisions at 80, 120, and 160 columns
      plus one sub-80 width.

## UI-2: Clarify startup and conversation hierarchy

**Blocked by:** UI-1.

- [x] Reduce normal startup to identity, actionable warnings, and one short
      orientation line; keep diagnostic context available when it matters.
- [x] Give user turns a clear but light anchor, keep assistant prose primary,
      and suppress redundant system or operational entries.
- [x] Normalize vertical rhythm between turns, reasoning, activity, errors,
      code, and lists with one small spacing vocabulary.
- [x] Reduce persistent shortcut copy to the minimum useful hint and leave full
      discovery in the existing help and command surfaces.
- [x] Preserve Markdown, code, table, citation, selection, and scroll behavior
      while updating focused transcript snapshots.

## UI-3: Compose the bottom action area

**Blocked by:** UI-1.

- [ ] Align session identity, editable input, queue information, operational
      status, and footer metadata as one bottom region.
- [ ] Keep the fill on editable input rows only; use spacing, alignment, and a
      single state accent to connect terminal-background metadata.
- [ ] Make one nearby status authoritative for ready, sending, streaming,
      running a tool, stopped, and failed states, removing duplicate notices.
- [ ] Prioritize input and the current state at compact and cramped widths;
      shorten or hide model, mode, timing, and queue metadata as needed.
- [ ] Preserve cursor placement, multiline editing, draft history, commands,
      mentions, queues, permissions, cancellation, and accessory surfaces.

## UI-4: Unify activity and semantic styling

**Blocked by:** UI-2 and UI-3.

- [ ] Route renderer styling through named semantic roles for primary,
      secondary, accent, active, success, warning, failure, selection, input,
      and focus surfaces across every shipped theme.
- [ ] Reuse the existing activity projection to group related tool work and
      keep running work visually active without adding transcript noise.
- [ ] Compress settled reads, searches, edits, and checks into quiet summaries;
      keep failures, cancellations, diffs, and verification evidence prominent
      or available through existing detail interaction.
- [ ] Use fixed-width state markers and stable spinner geometry so live updates
      do not shift surrounding content.
- [ ] Verify that state remains legible without color and that no component
      introduces an unowned accent, border, or background treatment.

## UI-5: Validate the complete experience

**Blocked by:** UI-4.

- [ ] Add named full-frame fixtures for startup, ordinary conversation,
      streaming, grouped running and settled tools, diff, successful check,
      failure, multiline input, and focused detail.
- [ ] Snapshot the representative matrix at 80×24, 120×32, 160×40, and one
      cramped size; review a smaller cross-theme subset for semantic contrast.
- [ ] Manually inspect hierarchy, rails, rhythm, truncation, cursor placement,
      scrolling, selection, overlay transitions, and resize behavior in a real
      terminal.
- [ ] Keep redraw and stale-cell coverage passing, and add regressions only for
      state boundaries changed by this milestone.
- [ ] Run `cargo fmt`, both workspace Clippy commands, and
      `cargo test --workspace` before closing the milestone.
