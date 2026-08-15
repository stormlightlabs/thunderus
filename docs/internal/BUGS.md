# Bugs

- [x] switching models in ../mariners-astrolabe (dalil) made the context
      go down to 97%
- [x] Context is erratic (this could be model related): with terra, the turn
      ended at 27% then jumped to 72% with a follow-up
- [ ] Keybind for steering vs. queueing doesn't work in all environments
- [x] We see 0% context then just "compact" after reasoning level. that should not be
      the case. Compacting should be a discrete state.
- [ ] the input icon shouldn't change based on state/status
- [x] Diffs aren't colored properly
- [ ] You can't move through a potential file path when the file picker is open.
      For example, if you put `@READ`, you can add & delete chars but can't move through it

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
