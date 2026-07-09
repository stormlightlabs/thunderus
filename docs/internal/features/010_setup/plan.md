---
title: "Setup, Doctor, And Reasoning Readiness"
status: Draft
captured: 2026-07-08
---

## Objective

Make `setup` and `doctor` the readiness surface for credentials, model
selection, config provenance, provider metadata, and reasoning-level support.

Reasoning is absorbed here because it is a provider/model readiness concern, not
a separate product area.

## Grounding

Reviewed:

- provider notebooks under `docs/src/content/docs/notebook/providers/`;
- `docs/src/content/docs/notebook/acp.md`;
- setup/auth/doctor/config code under `src/cli/commands/`,
  `src/core/auth.rs`, `src/core/diagnostics.rs`, and `src/core/config/mod.rs`;
- provider boundaries under `src/core/providers/`;
- ACP config option code under `src/server/`.

Provider notebook conclusions:

- The notebooks ground the validation strategy, not a universal provider
  reasoning-label set.
- Umans exposes model reasoning metadata through
  `capabilities.reasoning { supported, can_disable, levels, default_level }`.
- OpenCode Go/Zen model catalogs are dynamic; derive support from metadata when
  available and stay conservative otherwise.
- ChatGPT Codex is experimental; explicit reasoning needs a recorded smoke test
  before it is enabled.
- ACP is the right session-config surface for reasoning, but ACP does not prove
  provider wire support.

## Current State

Already present:

- `setup`, `login`, `logout`, `auth status`, `doctor`, and `config`;
- credential stores outside TOML;
- redacted doctor output;
- public setup/auth/doctor/config docs;
- safe TUI slash-command shortcuts.

Missing:

- a settled `reasoning` setting in CLI/config/env/session state;
- provider-local reasoning validation and request lowering;
- doctor readiness output for reasoning;
- setup affordances for metadata-supported reasoning choices;
- ACP/TUI mutation with validation and config persistence.

## Decisions

- Folder name is `010_setup`.
- `011_reasoning_levels` is retired and absorbed here.
- `auto` is the default everywhere.
- Normalized thndrs values are:
  `auto`, `none`, `minimal`, `low`, `medium`, `high`, `xhigh`.
- Those labels are a thndrs user contract, not proof of provider support.
- Setup may offer explicit values only when provider/model metadata or an
  explicit provider mapping supports them.
- Doctor may make cheap provider metadata requests when credentials exist.
- Known-bad explicit values fail locally before a provider request.
- Unknown support may reach the provider only when local validation cannot prove
  support.
- Reasoning is mutable per session.
- Successful reasoning changes persist to the config layer that currently owns
  `reasoning`.
- Explicit reasoning changes require validation before persistence.
- `auto` plus missing metadata is a warning.
- explicit reasoning plus missing/failed metadata is blocking readiness.

## Public Contract

### Setup

Existing commands remain:

```text
thndrs setup
thndrs setup --provider <provider>
thndrs setup --global
thndrs setup --project
```

Setup continues to detect workspace/config/provider/credentials/local tools,
write credentials only to deliberate credential stores, and keep secrets out of
TOML, logs, sessions, snapshots, prompt inspection, and command output.

Reasoning behavior:

- default to `auto`;
- never require a reasoning choice for first-run success;
- offer `low`, `medium`, and `high` only when exact metadata or an explicit
  provider mapping supports them;
- offer `none`, `minimal`, or `xhigh` only when metadata explicitly supports
  them;
- hide provider-reported labels outside thndrs' normalized set until mapped
  deliberately;
- keep `auto` and point to `doctor` when metadata is unavailable.

### Config, CLI, Environment

Add:

```toml
reasoning = "auto"
```

```text
thndrs --reasoning <auto|none|minimal|low|medium|high|xhigh>
THNDRS_REASONING=auto
```

Rules:

- precedence matches `model` and `websearch`;
- invalid values fail at parse boundaries;
- `reasoning` appears in redacted config output and origin tracking;
- model ids must not encode reasoning levels.

### Session Mutation

Reasoning is mutable like model and web search.

Mutation rules:

- validate before state or config changes;
- reject known-bad explicit values;
- reject explicit values when metadata cannot validate them;
- allow `auto` without metadata;
- persist successful changes to the owning config layer;
- if no layer owns `reasoning`, ask for scope or route through setup/config
  edit instead of guessing.

### Doctor

Existing commands remain:

```text
thndrs doctor
thndrs doctor --json
```

Doctor adds reasoning readiness:

- effective value and origin;
- selected provider/model;
- metadata source;
- validation confidence: `verified`, `known unsupported`, `unknown`, or
  `metadata unavailable`;
- whether an explicit value would be rejected before a request;
- setup hints when credentials or metadata are missing.

Severity:

- `auto` + missing metadata: warning.
- explicit + supported metadata: pass.
- explicit + unsupported metadata: blocking.
- explicit + metadata unavailable: blocking.
- invalid config/env/CLI: configuration error.

### Provider Contract

Add a provider-neutral `ReasoningLevel` plus typed validation results.

Provider rules:

- `auto` sends no reasoning override;
- provider modules own validation and wire serialization;
- unsupported explicit values produce local validation errors when known;
- provider-reported labels outside thndrs' normalized set require deliberate
  adapter mapping.

Provider-specific first pass:

- Umans validates against live `capabilities.reasoning`.
- OpenCode Go derives support from model metadata when available.
- OpenCode Zen rejects explicit values until a documented route supports them.
- ChatGPT Codex rejects explicit values until a smoke test records support.

### ACP And TUI

ACP exposes reasoning as a `ModelConfig` session option and validates updates
before mutating state or config.

The TUI should expose the same setting without becoming a settings dashboard:

- show selected reasoning in verbose diagnostics;
- add a minimal mutation path;
- represent setup, doctor, and reasoning-readiness UI state semantically before
  rendering;
- route bounded focused surfaces through the iocraft adapter only under
  `../012_iocraft/plan.md`;
- keep `/doctor` redacted, paste-safe, and non-interactive unless a focused
  detail surface is explicitly added;
- never accept API keys or secret material as slash-command arguments;
- preserve permission/setup recovery priority and prompt drafts.

## Rust Constraints

- Model reasoning level, validation confidence, readiness severity, metadata
  source, and config write target as enums or small structs.
- Keep pure parsing, precedence, validation, and severity projection free of
  filesystem/network side effects.
- Keep HTTP, filesystem, environment, terminal input, and config writes at
  command/provider/storage boundaries.
- Use typed `Result` errors for recoverable setup/config/validation/provider
  metadata failures.
- Prefer concrete helpers; add traits only for real testable boundaries.
- Avoid new dependencies unless existing crates or `std` cannot handle the
  requirement cleanly.

## Verification

Code changes must run:

```text
cargo fmt
cargo clippy --fix --allow-dirty --allow-staged
cargo clippy
cargo test
```

Public docs changes must run:

```text
pnpm --dir docs build
```

Required coverage is defined in `tasks.md`: parsing, precedence, owning-layer
persistence, provider validation, doctor severity, setup metadata behavior, ACP
updates, TUI mutation, and secret-redaction checks.

## Boundaries

Always:

- preserve current behavior when `reasoning` is absent;
- keep provider-specific wire details inside provider modules;
- label validation confidence honestly;
- keep secrets out of TOML, logs, sessions, snapshots, prompt inspection, and
  command output.

Ask first:

- adding dependencies;
- introducing OS keychain/keyring integration;
- changing provider env var names;
- enabling explicit ChatGPT Codex reasoning;
- choosing a config write target when no layer owns `reasoning`.

Never:

- silently downgrade unsupported explicit reasoning to `auto`;
- encode reasoning in model ids;
- send Responses-only reasoning fields to chat-completions routes without
  support;
- expose raw hidden chain-of-thought in UI.

## Deferred

- Rich TUI reasoning picker.
- OS keychain integration.
- OAuth/device-code setup flows.
- More granular provider budgets after concrete provider contracts exist.
