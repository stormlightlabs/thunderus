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

- [x] Add `src/core/auth.rs` with module docs.
- [x] Define supported API-key providers and their env var names.
- [x] Add global credential path `~/.thndrs/credentials.env`.
- [x] Add project credential path `.thndrs/credentials.env`.
- [x] Parse simple env assignment lines compatible with current `.env` support.
- [x] Write credential files atomically.
- [x] Set Unix file mode `0600` where supported.
- [x] Preserve unrelated credential entries when adding or removing one key.
- [x] Redact credential values in all display/debug paths.
- [x] Add `.thndrs/credentials.env` to `.git/info/exclude` when a git repo is
      present.
- [x] Tolerate missing home directory with a clear error.
- [x] Test read/write round trips.
- [x] Test removing one credential preserves others.
- [x] Test malformed credential files fail clearly without printing values.
- [x] Test Unix mode behavior where supported.
- [x] Test git exclude update is idempotent.

## P2: Provider Credential Resolution

- [x] Extend `auth::resolve_credential` to check process env, global store,
      project store, and workspace `.env` in precedence order.
- [x] Add `CredentialSource` enum with `Environment`, `GlobalStore`,
      `ProjectStore`, and `DotEnvLegacy` variants.
- [x] Add `credential_source()` returning source without the credential value.
- [x] Add `CredentialSource::label()` for human-readable source descriptions.
- [x] Add `umans::validate_api_key()` using lightweight `GET /v1/models/info`.
- [x] Add `opencode::validate_api_key()` using lightweight `GET /models`.
- [x] Keep `from_env_or_dotenv` unchanged for now (wired in P3).
- [x] Test source precedence: env > global store > project store > .env > none.
- [x] Test `resolve_credential` returns `None` for unknown keys.
- [x] Test empty env var is treated as missing.
- [x] Test credential source label and Debug do not leak values.
- [x] Test `KNOWN_API_KEY_VARS` is complete.
- [x] Wire `resolve_credential` into provider `from_env_or_dotenv`.
- [x] Add missing-key error with setup hint.
- [x] Test provider validation does not persist provider payloads (requires
      network mocking).

## P3: CLI Setup And Auth Commands

- [x] Add top-level `setup` command.
- [x] Add `setup --provider <umans|opencode-go>`.
- [x] Add `setup --global`.
- [x] Add `setup --project`.
- [x] Add `login <provider>`.
- [x] Add `logout <provider>`.
- [x] Add `auth status`.
- [x] Implement real hidden API-key input.
- [x] Detect non-interactive stdin and fail with actionable instructions unless
      sufficient flags are present.
- [x] Ask before writing global config, project config, or credential files.
- [x] Make setup avoid duplicating existing config keys.
- [x] Print the next command after successful setup.
- [x] Add parser tests for every new command and flag.
- [x] Add command-output tests for setup dry paths.
- [x] Add login/logout command tests using temp HOME/workspace.

## P4: Doctor Command

- [x] Add top-level `doctor`.
- [x] Add `doctor --json`.
- [x] Report app version.
- [x] Report resolved workspace.
- [x] Report selected model/provider.
- [x] Report loaded config files and redacted diagnostics.
- [x] Report credential status by provider source, never value.
- [x] Report `rg` availability.
- [x] Report `fd` availability.
- [x] Report session directory writability.
- [x] Report MCP configured/ready/skipped/failed counts.
- [x] Report ACP configured/enabled/disabled counts.
- [x] Report terminal capability summary if available.
- [x] Return exit code `0` for no blocking issues.
- [x] Return exit code `1` for blocking setup issues.
- [x] Return exit code `2` for invalid config or CLI usage.
- [x] Add human-output snapshot tests.
- [x] Add JSON fixture tests.
- [x] Add tests proving secrets are absent from doctor output.

## P5: Config Commands

- [x] Add `config path`.
- [x] Add `config show --redacted`.
- [x] Add `config edit --global`.
- [x] Add `config edit --project`.
- [x] Print global and project config paths with display policy.
- [x] Show effective config, origins, loaded files, and diagnostics redacted.
- [x] Respect `$EDITOR` for `config edit`.
- [x] If `$EDITOR` is missing, print the path and exit clearly.
- [x] Ask before creating config parent directories.
- [x] Add parser tests.
- [x] Add redacted output tests.
- [x] Add no-editor behavior tests.

## P6: TUI First-Run Recovery

- [x] Add app state for first-run/missing-provider-credential recovery.
- [x] Detect missing selected-provider credential before first provider-backed
      prompt.
- [x] Render focused recovery surface with provider, model, missing env var,
      and actions.
- [x] Add action to enter API key.
- [x] Add action to switch model/provider through existing picker affordance or
      a simple provider list.
- [x] Add action to show setup instructions.
- [x] Add action to continue without setup only when it will not immediately
      submit a provider-backed prompt.
- [x] Add action to quit.
- [x] Store entered keys through the same credential-store path as `login`.
- [x] Confirm global vs project storage before writing.
- [x] Keep prompt draft intact after recovery.
- [x] Add app update tests for each action.
- [x] Add renderer snapshots for normal, narrow, and tiny terminal widths.
- [x] Add tests proving ACP model recovery uses ACP-specific messaging.

## P6.5: TUI Slash Commands

- [x] Add `/doctor`.
- [x] Add `/auth status`.
- [x] Add `/config path`.
- [x] Add `/config show`.
- [x] Add `/setup`.
- [x] Add `/login <provider>`.
- [x] Add `/logout <provider>`.
- [x] Make `/setup` open the focused setup surface.
- [x] Make `/login <provider>` open hidden credential entry.
- [x] Make `/logout <provider>` require confirmation.
- [x] Reject slash commands that include API-key-looking extra arguments.
- [x] Do not add `/config edit`.
- [x] If a user enters `/config edit`, show the CLI command to run outside the
      TUI.
- [x] Preserve prompt draft after every slash-command success, cancellation, or
      failure.
- [x] Add command suggestion entries for supported slash commands.
- [x] Add app update tests for each slash command.
- [x] Add renderer snapshots for slash-command output and focused surfaces.
- [x] Add tests proving secrets never appear in slash-command transcript
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

- [x] `cargo fmt`
- [x] `cargo clippy --fix --allow-dirty --allow-staged`
- [x] `cargo clippy`
- [x] `cargo test auth`
- [x] `cargo test config`
- [x] `cargo test cli`
- [x] `cargo test app`
- [x] `cargo test renderer`
- [x] `cargo test providers`
- [x] `cargo test`

## Review Checkpoints

- [ ] After P1, review credential file format and precedence before wiring
      providers.
- [ ] After P3, manually review hidden input behavior on supported terminals.
- [ ] After P4, review doctor output for issue-paste safety.
- [ ] After P6, review TUI first-run flow before crates.io release.
