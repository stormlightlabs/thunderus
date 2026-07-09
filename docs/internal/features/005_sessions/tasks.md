---
title: Sessions Tasks
Status: Draft
Captured: 2026-07-04
---

## P1: Session Listing And Lookup

- [ ] Add a `SessionIdQuery` helper for exact and prefix lookup.
- [ ] Return a clear error for no match.
- [ ] Return a clear error for ambiguous prefixes with matching ids.
- [ ] Extend `SessionSummary` with id, path, updated time, message counts, tool
      counts, and failure counts.
- [ ] Add newest-first list tests.
- [ ] Add corrupt-line tolerance tests.
- [ ] Add tests for empty session directories.
- [ ] Add tests for non-session `.jsonl` files being ignored.

## P2: TUI Commands

- [ ] Add `history` to command suggestions.
- [ ] Add `resume <id>` to command suggestions.
- [ ] Add `session <id>` to command suggestions.
- [ ] Add `tokens` to command suggestions.
- [ ] Add `debug log [session-id]` to command suggestions.
- [ ] Implement `history` transcript output with a compact, capped list.
- [ ] Implement `session <id>` summary output.
- [ ] Implement `tokens` using accumulated session usage.
- [ ] Implement `debug log [session-id]` with line caps and redaction.
- [ ] Implement `resume <id>` transcript loading.
- [ ] Implement appending to an existing session writer on resume.
- [ ] Implement exclusive session writer locking.
- [ ] Preserve the current prompt draft when a non-mutating command fails.
- [ ] Reject resume while an agent turn is running unless the command is queued
      explicitly as text.

## P3: CLI Commands

- [ ] Convert `CliArgs` to support subcommands while preserving default TUI
      startup with no subcommand.
- [ ] Add `sessions list`.
- [ ] Add `sessions show <id>`.
- [ ] Add `sessions resume <id>`.
- [ ] Add `sessions inspect <id> --format json`.
- [ ] Add `sessions export <id> --format jsonl`.
- [ ] Add `debug tail --lines <n>`.
- [ ] Add `debug session-log <id> --lines <n>`.
- [ ] Add `--cwd` behavior for all session/debug subcommands.
- [ ] Add CLI parser tests for default startup and every subcommand.

## P4: Inspect And Export

- [ ] Build a renderer-independent `SessionInspection` projection.
- [ ] Include session metadata, title, provider, model, web-search mode, and app
      version.
- [ ] Include transcript entries in stable order.
- [ ] Include tool start/finish records and statuses.
- [ ] Include file-write audit metadata.
- [ ] Include shell execution metadata.
- [ ] Include token usage totals.
- [ ] Include loaded context metadata.
- [ ] Include context ledger, pin, drop, recovery, and compaction records when
      present.
- [ ] Include memory write and content-free memory delete records when present.
- [ ] Include activated skill metadata.
- [ ] Include redacted effective config metadata when present, otherwise
      `null`.
- [ ] Ensure inspect/export skips or redacts secret-looking values.
- [ ] Add JSON fixture tests.
- [ ] Add replay/projection tests for context ledger records.
- [ ] Add replay/projection tests for pin/drop/recovery records.
- [ ] Add replay/projection tests for memory write/delete records.
- [ ] Add replay/projection tests for compaction records.
- [ ] Test replaying `memory_delete` records does not delete files.
- [ ] Test replaying `compaction` records does not compact again.
- [ ] Add tests proving raw provider payloads are not exported.

## P5: Log Readers

- [ ] Add bounded tail reader for session log files.
- [ ] Add bounded tail reader for daily log files.
- [ ] Tolerate missing log directories and missing log files.
- [ ] Apply existing shell/tool secret redaction where possible.
- [ ] Add tests for caps, missing files, and redaction.

## P6: Docs

- [ ] Update public sessions docs with list/show/resume/inspect/export.
- [ ] Update CLI reference with session and debug subcommands.
- [ ] Update usage docs with `history`, `resume`, `session`, `tokens`, and
      debug log commands.
- [ ] Document session id prefixes and ambiguity behavior.
- [ ] Document what inspect/export stores and omits.
- [ ] Update README session section from placeholder text once commands exist.
