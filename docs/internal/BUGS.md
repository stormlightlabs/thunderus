# Bugs

- [ ] The transcript should be copyable with a small indicator/toast message in the status
      line that disappears after a few seconds.
- [ ] We need to either clean-up ANSI escape codes or show colors in output (this is most
      prevalent in diffs)
- [ ] We should show Ctrl+O to expand for the latest cell but click to expand for
      previous entries
- [x] Show an explicit context-compaction status. During a long xhigh turn the
      status line showed `Sending` while the internal state was `0% ctx left · compact`,
      leaving the user unable to tell why the response had paused.
- [ ] the `write_patch` tool shows its params then in the tool output shows
      path unavailable
