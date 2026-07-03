# ChatGPT Codex Provider Tasks

Status: Draft
Captured: 2026-07-03

## P0: Lock The Contract

- [x] Select `chatgpt-codex/<model-id>` as the provider model prefix.
- [x] Keep `umans-coder` as the default model.
- [x] Select `https://chatgpt.com/backend-api/codex/responses` as the
      ChatGPT-backed Codex endpoint.
- [x] Select device-code login as the primary ChatGPT auth flow.
- [x] Select browser PKCE login with localhost callback as the fallback auth
      flow.
- [x] Select `~/.thndrs/auth.json` as the persistent credential file.
- [x] Select process-local refresh locking with atomic temp-file replacement.
- [x] Select SSE as the transport.
- [x] Select provider-local Responses conversion.
- [x] Select `CHATGPT_CODEX_ACCESS_TOKEN` as the ephemeral token override.

## P1: Auth Foundations

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

## P2: Provider Foundations

- [ ] Add `src/providers/chatgpt_codex.rs`.
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

## P3: Stream Parsing

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

## P4: Agent Integration

- [ ] Export `chatgpt_codex` from `src/providers/mod.rs`.
- [ ] Add `ProviderKind::ChatGptCodex`.
- [ ] Route `chatgpt-codex/` model ids to `ProviderKind::ChatGptCodex`.
- [ ] Run ChatGPT Codex through the existing `run_provider` loop.
- [ ] Add ChatGPT Codex known models to the offline model picker.
- [ ] Show ChatGPT Codex provider status as ChatGPT-backed and experimental.
- [ ] Keep `OPENAI_API_KEY` separate from ChatGPT Codex credentials.
- [ ] Keep missing/expired credential failures prompt-restoring.
- [ ] Keep session metadata free of raw provider payloads and secrets.

## P5: Unit Tests

- [ ] Test model prefix parsing and raw model rejection.
- [ ] Test known model picker entries.
- [ ] Test missing credentials before network access.
- [ ] Test JWT account-id extraction success.
- [ ] Test JWT account-id extraction failure.
- [ ] Test credential storage JSON shape.
- [ ] Test Unix `0600` credential file mode where supported.
- [ ] Test credential redaction in debug/status output.
- [ ] Test expired credential refresh under the mutex.
- [ ] Test env token override does not write credential storage.
- [ ] Test logout removes only the ChatGPT Codex credential entry.
- [ ] Test header construction with redacted assertions.
- [ ] Test request body construction for text-only turns.
- [ ] Test request body construction with tool schemas.
- [ ] Test Responses input conversion for tool-result history.
- [ ] Test SSE text deltas.
- [ ] Test SSE visible reasoning deltas.
- [ ] Test SSE tool-call deltas.
- [ ] Test SSE usage events.
- [ ] Test SSE backend error events.
- [ ] Test malformed SSE payload handling.
- [ ] Test retry classifier for 429 throttling.
- [ ] Test retry classifier for terminal subscription-limit strings.
- [ ] Test retry classifier for 500, 502, 503, 504, timeout, and connection
      reset cases.

## P6: Ignored Live Tests

- [ ] Add ignored device-code login smoke test.
- [ ] Add ignored browser fallback login smoke test.
- [ ] Add ignored text-only streaming smoke test.
- [ ] Add ignored local tool-call round-trip smoke test.
- [ ] Add ignored expired-token refresh smoke test.
- [ ] Require explicit ChatGPT subscription prerequisites in every live test
      name or failure message.
- [ ] Require network access and real credentials for every live test.
- [ ] Keep every live test skipped by default.

## P7: Docs

- [ ] Update `docs/src/content/docs/guides/providers/chatgpt.md` with final
      usage steps after implementation.
- [ ] Document `chatgpt-codex/<model-id>` in model usage docs.
- [ ] Document `chatgpt-codex login` and `chatgpt-codex logout`.
- [ ] Document `CHATGPT_CODEX_ACCESS_TOKEN`.
- [ ] Document `~/.thndrs/auth.json` as sensitive local credential storage.
- [ ] Document that ChatGPT Codex credentials do not use `OPENAI_API_KEY`.
- [ ] Document ignored live smoke test prerequisites.

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo clippy --fix --all-targets --allow-dirty`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo test chatgpt_codex`
- [ ] `cargo test providers`
- [ ] `cargo test agent`
- [ ] `cargo test cli`
- [ ] `cargo test`
