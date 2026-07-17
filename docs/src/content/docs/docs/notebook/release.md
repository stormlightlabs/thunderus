---
title: "Release Scope Concepts for a Terminal Coding Harness"
Author: >
  Ratatui, Pi, Herdr, Gridland, Umans, SemVer, Keep a Changelog, Rust CLI book authors
Date: 2026-06-28
Captured: 2026-06-28
Tags: [release-planning, cli, tui, rust, coding-agent]
sources:
  - Internal notes in `docs/internal/notes/`
  - https://semver.org/
  - https://keepachangelog.com/en/1.1.0/
  - https://rust-cli.github.io/book/
---

## Summary

Release scope for a terminal coding harness should be defined by user-visible
contracts: provider reliability, sessions, configuration, file-change safety,
packaging, docs, and release hygiene. The exact milestone labels can change, but
each release should state what behavior is experimental and what users can rely
on.

## Key Ideas

- **Early releases prove the harness shape:** Ratatui, Gridland, and Pi notes
  point to the same early milestone: stable state/update/render flow, prompt
  entry, transcript rendering, streaming behavior, and deterministic tests.
- **Usable prereleases must work on real repos:** A usable prerelease needs a
  real provider, visible context loading, bounded web/search behavior, local
  tools, guarded file-edit operations, session persistence, and graceful
  error/stop behavior.
- **v1 defines the supported contract:** SemVer says 1.0.0 defines the public
  API. For a CLI app, that contract is not only Rust APIs; it includes CLI flags,
  config shape, session format, tool behavior, docs, and release notes.
- **Release notes are product surface:** Keep a Changelog argues for notable
  changes grouped by type instead of raw git logs. That matters once users are
  expected to upgrade.
- **CLI apps need external behavior tests:** The Rust CLI book recommends unit
  tests for core logic and integration tests for user-observable behavior.

## Claims & Evidence

| Claim                                                     | Support                                                                                                         | Caveat / Confidence                                                         |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Major version zero is allowed to change freely.           | SemVer says `0.y.z` is for initial development and the public API should not be considered stable.              | High. Good fit for experimental prereleases.                                |
| `1.0.0` means the public API is defined.                  | SemVer says version 1.0.0 defines the public API.                                                               | High. We must define what "public API" means for a TUI CLI.                 |
| Pre-release identifiers communicate instability.          | SemVer allows labels like `alpha` and `rc`; pre-release versions have lower precedence than the normal release. | High. Use this to distinguish unstable prereleases from release candidates. |
| A CLI should be tested through both units and the binary. | Rust CLI book separates unit tests from black-box integration tests under `tests/`.                             | High.                                                                       |
| Human-facing output should be consistent and clear.       | Rust CLI book recommends concise progress/error messages and consistent severity/log levels.                    | High; applies to transcript errors and non-TUI commands.                    |
| Machine-facing output should use parseable formats.       | Rust CLI book recommends JSON/line-delimited JSON when output is consumed by other programs.                    | Medium; only needed for documented machine-facing commands or exports.      |
| Packaging needs metadata and install path decisions.      | Rust CLI book calls out Cargo metadata, `cargo install`, and binary distribution tradeoffs.                     | High for v1.                                                                |

## Important Terms

| Term               | Meaning                                                                                                                   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| experimental build | Runnable harness that validates UI, state, provider/tool loops, and tests without promising stable contracts.             |
| alpha/prerelease   | Usable but unstable release: real provider, sessions, local tools, guarded file edits, and known rough edges.             |
| v1                 | Stable supported release: clear CLI/config/session/tool contracts, docs, packaging, and release process.                  |
| public API         | For this app: CLI flags, config file keys, env vars, session/event formats, tool behavior, and documented user workflows. |
| release candidate  | A pre-release build intended to become v1 if no blocking issues are found.                                                |
| changelog          | Human-authored record of notable changes grouped by categories such as Added, Changed, Fixed, Security.                   |

## Questions for Review

- What must an experimental build prove before it is useful to dogfood?
  - Require a stable prompt loop, visible transcript, deterministic tests, and graceful failure handling before dogfooding.
- What must a prerelease do on a real project before users can call it usable?
  - Require real provider calls, context loading, local tools, session audit, file-change safety, and clear error recovery.
- Which CLI/config/session contracts are stable enough for v1?
  - Stabilize only contracts that are documented, covered by tests, and unlikely to need shape changes after real usage.
- Which features sound attractive but should stay out until after v1?
  - Defer features that add new subsystems, hidden orchestration, or unclear safety semantics without improving the core coding loop.

## Connections

- Related ideas: Pi's visible context and event stream; Herdr's durable session
  discipline; Gridland's chat layout; Ratatui snapshot tests; provider
  reliability.
- Related sources: [pi](/docs/notebook/pi/), [herdr](/docs/notebook/herdr/), [ui-patterns](/docs/notebook/ui-patterns/),
  [umans](/docs/notebook/providers/umans/), [ratatui-testing](/docs/notebook/ratatui-testing/).
- Contradictions or tensions: prereleases need write/edit tools to be useful
  coding harnesses, but permission prompts are not a real sandbox. The
  conceptual compromise is clear local execution, narrow file operations, audit
  records, and external sandboxing when isolation matters.
- Conceptual uses: release gates, changelog categories, packaging checklist,
  machine-readable output policy, and v1 contract definition.

## Open Questions

- What should define release readiness for a terminal coding harness?
  - **Recommendation**: Define readiness by documented user-visible contracts: CLI/config shape, session/tool behavior, provider reliability, file-change safety, packaging, docs, and changelog discipline.
- Which guarded file operations are enough before richer editing exists?
  - **Recommendation**: Support the smallest auditable write operations that cover normal edits before adding richer patch or refactor tools.
- Should session JSONL be considered stable at v1 or documented as internal?
  - **Recommendation**: Treat only documented record fields as stable and reserve internal fields for migration until real resume/export workflows mature.
- Should local fallback search be part of the supported contract or treated as
  best-effort assistance?
  - **Recommendation**: Document fallback search as best-effort unless it has reliable tests, limits, and error behavior across expected environments.
- What install channel is the first v1 target: `cargo install`, GitHub release
  binaries, or both?
  - **Recommendation**: Start with the lowest-maintenance channel users already expect, then add binaries once release automation is repeatable.
