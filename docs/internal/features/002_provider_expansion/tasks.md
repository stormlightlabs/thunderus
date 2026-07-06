# Provider Expansion Tasks

Status: Draft
Captured: 2026-07-05

## P0: Lock The Contract

- [x] Select `opencode/<model-id>` as the OpenCode Zen model prefix.
- [x] Select `opencode/big-pickle` as the initial Zen model.
- [x] Select `https://opencode.ai/zen/v1/chat/completions` as the Big Pickle
      endpoint.
- [x] Select `https://opencode.ai/zen/v1/models` as the Zen model-discovery
      endpoint.
- [x] Select Big Pickle as the default free model while preserving the
      limited-free caveat in docs/setup.
- [x] Keep `opencode-go/` separate from `opencode/`.
- [x] Select `OPENCODE_ZEN_KEY` as the explicit Zen credential variable.
- [x] Select `chatgpt-codex/<model-id>` as the ChatGPT Codex model prefix.
- [x] Select `https://chatgpt.com/backend-api/codex/responses` as the
      ChatGPT-backed Codex endpoint.
- [x] Select device-code login as the primary ChatGPT Codex auth flow.
- [x] Select browser PKCE login with localhost callback as the fallback
      ChatGPT Codex auth flow.
- [x] Select `~/.thndrs/auth.json` as the ChatGPT Codex credential file.
- [x] Select `CHATGPT_CODEX_ACCESS_TOKEN` as the ephemeral ChatGPT Codex token
      override.
- [x] Define TTFT as client-observed submit-to-first-semantic-output latency.
- [x] Include ChatGPT Codex OAuth/subscription provider work in this provider
      expansion feature.

## P1: Research And Docs Notes

- [x] Add OpenCode Zen research notes under
      `docs/src/content/docs/notebook/providers/opencode-zen.md`.
- [x] Add TTFT research notes under `docs/src/content/docs/notebook/ttft.md`.
- [x] Add OpenCode Zen to the notebook sidebar.
- [x] Add TTFT to the notebook sidebar.
- [ ] Add public OpenCode Zen provider docs.
- [ ] Update the homepage coming-soon/provider wording to describe provider
      expansion, including OpenCode Zen and ChatGPT Codex.

## P2: OpenCode Zen Credential Foundations

- [x] Add `OPENCODE_ZEN_KEY` to credential constants.
- [x] Load `OPENCODE_ZEN_KEY` from environment and managed credential files.
- [x] Add `opencode-zen` to `login` and `logout` provider parsing.
- [x] Add `opencode-zen` to setup provider choices.
- [x] Add `opencode-zen` to doctor credential status.
- [x] Validate Zen keys with a lightweight model-list request when network is
      available.
- [x] Store Zen keys only through existing API-key credential storage.
- [x] Redact Zen keys from errors, verbose rows, snapshots, and session records.
- [x] Keep Zen credential behavior separate from `OPENCODE_GO_KEY`.

## P3: OpenCode Zen Provider Foundations

- [x] Add `src/core/providers/opencode/zen.rs` (move `opencode_go.rs` to `opencode/go.rs`)
- [x] Define `BASE_URL`, `CHAT_COMPLETIONS_URL`, `MODELS_URL`, and
      `MODEL_PREFIX`.
- [x] Parse `opencode/<model-id>` and strip the prefix for requests.
- [x] Reject raw Zen model ids with a clear provider error.
- [x] Define the initial known model list with `opencode/big-pickle`.
- [x] Build OpenAI-compatible streaming chat-completions request bodies.
- [x] Convert provider-neutral messages into chat `messages`.
- [x] Convert local tool schemas into chat `tools`.
- [x] Send streaming requests with `http_status_as_error(false)`.
- [x] Summarize non-2xx response bodies through the existing provider error
      path.
- [x] Fetch `/zen/v1/models` for optional validation and picker refresh.
- [x] Map model discovery ids into `opencode/<id>` picker entries.
- [x] Avoid machine-inferred pricing claims from model discovery.
- [x] Classify retryable HTTP and transport errors.
- [x] Classify terminal balance, unavailable-model, and free-period-ended
      errors as non-retryable.

## P4: Stream Parsing

- [x] Parse OpenAI-compatible SSE `data:` payloads from the Zen chat route.
- [x] Map assistant text deltas into assistant transcript events.
- [x] Map compatible reasoning deltas when present.
- [x] Collect tool-call argument deltas until the call is complete.
- [x] Convert completed function calls into `ToolUseRequest`.
- [x] Parse usage events into token usage events.
- [x] Detect completed, failed, cancelled, queued, and in-progress response
      states when present.
- [x] Convert backend error events into provider failures.
- [x] Treat malformed protocol events after connection as non-retryable
      provider failures.
- [x] Preserve the existing `ProviderTurn` contract for the agent loop.

## P5: Agent And Setup Integration

- [ ] Export `opencode_zen` from `src/core/providers/mod.rs`.
- [ ] Add `ProviderKind::OpenCodeZen`.
- [ ] Route `opencode/` model ids to `ProviderKind::OpenCodeZen`.
- [ ] Run OpenCode Zen through the existing `run_provider` loop.
- [ ] Add `opencode/big-pickle` to the offline model picker.
- [ ] Show Big Pickle picker text with concise free/caveat wording.
- [ ] Add OpenCode Zen to first-run recovery surfaces.
- [ ] Make Big Pickle the compiled default model.
- [ ] Make OpenCode Zen the default setup provider.
- [ ] Recover missing Zen credentials without silently changing away from the
      default model.
- [ ] Surface explicit recovery when Big Pickle is unavailable or removed from
      discovery.
- [ ] Keep `OPENAI_API_KEY`, `OPENCODE_GO_KEY`, and `OPENCODE_ZEN_KEY`
      separate.

## P6: ChatGPT Codex Auth Foundations

- [ ] Add a ChatGPT Codex auth module with `ChatGptCodexAuth`.
- [ ] Load `CHATGPT_CODEX_ACCESS_TOKEN` as a non-persisted process override.
- [ ] Decode JWT payloads without logging token contents.
- [ ] Extract `payload["https://api.openai.com/auth"].chatgpt_account_id`.
- [ ] Fail closed when the account id claim is missing or malformed.
- [ ] Add `~/.thndrs/auth.json` read/write helpers.
- [ ] Write file-backed credentials with Unix mode `0600`.
- [ ] Redact access tokens and refresh tokens from debug/status output.
- [ ] Store `access_token`, `refresh_token`, `expires_at_ms`, and `account_id`.
- [ ] Refresh expired credentials before provider requests.
- [ ] Serialize refreshes behind a process-local mutex.
- [ ] Add `chatgpt-codex login` CLI support.
- [ ] Implement device-code user-code request.
- [ ] Implement device-code polling and authorization-code exchange.
- [ ] Implement browser PKCE fallback login.
- [ ] Bind the browser callback listener to `localhost:1455/auth/callback`.
- [ ] Add `chatgpt-codex logout` CLI support.
- [ ] Delete only the `chatgpt_codex` credential entry on logout.

## P7: ChatGPT Codex Provider Foundations

- [ ] Add `src/core/providers/codex.rs`.
- [ ] Define `BASE_URL`, `MODEL_PREFIX`, known model ids, and status labels.
- [ ] Parse `chatgpt-codex/<model-id>` and strip the prefix for requests.
- [ ] Reject raw ChatGPT Codex model ids with a clear provider error.
- [ ] Build SSE headers with bearer auth, account id, originator,
      `OpenAI-Beta`, `accept`, and `content-type`.
- [ ] Keep header tests from snapshotting token values.
- [ ] Build a Responses-like streaming request body.
- [ ] Convert provider-neutral messages into Responses `input`.
- [ ] Convert local tool schemas into Responses `tools`.
- [ ] Include `store: false`, `stream: true`, `tool_choice: "auto"`,
      `parallel_tool_calls: true`, and low verbosity.
- [ ] Ignore encrypted reasoning payloads during persistence.
- [ ] Send streaming requests with `http_status_as_error(false)`.
- [ ] Summarize non-2xx response bodies through the existing provider error
      path.
- [ ] Classify retryable HTTP and transport errors.
- [ ] Classify terminal subscription, balance, quota, and monthly usage errors
      as non-retryable.

## P8: ChatGPT Codex Stream Parsing

- [ ] Parse SSE `data:` payloads from the ChatGPT Codex endpoint.
- [ ] Map assistant text deltas into assistant transcript events.
- [ ] Map visible reasoning deltas into reasoning transcript events.
- [ ] Collect tool-call argument deltas until the call is complete.
- [ ] Convert completed function calls into `ToolUseRequest`.
- [ ] Parse usage increments into token usage events.
- [ ] Detect completed, failed, incomplete, cancelled, queued, and in-progress
      response statuses.
- [ ] Convert backend error events into provider failures.
- [ ] Treat malformed protocol events after connection as non-retryable
      provider failures.
- [ ] Preserve the existing `ProviderTurn` contract for the agent loop.

## P9: ChatGPT Codex Agent Integration

- [ ] Export `chatgpt_codex` from `src/core/providers/mod.rs`.
- [ ] Add `ProviderKind::ChatGptCodex`.
- [ ] Route `chatgpt-codex/` model ids to `ProviderKind::ChatGptCodex`.
- [ ] Run ChatGPT Codex through the existing `run_provider` loop.
- [ ] Add ChatGPT Codex known models to the offline model picker.
- [ ] Show ChatGPT Codex provider status as ChatGPT-backed and experimental.
- [ ] Keep `OPENAI_API_KEY` separate from ChatGPT Codex credentials.
- [ ] Keep missing/expired credential failures prompt-restoring.
- [ ] Keep session metadata free of raw provider payloads and secrets.

## P10: TTFT State

- [ ] Add per-turn timing state to `App`.
- [ ] Start TTFT timing when a user turn is submitted locally.
- [ ] Reset pending TTFT state when a new turn starts.
- [ ] Stop TTFT on the first assistant text delta.
- [ ] Stop TTFT on the first visible reasoning delta.
- [ ] Stop TTFT on the first tool-call delta or completed tool call.
- [ ] Do not stop TTFT on provider status messages.
- [ ] Do not stop TTFT on usage-only events.
- [ ] Preserve one user-observed TTFT across retries.
- [ ] Retain the last completed turn TTFT after the run finishes.
- [ ] Avoid writing TTFT to session files until session observability explicitly
      needs it.

## P11: TTFT Statusline Rendering

- [ ] Render `ttft: pending` while a run is waiting for semantic output.
- [ ] Render compact millisecond values below one second.
- [ ] Render compact second values at and above one second.
- [ ] Keep the last measured TTFT visible after a run completes when width
      allows.
- [ ] Hide TTFT before core prompt/status affordances on narrow screens.
- [ ] Add renderer snapshots for pending TTFT.
- [ ] Add renderer snapshots for measured TTFT.
- [ ] Add renderer snapshots for retained TTFT after completion.
- [ ] Add renderer snapshots for narrow widths where TTFT is hidden.

## P12: Unit Tests

- [ ] Test `opencode/big-pickle` prefix parsing.
- [ ] Test raw `big-pickle` rejection.
- [ ] Test provider routing for `opencode/`.
- [ ] Test missing Zen credentials before network access.
- [ ] Test Zen credential redaction.
- [ ] Test Big Pickle request body construction for text-only turns.
- [ ] Test Big Pickle request body construction with tool schemas.
- [ ] Test model discovery mapping.
- [ ] Test model picker includes Big Pickle.
- [ ] Test compiled default model is `opencode/big-pickle`.
- [ ] Test setup default provider is OpenCode Zen.
- [ ] Test retry classifier for 429 throttling.
- [ ] Test retry classifier for 500, 502, 503, 504, timeout, and connection
      reset cases.
- [ ] Test retry classifier for terminal balance, unavailable-model, and
      free-period-ended errors.
- [ ] Test SSE text deltas.
- [ ] Test SSE tool-call deltas.
- [ ] Test SSE usage events.
- [ ] Test SSE backend error events.
- [ ] Test malformed SSE payload handling.
- [ ] Test ChatGPT Codex model prefix parsing and raw model rejection.
- [ ] Test ChatGPT Codex known model picker entries.
- [ ] Test ChatGPT Codex missing credentials before network access.
- [ ] Test ChatGPT Codex JWT account-id extraction success.
- [ ] Test ChatGPT Codex JWT account-id extraction failure.
- [ ] Test ChatGPT Codex credential storage JSON shape.
- [ ] Test ChatGPT Codex Unix `0600` credential file mode where supported.
- [ ] Test ChatGPT Codex credential redaction in debug/status output.
- [ ] Test ChatGPT Codex expired credential refresh under the mutex.
- [ ] Test ChatGPT Codex env token override does not write credential storage.
- [ ] Test ChatGPT Codex logout removes only the ChatGPT Codex credential
      entry.
- [ ] Test ChatGPT Codex header construction with redacted assertions.
- [ ] Test ChatGPT Codex request body construction for text-only turns.
- [ ] Test ChatGPT Codex request body construction with tool schemas.
- [ ] Test ChatGPT Codex Responses input conversion for tool-result history.
- [ ] Test ChatGPT Codex SSE text deltas.
- [ ] Test ChatGPT Codex SSE visible reasoning deltas.
- [ ] Test ChatGPT Codex SSE tool-call deltas.
- [ ] Test ChatGPT Codex SSE usage events.
- [ ] Test ChatGPT Codex SSE backend error events.
- [ ] Test ChatGPT Codex malformed SSE payload handling.
- [ ] Test ChatGPT Codex retry classifier for terminal subscription-limit
      strings.
- [ ] Test TTFT starts on submit.
- [ ] Test TTFT stops on first semantic output.
- [ ] Test TTFT ignores status and usage-only events.
- [ ] Test TTFT is retained after run completion.
- [ ] Test TTFT reset on the next turn.

## P13: Ignored Live Tests

- [ ] Add ignored Zen model-list smoke test.
- [ ] Add ignored Big Pickle text-only streaming smoke test.
- [ ] Add ignored Big Pickle local tool-call round-trip smoke test if supported.
- [ ] Add ignored ChatGPT Codex device-code login smoke test.
- [ ] Add ignored ChatGPT Codex browser fallback login smoke test.
- [ ] Add ignored ChatGPT Codex text-only streaming smoke test.
- [ ] Add ignored ChatGPT Codex local tool-call round-trip smoke test.
- [ ] Add ignored ChatGPT Codex expired-token refresh smoke test.
- [ ] Require `OPENCODE_ZEN_KEY` in every live test name or failure message.
- [ ] Require network access and real Zen credentials for every live test.
- [ ] Require explicit ChatGPT subscription prerequisites in every ChatGPT Codex
      live test name or failure message.
- [ ] Require network access and real ChatGPT credentials for every ChatGPT
      Codex live test.
- [ ] Mention limited-free pricing and privacy caveats in live test docs.
- [ ] Keep every live test skipped by default.

## P14: Public Docs

- [ ] Document `opencode/big-pickle` in model usage docs.
- [ ] Document OpenCode Zen setup and credential storage.
- [ ] Document `OPENCODE_ZEN_KEY`.
- [ ] Document `opencode-zen login` and `opencode-zen logout`.
- [ ] Document that `OPENCODE_ZEN_KEY` is separate from `OPENCODE_GO_KEY`.
- [ ] Document that Big Pickle free pricing is time-limited.
- [ ] Document the Big Pickle free-period privacy caveat.
- [ ] Document `chatgpt-codex/<model-id>` in model usage docs.
- [ ] Document `chatgpt-codex login` and `chatgpt-codex logout`.
- [ ] Document `CHATGPT_CODEX_ACCESS_TOKEN`.
- [ ] Document `~/.thndrs/auth.json` as sensitive local ChatGPT Codex
      credential storage.
- [ ] Document that ChatGPT Codex credentials do not use `OPENAI_API_KEY`.
- [ ] Document that ChatGPT Codex is ChatGPT-backed and experimental.
- [ ] Document TTFT statusline behavior in TUI usage docs.
- [ ] Document ignored live smoke test prerequisites.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --all-targets --allow-dirty`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test opencode_zen`
- [ ] `cargo test chatgpt_codex`
- [ ] `cargo test provider`
- [ ] `cargo test cli`
- [ ] `cargo test`
