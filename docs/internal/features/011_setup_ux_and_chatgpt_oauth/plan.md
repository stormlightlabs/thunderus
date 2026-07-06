# Setup UX And ChatGPT OAuth

Status: Draft
Captured: 2026-07-06

## Problem

The first-run setup flow now defaults to OpenCode Zen Big Pickle, but the setup
experience still behaves like the old API-key-only flow. That makes the default
feel abrupt: users see a missing `OPENCODE_ZEN_KEY` and a credential-store
prompt before they have a clear provider choice, default-model explanation, or
escape hatch.

ChatGPT Codex is worse. `thndrs login chatgpt-codex` uses the intended ChatGPT
OAuth path, but `thndrs setup --provider chatgpt-codex` still routes through
the generic API-key setup path because `SetupCommand` is built around
`ApiKeyProviderArg`. It can ask for a hidden "chatgpt-codex API key" even
though ChatGPT Codex is supposed to use device-code OAuth with browser PKCE
fallback.

The TUI recovery surface has the same product mismatch. It says "Missing
credential" and exposes generic setup actions. For ChatGPT Codex it can only
tell the user to leave the TUI and run `thndrs login chatgpt-codex`; it does
not provide an OAuth setup path.

## Milestone Outcome

A new user can run `thndrs setup` and understand the default provider before
being asked for credentials. The flow makes provider selection explicit,
preserves OpenCode Zen as the default, and offers a clean path to switch to
ChatGPT Codex, Umans, or OpenCode Go.

ChatGPT Codex setup uses ChatGPT OAuth everywhere setup is offered. It never
asks the user to paste a ChatGPT access token as the ordinary setup path.
`CHATGPT_CODEX_ACCESS_TOKEN` remains only an ephemeral process override for
automation and debugging.

## Goals

1. Replace API-key-shaped setup with provider-shaped setup.
2. Keep `opencode/big-pickle` as the built-in default model while making the
   OpenCode Zen credential requirement and caveats visible.
3. Make `thndrs setup --provider chatgpt-codex` run the same OAuth login path
   as `thndrs login chatgpt-codex`.
4. Improve the TUI first-run recovery copy and action model for API-key
   providers, ChatGPT OAuth, and ACP agents.
5. Keep implementation small: reuse existing auth storage, provider detection,
   OAuth helpers, model picker entries, and credential redaction.
6. Keep secrets out of TOML, logs, sessions, prompt inspection, snapshots, and
   command output.

## Current State

- `src/cli/commands/setup.rs` defines `ApiKeyProviderArg` and uses it for
  `umans`, `opencode-go`, `opencode-zen`, and `chatgpt-codex`.
- `ApiKeyProviderArg::env_var()` maps ChatGPT Codex to
  `CHATGPT_CODEX_ACCESS_TOKEN`, which should stay an env override, not the main
  setup credential.
- `thndrs setup` chooses `provider_for_model(&cli.model)`. With the current
  default model, fresh setup lands on OpenCode Zen.
- `setup::run()` prints workspace/provider/credential status, asks for
  global/project scope, maybe writes a model key, and then asks for a hidden API
  key when the credential is missing.
- `src/cli/commands/auth.rs` already has `run_chatgpt_codex_login()`, which
  attempts device-code OAuth and falls back to browser PKCE.
- `src/core/auth.rs` already stores ChatGPT Codex credentials in
  `~/.thndrs/auth.json`, supports refresh, and honors
  `CHATGPT_CODEX_ACCESS_TOKEN` before stored credentials.
- `src/cli/app.rs` treats ChatGPT Codex as authenticated through
  `auth::resolve_chatgpt_codex_auth()`, but the recovery surface for ChatGPT
  only supports switching model/provider, showing instructions, skipping, or
  quitting.
- `src/cli/renderer/live.rs` renders generic "Missing credential" setup copy.

## Public Contract

### Provider Setup Model

Setup should distinguish provider auth kinds:

- API-key providers:
  - `umans` stores `UMANS_API_KEY`.
  - `opencode-go` stores `OPENCODE_GO_KEY`.
  - `opencode-zen` stores `OPENCODE_ZEN_KEY`.
- OAuth providers:
  - `chatgpt-codex` stores refreshable ChatGPT Codex auth in
    `~/.thndrs/auth.json`.
  - `CHATGPT_CODEX_ACCESS_TOKEN` is reported as an environment override, but
    setup should not ask users to paste it as the normal credential path.
- ACP models:
  - `acp:<name>` remains agent-owned setup and should not be mixed into
    provider credential setup.

The code can keep the public provider argument names stable, but the internal
type should stop implying that every setup provider is an API-key provider.
Either rename `ApiKeyProviderArg` to a provider-neutral type or add a small
auth-kind layer next to it.

### CLI Setup

Supported commands remain:

```text
thndrs setup
thndrs setup --provider <umans|opencode-go|opencode-zen|chatgpt-codex>
thndrs setup --global
thndrs setup --project
```

Behavior:

- Start by showing a concise setup summary: workspace, selected model, provider,
  credential/auth status, and config scope if forced by flags.
- When no provider is forced, show an explicit provider choice before asking
  for credentials:
  - OpenCode Zen Big Pickle, marked as the default.
  - ChatGPT Codex, marked as OAuth/subscription-backed and experimental.
  - Umans.
  - OpenCode Go.
- For OpenCode Zen, preserve the Big Pickle limited-free and privacy caveat in
  short setup copy before asking for `OPENCODE_ZEN_KEY`.
- For API-key providers, keep hidden input, global/project credential store
  selection, validation, idempotent config writes, and git exclude behavior.
- For ChatGPT Codex, call the OAuth login path. Do not prompt for an API key or
  a global/project credential store.
- For ChatGPT Codex with `CHATGPT_CODEX_ACCESS_TOKEN` already present, report
  that the current process is authenticated by environment and ask whether to
  also create/update the stored OAuth credential.
- If OAuth is started, device-code login remains first. Browser PKCE remains
  the fallback when device code is unavailable.
- Non-interactive setup should fail with a clear message for OAuth providers
  unless a valid environment override is already present.

### TUI Recovery

The recovery surface should be provider-aware:

- API-key providers show:
  - provider label;
  - model id;
  - missing env var;
  - actions: enter API key, switch model/provider, show setup instructions,
    continue without setup when allowed, quit.
- ChatGPT Codex shows:
  - provider label;
  - model id;
  - auth status: missing ChatGPT OAuth credential;
  - actions: start ChatGPT OAuth login, switch model/provider, show setup
    instructions, continue without setup when allowed, quit.
- ACP missing setup keeps its separate ACP wording and actions.

The smallest acceptable TUI OAuth implementation is an instruction-driven
device-code flow inside the focused recovery surface:

1. Request a device code.
2. Render the verification URL and user code.
3. Poll on ticks while preserving prompt draft and cancellation.
4. Store the returned credentials in `~/.thndrs/auth.json`.
5. Return to the prompt when authenticated.

If that async polling work is too large for the first implementation pass,
the CLI fix must still land first, and the TUI must at least stop presenting
ChatGPT as an API-key credential.

### Docs And QA

Docs should align with behavior:

- CLI reference says `setup --provider chatgpt-codex` uses ChatGPT OAuth.
- ChatGPT provider docs say setup and login share the same OAuth path.
- Environment-variable docs keep `CHATGPT_CODEX_ACCESS_TOKEN` as an override,
  not as the main setup mechanism.
- Internal QA includes manual checks for both CLI and TUI setup recovery.

## Implementation Shape

Likely files:

- `src/cli/commands/setup.rs`: provider-neutral setup type or auth-kind layer,
  provider menu, ChatGPT OAuth branch, OpenCode Zen default copy.
- `src/cli/commands/auth.rs`: expose the ChatGPT OAuth login helper for reuse
  from setup, preserving redaction and auth-store behavior.
- `src/core/auth.rs`: add test seams only if needed for deterministic OAuth
  setup tests; avoid changing token file format unless required.
- `src/cli/app.rs`: add ChatGPT OAuth recovery stage and actions.
- `src/cli/renderer/live.rs`: render provider-aware setup/recovery copy.
- `src/cli/app/tests.rs`: update recovery behavior and slash-command tests.
- `src/cli/renderer/live.rs` tests/snapshots: update setup surface snapshots.
- `docs/src/content/docs/reference/cli.md`: document ChatGPT setup behavior.
- `docs/src/content/docs/reference/environment-variables.md`: clarify token
  override semantics.
- `docs/src/content/docs/providers/chatgpt.md`: document shared setup/login
  OAuth behavior.
- `docs/internal/qa.md`: add release checks for the revised setup paths.

## Boundaries

Always:

- Keep public provider names stable.
- Keep `opencode/big-pickle` as the default model unless a separate product
  decision changes it.
- Reuse existing ChatGPT OAuth storage and refresh behavior.
- Redact access tokens, refresh tokens, API keys, device tokens, and auth
  payloads in logs/snapshots.
- Preserve prompt drafts across setup recovery success, cancellation, and
  failure.
- Keep `CHATGPT_CODEX_ACCESS_TOKEN` as a process-local override.

Ask first:

- Changing the default model away from `opencode/big-pickle`.
- Adding a dependency for async OAuth polling, browser opening, terminal UI
  widgets, or keychain storage.
- Changing the ChatGPT auth JSON schema.
- Opening a browser automatically from setup.

Never:

- Ask for a ChatGPT Codex "API key" in normal setup.
- Store ChatGPT OAuth credentials in `.thndrs/credentials.env` or TOML.
- Store API-key provider secrets in TOML.
- Print tokens, keys, refresh tokens, auth JSON, or credential prefixes.
- Use `OPENAI_API_KEY` as a fallback for ChatGPT Codex.

## Verification

- `cargo fmt`
- `cargo clippy --fix --all-targets --allow-dirty`
- `cargo clippy --all-targets`
- `cargo test auth`
- `cargo test cli`
- `cargo test app`
- `cargo test renderer`
- `cargo test providers`
- `cargo test`
- Manual smoke:
  - clean temp home: `thndrs setup` and confirm OpenCode Zen default is clear;
  - clean temp home: `thndrs setup --provider chatgpt-codex` and confirm OAuth
    starts instead of API-key entry;
  - non-interactive stdin:
    `thndrs setup --provider chatgpt-codex` fails with OAuth-specific
    instructions unless an env override is present;
  - TUI with missing OpenCode Zen credential shows API-key recovery;
  - TUI with missing ChatGPT Codex auth shows OAuth recovery or explicit OAuth
    instructions, not API-key entry.

## Risks And Open Questions

- TUI device-code polling needs a clean tick/cancellation model. If it makes
  the first pass too large, ship the CLI setup fix first and keep TUI OAuth as
  the next task.
- Device-code endpoint behavior may drift. Keep live OAuth checks ignored by
  default and document real-account prerequisites.
- Browser PKCE fallback binds `localhost:1455`; failures should stay
  user-readable and should not leave partial auth files.
- OpenCode Zen copy must be direct but not alarmist: the setup path should
  explain default, required key, limited-free caveat, and privacy caveat without
  becoming a legal notice.
