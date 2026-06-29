# Testing

## Unit Tests

State transitions, CLI parsing, prompt assembly, tool wrappers, provider request
construction, and parsing logic are covered with unit tests.

## Layout Tests

Layout geometry is tested through pure `compute_view(Rect)` tests. These tests
check sidebar visibility, prompt/footer sizing, and narrow-terminal behavior
without opening a real terminal.

## Snapshot Tests

Ratatui rendering uses `TestBackend` plus `insta` snapshots at fixed terminal
sizes. Snapshots cover empty, submitted, streaming, reasoning, tool, error,
cancelled, banner, and narrow-layout states.

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
