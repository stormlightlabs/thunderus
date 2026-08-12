---
name: tui-design
description: Design, redesign, implement, review, and visually polish the thndrs Ratatui interface, then improve the skill from reusable user feedback. Use for the transcript, composer, status line, pickers, prompts, overlays, themes, responsive layout, keyboard or mouse interaction, accessibility, rendering performance, terminal lifecycle, TUI snapshots, or terminal screenshots under crates/thndrs/src/cli/renderer and related application code.
---

# TUI Design

Make thndrs feel composed, quiet, and responsive. Start with what the user sees and does. Let engineering support the design instead of defining it.

## Work from the live product

1. Inspect the running state, current code, and relevant tests or snapshots.
2. Treat `docs/public/screenshot.png` and `docs/tui/*.tape` as legacy artifacts until they are checked against the current product. Do not copy their composition by default.
3. Name the state being improved and the action the user should notice first.
4. Compare the state with the relevant patterns in [references/harness-patterns.md](references/harness-patterns.md). Adopt the reasoning, not another harness's skin.
5. State the intended hierarchy, density, alignment, color, and interaction change before editing.
6. Implement the smallest coherent change, render it, inspect it, and refine it once more.

For a narrow correction, keep this pass short. For a redesign, cover the whole state matrix before settling the visual system.

## Design the frame

- Give each state one obvious focal point: active output, a decision, a selection, or the composer.
- Keep the transcript open and readable. Separate turns with rhythm and semantic markers before adding boxes.
- Pin the composer to the bottom and let it grow upward. Apply its colored background only to editable input rows and intentional inset padding.
- Leave the session label, surrounding space, status, and footer on the terminal background.
- Establish a spacing rhythm and shared alignment anchors. One stray row or column can make a small TUI feel unfinished.
- Use borders for real containment, focus, or selection. Avoid nested chrome and decorative rules around passive content.
- Keep labels short, specific, and visually subordinate to the user's draft and current work.
- Use one dominant accent plus semantic success, warning, and error roles. Make every state legible without color.
- Keep live updates spatially stable. Streaming, spinners, status changes, and popup dismissal must not make unrelated content jump.
- Design empty, busy, failed, cancelled, permission, long-content, and tiny-terminal states with the same care as the ideal screenshot.

Read [references/harness-patterns.md](references/harness-patterns.md) for the visual audit, component patterns, polish rubric, and lessons from Codex, Grok Build, Amp, and Factory Droid.

## Protect the interaction

- Preserve drafts across recoverable errors, blocked submission, picker dismissal, and navigation.
- Route keys to the focused modal or picker before composer, active operation, and global actions.
- Give focus and selection a text, shape, or position cue in addition to color.
- Keep keyboard operation complete. Treat mouse support as optional and preserve terminal text selection when capture is off.
- Define Enter, Escape, Tab, arrows, page keys, cancellation, paste, resize, and focus restoration for every changed surface.
- Keep the active choice visible while scrolling and resizing.
- When a surface gains or loses focus, update its input routing, scrolling, and redraw ownership together.

## Preserve the rendering seams

Follow the established flow:

```text
application state -> RendererView -> semantic surfaces/rows -> Ratatui frame -> Crossterm
```

Keep state transitions outside rendering and keep layout, wrapping, truncation, and semantic projection pure where practical. Reuse the current renderer modules before adding abstractions or crates. For editable text, derive rendered wrapping and cursor coordinates from the same layout, and prefer word boundaries with grapheme splitting only for overlong words.

Read [references/ratatui-engineering.md](references/ratatui-engineering.md) only when changing terminal lifecycle, event handling, animation, themes, Unicode layout, dependencies, performance, or renderer boundaries.

## Judge the result visually

1. Render the affected states at realistic, narrow, and tiny sizes.
2. Compare before and after captures for hierarchy, balance, rhythm, alignment, contrast, chrome, copy, and spatial stability.
3. Run the narrowest behavioral and rendering checks needed for the change.
4. Inspect changed snapshots cell by cell. Passing snapshots do not establish polish.
5. Exercise a real terminal when cursor behavior, input timing, animation, terminal capabilities, or overall composition changed.

Use [references/verification.md](references/verification.md) to select states and evidence. Regenerate public screenshots or VHS fixtures only after the product state is approved and the fixture represents it accurately.

## Learn from feedback

Treat explicit user corrections and repeated visual-review findings as input to this skill. At the end of relevant TUI work, apply [references/skill-maintenance.md](references/skill-maintenance.md): update the smallest rule or reference that would prevent the issue from recurring, validate the skill, and mention the update in the handoff.

Do not encode one-off taste, temporary workarounds, or screenshot-specific coordinates. Replace stale guidance and consolidate duplicates so the skill becomes sharper rather than longer.

Stop when the requested state is polished and verified. Leave unrelated renderer cleanup and feature work alone.
