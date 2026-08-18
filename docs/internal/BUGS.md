# Bugs

- [x] switching models in ../mariners-astrolabe (dalil) made the context
      go down to 97%
- [x] Context is erratic (this could be model related): with terra, the turn
      ended at 27% then jumped to 72% with a follow-up
- [x] Keybind for steering vs. queueing doesn't work in all environments
- [x] We see 0% context then just "compact" after reasoning level. that should not be
      the case. Compacting should be a discrete state.
- [x] The input icon doesn't change based on state or status
- [x] Diffs aren't colored properly
- [x] You can't move through a potential file path when the file picker is open.
      For example, if you put `@READ`, you can add & delete chars but can't move through it
- [x] `?` pane behavior is off
  - [x] You have to hit the arrow keys to get through the first page before scrolling further
        down which is fine if the rows are highlighted
  - [x] The pane uses the old background color when it should be transparent.
  - [x] Our pickers should have fuzzy finding if their contents are deterministic
- [x] Keybind hint text shouldn't be bolded
- [x] The slash command ui doesn't look quite right
- [ ] padding on the right side both above & below the input isn't correct (should be the same as the left)
- [x] Model, Reasoning Level, & remaining context should be different colors. Remaining context should
      stay as-is (light grey)

### Fix transcript rendering

- [x] Shrink the permanent inline viewport.
      `INLINE_VIEWPORT_HEIGHT` is currently ~23 rows because it includes `MAX_SETUP_ROWS`. That reserves a large blank region during normal operation. Base the normal viewport on the composer/status surface instead—roughly **10–12 rows**.
- [x] Do not budget setup/auth UI into the normal viewport.
      Treat setup/auth as a special temporary surface, or let it clip/scroll within the normal live region. `LiveSurfaceLayout` already has clipping behavior suitable for this.
- [x] Lay out against Ratatui's actual viewport area.
      Don't pass `Terminal::size().height` into `inline_frame()`. Build the live frame using the `Frame::area()` height Ratatui actually gives the inline viewport. Right now the layout thinks it has the whole terminal available.
- [x] Keep bottom alignment, once the viewport is correctly sized.
      The bottom alignment in `render_logical_frame()` is desirable for keeping the composer pinned to the bottom. It only looks wrong because it's currently bottom-aligning ~6 rows inside a 23-row viewport.
- [x] Simplify resize handling.
      Consider dropping the explicit `Terminal::resize()` call on `Action::Resize` and just request an immediate repaint; Ratatui's inline viewport can follow backend resize during rendering.

#### Target architecture

```text
native terminal scrollback
──────────────────────────
streaming/live tail
(optional bounded detail)
composer: 1–8 rows
status:   1 row
──────────────────────────
```

## UI

- [ ] Let Enter open the selected tool result's full output, matching Ctrl+O.
- [ ] Collapse related edits and verification commands into semantic summaries such
      as `Changed map selection` and `Verified`, with the individual operations
      available in the expanded view.
- [ ] Give tool activities distinct semantic kinds for reads, searches, edits,
      tests, lints, builds, and shell commands. Preserve their subject, metrics,
      duration, exit code, preview, and raw output.
- [ ] Add compact, normal, and expanded transcript density levels, including a
      global way to switch the amount of tool detail shown.
- [ ] Extract structured failure diagnostics, including the cause, source location,
      and relevant code excerpt, instead of relying on string matching.
- [ ] Render diffs with an adaptive, high-fidelity layout instead of generic tool
      output formatting.
- [ ] Show elapsed time beside long-running commands while retaining the short live
      output tail, then collapse both into the completed result summary.

## Feature Parking Lot

- [ ] we need a herdr plugin/integration such that it recognizes thndrs as an
      agent
- [ ] `toml_edit` for the rest of the config would be nice
