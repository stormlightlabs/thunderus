# First-Run Setup And Credential UX Tasks

Status: Draft
Captured: 2026-07-05

## P0: Lock The Contract

- [x] Setup is a release-readiness affordance for first crates.io users.
- [x] Provider API keys stay out of TOML config.
- [x] First implementation supports API-key providers only:
      `umans` and `opencode-go`.
- [x] `thndrs setup` is idempotent and safe to re-run.
- [x] `thndrs login <provider>` writes provider credentials only after explicit
      confirmation.
- [x] `thndrs logout <provider>` removes only the selected provider's stored
      credential.
- [x] `thndrs doctor` is redacted and safe to paste into issues.
- [x] TUI missing-key recovery happens before accepting the first
      provider-backed prompt.
- [x] ACP models use ACP-specific recovery instead of provider API-key setup.
- [x] Hidden input must be real hidden input or use a reviewed dependency; no
      plaintext prompt fallback for API keys.
- [x] Project credential storage must be git-excluded when possible.
- [x] CLI commands are canonical; TUI slash commands expose only safe shortcuts
      and focused flows.
- [x] API keys are never accepted as slash-command arguments.
- [x] `config edit` remains CLI-only and is not implemented as a TUI slash
      command.

## P1: Credential Store

- [ ] Add `src/core/auth.rs` with module docs.
- [ ] Define supported API-key providers and their env var names.
- [ ] Add global credential path `~/.thndrs/credentials.env`.
- [ ] Add project credential path `.thndrs/credentials.env`.
- [ ] Parse simple env assignment lines compatible with current `.env` support.
- [ ] Write credential files atomically.
- [ ] Set Unix file mode `0600` where supported.
- [ ] Preserve unrelated credential entries when adding or removing one key.
- [ ] Redact credential values in all display/debug paths.
- [ ] Add `.thndrs/credentials.env` to `.git/info/exclude` when a git repo is
      present.
- [ ] Tolerate missing home directory with a clear error.
- [ ] Test read/write round trips.
- [ ] Test removing one credential preserves others.
- [ ] Test malformed credential files fail clearly without printing values.
- [ ] Test Unix mode behavior where supported.
- [ ] Test git exclude update is idempotent.

## P2: Provider Credential Resolution

- [ ] Extend provider key lookup to check process env first.
- [ ] Check global and project credential stores after process env.
- [ ] Preserve existing workspace `.env` fallback for compatibility.
- [ ] Return credential source metadata without values.
- [ ] Add provider auth status helpers for Umans and OpenCode Go.
- [ ] Add optional cheap validation hooks for Umans.
- [ ] Add optional cheap validation hooks for OpenCode Go.
- [ ] If validation is unavailable or network fails, report stored/unverified.
- [ ] Keep provider request logs free of credential values.
- [ ] Test source precedence: env, project credentials, global credentials,
      workspace `.env`, missing.
- [ ] Test missing-key errors include the variable name and setup hint.
- [ ] Test provider validation does not persist provider payloads.

## P3: CLI Setup And Auth Commands

- [ ] Add top-level `setup` command.
- [ ] Add `setup --provider <umans|opencode-go>`.
- [ ] Add `setup --global`.
- [ ] Add `setup --project`.
- [ ] Add `login <provider>`.
- [ ] Add `logout <provider>`.
- [ ] Add `auth status`.
- [ ] Implement real hidden API-key input.
- [ ] Detect non-interactive stdin and fail with actionable instructions unless
      sufficient flags are present.
- [ ] Ask before writing global config, project config, or credential files.
- [ ] Make setup avoid duplicating existing config keys.
- [ ] Print the next command after successful setup.
- [ ] Add parser tests for every new command and flag.
- [ ] Add command-output tests for setup dry paths.
- [ ] Add login/logout command tests using temp HOME/workspace.

## P4: Doctor Command

- [ ] Add top-level `doctor`.
- [ ] Add `doctor --json`.
- [ ] Report app version.
- [ ] Report resolved workspace.
- [ ] Report selected model/provider.
- [ ] Report loaded config files and redacted diagnostics.
- [ ] Report credential status by provider source, never value.
- [ ] Report `rg` availability.
- [ ] Report `fd` availability.
- [ ] Report session directory writability.
- [ ] Report MCP configured/ready/skipped/failed counts.
- [ ] Report ACP configured/enabled/disabled counts.
- [ ] Report terminal capability summary if available.
- [ ] Return exit code `0` for no blocking issues.
- [ ] Return exit code `1` for blocking setup issues.
- [ ] Return exit code `2` for invalid config or CLI usage.
- [ ] Add human-output snapshot tests.
- [ ] Add JSON fixture tests.
- [ ] Add tests proving secrets are absent from doctor output.

## P5: Config Commands

- [ ] Add `config path`.
- [ ] Add `config show --redacted`.
- [ ] Add `config edit --global`.
- [ ] Add `config edit --project`.
- [ ] Print global and project config paths with display policy.
- [ ] Show effective config, origins, loaded files, and diagnostics redacted.
- [ ] Respect `$EDITOR` for `config edit`.
- [ ] If `$EDITOR` is missing, print the path and exit clearly.
- [ ] Ask before creating config parent directories.
- [ ] Add parser tests.
- [ ] Add redacted output tests.
- [ ] Add no-editor behavior tests.

## P6: TUI First-Run Recovery

- [ ] Add app state for first-run/missing-provider-credential recovery.
- [ ] Detect missing selected-provider credential before first provider-backed
      prompt.
- [ ] Render focused recovery surface with provider, model, missing env var,
      and actions.
- [ ] Add action to enter API key.
- [ ] Add action to switch model/provider through existing picker affordance or
      a simple provider list.
- [ ] Add action to show setup instructions.
- [ ] Add action to continue without setup only when it will not immediately
      submit a provider-backed prompt.
- [ ] Add action to quit.
- [ ] Store entered keys through the same credential-store path as `login`.
- [ ] Confirm global vs project storage before writing.
- [ ] Keep prompt draft intact after recovery.
- [ ] Add app update tests for each action.
- [ ] Add renderer snapshots for normal, narrow, and tiny terminal widths.
- [ ] Add tests proving ACP model recovery uses ACP-specific messaging.

## P6.5: TUI Slash Commands

- [ ] Add `/doctor`.
- [ ] Add `/auth status`.
- [ ] Add `/config path`.
- [ ] Add `/config show`.
- [ ] Add `/setup`.
- [ ] Add `/login <provider>`.
- [ ] Add `/logout <provider>`.
- [ ] Make `/setup` open the focused setup surface.
- [ ] Make `/login <provider>` open hidden credential entry.
- [ ] Make `/logout <provider>` require confirmation.
- [ ] Reject slash commands that include API-key-looking extra arguments.
- [ ] Do not add `/config edit`.
- [ ] If a user enters `/config edit`, show the CLI command to run outside the
      TUI.
- [ ] Preserve prompt draft after every slash-command success, cancellation, or
      failure.
- [ ] Add command suggestion entries for supported slash commands.
- [ ] Add app update tests for each slash command.
- [ ] Add renderer snapshots for slash-command output and focused surfaces.
- [ ] Add tests proving secrets never appear in slash-command transcript
      entries.

## P7: Docs And README

- [ ] Update README quickstart with `cargo install --locked thndrs`,
      `thndrs setup`, and `thndrs`.
- [ ] Replace README setup/installation/usage TODOs relevant to this feature.
- [ ] Update installation docs with setup-first flow.
- [ ] Update CLI reference with setup/login/logout/auth/doctor/config commands.
- [ ] Update TUI usage docs with safe setup/auth/doctor/config slash commands.
- [ ] Update environment-variable docs with credential store precedence.
- [ ] Update configuration docs to explain why secrets stay out of TOML.
- [ ] Add troubleshooting section for missing API keys and invalid credentials.
- [ ] Mention `thndrs doctor --json` for bug reports.

## P8: Release Gate Review

- [ ] Run setup in a clean temp HOME and temp workspace.
- [ ] Run setup in a git repo and confirm `.git/info/exclude` is updated.
- [ ] Run setup with non-interactive stdin and confirm it fails usefully.
- [ ] Run login/logout for Umans against temp credential files.
- [ ] Run login/logout for OpenCode Go against temp credential files.
- [ ] Run doctor with no credentials and confirm exit code `1`.
- [ ] Run doctor with fake stored credentials and confirm output is redacted.
- [ ] Launch TUI with missing credentials and confirm recovery appears before
      prompt submission.
- [ ] Review whether provider validation is cheap enough for setup/login.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [ ] `cargo test auth`
- [ ] `cargo test config`
- [ ] `cargo test cli`
- [ ] `cargo test app`
- [ ] `cargo test renderer`
- [ ] `cargo test providers`
- [ ] `cargo test`

## Review Checkpoints

- [ ] After P1, review credential file format and precedence before wiring
      providers.
- [ ] After P3, manually review hidden input behavior on supported terminals.
- [ ] After P4, review doctor output for issue-paste safety.
- [ ] After P6, review TUI first-run flow before crates.io release.
