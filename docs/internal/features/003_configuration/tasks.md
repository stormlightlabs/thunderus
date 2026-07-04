# Configuration Tasks

Status: Draft
Captured: 2026-07-03

## P2: Wire Runtime Behavior

- [x] Make `default_workspace` apply only when `--cwd` is omitted.
- [x] Add `session_dir` support to session writer creation.
- [x] Preserve current `.thndrs/sessions` behavior as the default.
- [x] Keep `cwd` CLI-only and document why it is not a config key.
- [x] Surface config diagnostics in prompt inspection.
- [x] Surface config diagnostics in verbose startup rows.
- [x] Include effective provider, model, web-search mode, workspace, session
      directory, loaded config files, and key origins in inspect/export metadata
      as part of the initial inspect/export implementation.
- [x] Persist effective config metadata in the initial `session_meta` record
      under a `config` object.
- [x] Include loaded `AGENTS.md` path, scope, hash, and truncation metadata in
      session/inspect-export metadata alongside effective config metadata.
- [x] Include activated skill and loaded skill reference metadata in
      session/inspect-export metadata without exposing skill file contents
      unnecessarily.
- [x] Preserve provider/model and web-search metadata needed for future
      renderer-independent session export.
- [x] Keep full non-TUI inspect/export command implementation as session work
      unless it is intentionally implemented with this feature.

## P3: Tests

- [x] Add tests for the canonical global config path.
- [x] Add tests for the canonical project config path.
- [x] Add tests proving unsupported old/typo config paths are ignored.
- [x] Add tests that project config overrides global config.
- [x] Add tests that environment variables override config files.
- [x] Add tests that CLI flags override environment variables.
- [x] Add tests proving `print_prompt`, `cwd`, `no_alt_screen`, and `no_mouse`
      are rejected as TOML/env keys.
- [x] Add tests proving unknown `THNDRS_` environment variables are errors.
- [x] Add tests proving `--mouse` and `--no-mouse` conflict in one CLI
      invocation.
- [x] Add tests for invalid env values and parse diagnostics.
- [x] Add tests for boolean env values: `1`, `0`, `true`, `false`, `yes`,
      `no`, `on`, and `off`.
- [x] Add tests for path-list env parsing using platform path separators.
- [x] Add tests for file-relative `skill_dirs`, `session_dir`, and
      `default_workspace`.
- [x] Add tests proving secrets are not serialized into config diagnostics,
      logs, errors, prompt inspection, session metadata, snapshots, or export
      metadata.
- [x] Add tests proving secret-shaped TOML keys are rejected.
- [x] Add tests proving `lsp_enabled` and `THNDRS_LSP_ENABLED` are rejected as
      unknown keys.
- [x] Add session tests for custom `session_dir`.
- [x] Add session metadata tests for loaded config files, key origins,
      effective model, web-search mode, workspace, and session directory.
- [x] Add tests proving config path display uses workspace-relative,
      `~`-relative, or absolute paths according to the session/export
      convention.
- [x] Add prompt-inspection snapshots for config metadata.
- [x] Add effective-config snapshot tests.
- [x] Add tests proving repository search fallback diagnostics are not exposed
      as config keys.

## P4: Docs

- [x] Update public configuration docs with precedence, paths, keys, defaults,
      and examples.
- [x] Update environment-variable docs with all `THNDRS_` overrides and provider
      secret variables.
- [x] Add a sample TOML config that excludes secrets.
- [x] Update README configuration section from placeholder text.
- [x] Document the two supported config paths.
- [x] Document all supported `THNDRS_` config environment variables.
- [x] Document provider secret variables separately from ordinary config:
      `UMANS_API_KEY` and `OPENCODE_GO_KEY`.
- [x] Document `session_dir`, the default `.thndrs/sessions` behavior, and what
      metadata is safe to persist.
- [x] Document `default_workspace` and why `--cwd` remains CLI-only.
- [x] Document search mode selection through `websearch`.
- [x] Document common diagnostics for malformed TOML, unknown keys, invalid env
      values, unknown `THNDRS_` variables, and rejected secret-shaped keys.
- [x] Document how config/session changes are handled before the first stable
      release.
- [x] Remove public docs that advertise unsupported old/typo config paths.
- [x] Keep release notes/changelog updates outside this feature until behavior
      changes are implemented.

## P5: Boundary Decisions

- [x] LSP configuration is owned by the LSP/code-intelligence feature, not by
      this configuration milestone.
- [x] Repository search implementation details (`fd`, `rg`, fallbacks) are
      tool diagnostics, not config keys.
- [x] Provider-private state and provider secrets remain provider-owned.
- [x] Permission rules, plugins, commands, workflow definitions, and secret
      values are outside the ordinary TOML config schema.
- [x] Provider stream normalization, tool registry refactors, runtime/run
      controller work, renderer row-model work, and input command refactors have
      separate feature plans.
- [x] Session search, skill marketplace/install/publishing, and subagent
      orchestration are separate product features.

## Validation Commands

- [x] `cargo fmt`
- [x] `cargo clippy --fix --allow-dirty --allow-staged`
- [x] `cargo clippy`
- [x] `cargo test config`
- [x] `cargo test cli`
- [x] `cargo test session`
- [x] `cargo test`
