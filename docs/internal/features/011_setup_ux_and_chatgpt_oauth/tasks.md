# Setup UX And ChatGPT OAuth Tasks

Status: Draft
Captured: 2026-07-06

## P0: Lock The Contract

- [x] Keep `opencode/big-pickle` as the built-in default model.
- [x] Treat setup providers as provider entries with auth kinds, not as
      API-key-only entries.
- [x] Preserve public provider argument values:
      `umans`, `opencode-go`, `opencode-zen`, and `chatgpt-codex`.
- [x] Define ChatGPT Codex setup as OAuth/device-code first with browser PKCE
      fallback.
- [x] Keep `CHATGPT_CODEX_ACCESS_TOKEN` as an environment override only.
- [x] Do not store ChatGPT Codex auth in `.thndrs/credentials.env`.
- [x] Do not ask for a ChatGPT Codex API key in ordinary setup.
- [x] Keep API-key provider behavior hidden, redacted, and idempotent.
- [x] Keep ACP setup separate from provider credential setup.

## P1: Provider Setup Model

- [x] Audit current uses of `ApiKeyProviderArg`.
- [x] Decide whether to rename it to a provider-neutral type or add a separate
      `ProviderAuthKind`/metadata layer.
- [x] Add provider metadata for label, default model, auth kind, environment
      override, and setup summary copy.
- [x] Model API-key providers with their credential env vars.
- [x] Model ChatGPT Codex as OAuth with `~/.thndrs/auth.json` storage and
      `CHATGPT_CODEX_ACCESS_TOKEN` override status.
- [x] Update `provider_for_model()` call sites to use the provider-neutral
      model.
- [x] Keep parser behavior and command help stable.
- [x] Add unit tests for provider metadata and model-to-provider resolution.

## P2: CLI Setup Flow

- [x] Rewrite setup output to start with a concise summary:
      workspace, selected model, provider, and auth status.
- [x] Add an explicit provider selection prompt when `--provider` is omitted
      and stdin is interactive.
- [x] Mark OpenCode Zen Big Pickle as the default provider choice.
- [x] Include short OpenCode Zen setup copy covering required key,
      limited-free caveat, and privacy caveat.
- [x] Preserve `--provider`, `--global`, and `--project` behavior.
- [x] For API-key providers, keep hidden input and credential-store selection.
- [x] For API-key providers, keep validation and "stored but unverified"
      behavior.
- [x] For API-key providers, keep model config writes idempotent.
- [x] For ChatGPT Codex, branch to shared OAuth login instead of API-key input.
- [x] For ChatGPT Codex, skip global/project credential-store selection for
      auth storage.
- [x] For ChatGPT Codex, still allow global/project model config writes when
      the user chooses to write a default model.
- [x] If `CHATGPT_CODEX_ACCESS_TOKEN` is set, report environment auth and ask
      whether to create/update stored OAuth credentials.
- [x] Make non-interactive ChatGPT setup fail with OAuth-specific instructions
      unless auth is already available.
- [x] Add command-output tests for OpenCode Zen default setup copy.
- [x] Add command-output tests proving ChatGPT setup never asks for an API key.
- [x] Add tests for non-interactive ChatGPT setup failure.

## P3: Shared ChatGPT OAuth Helper

- [x] Expose `run_chatgpt_codex_login()` or extract a reusable helper that can
      be called by both `login` and `setup`.
- [x] Keep device-code login first.
- [x] Keep browser PKCE fallback when device-code login is unavailable.
- [x] Keep writing credentials only through `write_chatgpt_codex_credentials`.
- [x] Keep Unix `0600` auth-file behavior.
- [x] Preserve existing refresh-token storage format.
- [x] Add test seams for OAuth request/poll/write behavior without requiring
      real network access.
- [ ] Add tests proving OAuth output does not print access tokens or refresh
      tokens.
- [x] Add tests proving existing unrelated `~/.thndrs/auth.json` entries are
      preserved.

## P4: TUI Recovery Copy And Actions

- [ ] Split recovery rendering copy by auth kind: API key, ChatGPT OAuth, ACP.
- [ ] For API-key providers, keep actions:
      enter API key, switch model/provider, show setup instructions,
      continue without setup when allowed, quit.
- [ ] For ChatGPT Codex, replace API-key-oriented actions with:
      start ChatGPT OAuth login, switch model/provider, show setup instructions,
      continue without setup when allowed, quit.
- [ ] Update ChatGPT missing-auth status text to say OAuth credential, not API
      key.
- [ ] Keep ACP missing setup text separate and unchanged except for polish.
- [ ] Preserve prompt draft across every recovery action.
- [ ] Add app update tests for ChatGPT recovery action ordering.
- [ ] Add renderer snapshots for ChatGPT recovery at normal, narrow, and tiny
      widths.
- [ ] Add regression tests proving ChatGPT recovery cannot enter API-key input.

## P5: TUI OAuth Implementation

- [ ] Add a ChatGPT OAuth recovery stage for device-code request/polling.
- [ ] Request a device code when the user selects "start ChatGPT OAuth login".
- [ ] Render verification URL and user code in the focused recovery surface.
- [ ] Poll on ticks without blocking input rendering.
- [ ] Allow Esc to cancel polling without writing credentials.
- [ ] Store returned credentials in `~/.thndrs/auth.json`.
- [ ] On success, clear recovery and leave the prompt draft intact.
- [ ] On failure, show a redacted error and keep a path back to recovery.
- [ ] If browser PKCE fallback is needed from the TUI, decide whether to show a
      CLI instruction or implement callback handling in-app.
- [ ] Add app tests for request success, pending polling, poll success, poll
      failure, and cancellation.
- [ ] Add tests proving no device token, access token, or refresh token reaches
      transcript entries or snapshots.

## P6: Slash Commands

- [ ] Keep `/setup` opening provider-aware setup recovery.
- [ ] Keep `/login chatgpt-codex` OAuth-oriented, not API-key-oriented.
- [ ] Decide whether `/login chatgpt-codex` starts TUI OAuth directly or shows
      the same ChatGPT OAuth recovery surface.
- [ ] Keep `/logout chatgpt-codex` CLI-only unless auth-json mutation from the
      TUI gets a separate confirmation design.
- [ ] Reject slash-command arguments that look like secrets.
- [ ] Update command suggestions if labels change.
- [ ] Add slash-command tests for ChatGPT setup/login behavior.

## P7: Docs And Internal QA

- [ ] Update `docs/src/content/docs/reference/cli.md` so
      `setup --provider chatgpt-codex` says OAuth, not API-key setup.
- [ ] Update `docs/src/content/docs/providers/chatgpt.md` to say setup and
      login share the ChatGPT OAuth path.
- [ ] Update `docs/src/content/docs/reference/environment-variables.md` to
      clarify that `CHATGPT_CODEX_ACCESS_TOKEN` is an override, not normal
      setup.
- [ ] Update OpenCode Zen docs or setup docs if the default-provider copy needs
      a public explanation.
- [ ] Add QA checks for revised CLI setup behavior.
- [ ] Add QA checks for TUI ChatGPT recovery behavior.
- [ ] Add QA checks that no setup path stores ChatGPT credentials in
      `.thndrs/credentials.env`.

## P8: Verification

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --all-targets --allow-dirty`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test auth`
- [ ] `cargo test cli`
- [ ] `cargo test app`
- [ ] `cargo test renderer`
- [ ] `cargo test providers`
- [ ] `cargo test`

## Manual Smoke

- [ ] Run `thndrs setup` in a clean temp HOME and temp workspace.
- [ ] Confirm OpenCode Zen is presented as the default, not as an unexplained
      missing key.
- [ ] Confirm setup can switch from OpenCode Zen to another provider before
      credential entry.
- [ ] Run `thndrs setup --provider opencode-zen` and confirm hidden
      `OPENCODE_ZEN_KEY` entry still works.
- [ ] Run `thndrs setup --provider chatgpt-codex` and confirm OAuth starts.
- [ ] Confirm ChatGPT setup does not ask for global/project credential storage
      for auth.
- [ ] Confirm ChatGPT setup may still write model config when explicitly
      confirmed.
- [ ] Run non-interactive `thndrs setup --provider chatgpt-codex` and confirm
      it fails with OAuth-specific instructions.
- [ ] Launch the TUI with missing OpenCode Zen credentials and confirm API-key
      recovery appears before prompt submission.
- [ ] Launch the TUI with missing ChatGPT Codex auth and confirm OAuth recovery
      or OAuth instructions appear before prompt submission.
- [ ] Confirm `thndrs auth status` reports ChatGPT environment override or
      global auth without printing token material.

## Review Checkpoints

- [ ] After P1, review provider metadata naming before touching the UI flows.
- [ ] After P2/P3, manually review CLI ChatGPT setup before adding TUI OAuth.
- [ ] After P4, review TUI recovery wording and snapshots for the setup UX
      concern.
- [ ] After P5, review OAuth cancellation and redaction before live testing.
