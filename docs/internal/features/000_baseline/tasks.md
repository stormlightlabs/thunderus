# Tickets: thndrs Baseline

These tickets establish the minimum usable foundation in
[the baseline plan](plan.md). They deliberately extract tested seams and finish
the initial session UX; they do not rewrite the application. Work the frontier:
any ticket whose blockers are complete.

## Ticket 1: Establish The Operating Contract

**What to build:** Make the repository directives concise enough for an agent
or contributor to complete one assigned ticket without losing the product's
minimalism or scope boundaries.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] `AGENTS.md` states the minimal-agent principles, package dependency rule,
      one-ticket autonomy rule, approval boundaries, verification sequence, and
      human-only publishing rule.
- [x] `AGENTS.md` points to `000_baseline` for foundation work without copying
      later feature requirements.
- [x] `.gitignore` explicitly includes `!AGENTS.md`.

**Verification:**

- Review the directive against the baseline plan and confirm the ignore rule is
  present.

## Ticket 2: Create The Minimal Workspace

**What to build:** Convert the repository to a Cargo workspace while preserving
the existing `thndrs` package metadata and executable behavior.

**Blocked by:** Ticket 1: Establish The Operating Contract

**Acceptance criteria:**

- [x] The workspace has exactly the `thndrs`, `thndrs-agent`, and
      `thndrs-context` package members under `crates/`.
- [x] The libraries declare stable package identities, Apache-2.0 licensing,
      repository metadata, and pre-1.0 versions.
- [x] The `thndrs` package metadata and executable name are unchanged.
- [x] No empty `thndrs-acp` placeholder package or new dependency is created.

**Verification:**

- `cargo metadata --no-deps`
- `cargo test --workspace`

## Ticket 3: Extract The Agent Library Boundary

**What to build:** Move the existing reusable agent loop and typed contracts
into `thndrs-agent`, then adapt `thndrs` without a TUI or CLI dependency
leaking back into the library.

**Blocked by:** Ticket 2: Create The Minimal Workspace

**Acceptance criteria:**

- [x] The library owns provider-neutral turn/message/event, tool definition,
      cancellation, retry, permission, and harness/run contracts.
- [x] Public APIs expose no provider payloads, TUI/app state, direct local
      filesystem policy, terminal UI, or ACP RPC.
- [x] Applications supply tool execution and permission policy through typed
      adapters.
- [x] The fake-provider, provider/tool loop, and ACP event mapping retain
      their current behavior through focused tests.
- [x] `thndrs-agent` does not depend on `thndrs-context`.

**Verification:**

- `cargo test -p thndrs-agent`
- `cargo test -p thndrs --lib agent`
- `cargo test -p thndrs --test acp_server_smoke`

## Ticket 4: Extract Context With Explicit Memory Capability

**What to build:** Move context, scoped instructions, prompt-context
projection, session/audit records, and file-backed lexical memory into
`thndrs-context`; make memory unavailable by default and disabled in fresh
`thndrs` configuration.

**Blocked by:** Ticket 2: Create The Minimal Workspace

**Acceptance criteria:**

- [x] The library preserves the existing ledger, prompt projection, bounded
      instruction loading, memory source/index, redaction, recovery, and
      append-only session behavior.
- [x] It has no provider, terminal, ACP, or `thndrs-agent` dependency.
- [x] Default crate features exclude memory; the memory-enabled build is tested.
- [x] Fresh `thndrs` does not load, index, retrieve, or write memory until a
      user enables it.
- [x] The enable/disable state is visible in diagnostics and safe session
      metadata without persisting memory body text.

**Verification:**

- `cargo test -p thndrs-context`
- `cargo test -p thndrs --lib context`
- `cargo test -p thndrs --lib memory`

## Ticket 5: Compose The Existing Application Through Both Libraries

**What to build:** Replace the `thndrs` package's internal core imports with the
two extracted libraries and prove that the CLI/TUI remains a useful minimal
agent when memory is off.

**Blocked by:** Ticket 3: Extract The Agent Library Boundary; Ticket 4: Extract Context With Explicit Memory Capability

**Acceptance criteria:**

- [x] `thndrs` composes typed agent and context values at its application
      boundary without re-exporting internal implementation modules as a
      compatibility shortcut.
- [x] Existing prompt inspection, fake-provider, context/session, renderer,
      and ACP fake-client tests pass with memory disabled.
- [x] No TUI framework, plugin system, or broad application-state abstraction
      is introduced.

**Verification:**

- `cargo test -p thndrs`
- Manual smoke: fake provider and `--print-prompt` with memory disabled.

## Ticket 6: Make Sessions Usable

**What to build:** Complete the initial session workflow that was previously
planned in `005_sessions`: discover, safely resume, inspect, export, and read
session/log evidence from both the TUI and CLI.

**Blocked by:** Ticket 5: Compose The Existing Application Through Both Libraries

**Acceptance criteria:**

- [x] Exact and unambiguous-prefix session lookup, newest-first summaries, and
      corrupt/missing-file tolerance are implemented and tested.
- [x] TUI commands `history`, `resume <id>`, `session <id>`, `tokens`, and
      `debug log [session-id]` are suggested, bounded, and preserve a prompt
      draft after read-only command failure.
- [x] Resume appends only after an exclusive writer lock and never restores old
      live tool/run state.
- [x] CLI preserves no-subcommand TUI startup and adds `sessions list`, `show`,
      `resume`, `inspect --format json`, `export --format jsonl`, `debug tail`,
      and `debug session-log`, all respecting `--cwd`.
- [x] Inspection/export is renderer-independent, stable, redacted, and records
      transcript/tool/usage/context/memory/skill/config evidence without
      replaying a destructive side effect.
- [x] Session and daily log readers cap output, tolerate absence, and redact
      secret-looking values.
- [x] Public session, CLI, usage, and README documentation cover the completed
      commands, lookup rules, stored data, and omissions.

**Verification:**

- `cargo test -p thndrs --lib session`
- `cargo test -p thndrs --lib app`
- `cargo test -p thndrs`
- `pnpm --dir docs build`

## Ticket 7: Expose The ACP Server Through `thndrs acp serve`

**What to build:** Replace the standalone ACP executable with a protocol-clean
`thndrs acp serve` mode while preserving the baseline protocol and safety
behavior.

**Blocked by:** Ticket 5: Compose The Existing Application Through Both Libraries

**Acceptance criteria:**

- [x] The server runs from the primary `thndrs` executable and reuses its
      provider, tool, configuration, and session runtime without a duplicate
      application package.
- [x] The command remains protocol-clean and preserves configuration,
      containment, permission, redaction, cancellation, and local-session
      audit behavior.
- [x] Existing fake-client tests run against `thndrs acp serve`.
- [x] The standalone `thndrs-acp-server` executable no longer exists.

**Verification:**

- `cargo test -p thndrs --test acp_server_smoke`
- `target/debug/thndrs --cwd /path/to/project acp serve` with the fake client fixture

## Ticket 8: Make The Workspace Package-Ready

**What to build:** Verify the two libraries and the `thndrs` application
package without publishing any of them.

**Blocked by:** Ticket 6: Make Sessions Usable; Ticket 7: Expose The ACP Server Through `thndrs acp serve`

**Acceptance criteria:**

- [x] Each library has a minimal README/API overview and no application-only
      dependencies in its public contract.
- [x] All three packages pass `cargo package` without publishing.
- [x] Cross-library dependency checks prove the libraries remain independent.
- [x] The `thndrs` application package metadata remains unchanged.

**Verification:**

- `cargo package -p thndrs-agent --allow-dirty`
- `cargo package -p thndrs-context --allow-dirty`
- `cargo package -p thndrs --allow-dirty --config 'patch.crates-io.thndrs-agent.path="crates/thndrs-agent"' --config 'patch.crates-io.thndrs-context.path="crates/thndrs-context"'`
- `cargo test --workspace`

## Ticket 9: Baseline Release Gate

**What to build:** Run the full foundation verification suite and review the
workspace for accidental scope expansion before a human selects a later
feature.

**Blocked by:** Ticket 8: Make The Workspace Package-Ready

**Acceptance criteria:**

- [ ] Workspace, library package, fake agent, context/session,
      memory-disabled startup, ACP event mapping, and session UX checks pass.
- [ ] `AGENTS.md` reflects the delivered workspace and boundaries.
- [ ] No new crate, dependency, default capability, or application rewrite was
      introduced outside approved tickets.
- [ ] The next feature is explicitly selected by a human from the retained
      context, ACP, setup/reasoning, or iocraft plans.

**Verification:**

- `cargo fmt`
- `cargo clippy --workspace --fix --allow-dirty --allow-staged`
- `cargo clippy --workspace`
- `cargo test --workspace`
- `cargo package -p thndrs-agent`
- `cargo package -p thndrs-context`
- `cargo package -p thndrs`

## Frontier

Ticket 7 can start now. Ticket 8 remains blocked on Ticket 7. Work one ticket
per fresh agent context; a human selects the next ticket after each verified
handoff.
