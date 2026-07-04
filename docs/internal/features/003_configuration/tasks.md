# Configuration Tasks

Status: Draft
Captured: 2026-07-03

## P0: Define The Contract

- [x] Decide the supported config file paths.
- [x] Document supported config layers: defaults, global config, project config,
      environment variables, and CLI flags.
- [x] Define the final key list and value types for `model`, `websearch`,
      `tick_rate_ms`, `theme`, `mouse`, `verbose`, `skill_dirs`,
      `session_dir`, and `default_workspace`.
- [x] Decide that `print_prompt`, `cwd`, `no_alt_screen`, and `no_mouse` are
      CLI-only and are not TOML/env keys.
- [x] Decide that LSP is not configurable in this feature; `lsp_enabled` is an
      unknown key.
- [x] Define `THNDRS_` environment variable names for every non-secret config
      key.
- [x] Define boolean and list env parsing rules.
- [x] Define path-resolution rules for file-relative config values.
- [x] Define what effective config metadata can be persisted in sessions and
      exported without leaking secrets.
- [x] Decide that search mode is configured only through `websearch`; `fd` and
      `rg` fallback behavior remains tool/session metadata, not config.
- [x] Decide that provider secrets stay provider-owned and are not ordinary
      `THNDRS_` config values.
- [x] Decide that unsupported old/typo config paths are ignored before the first
      stable release instead of retained as aliases.

## P1: Implement Effective Config

- [ ] Introduce config source/layer metadata that records where each loaded
      value came from.
- [ ] Keep `Config` as the TOML schema with `deny_unknown_fields`.
- [ ] Add an env-config loader for `THNDRS_` overrides.
- [ ] Reject unknown `THNDRS_` variables with a clear error.
- [ ] Parse boolean env values case-insensitively for `1`, `0`, `true`,
      `false`, `yes`, `no`, `on`, and `off`.
- [ ] Parse path-list env values using platform path separators.
- [ ] Merge precedence as CLI flags over env vars over project config over
      global config over defaults.
- [ ] Track per-key origins with redacted labels such as `default`,
      `~/.thndrs/config.toml`, `.thndrs/config.toml`, `THNDRS_MODEL`, and
      `--model`.
- [ ] Record loaded config file metadata with source, display path, and
      lowercase SHA-256 hash of file bytes.
- [ ] Change CLI boolean parsing so omitted flags and explicit flags are
      distinguishable during merge.
- [ ] Implement `--mouse` / `--no-mouse` as conflicting CLI flags that both map
      to the single `mouse` effective key.
- [ ] Resolve `skill_dirs`, `session_dir`, and `default_workspace` relative to
      the config file that declared them.
- [ ] Deduplicate merged skill directories after path resolution.
- [ ] Load only `~/.thndrs/config.toml` for global config.
- [ ] Load only `.thndrs/config.toml` for project config.
- [ ] Reject secret-shaped TOML keys with a clear error.
- [ ] Keep provider secret lookup in provider code through `UMANS_API_KEY`,
      `OPENCODE_GO_KEY`, and workspace `.env`.
- [ ] Ensure effective config, diagnostics, logs, prompt inspection, sessions,
      and exports never include raw provider secret values or raw `.env`
      contents.

## P2: Wire Runtime Behavior

- [ ] Make `default_workspace` apply only when `--cwd` is omitted.
- [ ] Add `session_dir` support to session writer creation.
- [ ] Preserve current `.thndrs/sessions` behavior as the default.
- [ ] Keep `cwd` CLI-only and document why it is not a config key.
- [ ] Surface config diagnostics in prompt inspection.
- [ ] Surface config diagnostics in verbose startup rows.
- [ ] Include effective provider, model, web-search mode, workspace, session
      directory, loaded config files, and key origins in inspect/export metadata
      as part of the initial inspect/export implementation.
- [ ] Persist effective config metadata in the initial `session_meta` record
      under a `config` object.
- [ ] Include loaded `AGENTS.md` path, scope, hash, and truncation metadata in
      session/inspect-export metadata alongside effective config metadata.
- [ ] Include activated skill and loaded skill reference metadata in
      session/inspect-export metadata without exposing skill file contents
      unnecessarily.
- [ ] Preserve provider/model and web-search metadata needed for future
      renderer-independent session export.
- [ ] Keep full non-TUI inspect/export command implementation as session work
      unless it is intentionally implemented with this feature.

## P3: Tests

- [ ] Add tests for the canonical global config path.
- [ ] Add tests for the canonical project config path.
- [ ] Add tests proving unsupported old/typo config paths are ignored.
- [ ] Add tests that project config overrides global config.
- [ ] Add tests that environment variables override config files.
- [ ] Add tests that CLI flags override environment variables.
- [ ] Add tests proving `print_prompt`, `cwd`, `no_alt_screen`, and `no_mouse`
      are rejected as TOML/env keys.
- [ ] Add tests proving unknown `THNDRS_` environment variables are errors.
- [ ] Add tests proving `--mouse` and `--no-mouse` conflict in one CLI
      invocation.
- [ ] Add tests for invalid env values and parse diagnostics.
- [ ] Add tests for boolean env values: `1`, `0`, `true`, `false`, `yes`,
      `no`, `on`, and `off`.
- [ ] Add tests for path-list env parsing using platform path separators.
- [ ] Add tests for file-relative `skill_dirs`, `session_dir`, and
      `default_workspace`.
- [ ] Add tests proving secrets are not serialized into config diagnostics,
      logs, errors, prompt inspection, session metadata, snapshots, or export
      metadata.
- [ ] Add tests proving secret-shaped TOML keys are rejected.
- [ ] Add tests proving `lsp_enabled` and `THNDRS_LSP_ENABLED` are rejected as
      unknown keys.
- [ ] Add session tests for custom `session_dir`.
- [ ] Add session metadata tests for loaded config files, key origins,
      effective model, web-search mode, workspace, and session directory.
- [ ] Add tests proving config path display uses workspace-relative,
      `~`-relative, or absolute paths according to the session/export
      convention.
- [ ] Add prompt-inspection snapshots for config metadata.
- [ ] Add effective-config snapshot tests.
- [ ] Add tests proving repository search fallback diagnostics are not exposed
      as config keys.

## P4: Docs

- [ ] Update public configuration docs with precedence, paths, keys, defaults,
      and examples.
- [ ] Update environment-variable docs with all `THNDRS_` overrides and provider
      secret variables.
- [ ] Add a sample TOML config that excludes secrets.
- [ ] Update README configuration section from placeholder text.
- [ ] Document the two supported config paths.
- [ ] Document all supported `THNDRS_` config environment variables.
- [ ] Document provider secret variables separately from ordinary config:
      `UMANS_API_KEY` and `OPENCODE_GO_KEY`.
- [ ] Document `session_dir`, the default `.thndrs/sessions` behavior, and what
      metadata is safe to persist.
- [ ] Document `default_workspace` and why `--cwd` remains CLI-only.
- [ ] Document search mode selection through `websearch`.
- [ ] Document common diagnostics for malformed TOML, unknown keys, invalid env
      values, unknown `THNDRS_` variables, and rejected secret-shaped keys.
- [ ] Document how config/session changes are handled before the first stable
      release.
- [ ] Remove public docs that advertise unsupported old/typo config paths.
- [ ] Keep release notes/changelog updates outside this feature until behavior
      changes are implemented.

## P5: Explicit Non-Goals To Preserve

- [ ] Do not add `lsp_enabled` or any other LSP config key in this feature.
- [ ] Do not add config keys for `fd`, `rg`, LSP server paths, provider-private
      state, permission rules, plugins, commands, workflow definitions, or
      secret values.
- [ ] Do not fold provider stream normalization, tool registry refactors,
      runtime/run controller work, renderer row-model work, or input command
      refactors into the configuration implementation.
- [ ] Do not implement session search, skill marketplace/install/publishing, or
      subagent orchestration as part of configuration.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy`
- [ ] `cargo test config`
- [ ] `cargo test cli`
- [ ] `cargo test session`
- [ ] `cargo test`
