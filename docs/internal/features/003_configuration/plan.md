# Configuration Plan

Status: Draft
Owner: thndrs maintainers
Captured: 2026-07-03

## Background

`thndrs` already has a small TOML configuration implementation:

- `src/config.rs` loads optional global and project TOML files.
- `src/cli.rs` merges CLI flags over TOML over built-in defaults.
- `docs/src/content/docs/guides/reference/configuration.md` documents current
  config paths and user-facing keys.
- Provider secrets are read from environment variables or a workspace `.env`
  file by provider code, not from TOML config.
- Sessions are stored under `.thndrs/sessions/` and currently do not have a
  configurable root.
- Skills can be discovered from built-in user/project locations plus configured
  `skill_dirs`.

The current implementation is useful but not yet the full public contract.
Environment overrides, session path configuration, default workspace behavior,
inspect/export metadata, and config/env documentation are part of this feature.

## Reference Review

The reference projects point to a few patterns worth keeping:

- The Rust CLI Book's config-file guidance keeps the user-facing contract small:
  defaults, config files, environment, and CLI flags should be predictable and
  tested.
- Codex models configuration as explicit layers, with higher-precedence sources
  overriding lower-precedence sources and enough metadata to explain what won.
- Goose keeps the everyday precedence simple: environment variables override
  config files, which override defaults. Secrets stay outside ordinary config.
- Aider uses generated sample config and dotenv help so docs do not drift from
  supported settings.
- OpenCode treats config as declarations for sources and runtime defaults, not
  as a place to embed a second workflow system.

`thndrs` stays simpler than Codex. Configuration sources are exactly built-in
defaults, global TOML, project TOML, `THNDRS_` environment variables, and CLI
flags. There are no managed enterprise layers, profiles, remote config sources,
or config RPC services in this feature.

## Problem

Configuration has gaps that will become user-visible as the CLI grows:

- Public docs say `thndrs` uses environment variables, but only provider
  secrets are currently read from env or `.env`.
- There is no central effective configuration object that can explain sources,
  defaults, and overrides.
- CLI flag booleans cannot distinguish "not supplied" from "supplied false",
  which makes negative overrides fragile once env variables join the stack.
- Config loading currently accepts extra path spellings that should be removed
  before release.
- `cwd` is CLI-only, while the roadmap calls for default workspace behavior.
- Session path needs a config key before users depend on ad hoc defaults.
- Inspect/export should be able to report the effective model, search mode,
  config sources, and loaded context without leaking secrets.

## Milestone Outcome

At the end of this feature, a user should be able to predict exactly how
`thndrs` chooses its model, web-search mode, workspace, session storage, UI
options, and skill roots. A contributor should be able to add a new config key
by touching a small schema, merge path, docs entry, and focused tests.

The command line remains the highest-precedence interactive override. Config
files remain optional. Secrets remain out of TOML examples.

## Goals

1. Define the durable supported config file paths, key names, value types, and
   defaults.
2. Implement precedence as CLI flags over environment variables over project
   config over global config over defaults.
3. Keep provider secrets separate from ordinary config, with redacted diagnostics
   when they are discovered through env or `.env`.
4. Add a small effective-config projection that can be tested and used by docs,
   prompt inspection, sessions, and inspect/export commands.
5. Add config support for session path and default workspace behavior without
   expanding into profiles or project management.
6. Document config and environment behavior from the same shape the code tests.
7. Keep configuration focused on runtime defaults and discovery roots: no hot
   reload, GUI settings editor, permission rules, plugins, commands, workflow
   definitions, or secret values.

## Public Contract

### Config Files

TOML is the only config file format.

Supported global file:

- `~/.thndrs/config.toml`

Supported project file:

- `.thndrs/config.toml`

No alternate spellings are supported. This project has not shipped a stable
release, so accepting old paths would add permanent complexity without a user
benefit. Existing `.thdrs/*`, `.thndrs.toml`, `.thndrs/.thndrs.toml`, and
`.thndrs/thndrs.toml` files are ignored.

Unknown keys and malformed TOML remain errors.

### Precedence

Effective configuration is resolved in this order:

1. CLI flags.
2. Environment variables.
3. Project config.
4. Global config.
5. Built-in defaults.

Within a layer, there is at most one file. Global and project layers do not
merge multiple files from the same layer.

Collection keys append across layers in precedence order and then deduplicate by
resolved path. For `skill_dirs`, that means global paths first, then project
paths, then environment paths, then CLI paths.

### Keys

Supported TOML config keys:

- `model`: default completion model.
- `websearch`: `auto`, `native`, `exa`, or `none`.
- `tick_rate_ms`: event poll interval.
- `theme`: UI color theme.
- `mouse`: enable focused mouse capture when `true`; disable it when `false`.
- `verbose`: show diagnostic rows.
- `skill_dirs`: additional local skill discovery roots.
- `session_dir`: directory for append-only session JSONL files.
- `default_workspace`: workspace path used when `--cwd` is not passed.

CLI-only flags:

- `--cwd`: one-run workspace override.
- `--print-prompt`: print prompt assembly and exit.
- `--no-alt-screen`: retained CLI no-op while the current parser accepts it.
- `--no-mouse`: one-run override for `mouse = false`.

`print_prompt` is not allowed in TOML or env because a persistent setting that
exits immediately makes normal startup surprising. `no_mouse` is not a TOML/env
key; persistent mouse behavior is configured with `mouse = true` or
`mouse = false`.

`default_workspace` defaults to the current process directory (`.`). After that
path is selected, normal workspace discovery still walks to the git root when a
git repository contains the selected directory.

LSP is not configurable in this feature. `lsp_enabled` is not a supported TOML
or environment key, and `deny_unknown_fields` rejects it like any other unknown
key. `THNDRS_LSP_ENABLED` is an unknown `THNDRS_` environment variable and is
an error.

### Environment Variables

Use a `THNDRS_` prefix for ordinary config overrides:

- `THNDRS_MODEL`
- `THNDRS_WEBSEARCH`
- `THNDRS_TICK_RATE_MS`
- `THNDRS_THEME`
- `THNDRS_MOUSE`
- `THNDRS_VERBOSE`
- `THNDRS_SKILL_DIRS`
- `THNDRS_SESSION_DIR`
- `THNDRS_DEFAULT_WORKSPACE`

Boolean env values should accept `1`, `0`, `true`, `false`, `yes`, `no`, `on`,
and `off`, case-insensitively. List values use platform path separators where
they represent paths.

Invalid env values are errors. They do not fall back to lower-precedence
sources, because silently ignoring an env override makes debugging harder.
Unknown `THNDRS_` environment variables are also errors. This catches misspelled
keys and prevents users from believing an override was applied.

Provider secrets stay provider-specific:

- `UMANS_API_KEY`
- `OPENCODE_GO_KEY`

Secrets may be read from the process environment or workspace `.env`, but they
are never serialized into effective config, logs, sessions, prompt inspection,
or docs examples. TOML keys ending in `_api_key`, `_token`, `secret`, or
`password` are rejected with `secret_in_config`.

## Implementation Shape

### Typed Sources

Implement these concrete types:

```rust
pub struct EffectiveConfig {
    pub runtime: Cli,
    pub layers: Vec<LoadedConfigLayer>,
    pub origins: BTreeMap<ConfigKey, ConfigOrigin>,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

pub struct LoadedConfigLayer {
    pub source: ConfigSource,
    pub config: Config,
    pub path: Option<PathBuf>,
    pub hash: Option<String>,
}

pub enum ConfigSource {
    Default,
    GlobalFile,
    ProjectFile,
    Environment,
    CliFlag,
}

pub enum ConfigKey {
    Model,
    Websearch,
    TickRateMs,
    Theme,
    Mouse,
    Verbose,
    SkillDirs,
    SessionDir,
    DefaultWorkspace,
}

pub struct ConfigOrigin {
    pub source: ConfigSource,
    pub detail: String,
}

pub struct ConfigDiagnostic {
    pub code: ConfigDiagnosticCode,
    pub message: String,
    pub path: Option<PathBuf>,
    pub key: Option<ConfigKey>,
}
```

`hash` is SHA-256 over the loaded file bytes. Use the lower-case hex digest.
`detail` is a redacted label such as `default`, `~/.thndrs/config.toml`,
`.thndrs/config.toml`, `THNDRS_MODEL`, or `--model`.

### CLI Parsing

Keep public `Cli` as normalized runtime state. `CliArgs` must preserve presence
for mergeable flags:

- value flags remain `Option<T>`;
- boolean flags use `Option<bool>` with `ArgAction::SetTrue` and no default;
- `--mouse` sets `mouse = Some(true)`;
- `--no-mouse` sets `mouse = Some(false)`;
- `--mouse` and `--no-mouse` conflict in a single CLI invocation.

Config and env expose only the positive key `mouse`.

### Path Resolution

Resolve relative path config values against the file that declared them, not
against the current process directory after merging. This applies to:

- `skill_dirs`
- `session_dir`
- `default_workspace`

Environment path values resolve against the process current directory after
`default_workspace`/`--cwd` selection. CLI path values resolve the way the CLI
currently resolves paths: relative to the process current directory. The
effective runtime stores absolute normalized paths for `skill_dirs`,
`session_dir`, and `default_workspace`.

`session_dir` default is `<workspace>/.thndrs/sessions`.

### Diagnostics

Config diagnostics should be visible through:

- startup transcript/status rows when `verbose` is enabled;
- prompt inspection;
- inspect/export commands;
- public docs for common errors.

Diagnostics include loaded config file paths, unknown key errors, parse errors,
invalid env values, and rejected secret-shaped TOML keys.

## Inspect And Export Fit

Inspect/export are session features, but they need config metadata. Their
configuration-related contract is:

- include effective provider, model, web-search mode, workspace, and session
  directory;
- include loaded config files and per-key origins;
- include loaded `AGENTS.md` files, scopes, hashes, and truncation state;
- include activated skill and loaded skill reference metadata;
- exclude API keys and raw `.env` contents.

Persist this metadata in the initial `session_meta` record under a `config`
object:

```json
{
  "files": [
    {
      "path": "~/.thndrs/config.toml",
      "source": "global",
      "sha256": "..."
    }
  ],
  "origins": {
    "model": "env:THNDRS_MODEL",
    "websearch": "project:.thndrs/config.toml",
    "session_dir": "default"
  },
  "diagnostics": [
    {
      "code": "invalid_env_value",
      "message": "THNDRS_WEBSEARCH must be one of auto, native, exa, none"
    }
  ]
}
```

Paths in session/export metadata use the existing path display convention:
workspace-relative when inside the workspace, `~`-relative when inside the home
directory, absolute otherwise. This is intentional because config provenance is
part of the audit trail.

## Verification

Required automated checks:

- Config file candidate tests.
- Global/project merge tests.
- CLI over env over project over global over defaults tests.
- Boolean/env parsing tests.
- Path resolution tests for file-relative `skill_dirs`, `session_dir`, and
  `default_workspace`.
- Unknown key and malformed TOML tests.
- Secret redaction tests.
- Prompt inspection snapshots.
- Effective-config snapshots.
- Session metadata tests for effective config fields that are persisted.

Required commands for implementation work:

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test config`
- `cargo test cli`
- `cargo test session`
- `cargo test`
