# Bugs

## Transcript usability

The transcript is harder to scan than Codex or Pi. Reasoning, status updates,
tool activity, failures, retries, and assistant prose compete in one vertical
stream. The application records the right information, but needs a clearer
reading hierarchy.

- Make assistant prose the dominant transcript layer. Render reasoning and
  status updates more quietly and compactly.
- Give each tool call one stable lifecycle block with its action, target,
  current state, and concise result. Update that block in place instead of
  adding visual traffic.
- Separate the final response from preceding operational entries with
  consistent spacing and a restrained marker.
- Show when the transcript is anchored away from the latest entry, including a
  clear hint such as `End to follow`. Make returning to follow mode visible.
- Keep the footer focused on immediate operational state. Move secondary
  telemetry such as quota and token details to an inspectable status view.
- Show the tool-output expansion shortcut beside collapsed output, for example
  `Ctrl+O details`, instead of relying on users to discover it elsewhere.
- Replace ambiguous spinners with precise state labels such as `Thinking`,
  `Running cargo test`, `Stopping`, and `Stopped`.
