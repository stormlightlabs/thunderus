# Bugs

- [ ] The transcript should be copyable with a small indicator/toast message in the status
      line that disappears after a few seconds.
- [x] Strip ANSI escape codes from tool output before rendering, including diffs.
- [ ] We should show Ctrl+O to expand for the latest cell but click to expand for
      previous entries
- [x] Show an explicit context-compaction status. During a long xhigh turn the
      status line showed `Sending` while the internal state was `0% ctx left · compact`,
      leaving the user unable to tell why the response had paused.
- [x] Show the edited path for `write_patch` activity instead of `path unavailable`.
