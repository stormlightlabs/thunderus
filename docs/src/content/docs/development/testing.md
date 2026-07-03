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
