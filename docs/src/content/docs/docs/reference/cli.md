---
title: "CLI Reference"
---

## Options

- `--cwd <path>`: working directory for context loading and display.
- `--model <name>`: completion model.
- `--websearch <duckduckgo|searxng|none>`: application-owned web-search backend.
- `--websearch-url <url>`: SearXNG HTTP(S) base URL when `searxng` is selected.
- `--verbose`: show diagnostic transcript rows such as provider events and log paths.
- `--theme <eldritch-minimal|iceberg-dark|catppuccin-mocha>`: UI color theme.
- `--mouse`: enable terminal mouse capture for transcript scrolling and overlay events (the default).
- `--no-mouse`: disable capture for native terminal selection and scrollback.
- `--print-prompt`: print the assembled prompt bundle and exit without calling the provider.
- `--ephemeral`, `--no-session`: keep the run in memory without session JSONL,
  artifact bodies, or session and daily logs. Resume and session naming are not
  available in this mode; shared settings and prompt history keep their normal
  policies.
- `--capture-context-content`: retain sanitized, bounded provider-neutral
  request content and artifact bodies for this run. Metadata-only capture is the
  default. This option cannot be combined with `--ephemeral`.

## Commands

### Auth

- `thndrs setup [--provider <chatgpt-codex|opencode-zen|opencode-go>]`: choose a provider, verify or establish its credential, and apply that provider's initial model when writing a config file. API-key providers use hidden key entry; `chatgpt-codex` uses ChatGPT OAuth. Use `/model`, `--model`, or configuration to select another model.
- `thndrs setup --global`: prefer the user config and credential store.
- `thndrs setup --project`: prefer the workspace config and credential store.
- `thndrs login <provider>`: replace or renew a provider credential. API-key providers use hidden input and explicit confirmation.
- `thndrs logout <provider>`: remove the selected provider's stored credential.
- `thndrs auth status`: show provider credential sources without values.
- `thndrs doctor`: print redacted human-readable setup diagnostics.
- `thndrs doctor --json`: print redacted machine-readable diagnostics for bug reports.
- `thndrs skills doctor`: show duplicate skill names, the selected path, and ignored paths.
- `thndrs config path`: print global and project config paths.
- `thndrs config show --redacted`: print effective config, origins, loaded files, and diagnostics.
- `thndrs config edit --global`: open the global config file with `$EDITOR`.
- `thndrs config edit --project`: open the project config file with `$EDITOR`.
- `thndrs login chatgpt-codex [--oauth-method <browser|device-code>]`: log in through ChatGPT OAuth.
- `thndrs logout chatgpt-codex`: remove the stored ChatGPT Codex credential entry.

Inside the TUI, `/context` or `/context show` opens the active working set;
`/context item <id>` prints one item's details. `/doctor` reports source, pin,
drop, model-limit, budget, and compaction-review health.

Task-local controls are `/context pin <id-or-path>`, `/context drop <id>`,
`/context drop --reset`, `/context recover <id-or-handle>`, and `/context
release <id>`. Verification uses `/context verify propose <protected-id>
<candidate-id>`, then `approve`, `reject`, or `release` with the returned
relation id. `/context export <path> [json|markdown] [--artifacts]` writes the
bounded redacted projection.

Use `/compact` while idle to summarize a closed prefix with the selected model.
Resolve pending summaries with `/context review approve` or `/context review
reject`. The original transcript remains in the append-only session.

Supported setup and login providers are ChatGPT Codex, OpenCode Zen, and
OpenCode Go. API-key providers use hidden input. ChatGPT Codex setup and login
use OAuth and store refreshable credentials in `~/.thndrs/auth.json`, not in
`.thndrs/credentials.env`. Retired provider configurations fail with an
actionable unsupported-route diagnostic.

### Headless Run

`thndrs run [--jsonl] [--stdin-max-bytes <bytes>] [prompt]` runs one prompt
through the same provider, tool, context, and session paths as the TUI without
opening an interface.

- Without `--jsonl`, assistant text streams to stdout and lifecycle diagnostics
  stream to stderr. The command exits with `0` on success, `1` for a run
  failure, `2` when setup is required, `3` when a permission request requires
  the TUI, and `4` after cancellation.
- `--jsonl` emits one versioned JSON object per stdout line. Records use stable,
  provider-neutral types for text, reasoning, usage, retries, tool activity,
  and terminal outcomes. Human diagnostics remain on stderr.
- When stdin is piped, its UTF-8 content is used as the prompt or appended after
  an explicit prompt with a blank line. Terminal stdin is not read. The input
  limit defaults to 64 KiB; `--stdin-max-bytes` accepts values from 1 byte to
  16 MiB.

### ACP (Agent Context Protocol)

ACP agents are selected in the TUI with `--model acp:<name>`. They are
configured under `[acp_agents.<name>]`.

- `thndrs acp list`: list configured ACP agents.
- `thndrs acp inspect <name>`: show one configured ACP agent with command values redacted.
- `thndrs acp smoke <name> --prompt <text>`: initialize an ACP agent, create a temporary session, send one prompt, and print streamed events.
- `thndrs acp logout <name>`: call agent-owned logout when advertised.
- `thndrs acp list-sessions <name>`: list external sessions owned by an ACP agent when it advertises `session/list`.
- `thndrs acp load-session <name> <session-id>`: load an external ACP session and print replayed `session/update` events.
- `thndrs acp resume-session <name> <session-id>`: resume an external ACP session without replaying history.
- `thndrs acp close-session <name> <session-id>`: close an external ACP session when the agent advertises `session/close`.
- `thndrs acp registry [--file <path>]`: list agents from the read-only official ACP Registry metadata without installing them.
- `thndrs acp install <registry-id> [--name <name>] [--file <path>] --yes`: add a supported registry agent to workspace ACP config and installed-agent metadata.
- `thndrs acp update <name> [--file <path>] --yes`: update a registry-managed ACP agent in workspace ACP config and installed-agent metadata.

For more information see [ACP](/docs/usage/acp/).

### ACP Agent Server

`thndrs acp serve` exposes `thndrs` as an ACP stdio agent for editors and IDEs.
stdout is protocol-only; diagnostics go to stderr.

- `thndrs --cwd <path> acp serve`: start the ACP server for a workspace.
- `--model <model>`: provider model for new sessions.
- `--websearch <duckduckgo|searxng|none>`: application-owned web-search backend.
- `--websearch-url <url>`: SearXNG HTTP(S) base URL.
- `--session-dir <path>`: append-only local session JSONL directory.

### MCP (Model Context Protocol)

- `thndrs mcp list`: list configured and trust-blocked MCP servers with their
  status, transport, configuration source, precedence, and containment.
- `thndrs mcp status`: show whether the current project MCP file is trusted and
  print its current hash and workspace scope.
- `thndrs mcp trust`: trust the current workspace's `.thndrs/mcp.toml` at its
  exact file hash.
- `thndrs mcp revoke`: remove MCP trust for the current workspace.
- `thndrs mcp test <name>`: initialize one server and print
  `<server>\tready\t<N> tools`, followed by startup diagnostics.
- `thndrs mcp tools <name>`: list provider-visible namespaced tools from one
  server as `<mcp__server__tool>\t<description>`.
- `thndrs mcp call <server> <tool> --json <args>`: call one original MCP tool
  name with JSON object arguments and print capped tool output lines.

The `<tool>` argument for `mcp call` is the original MCP tool name reported by
the server. Provider-facing names are namespaced as `mcp__{server}__{tool}`.

### Session History & Management

- `thndrs context [--session <id>]`: inspect the latest terminal request in
  the newest session, or in the selected session.
- `thndrs context --json [--session <id>]`: print bounded context history with
  snapshots, diffs, accounting, transformations, diagnostics, capture policy,
  and measurement provenance. Content appears only for opted-in runs.
- `thndrs context changes [<from-request> <to-request>] [--session <id>]`:
  compare two terminal request attempts. Without request ids, compare the
  latest two.
- `thndrs context telemetry [--session <id>]`: export content-free,
  provider-neutral metrics from persisted context records through the
  OpenTelemetry SDK's stdout exporter.
- `thndrs usage [--json] [--session <id>]`: report persisted provider usage for
  the newest or selected session.
- `thndrs sessions list`: scan live, archived, and trashed sessions newest first
  with identity, activity, usage, storage, lock, corruption, and lineage state.
- `thndrs sessions latest`: print the newest local session.
- `thndrs sessions titles`: list local titles newest first.
- `thndrs sessions show <id>`: print replayable transcript entries.
- `thndrs sessions resume <id>`: validate and exclusively lock a live session
  for append-only continuation.
- `thndrs sessions fork <id> <turn-id>`: create an independent session from a
  replayable settled turn and record its lineage.
- `thndrs sessions rename <id> <name>`: change the display title without
  changing session identity.
- `thndrs sessions inspect <id> --json`: print the stable redacted JSON
  projection.
- `thndrs sessions export <id> --format <json|jsonl|markdown|html>`: export a
  redacted record stream or bounded review copy.
- `thndrs sessions storage [--format <human|json>]`: report storage totals and
  policy-reclaimable bytes.
- `thndrs sessions prune [--older-than <days>] [--keep-count <count>]
  [--dry-run] [--format <human|json>]`: preview or apply retention to eligible
  unprotected live sessions.
- `archive`, `unarchive`, `pin`, and `unpin` change session lifecycle state.
- `delete <id>` previews a reversible move to trash; add `--yes` to apply it.
  `restore <id>` restores it during the configured grace period.
- `purge` previews irreversible workspace-session cleanup; add `--yes` to apply
  it. Destructive commands require `--allow-pinned` for pinned sessions.

`session` is accepted as an alias for `sessions`. `<id>` may be exact or a
unique prefix; ambiguous prefixes are rejected. All commands use `--cwd` to
find the workspace session directory unless `--session-dir` is supplied.
Lifecycle, storage, prune, and purge reports support human or JSON output.

### Debugging

- `thndrs debug tail [--lines <count>]`: read the newest bounded, redacted daily log.
- `thndrs debug session-log <id> [--lines <count>]`: read a bounded, redacted
  per-session log.
