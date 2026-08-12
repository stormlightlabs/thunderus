# Bugs

- [ ] Steering prompts show up as "queued steering" but should just show "Steering"
- [ ] The transcript should be copyable with a small indicator/toast message in the status
      line that disappears after a few seconds.
- [ ] We need to either clean-up ANSI escape codes or show colors in output (this is most
      prevalent in diffs)
- [ ] We should show Ctrl+O to expand for the latest cell but click to expand for
      previous entries
- [x] `--keep-count` in `thndrs session prune` should alias to `--keep/--k/-k`
  - [x] There should also be progress output in stderr before results in stdout
- [ ] When reaching the end of the input, it breaks at letter, not word
