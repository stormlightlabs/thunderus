---
title: "Testing"
---

## Unit Tests

State transitions, CLI parsing, prompt assembly, tool wrappers, provider request
construction, and parsing logic are covered with unit tests.

## Layout Tests

Renderer geometry is tested through pure row-model helpers. These tests check
wrapping, padding, prompt cursor placement, live-region sizing, and
narrow-terminal behavior without opening a real terminal.

## Snapshot Tests

Renderer snapshots use the row model plus `insta` at fixed terminal sizes.
Backend tests assert important terminal escape sequences such as full-screen
clear/purge. Snapshots cover prompt, picker/help, startup, submitted,
streaming, reasoning, tool, error, banner, and narrow-layout states.

## Fixture Driven

### Providers

Provider tests use no-network fixtures for request construction, metadata
parsing, and stream parsing.

## Ignored Live Tests

Provider live smoke tests are ignored by default. OpenCode Zen live tests
require network access and a real `OPENCODE_ZEN_KEY`; their names and failure
messages call out the Big Pickle limited-free and privacy prerequisites.

ChatGPT Codex live tests require network access plus real ChatGPT subscription
credentials. They cover login, streaming, tool calls, and refresh behavior only
when explicitly enabled.

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
