---
name: remote-operation
last_updated: 2026-09-19
id: 01M2VZFXVCF6XSDF3PWMQYTWHT
---

# Where runs happen

## Problem

A run needs a host that can hold a credential, reach GitHub, and be driven by
the operator from wherever they are. The two cloud hosts each fail one of those.

Claude Code cloud environments warn against putting secrets in environment
variables, because anyone who uses the environment can read them. The mechanism
built for that case, an API credential the agent proxy attaches without the
session seeing it, excludes GitHub. Its GitHub proxy also serves only a pinned
set of GraphQL operations and names Projects v2 as unreachable, returning a 403
for a `GH_TOKEN` the caller supplies.

Codex cloud encrypts secrets but makes them "only available to setup scripts",
and removes them before the agent phase starts.

Cloud containers also start cold, which is why `.claude/hooks/session-start.sh`
exists at all: no Cargo registry, no `target/`, no `docs/node_modules`.

## Decisions

### Runs happen on the operator's machine

The machine stays on and the operator reaches it. This is what lets the GitHub
App private key live at a path with mode 600 instead of in a cloud
configuration, which is the condition `agent-attribution.md`
(`01M2VZFXTS1HCMB42NHHZ5DECH`) depends on. It also removes the second GitHub
transport: nothing has to dispatch a workflow to write as the app.

The cost is a single machine holding a credential that can write to every
repository the app is installed on. Full-disk encryption, the key at mode 600,
and a narrow installation are what that buys back.

### Claude Code Remote Control is the first-party surface

`claude remote-control` runs a server on the machine and connects
claude.ai/code or the mobile app to sessions that keep running locally, so
filesystem access and execution stay on the machine. It serves up to 32
concurrent sessions, and `--spawn worktree` gives each on-demand session its own
git worktree, which is the rule
`.claude/agents/implementer.md` already enforces for dispatched implementers. A
sleeping laptop or a dropped network reconnects on its own, with messages and
permission prompts queued in the meantime.

It requires a subscription and rejects API keys. Run it under a
`systemd --user` unit so a reboot does not leave it absent.

### Tailscale for reach, tmux for persistence, Eternal Terminal for reconnects

These are three problems and one tool each. Tailscale puts the machine on a
tailnet so nothing listens on the public internet. tmux is what survives a
dropped client, and the repository already runs it for interactive TUI QA.
Eternal Terminal reconnects over TCP while keeping full scrollback.

Mosh is the wrong pick here. It keeps only the characters currently on screen in
sync, so scrollback is lost, and it carries no port forwarding or file transfer.
It also does not persist anything: if the client dies, so does the program,
unless tmux is holding it.

### Pi runs this workflow already

Pi discovers project skills from `.agents/skills` in the working directory and
its ancestors, and finds directories containing `SKILL.md` recursively. The
`.agents/skills` symlink `CLAUDE.md` requires is what makes all twelve skills
visible with no configuration. Pi reads `AGENTS.md` and `CLAUDE.md` as context
files.

The commands are close to drop-in. Pi's prompt templates use `description` and
`argument-hint` frontmatter and expand `$ARGUMENTS`, which is exactly what
`.claude/commands/impl.md` contains. Only discovery differs, and
`"prompts": [".claude/commands"]` in `.pi/settings.json` covers it. Template
discovery there is not recursive, and `.claude/commands/` is flat.

Pi has no remote control in the installed build. Its experimental remote
harness exists, but its own development documentation records that the
server and client commands are "excluded from npm packages and standalone
binaries". So Pi is reached through tmux over the tailnet.

### Pi's machine interface is its own JSONL RPC, not ACP

`pi --mode rpc` speaks a line-delimited JSON protocol over stdin and stdout,
with commands in and events out. It is not ACP, so `model = "acp:pi"` does not
reach it. The framing is strict: LF only, and Pi's documentation names Node's
`readline` as non-compliant because it also splits on U+2028 and U+2029.

### Lndrs gets no worker kind for Pi

Pi is transitional. It runs GPT models until `thndrs` can be dogfooded in those
roles, and then it leaves the way Codex already has.
`../features/lndrs/plan.md` (`01M2S3GAM20S0VNJ4A10MBBWR0`) keeps its two worker
kinds, ACP and a PTY pane, and Pi is driven as a pane or not at all. A third
kind would be built for a worker already scheduled for removal.

### The reviewer gate is an extension written here

Pi's `tool_call` event can block a call, returning
`{ block: true, reason, terminate? }`. That makes "a reviewer does not edit
code" a check rather than a sentence in a skill. Blocking `write` and `edit` is
exact; blocking a mutating `bash` command is a pattern list, and Pi's own
security documentation says project trust does not sandbox tool calls and points
at containers for a real boundary.

This one is written in the repository rather than installed. Pi packages run
with full system access and extensions execute arbitrary code, and the machine
now holds an app private key, so a third-party extension is a credential
exposure rather than only a supply-chain risk. `pi-web-access` for web search
and `pi-mcp-extension` for MCP servers are the two worth reading and pinning;
subagent packages wait until a run needs dispatch under Pi.

## Open

Whether `pi-web-ui`, a third-party browser cockpit, is an acceptable second
remote surface. It would cover from a phone what Remote Control covers for
Claude. Settled by reading its source, against the key on the same machine.

Whether the reviewer gate's pattern list stays maintainable, or whether a
container is the honest answer. Settled by counting what the list has to grow to
cover after the first few review passes run under it.

## Sources

- [Remote Control](https://code.claude.com/docs/en/remote-control) and
  [Configure cloud environments](https://code.claude.com/docs/en/cloud-environments),
  read 2026-09-18, for the local execution guarantee, session capacity,
  `--spawn worktree`, reconnection, the subscription requirement, the
  environment variable warning, and the GraphQL restriction.
- [Codex cloud environments](https://developers.openai.com/codex/cloud/environments/),
  for secrets reaching setup scripts only.
- `@earendil-works/pi-coding-agent` 0.85.1 as installed, reading
  `docs/skills.md`, `docs/prompt-templates.md`, `docs/rpc.md`,
  `docs/extensions.md`, `docs/security.md`, and `docs/development.md`, plus
  `pi --help`. Read from the shipped package rather than the website.
- [Pi package catalog](https://pi.dev/packages), for `pi-web-access`,
  `pi-mcp-extension`, `pi-web-ui`, and the subagent packages.
- [Mosh](https://mosh.org/) and [Eternal Terminal](https://eternalterminal.dev/),
  for screen-only synchronization against journaled scrollback.
