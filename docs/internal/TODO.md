# Parking Lot

## RR-1: Make workspace writes atomic

**What to build:** Make `create_file`, `replace_range`, and every `write_patch`
operation preserve the previous target when validation or writing fails. The
implemented behavior must match the public unchanged-on-failure guarantee.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] Edits and replacements write to a same-directory temporary file and only
      replace the target after the complete content has been written and closed.
- [x] Existing file permissions are preserved where the platform exposes them.
- [x] Creation remains no-clobber: it never overwrites a target that appeared
      after validation.
- [x] Failed writes do not leave a partial target or a stale temporary file.
- [x] Stale-hash, duplicate-match, overlap, symlink-containment, and concurrent
      cooperating-writer behavior remain intact.
- [x] Deterministic failure-injection tests prove that the previous bytes survive
      a failed write.
- [x] Public tool and safety documentation describes the guarantees that the
      implementation actually provides on each supported platform.

**Verification:**

- `cargo test -p thndrs tools::create_file`
- `cargo test -p thndrs tools::replace_range`
- `cargo test -p thndrs tools::write_patch`
- `cargo clippy -p thndrs --all-targets --all-features -- -D warnings`

## RR-2: Implement real background process ownership

**What to build:** Make `run_shell` return promptly for a background command and
keep the actual running child under application ownership until it exits or is
cancelled.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] `background: true` does not wait for the child to exit before returning a
      successful tool result.
- [x] The process registry contains the live child, its real cancellation
      handle, start time, command, working directory, and bounded output state.
- [x] `:bg` lists only processes that are still running and removes or marks a
      process when it exits.
- [x] Cancelling one process and quitting the application terminate and reap the
      owned children without leaving zombies or detached commands.
- [x] Foreground timeout, output caps, redaction, cancellation, and session audit
      behavior remain unchanged.
- [x] Session records distinguish a background process starting, exiting,
      failing, timing out, and being cancelled.
- [x] Tests use a genuinely long-running child and prove that the agent remains
      responsive while it runs.

**Verification:**

- `cargo test -p thndrs tools::shell`
- `cargo test -p thndrs background_process`
- Manual TUI check: start, list, cancel, and observe a background process.

## RR-3: Make ACP field caps UTF-8 safe

**What to build:** Cap ACP tool input, output, and content without slicing a
string between UTF-8 code units.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] The cap helper truncates by a valid UTF-8 boundary and preserves the
      existing byte bound and truncation marker.
- [x] Long ASCII, CJK, emoji, combining-mark, and mixed strings never panic.
- [x] Raw tool input, raw output, and text content all use the safe helper.
- [x] Redaction still happens before capped data reaches the transcript or
      session path.
- [x] Regression tests place multibyte characters directly across the cap
      boundary.

**Verification:**

- `cargo test -p thndrs acp`
- `cargo test -p thndrs maps_tool`
- `cargo clippy -p thndrs --all-targets --all-features -- -D warnings`

## RR-4: Align clean-install diagnostics with onboarding

**What to build:** Make `doctor` describe an unset model as incomplete setup
instead of silently diagnosing Umans, and direct users to a working support URL.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] With an empty model, `doctor` reports no selected provider and does not
      claim that `UMANS_API_KEY` is the blocking credential.
- [x] The setup hint asks the user to choose a provider through `thndrs setup`.
- [x] Once a model is selected, provider-specific credential diagnostics behave
      as they do today.
- [x] Human and JSON reports use the same state and remain free of credential
      values.
- [x] The reported documentation or support URL returns a successful response
      and matches the public site or repository URL.
- [x] The installation requirements and `doctor` output agree about whether
      `rg` and `fd` are required, optional, or replaceable by a fallback.

**Verification:**

- `cargo test -p thndrs cli::commands::doctor`
- Run `thndrs doctor --json` with a clean `HOME` and disposable workspace.
- Check the reported support URL with an HTTP HEAD request.

## RR-5: Fix the application crate archive

**What to build:** Make the `thndrs` crate archive complete and ready for the
two-stage crates.io publication flow.

**Blocked by:** None - archive metadata and contents can be fixed immediately;
final package verification remains blocked until `thndrs-agent 0.1.0` is
available from crates.io.

**Acceptance criteria:**

- [x] The archive includes the Apache-2.0 license, `Cargo.toml`, application
      README, intended sources, tests, and fixtures.
- [x] The archive excludes credentials, `.thndrs` state, logs, editor files,
      build output, internal planning documents, and unrelated repository files.
- [x] The packaged README's links and installation instructions work for a
      crates.io user rather than relying on the workspace root.
- [x] Package metadata, README wording, and the changelog consistently label the
      application as experimental pre-1.0 software.
- [x] Package review records file count, compressed size, included fixtures, and
      the reviewer without recording secrets.

**Verification:**

- `cargo package -p thndrs --allow-dirty --list`
- After `thndrs-agent 0.1.0` is published: `cargo package -p thndrs --allow-dirty`
- `tar -tzf target/package/thndrs-0.1.0.crate`

## RR-6: Make strict lint and API documentation checks green

**What to build:** Remove the current all-target Clippy warnings and Rustdoc
errors so strict checks can be used as release gates.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] Test-only redundant clones, boolean assertion style, and needless
      collections are cleaned up without weakening tests.
- [x] Public documentation contains no broken, private, stale, or invalid HTML
      links.
- [x] Names in module documentation match current types and functions, including
      cancellation and provider-lowering terminology.
- [x] The Git status watcher test is deterministic under the normal suite and no
      longer ignored as flaky.
- [x] Normal and strict Clippy and Rustdoc commands pass on the declared MSRV as
      well as the current stable toolchain.

**Verification:**

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
- `cargo test --workspace --all-features`

## RR-7: Add continuous release checks

**What to build:** Add CI that runs the checks required to keep the alpha
installable and catches regressions in both crates before merge.

**Blocked by:** RR-6: Make strict lint and API documentation checks green

**Acceptance criteria:**

- [x] CI runs formatting, strict all-target Clippy, workspace tests, strict
      Rustdoc, documentation build, and `git diff --check`.
- [x] CI tests the declared MSRV and current stable Rust without unnecessarily
      duplicating the full matrix.
- [x] `thndrs-agent` package verification and archive-content checks run without
      publishing.
- [x] The workflow checks for vulnerable or disallowed dependencies and records
      the chosen policy and tool in the repository.
- [x] Live provider tests remain explicit, credential-free manual gates rather
      than silently skipped CI assurances.
- [x] CI uses the lockfiles and does not modify source or accept snapshots.

**Verification:**

- Run the workflow on a branch with all jobs passing.
- Prove each job fails when its corresponding check is deliberately broken.

## RR-8: Finalize the `thndrs-agent` 0.1 release contract

**What to build:** Finish the provider-neutral library's public release contract
and record the evidence needed for a separate publication decision.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] Public API breaks receive migration notes in the changelog when they are
      introduced; public docs do not carry a speculative compatibility notice.
- [x] Public APIs remain provider-neutral and contain no application filesystem,
      session, terminal, CLI, or provider wire types.
- [x] The documented consumer example compiles as a doctest or package example.
- [x] Strict Clippy, strict Rustdoc, unit tests, doctests, and package
      verification pass independently of the application crate.
- [x] A human reviews the archive's license, README, fixture, benchmark, and Rust
      sources and records the result in the release evidence.
- [x] Publication remains a separate, explicit owner decision.

**Verification:**

- `cargo clippy -p thndrs-agent --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc -p thndrs-agent --all-features --no-deps`
- `cargo test -p thndrs-agent --all-features`
- `cargo package -p thndrs-agent --allow-dirty`

## RR-9: Prove the registry-to-clean-install path

**What to build:** Exercise the exact publication order and prove that a user can
install the application from crates.io without a source checkout.

**Blocked by:** RR-5: Fix the application crate archive; RR-7: Add continuous
release checks; RR-8: Finalize the `thndrs-agent` 0.1 release contract; explicit
owner approval to publish `thndrs-agent 0.1.0`.

**Acceptance criteria:**

- [ ] Publish `thndrs-agent 0.1.0` only after direct approval and verify registry
      availability and docs.rs output.
- [ ] Package `thndrs` against the registry dependency and review its final
      archive before requesting application publication approval.
- [ ] Install `thndrs` with `cargo install --locked thndrs` under a clean `HOME`.
- [ ] `thndrs --version`, first-run provider choice, CLI setup, `doctor`, config
      inspection, and empty session listing work from the installed binary.
- [ ] The clean first run does not assume a provider or model and does not write
      credential material before authentication succeeds.
- [ ] Record versions, revision, platform, architecture, Rust/Cargo versions,
      commands, and redacted results in the release evidence.
- [ ] Publishing `thndrs` and creating a tag remain separate approval steps.

**Verification:**

- `cargo install --locked thndrs`
- `thndrs --version`
- Clean-`HOME` first-run and CLI smoke from a disposable workspace.

## RR-10: Execute real-provider and terminal release smokes

**What to build:** Complete the human checks that deterministic tests cannot
cover: current provider authentication, one bounded coding task per first-class
provider, session recovery, and real-terminal behavior.

**Blocked by:** RR-1: Make workspace writes atomic; RR-2: Implement real
background process ownership; RR-3: Make ACP field caps UTF-8 safe; RR-4: Align
clean-install diagnostics with onboarding; RR-9: Prove the registry-to-clean-install
path.

**Acceptance criteria:**

- [ ] ChatGPT Codex browser OAuth, explicit device-code OAuth, cancellation,
      expired/revoked credential recovery, and transient service failure are
      exercised without recording tokens, account identifiers, or OAuth URLs.
- [ ] Umans hidden credential entry, environment override behavior, rejected-key
      recovery, and transient service failure are exercised without recording
      credentials.
- [ ] Each provider completes a bounded edit, uses local tools, runs verification,
      exposes inspectable output, and resumes the resulting session.
- [ ] Session inspection, export, logs, diagnostics, and prompt inspection are
      reviewed for secret leakage.
- [ ] Normal, narrow, short, Unicode, CJK, emoji, combining-mark, long-path,
      monochrome, setup, picker, permission, help, and detail surfaces are
      reviewed in a real terminal.
- [ ] Known provider or terminal limitations are added to public documentation
      before approval.

**Verification:**

- Complete and sign off the applicable sections of `docs/internal/qa/README.md`
  and its channel checklists.
- Run the ignored live tests individually only with the required account and
  privacy prerequisites.

## RR-11: Approve or reject the public alpha candidate

**What to build:** Produce one complete release evidence packet and make an
explicit go/no-go decision for `thndrs 0.1.0`.

**Blocked by:** RR-1 through RR-10

**Acceptance criteria:**

- [ ] Every preceding ticket has passing verification evidence or a documented,
      owner-approved alpha limitation that is accurate in public documentation.
- [ ] The release checklist contains the candidate revision, environment,
      archive reviews, clean install, provider smokes, terminal review, and
      redacted results.
- [ ] The changelog describes the shipped application, `thndrs-agent` contract,
      known limitations, and migration expectations without stale provider or
      default-model claims.
- [ ] The owner separately approves application publication and tagging.
- [ ] The evidence packet and repository contain no credential, token, account
      identifier, authorization URL, callback URL, or registry secret.

**Verification:**

- Re-run the complete command list in `docs/internal/qa/README.md` from the approved
  candidate revision.
- Review the final crates.io pages, docs site, repository links, and release
  notes after publication.
