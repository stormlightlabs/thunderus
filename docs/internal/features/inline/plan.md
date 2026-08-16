# Inline Transcript and Ratatui Surfaces

thndrs should run on the normal terminal screen. Completed transcript entries
belong in native terminal history, while Ratatui owns the composer and bounded
application views such as pickers, permission prompts, detail panels, and a
future sidebar-like view.

This is a surface migration, not a new application architecture. The existing
`App`, update loop, input model, prompt editing behavior, and semantic view
projection remain shared. The transcript renderer also remains authoritative
for transcript content. Ratatui may be the final cell writer needed to make a
terminal update atomic, but it must not become the owner of transcript history,
navigation, selection, wrapping policy, or stable/live classification.

## Success criteria

- Completed transcript blocks are appended once to native terminal history and
  remain available through terminal scrollback, selection, and copy.
- Streaming assistant text and running tools remain mutable until finalized.
  Finalization moves a block from the mutable tail to terminal history without
  duplication, disappearance, or a visible intermediate frame.
- Ratatui owns the composer and every bounded interactive view. Adding a new
  view does not require changing transcript history or scrollback code.
- Composer editing, cursor placement, word wrapping, queued input, paste,
  accessories, key hints, and focused surfaces retain their current behavior.
- Resize, suspend/resume, shell execution, clear, cancellation, and process exit
  leave the terminal usable. Resize never purges and reconstructs committed
  history.
- The operation vocabulary defined below is applied once in the semantic
  transcript projection and is identical in live and committed output.
- The alternate-screen implementation remains available only while the inline
  path is being verified. It does not survive as a second indefinitely
  maintained default renderer.

## Current state

The application already has most of the right seams:

- `App` and `SemanticUiView` expose renderer-neutral state.
- The transcript projection distinguishes stable and live rows.
- Focused surfaces are projected separately from transcript content.
- `RatatuiSurface` currently draws one complete alternate-screen viewport.
- `AlternateViewport` still owns transcript scrolling, selection, anchoring,
  and full-screen composition.
- `run_inline` currently enters the alternate screen despite its name.

The migration should preserve the semantic seams and replace the terminal
surface around them. It should not move application state into a new widget
tree or introduce a second transcript model.

## What previous attempts taught us

The July handoffs and the renderer history show repeated movement between three
designs. The failures came from unclear ownership more often than from the
choice of rendering library.

| Attempt | What worked | Why it churned |
| --- | --- | --- |
| Ratatui inline viewport plus raw Crossterm scroll-region writes | Native history and a live prompt region were possible | Ratatui and ad hoc backend writes both moved the cursor and terminal rows. Width changes required purge-and-replay recovery, and background artifacts appeared. |
| Full direct terminal renderer (`54364b9`, later repaired through the July handoffs and `554e112`) | Stable history, streaming output, and prompt placement could be coordinated in one transaction | The direct renderer also absorbed composer layout, focused views, terminal geometry, and lifecycle. Every UI feature increased the amount of custom terminal machinery. |
| Alternate-screen Ratatui (`5b39389` and the current bounded renderer) | One owner simplified frame rendering, resize, composer work, and focused views | thndrs again owned the transcript viewport, scrolling, selection, and copy, so native scrollback was lost and transcript-specific state grew in `AlternateViewport`. |
| iocraft inline spike | Demonstrated that a component library could render a dock | Physical-row diffing left stale frames after terminal reflow and paste support was incomplete. Giving another framework terminal lifecycle and event ingestion would duplicate established behavior. |

The stable direction is therefore neither a direct-renderer rewrite nor a
mixed pair of terminal writers. One coordinator owns the terminal transaction.
The transcript renderer supplies transcript rows and commit boundaries;
Ratatui owns only bounded interactive UI.

## Ownership model

| Owner | Responsibilities | Must not own |
| --- | --- | --- |
| Application and semantic projection | Session state, input state, transcript blocks, tool lifecycle, focused-view data, semantic operation kind | Terminal coordinates or backend-specific cells |
| Inline transcript renderer | Transcript wording, Markdown/tool projection, wrapping, stable block identity, mutable tail, operation vocabulary | Composer chrome, view layout, native scrollback navigation |
| Ratatui bounded surface | Composer layout and cursor, accessories, pickers, prompts, detail panels, and future bounded views | Committed transcript history, transcript selection, transcript scroll position |
| Inline terminal coordinator | Raw-mode lifecycle, bracketed paste, keyboard enhancements, inline viewport reservation, ordered commit/draw/flush, resize, suspend/resume, cleanup | Transcript semantics or a second copy of application state |

The coordinator may adapt already projected transcript rows into Ratatui cells
so the commit and live-surface draw happen through one terminal abstraction.
That transport detail does not transfer transcript ownership to Ratatui.

## Rendering contract

### One terminal transaction

Each dirty update takes one semantic snapshot and performs these operations in
order:

1. Hide or relocate the cursor as required by the active transaction.
2. Append newly stable transcript blocks above the inline viewport.
3. Remove those blocks from the mutable transcript tail.
4. Draw the remaining mutable transcript rows and the Ratatui-owned bounded
   surface.
5. Restore the composer cursor and flush once.

The implementation should start with Ratatui's `Viewport::Inline` and
`Terminal::insert_before`. It must not call `backend_mut()` to perform separate
scroll-region mutations behind Ratatui's buffer. If the public Ratatui API
cannot support dynamic composer/view height without stale rows or history
damage, that is a cutover blocker: revise the surface boundary before building
another hybrid cursor protocol.

The inline viewport reservation may change when the composer wraps, an
accessory appears, or a focused view opens. One coordinator computes that
height and performs the relayout. Transcript and individual views do not move
the terminal viewport themselves.

### Stable transcript commits

Stability belongs to semantic blocks, not physical rows. A user message, a
finished assistant response, a completed tool result, or another finalized
entry may be committed. Streaming assistant text, reasoning, running tools,
elapsed-time displays, and other changing content remain in the mutable tail.

The commit checkpoint uses stable block identity and generation, not a wrapped
row count or rendered-text hash. Wrapping a block at the current terminal width
happens only when it is first committed. Once written, the terminal owns those
cells. A later resize may change how the terminal emulator presents old lines,
but thndrs does not purge and replay them.

On a fresh or resumed invocation, the coordinator may hydrate the new terminal
session with the current stable transcript once. In-session resize, theme
change, compaction, or focused-view transitions must not trigger hydration
again. Clearing the application transcript begins a new commit generation and
clears the active inline surface; it does not claim to erase emulator
scrollback on terminals that retain it.

### Mutable transcript tail

The transcript renderer supplies a bounded, reflowable tail for content that is
not yet stable. The coordinator places it immediately above the composer and
focused view. It may be clipped when a bounded view needs more room, but its
authoritative semantic block remains intact and is reprojected on the next
draw.

Recently committed transcript rows are not copied back into the live surface
to fill space. This avoids duplicate text, unstable selection, and the old
resize path that treated terminal history as a reconstructable application
viewport.

### Ratatui-owned surfaces

The composer becomes a Ratatui component over existing prompt state. It owns
its border, padding, accessories, key hints, height calculation, and terminal
cursor. Focused views use the same bounded-surface layout contract. The
contract must support replacing or arranging a focused view without inspecting
or rewriting transcript history.

A future "sidebar" in this model is a bounded application view within the live
surface. A permanently full-height column beside already committed terminal
history is not possible without returning to a full-screen application-owned
viewport; that is a separate mode and is outside this plan.

Inline mode does not enable mouse capture by default. Native terminal selection
and copy are part of the feature, while composer and focused-view interactions
remain keyboard-driven. Alternate-screen mouse selection is not ported into
the transcript renderer.

## Operation symbol vocabulary

Add a typed semantic operation kind and map it to this vocabulary:

| Operation | Symbol | Default label |
| --- | ---: | --- |
| Skill | `§` | `Skill` |
| Run / shell | `$` | `Ran` |
| Search | `/` | `Searched` |
| Read | `›` | `Read` |
| Explore | `⌁` | `Explored` |
| Edit / patch | `∆` | `Edited` |
| Create / write | `+` | `Wrote` |
| Delete | `−` | `Removed` |
| Fetch / network | `↗` | `Fetched` |
| Retry / refresh | `⟳` | `Retried` |
| Tool / MCP | `@` | `Tool` |
| Subagent / parallel | `∥` | `Agent` |
| Warning / blocked | `!` | `Blocked` |

Classification happens before surface rendering. Prefer structured tool and
transcript metadata; use the generic `Tool / MCP` category when no more precise
operation is known. Do not make either surface infer an operation by parsing a
rendered label. Create, edit, and delete remain distinct only when the
underlying change data can distinguish them reliably.

The operation glyph is not a success or failure marker. Lifecycle remains a
separate semantic field and may change the verb, color, or adjacent status
while the operation glyph stays fixed. Every glyph is followed by a readable
label, so meaning does not depend on color or symbol recognition. Live and
committed forms use the same classification and wording.

## Terminal lifecycle and input

Keep the existing event loop and normalized input path. Inline mode owns raw
mode, bracketed paste, supported keyboard enhancements, cursor visibility, and
cleanup in one guard. It does not enter the alternate screen or enable mouse
capture by default.

Before suspension or an interactive child process, settle the current terminal
transaction and leave the bounded surface cleanly. On resume, re-enter the
owned modes, invalidate the mutable surface, and redraw it without replaying
committed history. Cleanup must run after normal exit, error, cancellation, and
panic through the same guard.

## Rollout

Build the inline coordinator alongside the current alternate surface behind a
temporary experimental selector. Use it to prove the dynamic-height,
streaming-to-stable, paste, resize, suspend/resume, and cleanup paths in real
terminals. The selector is a migration aid, not a permanent user-facing choice.

Once the inline acceptance matrix passes, make it the default and remove the
alternate viewport's transcript navigation, selection, cache, and rendering
responsibilities. Retain an alternate-screen surface only when a future view
has a concrete full-screen requirement; do not keep two transcript renderers
for hypothetical use.

## Verification

Pure tests should cover semantic block stability, exact-once checkpoints,
live-to-stable transitions, operation classification, symbol/label mapping,
wrapping, and width changes. Ratatui `TestBackend` snapshots should cover the
composer and each bounded focused view at narrow and normal sizes.

PTY or captured ANSI integration tests should establish that:

- stable blocks are inserted once and in order;
- the mutable tail is replaced rather than appended;
- resize does not emit scrollback purge or replay old stable blocks;
- opening and closing a view does not alter transcript history;
- bracketed paste arrives as one paste action;
- suspend/resume and child processes do not duplicate history;
- cursor and terminal modes are restored on every exit path.

Before cutover, manually exercise a normal terminal and tmux with a long
streaming response, a running command, multiline paste, narrow/wide resizes,
native selection, a focused picker or permission prompt, suspend/resume, and
exit during active work. Record terminal-specific failures as blockers rather
than adding emulator-specific cursor workarounds without a reproducible test.

## Boundaries

- Do not rewrite `App`, the update/effect loop, provider streaming, prompt
  editing, or session persistence for this migration.
- Do not let Ratatui rebuild, navigate, select, or retain committed transcript
  history.
- Do not introduce raw terminal writes from both transcript and surface
  renderers.
- Do not purge and replay native history as a resize strategy.
- Do not port alternate-screen mouse capture or transcript key navigation into
  inline mode; the terminal supplies scrollback, selection, and copy.
- Do not design a persistent full-height sidebar beside native history under
  this feature.
- Do not broaden the symbol work into a general icon or theme redesign.

## Risks

- Ratatui's inline viewport has stricter height and cursor assumptions than the
  alternate viewport. Dynamic surface height must be proven before cutover.
- Terminals differ in reflow, synchronized updates, and scroll-region behavior.
  The design minimizes reliance on those differences but still needs PTY and
  manual coverage.
- A semantic block can change after appearing stable if lifecycle transitions
  are misclassified. Exact-once commits require an explicit finality contract,
  not a heuristic based on current row text.
- Native scrollback cannot place a permanent application-owned column beside
  old lines. Future views must respect the bounded-surface contract or choose a
  separate full-screen mode explicitly.
