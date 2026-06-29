# Development Workflow

## Formatting

Use `cargo fmt` before submitting code changes.

### Testing

Run the full test suite:

```sh
cargo test
```

For snapshot-specific work, run:

```sh
cargo insta test
```

## Snapshots

Use `cargo insta review` to inspect changed TUI snapshots and accept only
intentional visual changes.

## Debugging

Use `--print-prompt` to inspect prompt assembly without making a provider call.

## Checks

### TUI

For UI work, check both normal and narrow snapshot states. When possible, also
run the TUI in a real terminal to inspect color and spacing.

### Release

Before release work, run formatting, tests, snapshots, and package checks.
