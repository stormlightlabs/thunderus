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

- [ ] `AGENTS.md` states the minimal-agent principles, package dependency rule,
      one-ticket autonomy rule, approval boundaries, verification sequence, and
      human-only publishing rule.
- [ ] `AGENTS.md` points to `000_baseline` for foundation work without copying
      later feature requirements.
- [ ] `.gitignore` explicitly includes `!AGENTS.md`.

**Verification:**

- Review the directive against the baseline plan and confirm the ignore rule is
  present.

## Ticket 2: Create The Minimal Workspace

**What to build:** Convert the repository to a Cargo workspace while preserving
the existing `thndrs` package metadata and executable behavior.

**Blocked by:** Ticket 1: Establish The Operating Contract

**Acceptance criteria:**

- [ ] The workspace has exactly the root `thndrs` package plus
      `thndrs-agent` and `thndrs-context` library members.
- [ ] The libraries declare stable package identities, Apache-2.0 licensing,
      repository metadata, and pre-1.0 versions.
- [ ] The root `thndrs` package metadata and executable name are unchanged.
- [ ] No empty `thndrs-acp` placeholder package or new dependency is created.

**Verification:**

- `cargo metadata --no-deps`
- `cargo test --workspace`

## Ticket 3: Extract The Agent Library Boundary

**What to build:** Move the existing reusable agent loop and typed contracts
into `thndrs-agent`, then adapt `thndrs` without a TUI or CLI dependency
leaking back into the library.

**Blocked by:** Ticket 2: Create The Minimal Workspace

**Acceptance criteria:**

- [ ] The library owns provider-neutral turn/message/event, tool definition,
      cancellation, retry, permission, and harness/run contracts.
- [ ] Public APIs expose no provider payloads, TUI/app state, direct local
      filesystem policy, terminal UI, or ACP RPC.
- [ ] Applications supply tool execution and permission policy through typed
      adapters.
- [ ] The fake-provider, provider/tool loop, and ACP event mapping retain
      their current behavior through focused tests.
- [ ] `thndrs-agent` does not depend on `thndrs-context`.

**Verification:**

- `cargo test -p thndrs-agent`
- `cargo test -p thndrs --lib agent`
- `cargo test -p thndrs --bin thndrs-acp-server`

## Ticket 4: Extract Context With Explicit Memory Capability

**What to build:** Move context, scoped instructions, prompt-context
projection, session/audit records, and file-backed lexical memory into
`thndrs-context`; make memory unavailable by default and disabled in fresh
`thndrs` configuration.

**Blocked by:** Ticket 2: Create The Minimal Workspace

**Acceptance criteria:**

- [ ] The library preserves the existing ledger, prompt projection, bounded
      instruction loading, memory source/index, redaction, recovery, and
      append-only session behavior.
- [ ] It has no provider, terminal, ACP, or `thndrs-agent` dependency.
- [ ] Default crate features exclude memory; the memory-enabled build is tested.
- [ ] Fresh `thndrs` does not load, index, retrieve, or write memory until a
      user enables it.
- [ ] The enable/disable state is visible in diagnostics and safe session
      metadata without persisting memory body text.

**Verification:**

- `cargo test -p thndrs-context`
- `cargo test -p thndrs --lib context`
- `cargo test -p thndrs --lib memory`

## Ticket 5: Compose The Existing Application Through Both Libraries

**What to build:** Replace the root package's internal core imports with the
two extracted libraries and prove that the CLI/TUI remains a useful minimal
agent when memory is off.

**Blocked by:** Ticket 3: Extract The Agent Library Boundary; Ticket 4: Extract Context With Explicit Memory Capability

**Acceptance criteria:**

- [ ] `thndrs` composes typed agent and context values at its application
      boundary without re-exporting internal implementation modules as a
      compatibility shortcut.
- [ ] Existing prompt inspection, fake-provider, context/session, renderer,
      and ACP fake-client tests pass with memory disabled.
- [ ] No TUI framework, plugin system, or broad application-state abstraction
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

- [ ] Exact and unambiguous-prefix session lookup, newest-first summaries, and
      corrupt/missing-file tolerance are implemented and tested.
- [ ] TUI commands `history`, `resume <id>`, `session <id>`, `tokens`, and
      `debug log [session-id]` are suggested, bounded, and preserve a prompt
      draft after read-only command failure.
- [ ] Resume appends only after an exclusive writer lock and never restores old
      live tool/run state.
- [ ] CLI preserves no-subcommand TUI startup and adds `sessions list`, `show`,
      `resume`, `inspect --format json`, `export --format jsonl`, `debug tail`,
      and `debug session-log`, all respecting `--cwd`.
- [ ] Inspection/export is renderer-independent, stable, redacted, and records
      transcript/tool/usage/context/memory/skill/config evidence without
      replaying a destructive side effect.
- [ ] Session and daily log readers cap output, tolerate absence, and redact
      secret-looking values.
- [ ] Public session, CLI, usage, and README documentation cover the completed
      commands, lookup rules, stored data, and omissions.

**Verification:**

- `cargo test -p thndrs --lib session`
- `cargo test -p thndrs --lib app`
- `cargo test -p thndrs`
- `pnpm --dir docs build`

## Ticket 7: Package The ACP Server As `thndrs-acp`

**What to build:** Move the proven ACP server into the fourth workspace package
and preserve the baseline protocol and safety behavior.

**Blocked by:** Ticket 5: Compose The Existing Application Through Both Libraries

**Acceptance criteria:**

- [ ] `thndrs-acp` depends directly on `thndrs-agent` and `thndrs-context`,
      not on the CLI/TUI package.
- [ ] The executable remains protocol-clean and preserves configuration,
      containment, permission, redaction, cancellation, and local-session
      audit behavior.
- [ ] Existing fake-client tests run against the packaged executable.
- [ ] The root `thndrs` package no longer owns the ACP executable.

**Verification:**

- `cargo test -p thndrs-acp`
- `cargo run -p thndrs-acp` with the fake client fixture

## Ticket 8: Make The Workspace Package-Ready

**What to build:** Verify the two libraries and two application packages as
independent crates without publishing any of them.

**Blocked by:** Ticket 6: Make Sessions Usable; Ticket 7: Package The ACP Server As `thndrs-acp`

**Acceptance criteria:**

- [ ] Each library has a minimal README/API overview and no application-only
      dependencies in its public contract.
- [ ] All four packages pass `cargo package` without publishing.
- [ ] Cross-library dependency checks prove the libraries remain independent.
- [ ] The root application package metadata remains unchanged.
- [ ] Any API intentionally exposed for future ACP use is documented as
      pre-1.0 and covered by a focused integration test.

**Verification:**

- `cargo package -p thndrs-agent`
- `cargo package -p thndrs-context`
- `cargo package -p thndrs`
- `cargo package -p thndrs-acp`
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
- `cargo package -p thndrs-acp`

## Frontier

Ticket 1: Establish The Operating Contract can start immediately. Work one
ticket per fresh agent context; a human selects the next ticket after each
verified handoff.
