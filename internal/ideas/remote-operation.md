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
concurrent sessions by default, and `--spawn worktree` gives each on-demand
session its own git worktree, which matches what the `worktree` skill's
**Who gets one** gives a local session working directly. A sleeping laptop or a
dropped network reconnects on its own, with messages and permission prompts
queued in the meantime.

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
binaries".

### Pi's machine interface is its own JSONL RPC, not ACP

`pi --mode rpc` speaks a line-delimited JSON protocol over stdin and stdout,
with commands in and events out. It is not ACP, so `model = "acp:pi"` does not
reach it. The framing is strict: LF only, and Pi's documentation names Node's
`readline` as non-compliant because it also splits on U+2028 and U+2029.

### Remote work is Claude only

Codex and Pi keep their roles in `../models.md` and keep running this workflow
locally. Neither gains a remote surface: Pi's is excluded from its published
builds, and Codex cloud is another host rather than a way into this machine. A
run started away from the machine is a Claude Code session, and the shell over
the tailnet exists to operate the machine rather than to drive another harness.

### Lndrs gets no worker kind for Pi

`../features/lndrs/plan.md` (`01M2S3GAM20S0VNJ4A10MBBWR0`) keeps its two worker
kinds, ACP and a PTY pane, and Pi is driven as a pane. Its JSONL RPC would be a
better interface than a pane, and that is not reason enough for a third kind.

### Third-party Pi packages are read before they are installed

Pi packages run with full system access and extensions execute arbitrary code,
and the machine now holds an app private key, so a third-party extension is a
credential exposure rather than only a supply-chain risk.

`pi-web-access` for web search and `pi-mcp-extension` for MCP servers are the
two worth reading and pinning. Subagent packages wait until a run needs dispatch
under Pi.

### A read-only tree replaces the pattern list

A denylist of mutating `bash` commands is not maintainable. Pi hands the tool
one command string for `bash -c`, so a list has to parse shell, and `eval`,
substitution, quoting, and `python -c` all pass through it. Pi's own security
documentation says project trust does not sandbox tool calls, and points at
containers instead. The set of ways to write a file is open-ended too: `sed -i`,
`tee`, a redirection, `dd`, `patch`, `git apply`, `install`, `truncate`.

Mount everything read-only and open holes deliberately. The `review` skill
already reads through `git show <commit>:<path>` rather than a checkout, so a
reviewer needs no writable path at all:

```sh
env -u SSH_AUTH_SOCK -u GH_TOKEN -u GITHUB_TOKEN \
  bwrap --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp \
        --tmpfs /run/user/"$UID" --tmpfs "$HOME/.config" --tmpfs "$HOME/.ssh" \
        --chdir "$PWD" --unshare-pid -- pi -xt write,edit
```

A read-only root rather than `--dev-bind / /` with holes cut in it, because the
second is the denylist this section just rejected, moved to the mount layer. It
has to name everything a reviewer must not reach, and a name left out is a
writable path.

The credential mounts are what the first attempt at this missed. Read-only alone
leaves a reviewer authenticated: `gh` reads its token from the keyring under
`/run/user/$UID`, ssh reads a key from `~/.ssh`, and either one can merge the
pull request under review. Unsetting `SSH_AUTH_SOCK` is not enough on its own,
because ssh falls back to the key files.

Checked on 2026-09-19 under `bubblewrap` 0.11.2, running the command above:
`git log` and `git show` answer, `touch` fails with `Read-only file system`,
`gh auth status` reports not logged in, `ssh -T git@github.com` fails host key
verification, and `git push --dry-run` fails on access rights.

Keep blocking `write` and `edit` with `-xt`, and use Pi's `tool_call` event,
which can block a call by returning `{ block: true, reason }`, to say why. The
mounts are the boundary. The tool denylist makes the refusal legible, so a
reviewer reaching for an editing tool is told what it is rather than reading a
permission error.

## Open

What else the reviewer sandbox has to hide from reads. The write set is empty by
default now, so a directory left out leaks a read rather than a write, but the
`--tmpfs` list still enumerates credentials by hand and covers the two this
machine is known to hold. Settled by listing what the home directory carries
before the first review pass runs under it.

Whether Pi runs at all with `$HOME/.config` and the session directory read-only.
The mounts were checked against `git` and `gh`, not against `pi`, which writes
sessions under `~/.pi`. Settled by running one review pass and reading what it
fails on.

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
