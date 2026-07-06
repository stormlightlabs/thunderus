# v0.1 Release QA Checklist

Run this from a clean checkout before cutting the release. Use real credentials
only where explicitly called out, and do not paste secrets into notes.

## Build And Static Checks

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --all-targets --allow-dirty`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test`
- [ ] `pnpm --dir docs build`
- [ ] Confirm generated docs report all internal links valid.
- [ ] Confirm `CHANGELOG.md` covers user-visible changes.
- [ ] Confirm `docs/internal/archive/v0.1.md` includes completed feature notes.

## Fresh Install Path

- [ ] Install with `cargo install --locked thndrs` in a clean environment.
- [ ] Run `thndrs --version`.
- [ ] Run `thndrs setup` in a temp HOME and temp workspace.
- [ ] Confirm setup prints workspace, provider, credential status, and next
  command.
- [ ] Confirm setup does not duplicate existing config keys when run twice.
- [ ] Run setup in a git repo and confirm `.git/info/exclude` includes
  `.thndrs/credentials.env`.
- [ ] Run setup with non-interactive stdin and confirm it fails with useful
  instructions.

## Config And Credentials

- [ ] `thndrs config path` prints global and project paths.
- [ ] `thndrs config show --redacted` prints effective config without secrets.
- [ ] `thndrs config edit --global` handles missing `$EDITOR` clearly.
- [ ] `thndrs config edit --project` creates parent directories only after
  confirmation.
- [ ] `thndrs login umans` stores only `UMANS_API_KEY`.
- [ ] `thndrs logout umans` removes only `UMANS_API_KEY`.
- [ ] `thndrs login opencode-go` stores only `OPENCODE_GO_KEY`.
- [ ] `thndrs logout opencode-go` removes only `OPENCODE_GO_KEY`.
- [ ] `thndrs login opencode-zen` stores only `OPENCODE_ZEN_KEY`.
- [ ] `thndrs logout opencode-zen` removes only `OPENCODE_ZEN_KEY`.
- [ ] `thndrs auth status` shows sources and never values.
- [ ] Credential files are mode `0600` on Unix.
- [ ] Project credentials are ignored by git.
- [ ] Process env credentials override global, project, and `.env` credentials.

## Doctor

- [ ] `thndrs doctor` exits `1` when the selected provider is missing
  credentials.
- [ ] `thndrs doctor --json` exits `1` for the same blocking setup issue.
- [ ] With fake stored credentials, doctor output is redacted.
- [ ] With valid credentials, doctor exits `0`.
- [ ] Doctor reports `rg`, `fd`, session directory, MCP counts, ACP counts, and
  terminal summary.
- [ ] `doctor --json` is safe to paste into an issue.

## TUI First Run

- [ ] Launch TUI with missing provider credentials.
- [ ] Confirm recovery appears before prompt submission.
- [ ] Confirm prompt draft survives setup, cancellation, model switching, and
  quit paths.
- [ ] Enter an API key through the recovery surface and confirm hidden input.
- [ ] Store a key globally and confirm it works on restart.
- [ ] Store a key for the project and confirm it works on restart.
- [ ] Confirm API-key-looking slash-command arguments are rejected.
- [ ] Confirm ACP models show ACP recovery, not provider API-key setup.

## TUI Slash Commands

- [ ] `/doctor` appends redacted diagnostics.
- [ ] `/auth status` appends provider status without values.
- [ ] `/config path` appends config paths.
- [ ] `/config show` appends redacted effective config.
- [ ] `/setup` opens the setup surface.
- [ ] `/login opencode-zen` opens hidden credential entry.
- [ ] `/logout opencode-zen` opens a confirmation surface.
- [ ] `/config edit` tells the user to run the CLI command outside the TUI.

## Providers

- [ ] OpenCode Zen missing credentials fail before network access.
- [ ] With `OPENCODE_ZEN_KEY`, `opencode/big-pickle` streams a small response.
- [ ] Big Pickle picker text keeps the limited-free/privacy caveat.
- [ ] OpenCode Go still uses `opencode-go/` and `OPENCODE_GO_KEY`.
- [ ] Umans still uses `umans-coder` and `UMANS_API_KEY`.
- [ ] ChatGPT Codex missing credentials fail before network access.
- [ ] `thndrs login chatgpt-codex` device-code flow works with a real account.
- [ ] ChatGPT Codex browser PKCE fallback works when tested manually.
- [ ] `CHATGPT_CODEX_ACCESS_TOKEN` works for one process and does not write
  `~/.thndrs/auth.json`.

## Model And Statusline

- [ ] Fresh config defaults to `opencode/big-pickle`.
- [ ] `--model umans-coder` overrides config.
- [ ] `THNDRS_MODEL` overrides TOML and is overridden by `--model`.
- [ ] Model picker includes Umans, OpenCode Go, OpenCode Zen, ChatGPT Codex,
  and configured ACP agents.
- [ ] TTFT shows `ttft: pending` before first semantic model output.
- [ ] TTFT switches to milliseconds or seconds after first output.
- [ ] TTFT hides before core fields on narrow terminals.

## Sessions And Privacy

- [ ] Session metadata includes safe config origins and provider/model labels.
- [ ] Session records do not include provider API keys.
- [ ] ChatGPT access and refresh tokens do not appear in logs, sessions, prompt
  inspection, or snapshots.
- [ ] Shell output redaction catches common token patterns.
- [ ] Tool write and shell audit records still appear after successful runs.

## MCP And ACP

- [ ] `thndrs mcp list` handles empty config.
- [ ] `thndrs mcp test <server>` reports ready/skipped/failed clearly.
- [ ] `thndrs mcp tools <server>` lists namespaced tools.
- [ ] `thndrs mcp call <server> <tool> --json <args>` records bounded output.
- [ ] `thndrs acp list` handles empty config.
- [ ] `thndrs acp inspect <name>` redacts command env.
- [ ] `thndrs acp smoke <name> --prompt "ping"` works with a configured agent.
- [ ] ACP permission prompts can approve and reject a request.

## Docs

- [ ] README quickstart points to `cargo install --locked thndrs`,
  `thndrs setup`, and `thndrs`.
- [ ] README config section links to the live config reference.
- [ ] Installation docs describe setup-first flow.
- [ ] Provider docs cover OpenCode Zen, OpenCode Go, Umans, and ChatGPT Codex.
- [ ] Environment variable docs describe provider credential precedence.
- [ ] TUI docs list safe setup/auth/config slash commands.
- [ ] Security docs still warn that local tools are not a sandbox.

## Release Packaging

- [ ] `cargo package` succeeds.
- [ ] Inspect packaged files for missing docs, license, README, or generated
  artifacts that should not ship.
- [ ] Install from the local package and run `thndrs setup`.
- [ ] Tag only after tests, docs build, package check, and manual smoke pass.
