# Release Checklist And Publication Sequence

This is the release-owner runbook for `thndrs-agent` and `thndrs`. Complete it
in order. Record redacted results as the release evidence.

The installation channels are phased:

| Release | Channel             | Published artifact                                                     |
| ------- | ------------------- | ---------------------------------------------------------------------- |
| v0.1    | [Cargo](cargo.md)   | `thndrs-agent` and `thndrs` on crates.io                               |
| v0.2    | [Homebrew](brew.md) | Tagged binaries and `Casks/thndrs.rb` in `stormlightlabs/homebrew-tap` |
| v0.3    | [Shell](shell.md)   | A versioned installer that uses the v0.2 binary archives and checksums |

Complete the common gate in this document before using a channel checklist. Publishing a crate version is permanent. Never publish from a dirty checkout,
never pass `--allow-dirty` to the real publish command, and never record
registry tokens, credentials, account identifiers, authorization URLs,
callback URLs, or device codes.

## Release Record

- Application version:
- Library version, if changed:
- Candidate revision:
- Release owner:
- Reviewer:
- Date and timezone:
- Platform and architecture:
- Rust and Cargo versions:
- Terminal emulator, font, width, and height:
- CI run:
- Changelog section:

## Quality Assessment: 2026-07-16

**Verdict: strong alpha, not ready to publish.** The architecture and test
discipline are unusually good for a v0.1 project, but two application runtime
boundaries do not yet meet their documented behavior. Do not publish either
crate until the high-severity findings below are fixed and the complete release
gate passes again.

This assessment reviewed revision `dbe61d5` and the preceding failed CI result
for revision `dccd055`. It did not use real provider credentials or publish to
an external registry.

### Release blockers

#### High: `read_url` can send requests to private services

`is_private_url` rejects literal private IP addresses but treats every domain
name as public. `fetch_url` also lets `ureq` follow redirects before it checks
the final URL. A public URL can therefore redirect to a loopback, link-local, or
private address and receive a request before `thndrs` reports the target as
rejected. A domain that resolves to a private address bypasses the string check
without a redirect.

- Evidence: `crates/thndrs/src/core/search.rs:213` classifies
  `url::Host::Domain(_)` as public. `crates/thndrs/src/core/search.rs:262`
  enables automatic redirects, performs the request, and checks the final URL
  only at `crates/thndrs/src/core/search.rs:287`.
- Risk: a model-controlled `read_url` or search result can issue blind requests
  to services on the user's machine or private network. This contradicts the
  tool's public-network contract and the security documentation.
- Required fix: resolve and reject non-global addresses before connecting,
  handle redirects one hop at a time, validate every resolved destination, and
  prevent DNS rebinding between validation and connection. Add deterministic
  tests for a public redirect to loopback and for a hostname that resolves to a
  private address. Disabling `read_url` and result-page fetching for v0.1 is the
  simpler safe fallback.

#### High: shell cancellation owns one process, not the process tree

Foreground and background commands call `Child::kill` on the direct child.
They do not create an isolated Unix process group or a Windows job object. A
command can spawn descendants that survive timeout, cancellation, registry
shutdown, or application exit. In the foreground path, a surviving descendant
can retain the inherited stdout or stderr pipe and keep the reader-thread joins
from returning.

- Evidence: `crates/thndrs/src/core/tools/shell.rs:1160` spawns a plain
  `Command`; `wait_with_timeout` at `crates/thndrs/src/core/tools/shell.rs:889`
  kills only that `Child`; the foreground path joins both pipe readers at
  `crates/thndrs/src/core/tools/shell.rs:1204`. The background control and
  monitor use the same direct-child kill behavior.
- Risk: cancelled build tools, test runners, shells, and language servers can
  continue modifying the workspace or consuming resources. A foreground turn
  can remain stuck after its advertised timeout.
- Required fix: give each command an owned process group or job and terminate
  and reap the group on cancellation, timeout, registry shutdown, and drop. Add
  foreground and background tests in which a direct child starts a descendant
  that inherits the output pipes.

#### Release gate: the exact candidate does not have a completed green CI run

Revision `dccd055` completed its MSRV and dependency-policy jobs, but its stable
quality job failed under Rust 1.97.0. Clippy reported a redundant borrow in a
`format!` argument at `crates/thndrs/src/lib.rs:170`; Rust 1.96.1 did not report
that lint locally. Revision `dbe61d5` removes the borrow, and its workflow was
still running when this assessment was recorded.

- Required gate: require the current workflow to pass on the exact candidate
  revision. If it does not, fix the failure and restart the release review from
  the resulting revision.

### Important follow-up before freezing the library API

`thndrs-agent::AgentRun` says it owns the background thread, but it discards the
`JoinHandle` immediately. Dropping the value neither cancels nor joins the run,
thread panics cannot be observed, and `into_events` consumes the cancellation
handle unless the caller cloned it first.

- Evidence: `crates/thndrs-agent/src/run.rs:21` spawns and detaches the thread;
  the struct stores only a receiver and `CancelToken`.
- Recommendation: decide the lifecycle contract before publishing v0.1. Either
  own completion explicitly or describe this as a detached helper and make the
  cancellation/completion handles difficult to lose. Add drop, panic, and
  receiver-disconnect tests for the chosen contract.

This is an API and maintainability concern rather than a demonstrated bug in
the application, which clones the cancellation token before consuming the
receiver.

### Quality strengths

- The library boundary is genuinely provider-neutral. `thndrs-agent` contains
  typed messages, events, tool contracts, cancellation, accounting, context
  policy, and replay evaluation without application filesystem, terminal, CLI,
  provider client, or wire types.
- Side effects are concentrated in application adapters. File writes use
  stale-hash checks, containment checks, temporary files, synchronization,
  no-clobber creation, and atomic replacement where the platform supports it.
- Tests cover state transitions, serialization, redaction, OAuth recovery,
  provider stream parsing, context accounting, terminal projections, ACP
  transport, tool errors, symlink escapes, concurrent writes, timeouts, and
  cancellation. The live-network tests are explicitly ignored rather than
  silently presented as CI coverage.
- CI defines strict all-target Clippy, workspace tests and doctests, strict
  Rustdoc, public documentation, the declared MSRV, package verification, and
  dependency policy.
- No production `unsafe` block was found. The visible `unsafe` blocks are test
  scaffolding for process-wide environment mutation under Rust 2024.
- Recoverable I/O, provider, parsing, and tool failures generally cross
  `Result` boundaries with useful context. The production `expect` calls found
  guard fixed constants or documented branch invariants.

### Maintainability risks

- All 260 non-merge commits in the available history have one author. This is a
  review and continuity risk even though it says nothing negative about the
  code itself. Require a second human reviewer for both package archives and
  the two runtime-boundary fixes.
- Several core files carry too many responsibilities: `core/agent.rs`,
  `core/auth.rs`, `core/session/mod.rs`, `core/search.rs`, and
  `server/handlers.rs` each exceed 1,000 lines before or including substantial
  inline tests. Split them only along existing domain seams when making related
  changes; a broad pre-release refactor would add risk.
- The dependency graph contains duplicate generations of the HTML parsing
  stack through `lectito` and the direct `scraper` dependency. This is a build
  size and maintenance cost, not a v0.1 correctness blocker. Revisit it when
  search or extraction next changes.
- The documentation build passes but warns that Astro's `markdown.gfm` and
  `markdown.smartypants` settings are deprecated. Move them before the next
  Astro major upgrade.

### Checks run for this assessment

| Check                                                                                | Result                                                                                |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| `cargo fmt --all -- --check`                                                         | Passed                                                                                |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`      | Passed locally with Rust 1.96.1                                                       |
| `cargo test --workspace --all-features --locked`                                     | Passed: 1,570 tests and doctests; 14 live tests ignored                               |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked` | Passed                                                                                |
| `pnpm --dir docs build`                                                              | Passed: 75 pages and all internal links                                               |
| `cargo deny check --all-features --locked`                                           | Not available locally; the remote dependency-policy job passed                        |
| Remote stable quality job                                                            | `dccd055` failed under Rust 1.97.0; the corrected `dbe61d5` run was still in progress |

The two crate names returned no results from `cargo search`, and
`cargo info --registry crates-io` confirmed that neither `thndrs@0.1.0` nor
`thndrs-agent@0.1.0` is currently in the registry index. Name availability is
still first-come, first-served until the initial publishes complete.

## Common Release Gate

Complete this gate for every release before publishing any channel.

### Candidate

- [ ] The application version in `crates/thndrs/Cargo.toml` is correct.
- [ ] If the library changed, its version in
      `crates/thndrs-agent/Cargo.toml` is correct and the application's dependency
      requirement accepts that version.
- [ ] `Cargo.lock`, `CHANGELOG.md`, package READMEs, and public installation
      docs agree with the release.
- [ ] Package metadata points to `https://github.com/stormlightlabs/thunderus`,
      `https://thndrs.stormlightlabs.org`, the correct README, and the Apache-2.0
      license.
- [ ] The public README and site contain installed-user commands, no
      placeholders, no superseded default-model claims, and no source-checkout
      instructions in user workflows.
- [ ] The changelog describes compatibility or migration requirements for any
      public `thndrs-agent` API break.
- [ ] The candidate revision is the revision reviewed by CI and the release
      owner.

### Automated checks

Run from the workspace root:

```sh
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
pnpm --dir docs build
git diff --check
```

- [ ] All checks pass.
- [ ] The working tree contains only the intended release changes after the
      fixing commands.
- [ ] Stable CI, Rust 1.88 MSRV CI, dependency policy, and documentation CI
      pass on the candidate revision.

Record:

- Check results:
- Documentation build result:
- CI result:
- Diff reviewer:

### Installed application acceptance

Use a clean home directory and disposable workspace. Do not perform real
provider checks in a repository that contains work you care about.

- [ ] `thndrs --version` reports the release version.
- [ ] A first run opens required setup before accepting a coding prompt.
- [ ] No provider or model is silently selected on a clean first run.
- [ ] `thndrs setup` provides the equivalent CLI route.
- [ ] `thndrs doctor` and `thndrs doctor --json` provide redacted, actionable
      diagnostics.
- [ ] A rejected credential blocks a coding prompt and names the correct
      `thndrs login <provider>` recovery command.
- [ ] A network, rate-limit, or service failure asks the user to retry rather
      than reporting a valid credential as invalid.
- [ ] Session inspection, export, logs, diagnostics, and prompt inspection do
      not expose credentials or secret-looking values.
- [ ] Normal, narrow, and short terminal layouts remain usable with long paths,
      Unicode, CJK, emoji, and combining marks.
- [ ] Monochrome or reduced-color output remains understandable.
- [ ] The TUI and public docs state that local tools are not an operating-system
      sandbox.

For the human provider gate:

- [ ] ChatGPT Codex browser OAuth, explicit device-code OAuth, cancellation,
      expired or revoked credential recovery, and transient service failure work.
- [ ] Umans hidden credential entry, `UMANS_API_KEY` override behavior,
      rejected-key recovery, and transient service failure work.
- [ ] Each first-class provider completes one bounded coding task, uses approved
      local tools, runs verification, exposes inspectable output, and resumes the
      resulting session.

Record only redacted results:

- Clean-home preparation:
- Setup and diagnostics:
- ChatGPT Codex smoke:
- Umans smoke:
- Terminal review:
- Session and redaction review:

## Failure And Recovery

- A failed dry run or archive review stops the release. Fix the candidate,
  rerun the common gate, and create new evidence.
- A Cargo publish timeout is not proof of failure. Check crates.io and the
  registry index before retrying.
- A broken crate version can be yanked, but it cannot be deleted or overwritten.
  Fix it with a new patch release.
- A bad binary archive or checksum must be replaced by a new patch release. Do
  not silently replace an asset referenced by Homebrew or the installer.
- A bad tap change must be reverted or advanced to a fixed patch release after
  checking whether users can still install the previous version.
- A bad stable installer can point back to the last known-good immutable script;
  preserve the failed version and its incident record.
