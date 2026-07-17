---
title: "Testing"
---

## Unit Tests

State transitions, CLI parsing, prompt assembly, tool wrappers, provider request
construction, and parsing logic are covered with unit tests.

## Layout

Renderer geometry is tested through pure row-model helpers. These tests check
wrapping, padding, prompt cursor placement, live-region sizing, and
narrow-terminal behavior without opening a real terminal.

## Snapshots

Renderer snapshots use the row model plus `insta` at fixed terminal sizes.
Backend tests assert important terminal escape sequences such as full-screen
clear/purge.

Snapshots cover prompt, picker/help, setup, permissions, detail surfaces,
startup, submitted, streaming, reasoning, tool, error, banner, and narrow-layout
states.

The renderer suite also exercises normal, tiny-height, Unicode, long-line, clipping,
and monochrome-equivalent fallbacks, including surface priority and secret masking.

## Fixtures

### Providers

Provider tests use no-network fixtures for request construction, metadata
parsing, and stream parsing.

### Context replay

Context replay fixtures live in the `thndrs-agent` under `fixtures/context-replay/`.
They are versioned JSON and keep required facts separate from projection prose.

If you're adding a fixture, it should:

1. use `schema_version: "context-replay-v1"`;
2. keep item order stable and give every item a unique id;
3. attach stable `fact_ids` to evidence items and list their descriptions in
   `required_facts`;
4. add recovery cases for every artifact handle whose availability matters;
5. add a `ReplayScenario` value for each adversarial case it covers; and
6. record provider usage only when a real provider fixture supplies it.

Run the deterministic evaluator tests with:

```sh
cargo test -p thndrs-agent replay
```

Run the Divan measurements with:

```sh
cargo bench -p thndrs-agent --bench context_projection
```

Benchmarks measure selection, projection, receipts, and report export. They do
not decide whether a candidate is correct: the evaluator fails before a report
is produced when a required fact or expected recovery outcome is lost.

## Ignored Live Tests

Provider live smoke tests are ignored by default. OpenCode Zen live tests
require network access and a real `OPENCODE_ZEN_KEY`; their names and failure
messages call out the Big Pickle limited-free and privacy prerequisites.

ChatGPT Codex live tests require network access plus real ChatGPT subscription
credentials. They cover login, streaming, tool calls, and refresh behavior only
when explicitly enabled.

CI never injects provider credentials or runs ignored tests. A release owner
runs each applicable live test by its full name as a manual release gate and
records only redacted results in `docs/internal/qa.md`.

## Continuous Integration

The CI workflow runs the full formatting, strict Clippy, test, strict Rustdoc,
and public documentation gates on stable Rust. A separate Rust 1.88 job runs
`cargo check` across all targets and features so the MSRV does not duplicate
the full stable suite.

The stable job also verifies the `thndrs-agent` package and prints its archive
contents. The package manifest's `include` list is the archive allowlist.
`cargo-deny` checks RustSec advisories, licenses, duplicate and wildcard
dependencies, and dependency sources using the policy in `deny.toml`. All Cargo
and pnpm commands use their committed lockfiles. CI fails if a check changes
tracked files; it never accepts snapshots.

### Search

Search and extraction tests use local fixtures before any live web behavior.

## cargo-insta Workflow

Run snapshot tests:

```sh
cargo insta test
```

Review changed snapshots:

```sh
cargo insta review
```

Accept intentional changes:

```sh
cargo insta accept
```
