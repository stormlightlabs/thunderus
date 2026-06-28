---
title: Release Scope for fake/v0, alpha, and v1
Author: >
  Ratatui, Pi, Herdr, Gridland, Umans, SemVer, Keep a Changelog, Rust CLI book authors
Date: 2026-06-28
Captured: 2026-06-28
Tags: [release-planning, cli, tui, rust, coding-agent]
---

Source:

- Internal notes in `docs/internal/notes/`
- https://semver.org/
- https://keepachangelog.com/en/1.1.0/
- https://rust-cli.github.io/book/tutorial/testing.html
- https://rust-cli.github.io/book/tutorial/packaging.html
- https://rust-cli.github.io/book/in-depth/config-files.html
- https://rust-cli.github.io/book/in-depth/exit-code.html
- https://rust-cli.github.io/book/in-depth/human-communication.html
- https://rust-cli.github.io/book/in-depth/machine-communication.html

## Summary

The current `thndrs` roadmap is appropriately small for a fake/v0 proof, but a
usable alpha and v1 need explicit scope for provider reliability, sessions,
configuration, safe file changes, packaging, and release hygiene.

## Key Ideas

- **fake/v0 proves the harness, not coding usefulness:** Ratatui, Gridland, and
  Pi notes point to the same first milestone: stable state/update/render flow,
  prompt entry, transcript rendering, fake streaming, and deterministic tests.
- **alpha must be useful on a real repo:** To be usable, alpha needs the Umans
  provider, visible context loading, native web search, read-only local tools,
  guarded file-edit operations, session persistence, and graceful error/stop behavior.
- **v1 defines the supported contract:** SemVer says 1.0.0 defines the public
  API. For a CLI app, that contract is not only Rust APIs; it includes CLI flags,
  config shape, session format, tool behavior, docs, and release notes.
- **Release notes are product surface:** Keep a Changelog argues for notable
  changes grouped by type instead of raw git logs. That matters once users are
  expected to upgrade.
- **CLI apps need external behavior tests:** The Rust CLI book recommends unit
  tests for core logic and integration tests for user-observable behavior.

## Claims & Evidence

| Claim                                                     | Support                                                                                                         | Caveat / Confidence                                                      |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Major version zero is allowed to change freely.           | SemVer says `0.y.z` is for initial development and the public API should not be considered stable.              | High. Good fit for fake/v0 and alpha.                                    |
| `1.0.0` means the public API is defined.                  | SemVer says version 1.0.0 defines the public API.                                                               | High. We must define what "public API" means for a TUI CLI.              |
| Pre-release identifiers communicate instability.          | SemVer allows labels like `alpha` and `rc`; pre-release versions have lower precedence than the normal release. | High. Use this to distinguish fake/v0, alpha, and v1 release candidates. |
| A CLI should be tested through both units and the binary. | Rust CLI book separates unit tests from black-box integration tests under `tests/`.                             | High.                                                                    |
| Human-facing output should be consistent and clear.       | Rust CLI book recommends concise progress/error messages and consistent severity/log levels.                    | High; applies to transcript errors and non-TUI commands.                 |
| Machine-facing output should use parseable formats.       | Rust CLI book recommends JSON/line-delimited JSON when output is consumed by other programs.                    | Medium for v1; useful for `--print-events` or export later, not fake/v0. |
| Packaging needs metadata and install path decisions.      | Rust CLI book calls out Cargo metadata, `cargo install`, and binary distribution tradeoffs.                     | High for v1.                                                             |

## Important Terms

| Term              | Meaning                                                                                                                   |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------- |
| fake/v0           | First runnable harness with fake agent stream; validates UI, state, and tests.                                            |
| alpha             | Usable but unstable release: real provider, real sessions, read-only tools, guarded file edits, known rough edges.        |
| v1                | Stable supported release: clear CLI/config/session/tool contracts, docs, packaging, and release process.                  |
| public API        | For this app: CLI flags, config file keys, env vars, session/event formats, tool behavior, and documented user workflows. |
| release candidate | A pre-release build intended to become v1 if no blocking issues are found.                                                |
| changelog         | Human-authored record of notable changes grouped by categories such as Added, Changed, Fixed, Security.                   |

## Questions for Review

- What is the smallest fake/v0 that proves the Ratatui harness without pretending
  to be a coding agent?
- What must alpha do on a real project for us to call it usable?
- Which CLI/config/session contracts are stable enough for v1?
- Which features sound attractive but should stay out until after v1?

## Connections

- Related ideas: Pi's visible context and event stream; Herdr's durable session
  discipline; Gridland's chat layout; Ratatui snapshot tests; Umans as first provider.
- Related sources: [pi](pi.md), [herdr](herdr.md), [ui-patterns](ui-patterns.md),
  [umans](providers/umans.md), [ratatui-testing](ratatui-testing.md).
- Contradictions or tensions: alpha/v1 need write/edit tools to be a coding harness,
  but the project guidance strongly prefers avoiding permission theater. The
  compromise is simple explicit confirmation plus narrow file operations, not a
  complex policy engine.
- Useful applications: Release gates in `ROADMAP.md`, actionable grouped tasks
  in `TODO.md`, and future `CHANGELOG.md`/packaging checklist.

## Open Questions

- Which guarded file operations are enough for alpha before richer editing exists?
- Should session JSONL be considered stable at v1 or documented as internal?
- Should local Lectito fallback search be alpha or v1 if Umans native search is
  reliable enough?
- What install channel is the first v1 target: `cargo install`, GitHub release
  binaries, or both?

## Takeaways

- The existing roadmap is enough for fake/v0, not enough for v1.
- Alpha should mean "real provider and real repo use," not "all planned features."
- v1 should be defined by stable contracts, safe file operations, packaging,
  documentation, and a release process.
