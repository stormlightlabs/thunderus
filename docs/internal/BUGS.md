# Bugs

- [ ] when trying out `/commit`, I got this error:
  - HTTP 400: Error from provider (Console Go): Upstream request failed: `[invalid_request_error]`
    The `reasoning_content` in the thinking mode must be passed back to the API.
  - retrying proceeded just fine
- [ ] Thinking should wrap, not be truncated
- [ ] Spacing between the rendered Thinking blocks and tool calls seems excessive.

## UI

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
- [ ] Shift-Tab reasoning level cycling instead of forcing the user to select right away
