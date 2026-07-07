# Reasoning Levels Tasks

Status: Draft
Captured: 2026-07-07

## P0: Lock The Contract

- [ ] Confirm `reasoning` is the public setting name.
- [ ] Confirm accepted values:
      `auto`, `none`, `minimal`, `low`, `medium`, `high`, `xhigh`.
- [ ] Confirm `auto` means "omit provider reasoning override".
- [ ] Confirm default behavior remains unchanged when no setting is supplied.
- [ ] Confirm explicit unsupported levels fail clearly instead of silently
      downgrading.
- [ ] Confirm model ids will not encode reasoning level.
- [ ] Confirm provider modules own provider wire serialization.
- [ ] Confirm first implementation may expose config/CLI/ACP before every
      provider supports explicit reasoning.
- [ ] Confirm ChatGPT Codex explicit reasoning requires a smoke test before
      enabling.
- [ ] Confirm TUI interactive controls are deferred until the request contract
      is stable.

## P1: Shared Reasoning Type

- [ ] Add module docs where the type lives.
- [ ] Add `ReasoningLevel` with variants for all public values.
- [ ] Implement stable lowercase label rendering.
- [ ] Implement parsing from lowercase labels.
- [ ] Implement `serde` support for TOML config.
- [ ] Implement `clap::ValueEnum` support for CLI parsing.
- [ ] Add helper for `is_auto()`.
- [ ] Add helper returning provider payload strings for explicit levels.
- [ ] Add unit tests for every valid value.
- [ ] Add unit tests for invalid values.
- [ ] Add doc comments for exported symbols.

## P2: Config And CLI Plumbing

- [ ] Add `reasoning: Option<ReasoningLevel>` to `Config`.
- [ ] Merge `reasoning` with the same precedence pattern as `model`.
- [ ] Add default `ReasoningLevel::Auto` to default effective config.
- [ ] Add `THNDRS_REASONING` environment loading.
- [ ] Track config origin for `reasoning`.
- [ ] Add `--reasoning <level>` to `Cli`.
- [ ] Apply CLI override when the flag is present.
- [ ] Include `reasoning` in redacted config show output.
- [ ] Add config parser tests for TOML values.
- [ ] Add env precedence tests.
- [ ] Add CLI precedence tests.
- [ ] Add invalid-value tests for TOML/env/CLI.

## P3: Run Config And Agent Loop

- [ ] Add `reasoning` to `AgentRunConfig`.
- [ ] Preserve `AgentRunConfig::new` ergonomics with an `auto` default or update
      all callers in one pass.
- [ ] Thread reasoning into TUI run creation.
- [ ] Thread reasoning into ACP run creation.
- [ ] Include reasoning in provider-run tracing fields.
- [ ] Include reasoning in provider status rows only where it improves
      diagnostics.
- [ ] Ensure fake provider runs still work with default `auto`.
- [ ] Add tests covering default `auto` in `AgentRunConfig`.
- [ ] Add tests proving existing fake-provider prompt paths still pass.

## P4: Provider Trait Boundary

- [ ] Decide whether to add one `reasoning` argument or introduce
      `ProviderRequestConfig`.
- [ ] Update `StreamingProvider::send_streaming_request`.
- [ ] Update all provider implementations.
- [ ] Update `ProviderTurnRequest` to carry reasoning.
- [ ] Update retry/request helpers to pass reasoning through unchanged.
- [ ] Add tests or compile coverage for every provider implementation.
- [ ] Keep module ordering aligned with AGENTS.md style.

## P5: Provider Validation

- [ ] Add provider-local validation hook or helper for reasoning levels.
- [ ] Make `auto` always valid.
- [ ] Add Umans validation using `ModelInfo.capabilities.reasoning`.
- [ ] Reject Umans explicit levels when `supported == false`.
- [ ] Reject Umans `none` when `can_disable == false`.
- [ ] Enforce Umans `levels` when the list is non-empty.
- [ ] Treat empty Umans `levels` as provider-default-only until request support
      is confirmed.
- [ ] Add static unsupported validation for OpenCode Zen explicit levels.
- [ ] Add conservative OpenCode Go validation by endpoint/model family.
- [ ] Add conservative ChatGPT Codex validation that keeps explicit levels
      disabled until smoke-tested.
- [ ] Add actionable error messages for unsupported levels.
- [ ] Add unit tests for each validation branch.

## P6: Request Body Serialization

- [ ] Ensure all providers omit reasoning fields for `auto`.
- [ ] Add Responses-like `reasoning: { "effort": "<level>" }` helper only for
      providers that explicitly enable it.
- [ ] Do not add reasoning fields to OpenAI-compatible chat-completions helpers
      by default.
- [ ] Do not invent an Umans request field without provider confirmation.
- [ ] Do not add Anthropic `thinking` fields without model/provider support.
- [ ] Add request-body tests for omitted `auto`.
- [ ] Add request-body tests for any enabled explicit provider mapping.
- [ ] Add regression tests proving unsupported chat-completions routes are not
      sent Responses-only fields.

## P7: ACP Session Config

- [ ] Add `REASONING_CONFIG_OPTION_ID`.
- [ ] Include `reasoning` in initial config option ids.
- [ ] Include `reasoning` in ACP config option specs.
- [ ] Add `ConfigOptionValue::Reasoning`.
- [ ] Validate ACP reasoning values.
- [ ] Add `reasoning_config_option`.
- [ ] Use `SessionConfigOptionCategory::ModelConfig`.
- [ ] Store reasoning in ACP session metadata.
- [ ] Apply reasoning updates to future prompt turns.
- [ ] Preserve existing `model` and `websearch` behavior.
- [ ] Add ACP config option tests.
- [ ] Add ACP prompt-turn tests proving updated reasoning is used.

## P8: Diagnostics And Persistence

- [ ] Decide whether reasoning belongs in session metadata, per-turn records, or
      both.
- [ ] Include reasoning in startup or provider diagnostics where useful.
- [ ] Ensure session records do not leak provider internals.
- [ ] Update inspect/export only if existing metadata patterns require it.
- [ ] Add tests for serialized metadata if persistence changes.
- [ ] Add snapshot updates only for intentional UI/status changes.

## P9: Public Docs If Needed

- [ ] Update CLI reference if the feature ships publicly.
- [ ] Update configuration reference with `reasoning`.
- [ ] Update environment-variable docs with `THNDRS_REASONING`.
- [ ] Update usage/models docs to explain `auto` and provider-dependent support.
- [ ] Update ACP usage docs if ACP config options are documented publicly.
- [ ] Run `pnpm --dir docs build` for public docs changes.

## P10: Verification

- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy --fix --allow-dirty --allow-staged`.
- [ ] Run `cargo clippy`.
- [ ] Run `cargo test`.
- [ ] Run focused provider request-body tests.
- [ ] Run focused ACP config-option tests.
- [ ] Manually inspect error messages for unsupported explicit levels.
- [ ] Manually inspect that default `auto` sends no new provider fields.
- [ ] If public docs changed, run `pnpm --dir docs build`.

## Review Checkpoints

- [ ] After P1/P2, review public values and config precedence.
- [ ] After P4, review trait boundary complexity before adding provider-specific
      behavior.
- [ ] After P5, review unsupported-level errors for clarity.
- [ ] After P6, review serialized request bodies for every provider route.
- [ ] After P7, review ACP client compatibility.
- [ ] Before enabling ChatGPT Codex explicit reasoning, run and record a smoke
      test against the experimental backend.
