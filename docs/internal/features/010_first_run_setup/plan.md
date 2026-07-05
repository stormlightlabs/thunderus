# First-Run Setup And Credential UX

Status: Draft
Captured: 2026-07-05

## Problem

`thndrs` is installable through Cargo, but the first-run path still assumes the
user already knows which provider to use, where credentials belong, how config
layers work, and how to diagnose missing local tools.

Current behavior is powerful but too sharp for a crates.io release:

- provider keys are read from process env or workspace `.env`, but there is no
  guided way to add, validate, or remove them;
- missing credentials are discovered late, usually after the user submits a
  prompt;
- setup requirements such as `rg`, `fd`, writable session directories, and
  config diagnostics are spread across docs and startup rows;
- users cannot ask the binary "what is wrong with my install?" without opening
  the TUI;
- config paths are documented, but there is no command to show, create, or edit
  them safely.

For a first public install channel, the product needs an obvious path from
`cargo install --locked thndrs` to a working first prompt.

## Milestone Outcome

A new user can run `thndrs setup`, choose a provider, enter an API key without
echoing it, write the credential to a deliberate local secret store, validate
the setup, and then run `thndrs`.

If the user skips setup and opens the TUI, `thndrs` detects missing credentials
before the first prompt and shows a focused recovery surface. A support request
can start with `thndrs doctor`, which prints safe redacted diagnostics.

## Goals

1. Add an idempotent `setup` command for first-run readiness.
2. Add provider-scoped `login` and `logout` commands for API-key credentials.
3. Add a TUI first-run/missing-credential recovery surface.
4. Add a redacted `doctor` command for install and environment diagnostics.
5. Add small config affordances: show redacted config, print config paths, and
   open the right config file.
6. Keep secrets out of TOML, logs, sessions, inspect/export, prompt inspection,
   snapshots, and command output.
7. Keep the first implementation simple: API-key providers only, no account
   OAuth, no hosted keychain abstraction unless a concrete provider requires
   it.

## Public Contract

### Setup Command

Add:

```text
thndrs setup
thndrs setup --provider <umans|opencode-go>
thndrs setup --global
thndrs setup --project
```

Behavior:

- Runs outside the TUI.
- Detects current workspace, git root, config files, provider selection,
  credential presence, session directory writability, `rg`, `fd`, and basic
  terminal capability.
- Offers to create either global config or project config.
- Offers to enter a provider API key if the selected provider is missing a key.
- Does not print secrets or store them in TOML.
- Prints the exact next command after success, usually `thndrs`.
- Can be re-run safely without duplicating config keys or credential entries.

Default scope:

- If run inside a git workspace, ask whether setup should be global or project
  scoped.
- If non-interactive stdin is detected, fail with instructions unless enough
  flags are supplied to complete without prompts.

### Login And Logout

Add:

```text
thndrs login <provider>
thndrs logout <provider>
thndrs auth status
```

Supported first providers:

- `umans`: manages `UMANS_API_KEY`.
- `opencode-go`: manages `OPENCODE_GO_KEY`.

`login` behavior:

- Prompts for an API key with hidden input.
- Confirms whether to store the key globally or for the current project.
- Writes to a deliberate secret store, not TOML.
- Validates the credential with the cheapest reliable provider check when
  possible; if validation is unavailable or network fails, clearly marks the
  key as stored but unverified.
- If an environment variable already supplies the key, explain that it has
  higher precedence and ask before writing a stored key.

`logout` behavior:

- Removes only the selected provider's stored key from the selected store.
- If an environment variable still supplies the key, report that logout removed
  stored credentials but the provider will remain authenticated through the
  environment.
- Does not delete unrelated `.env` or credential entries.

`auth status` behavior:

- Shows provider names and credential source only: environment, global store,
  project store, missing, or invalid.
- Never prints key values, hashes, prefixes, suffixes, or lengths.

### Secret Storage

Preferred first implementation:

- Global: `~/.thndrs/credentials.env`
- Project: `.thndrs/credentials.env`

Rules:

- Unix files are created with mode `0600` where supported.
- `.thndrs/credentials.env` is added to `.git/info/exclude` when inside a git
  repo.
- Existing workspace `.env` remains supported for compatibility but setup
  should prefer `.thndrs/credentials.env`.
- Secret stores contain only provider secret variables, not ordinary config.
- Parsing supports the same simple env syntax already accepted by provider
  `.env` loading.

### TUI First-Run Recovery

Before accepting the first provider-backed prompt, detect whether the selected
provider has a usable credential.

If not, render a focused setup surface with:

- selected provider and model;
- missing variable name;
- actions: enter key, switch model/provider, open setup instructions, continue
  without provider setup, quit.

Entering a key uses the same storage path and validation behavior as
`thndrs login`. The surface must not write secrets until the user confirms the
target store.

ACP models (`acp:<name>`) use ACP agent auth behavior instead of provider API
key setup. Missing ACP agent config should show an ACP-specific recovery path:
list configured agents, inspect config, or run `thndrs acp registry`.

### TUI Slash Commands

The CLI commands are canonical. The TUI exposes only the safe subset as slash
commands, and commands that need interaction open a focused surface instead of
accepting secrets or long forms inline.

Supported slash commands:

- `/doctor`: show redacted setup diagnostics in the transcript.
- `/auth status`: show provider credential source/status without values.
- `/config path`: show global and project config paths.
- `/config show`: show redacted effective config and diagnostics.
- `/setup`: open the focused first-run/setup surface.
- `/login <provider>`: open the credential entry surface for that provider.
- `/logout <provider>`: open a confirmation surface before removing the stored
  credential.

Rules:

- Never accept an API key as a slash-command argument.
- `/login <provider>` must not proceed through ordinary prompt text input; it
  opens hidden credential entry.
- `/setup` is a focused workflow, not a transcript questionnaire.
- `/config edit` is not a slash command. From inside the TUI, show a short
  message telling the user to run `thndrs config edit --global` or
  `thndrs config edit --project` outside the TUI.
- Slash commands should preserve the current prompt draft after success,
  cancellation, or failure.

### Doctor Command

Add:

```text
thndrs doctor
thndrs doctor --json
```

Human output should be concise and safe to paste into an issue. JSON output is
for automation and tests.

Diagnostics include:

- app version;
- resolved workspace;
- default model and provider;
- config files loaded and redacted diagnostics;
- credential status by provider source, never value;
- `rg` and `fd` availability;
- session directory writability;
- MCP server counts and load diagnostics;
- ACP agent counts and load diagnostics;
- terminal capability summary when available;
- docs URL and setup hint when a required item fails.

Exit codes:

- `0`: no blocking issues.
- `1`: blocking setup issue such as missing credentials for selected provider.
- `2`: invalid config or CLI usage.

### Config Commands

Add:

```text
thndrs config path
thndrs config show --redacted
thndrs config edit --global
thndrs config edit --project
```

`config show --redacted` prints effective config, origins, loaded files, and
diagnostics without secret values. `config edit` opens the file named by
`$EDITOR` when available; without `$EDITOR`, it prints the path and exits with
a clear message. Creating parent directories is allowed after confirmation.

## Implementation Shape

Likely files:

- `src/cli/mod.rs`: add top-level `Setup`, `Login`, `Logout`, `Auth`,
  `Doctor`, and `Config` commands.
- `src/core/auth.rs`: credential store paths, parsing, redaction, read/write,
  and permission handling.
- `src/core/providers/mod.rs`: provider auth metadata and cheap validation
  hooks.
- `src/core/providers/umans.rs`: credential status and optional validation.
- `src/core/providers/opencode.rs`: credential status and optional validation.
- `src/cli/app.rs`: first-run/missing-credential state transition.
- `src/cli/renderer/live.rs`: focused first-run recovery surface.
- `src/core/diagnostics.rs`: doctor projection shared by human and JSON output.
- `docs/src/content/docs/getting-started/installation.md`: setup-first install
  flow.
- `docs/src/content/docs/reference/cli.md`: command reference updates.
- `docs/src/content/docs/reference/environment-variables.md`: credential store
  behavior and precedence.
- `README.md`: replace setup/install TODOs with the canonical path.

Prefer standard library terminal input only if hidden input can be done
correctly on Unix and Windows. If a small dependency is needed for hidden
password entry, add it intentionally and cover it in review.

## Boundaries

Always:

- Redact secrets by default.
- Keep provider keys out of TOML.
- Prefer idempotent file edits.
- Preserve existing environment and workspace `.env` compatibility.
- Test non-interactive failure behavior.

Ask first:

- Adding a dependency for hidden input or opening editors.
- Introducing OS keychain/keyring integration.
- Changing provider environment variable names.
- Removing workspace `.env` support.

Never:

- Echo or log API keys.
- Store secrets in sessions, prompt inspection, config metadata, snapshots, or
  docs examples.
- Modify tracked git files to hide secrets without explicit user action.
- Delete a user's existing `.env` or credential file wholesale.

## Deferred Milestones

- OS keychain integration after API-key file storage is stable and a provider
  needs stronger local secret storage.
- OAuth/device-code flows for providers that require account login.
- Rich provider account status, billing, quota, or subscription checks.
- Full interactive setup for MCP servers and ACP registry agents.
- Shell completion generation and install verification for packaged binaries.

## Verification

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test auth`
- `cargo test config`
- `cargo test cli`
- `cargo test app`
- `cargo test renderer`
- `cargo test providers`
- `cargo test`
- Manual smoke:
  - clean temp home: `thndrs setup`
  - missing key: `thndrs doctor`
  - login/logout round trip for each API-key provider
  - first-run TUI recovery with a fake provider key

## Risks And Open Questions

- Hidden input may require a dependency; implement carefully instead of faking
  secrecy with ordinary stdin.
- Provider validation should avoid expensive model calls. If no cheap endpoint
  exists, mark credentials as stored but unverified.
- Storing project credentials under `.thndrs/credentials.env` is safer than
  ordinary `.env`, but users can still commit it accidentally if `.git` is not
  available. Warn clearly.
- A TUI setup surface should be small and deterministic; do not turn it into a
  full settings UI before release.
