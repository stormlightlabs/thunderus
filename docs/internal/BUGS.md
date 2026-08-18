# Bugs

- [ ] padding on the right side both above & below the input isn't correct (should be the same as the left)
  - [ ] We probably shouldn't have so much horizontal padding for transcript rows,
        as the spaces/empty cells create whitespace if anything is copied
- [ ] Using a picker/slash command pushes up the transcript

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
