# Tickets: Setup, Doctor, And Reasoning Readiness

These tickets build the setup/readiness surface described in
`docs/internal/features/010_setup/plan.md`. Work the frontier: any ticket whose
blockers are complete.

## Ticket 1: Add The Shared Reasoning Setting

**What to build:** Add a provider-neutral reasoning setting that can be parsed
from CLI, TOML, environment, ACP, and session state.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] A shared `ReasoningLevel` type supports `auto`, `none`, `minimal`, `low`,
      `medium`, `high`, and `xhigh`.
- [ ] Validation confidence, readiness severity, metadata source, and config
      write target are represented with typed domain values.
- [ ] Lowercase labels are stable for display, config, env, ACP, and persisted
      metadata.
- [ ] `auto` is the default and has an `is_auto()` helper.
- [ ] Invalid labels fail clearly at parse boundaries.
- [ ] Exported symbols and modules have docs consistent with AGENTS.md.

**Verification:**

- `cargo test reasoning`
- `cargo test config`
- `cargo test cli`

## Ticket 2: Wire Reasoning Through Config And CLI

**What to build:** Make `reasoning` part of effective runtime config with the
same precedence and provenance behavior as model and web search.

**Blocked by:** Ticket 1: Add The Shared Reasoning Setting

**Acceptance criteria:**

- [ ] TOML accepts `reasoning = "auto"`.
- [ ] `THNDRS_REASONING` is supported.
- [ ] `thndrs --reasoning <level>` is supported.
- [ ] Precedence is defaults < global config < project config < env < CLI.
- [ ] `config show --redacted` includes reasoning and origin.
- [ ] Unknown TOML keys, invalid env values, and invalid CLI values fail
      clearly.

**Verification:**

- `cargo test config`
- `cargo test cli`

## Ticket 3: Persist Reasoning To The Owning Config Layer

**What to build:** Add a narrow persistence helper for successful session
reasoning changes.

**Blocked by:** Ticket 2: Wire Reasoning Through Config And CLI

**Acceptance criteria:**

- [ ] If `reasoning` came from global config, successful changes update global
      config.
- [ ] If `reasoning` came from project config, successful changes update
      project config.
- [ ] If `reasoning` came from env, CLI, or default, mutation refuses to guess a
      write target and returns an actionable message.
- [ ] The refusal is a typed error that callers can handle without string
      matching.
- [ ] File edits preserve unrelated TOML keys and comments as much as the
      existing config write helpers do.
- [ ] Failed validation never writes config.

**Verification:**

- `cargo test config`
- focused temp HOME/workspace tests for global, project, env, CLI, and default
  origins.

## Ticket 4: Thread Reasoning Through Agent Runs

**What to build:** Carry the effective reasoning setting from CLI/config/session
state to provider request creation.

**Blocked by:** Ticket 2: Wire Reasoning Through Config And CLI

**Acceptance criteria:**

- [ ] `AgentRunConfig` includes reasoning and defaults to `auto`.
- [ ] TUI run creation uses current session reasoning.
- [ ] ACP run creation uses current session reasoning.
- [ ] Fake provider runs still work with default `auto`.
- [ ] Provider status/tracing may include the reasoning label without leaking
      provider internals.

**Verification:**

- `cargo test agent`
- `cargo test app`
- `cargo test server`

## Ticket 5: Add Provider Validation And Request Boundaries

**What to build:** Give providers a local reasoning validation hook and ensure
providers omit request fields for `auto`.

**Blocked by:** Ticket 4: Thread Reasoning Through Agent Runs

**Acceptance criteria:**

- [ ] `auto` is valid for every provider and sends no reasoning override.
- [ ] Known unsupported explicit values fail before provider request.
- [ ] Umans validation uses `capabilities.reasoning`.
- [ ] OpenCode Go validation is metadata or route-family gated.
- [ ] OpenCode Zen rejects explicit values until support is documented.
- [ ] ChatGPT Codex rejects explicit values until a smoke test enables them.
- [ ] Provider-specific serialization remains inside provider modules.
- [ ] Provider metadata/network failures are converted into typed validation
      results before doctor/setup consume them.
- [ ] No new provider-wide trait is introduced unless concrete duplication or a
      test boundary requires it.

**Verification:**

- `cargo test providers`
- request-body tests proving `auto` omits reasoning fields.
- provider validation tests for supported, unsupported, and metadata-missing
  cases.

## Ticket 6: Extend Doctor With Reasoning Readiness

**What to build:** Teach `doctor` to validate effective reasoning against the
selected provider/model using cheap metadata requests when credentials exist.

**Blocked by:** Ticket 5: Add Provider Validation And Request Boundaries

**Acceptance criteria:**

- [ ] Human doctor output shows effective reasoning, origin, provider/model,
      validation confidence, and severity.
- [ ] JSON doctor output exposes the same data without secrets.
- [ ] Cheap metadata requests are timeout-bounded and provider-scoped.
- [ ] Doctor severity is computed from typed readiness data, not display text.
- [ ] `auto` with missing metadata is a warning.
- [ ] Explicit reasoning with missing metadata is blocking.
- [ ] Explicit unsupported reasoning is blocking.
- [ ] Exit codes remain `0`, `1`, and `2` according to existing policy.
- [ ] Secrets are absent from all doctor output.

**Verification:**

- `cargo test doctor`
- doctor snapshot tests for `auto`, supported explicit, unsupported explicit,
  missing metadata, missing credentials, and network failure.

## Ticket 7: Make Setup Reasoning-Aware

**What to build:** Keep setup simple while letting it write supported reasoning
config when the selected provider/model can be validated.

**Blocked by:** Ticket 6: Extend Doctor With Reasoning Readiness

**Acceptance criteria:**

- [ ] Setup defaults reasoning to `auto`.
- [ ] Setup does not require a reasoning choice for first-run success.
- [ ] With metadata, setup offers supported common explicit levels only.
- [ ] `none`, `minimal`, and `xhigh` appear only when metadata explicitly
      supports them.
- [ ] Provider-reported labels outside thndrs' normalized value set are not
      shown until mapped deliberately.
- [ ] Without metadata, setup keeps `auto` and points to doctor.
- [ ] Setup never offers unsupported values.
- [ ] Setup remains idempotent and does not duplicate config keys.
- [ ] Setup uses deterministic provider metadata fakes in tests instead of live
      network calls.

**Verification:**

- `cargo test setup`
- temp HOME/workspace command-output tests for metadata-present and
  metadata-missing paths.

## Ticket 8: Expose Reasoning As ACP Session Config

**What to build:** Add reasoning to ACP session config options with validation,
session mutation, and owning-layer persistence.

**Blocked by:** Ticket 3: Persist Reasoning To The Owning Config Layer; Ticket
5: Add Provider Validation And Request Boundaries

**Acceptance criteria:**

- [ ] ACP initial config option ids include `reasoning`.
- [ ] ACP config options expose the full accepted value set.
- [ ] `session/set_config_option` validates reasoning before changing state.
- [ ] Successful changes persist to the owning config layer.
- [ ] Failed validation leaves session state and config unchanged.
- [ ] Future prompt turns use the updated reasoning value.
- [ ] Existing model and websearch behavior is unchanged.
- [ ] ACP validation errors preserve machine-readable option id and reason.

**Verification:**

- `cargo test server::config_options`
- ACP prompt-turn tests proving updated reasoning is used.

## Ticket 9: Add Minimal TUI Reasoning Mutation

**What to build:** Add a small TUI path for changing reasoning without adding a
full settings picker.

**Blocked by:** Ticket 3: Persist Reasoning To The Owning Config Layer; Ticket
5: Add Provider Validation And Request Boundaries

**Acceptance criteria:**

- [ ] The TUI can change reasoning for the current session through a minimal
      command or focused surface.
- [ ] Changes validate before state or config mutation.
- [ ] Successful changes persist to the owning config layer.
- [ ] Failure output is actionable and does not discard the prompt draft.
- [ ] Setup, doctor, and reasoning-readiness UI state is represented
      semantically before rendering.
- [ ] Any bounded focused surface follows the iocraft hardening rules in
      `../012_iocraft/plan.md`.
- [ ] `/doctor` transcript output remains redacted, paste-safe, and
      non-interactive unless a focused detail surface is explicitly added.
- [ ] API keys and secret material are rejected as slash-command arguments.
- [ ] Diagnostics can show the selected reasoning level when verbose mode is
      enabled.
- [ ] No rich picker is added in this ticket.
- [ ] The TUI command path does not parse success/failure by matching display
      strings.

**Verification:**

- `cargo test app`
- renderer/app tests proving prompt draft preservation on success and failure.

## Ticket 10: Update Public Docs For Setup Reasoning

**What to build:** Promote the implemented setup/reasoning contract into public
docs after code behavior is stable.

**Blocked by:** Tickets 6, 7, 8, and 9

**Acceptance criteria:**

- [ ] CLI reference documents `--reasoning`.
- [ ] Configuration reference documents `reasoning`.
- [ ] Environment reference documents `THNDRS_REASONING`.
- [ ] Setup/quick-start docs explain `auto` and provider-dependent explicit
      support.
- [ ] ACP docs mention the reasoning config option.
- [ ] Doctor troubleshooting explains validation confidence and blocking
      explicit values.

**Verification:**

- `pnpm --dir docs build`

## Ticket 11: Final Release Gate

**What to build:** Run the end-to-end release-readiness checks for setup,
doctor, reasoning, provider validation, ACP, and TUI mutation.

**Blocked by:** Tickets 6, 7, 8, 9, and 10

**Acceptance criteria:**

- [ ] Clean temp HOME setup succeeds with default `auto`.
- [ ] Doctor with no credentials reports blocking credential issues safely.
- [ ] Doctor with explicit unsupported reasoning exits `1`.
- [ ] Doctor with explicit supported reasoning exits `0`.
- [ ] Default `auto` sends no provider reasoning fields.
- [ ] ACP reasoning updates validate, persist, and affect future turns.
- [ ] TUI reasoning changes validate, persist, and preserve prompt draft.
- [ ] ChatGPT Codex explicit reasoning remains disabled without a smoke test.

**Verification:**

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged`
- `cargo clippy`
- `cargo test`
- `pnpm --dir docs build`

## Frontier

Tickets that can start immediately:

- Ticket 1: Add The Shared Reasoning Setting
