# v0.1 Release QA Checklist

Use this checklist for the release candidate and the human publication gate.
Record commands, versions, dates, terminal details, and redacted results. Do
not record credentials, account identifiers, registry tokens, authorization
URLs, callback URLs, device codes, or tool output that contains secrets.

## Candidate Record

- Candidate version:
- Candidate revision:
- Reviewer:
- Date and timezone:
- Platform and architecture:
- Rust and Cargo versions:
- Terminal emulator, font, width, and height:

## Build And Documentation Checks

- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --fix --allow-dirty --allow-staged`
- [ ] `cargo clippy --workspace`
- [ ] `cargo test --workspace`
- [ ] `pnpm --dir docs build`
- [ ] `git diff --check`
- [ ] Public README and site contain installed-user commands, no placeholders,
  no superseded default-model claims, and no source-checkout instructions in
  user workflows.
- [ ] Changelog describes the v0.1 release candidate and the
  `thndrs-agent` v0 compatibility and migration policy.

Record command output locations or a concise redacted result:

- Build/check evidence:
- Documentation build evidence:
- Markdown/diff reviewer:

## Package Archive Review

Run before publishing `thndrs-agent`:

```sh
cargo package -p thndrs-agent --allow-dirty
tar -tzf target/package/thndrs-agent-0.1.0.crate
```

- [ ] Archive includes `Cargo.toml`, `README.md`, `LICENSE`, and intended Rust
  sources.
- [ ] Archive contains no credentials, local sessions, generated build output,
  editor files, or unrelated repository content.
- [ ] `thndrs-agent` README and documentation example describe a
  provider-neutral library boundary.

Record:

- Archive path:
- Archive reviewer:
- Included sources and fixtures reviewed:
- Excluded/generated artifact result:

### `thndrs-agent` pre-publication archive evidence

Prepared on 2026-07-16 by Codex. This is a mechanical and API-boundary review,
not the required human archive review or permission to publish.

- Archive: `target/package/thndrs-agent-0.1.0.crate`
- Contents: 21 files, 176.4 KiB before compression, 43.5 KiB compressed
- Included material: Apache-2.0 license, package metadata, library README, 13
  Rust source files, the context replay benchmark, and its JSON fixture
- Exclusion review: no credentials, `.thndrs` state, logs, editor files, build
  output, application sources, internal planning documents, or unrelated files
- API-boundary review: public modules expose agent contracts, context policy,
  accounting, replay evaluation, hooks, cancellation, and run control. They do
  not expose application session, terminal, CLI, provider client, or provider
  wire types. Path values are inert context metadata; the crate performs no
  filesystem I/O.
- Verification: `cargo package -p thndrs-agent --allow-dirty --locked`
  packaged and compiled successfully; `tar -tzf
  target/package/thndrs-agent-0.1.0.crate` listed the reviewed contents

After `thndrs-agent 0.1.0` is available from crates.io, repeat the archive
review for `thndrs` before approving its publication. Confirm that its README,
license, application sources, intended tests/fixtures, and no generated or
secret material are present.

### Pre-publication `thndrs` archive review

Reviewed on 2026-07-16 by Codex. Cargo assembled the application archive with
a command-local crates.io patch pointing to the workspace copy of
`thndrs-agent`; the package manifest still names the registry dependency. The
normal two-stage package check remains due after `thndrs-agent 0.1.0` is
available from crates.io.

- Archive: `target/package/thndrs-0.1.0.crate`
- Contents: 226 files, 2.8 MiB before compression, 542.4 KiB compressed
- Fixtures: three provider fixtures and two ACP smoke-test fixtures
- Included material: Apache-2.0 license, package metadata, application README,
  Rust sources, snapshots, tests, prompt fragments, and the five fixtures
- Exclusion review: no credentials, `.thndrs` state, logs, editor files, build
  output, internal planning documents, or unrelated repository files

## Clean Install Evidence

Run this only after the publication order makes the application installable:

```sh
cargo install --locked thndrs
thndrs --version
```

- [ ] Use a clean HOME and a disposable workspace.
- [ ] Launch `thndrs` and confirm required setup appears before a coding prompt
  can submit.
- [ ] Confirm no provider or model is silently selected on the fresh install.
- [ ] Confirm `thndrs setup` offers the equivalent CLI route.
- [ ] Confirm `thndrs doctor` and `thndrs doctor --json` provide redacted,
  actionable diagnostics.
- [ ] Confirm a rejected credential blocks a coding prompt and gives the
  appropriate `thndrs login <provider>` recovery action.
- [ ] Confirm a network, rate-limit, or service failure asks the user to retry
  setup instead of calling the credential invalid.

Record:

- Install source and command:
- HOME/workspace preparation:
- Installed version:
- Setup and diagnostic result:

## First-Class Provider Evidence

Use a disposable repository and a real provider account only during the human
release gate. Record the date, selected model, task scope, commands run, and
result without recording account data or credentials.

### ChatGPT Codex

- [ ] Browser-first setup opens or displays a copyable authorization URL and
  completes through the callback or pasted full redirect URL.
- [ ] Explicit headless login uses
  `thndrs login chatgpt-codex --oauth-method device-code`; it is never selected
  automatically.
- [ ] A failed, expired, or cancelled login leaves no credential material in
  session files, logs, prompt inspection, or the transcript.
- [ ] An already-stored expired or revoked OAuth credential blocks setup or the
  first provider request, preserves the prompt draft, and offers `/login
  chatgpt-codex` recovery. A transient OAuth or catalog outage instead asks the
  user to retry and does not call the credential invalid.
- [ ] A bounded coding task can use approved local tools, run verification,
  inspect output, and resume its resulting session.

Record:

- Browser OAuth result:
- Explicit device-code result:
- Stored-credential recovery result:
- Coding task, verification, and session-resume result:

### Umans

- [ ] Setup accepts an Umans credential through hidden input or a process-local
  `UMANS_API_KEY`; no key appears in TOML, diagnostics, sessions, or snapshots.
- [ ] A rejected key gives a redacted recovery path to `thndrs login umans`.
- [ ] A rejected process-local `UMANS_API_KEY` names the environment override
  and tells the user to replace or unset it before login; it does not pretend a
  stored credential will take precedence.
- [ ] A network, rate-limit, or service failure asks the user to retry setup
  rather than replacing a credential that was not rejected.
- [ ] A bounded coding task can use approved local tools, run verification,
  inspect output, and resume its resulting session.

Record:

- Setup and credential result:
- Environment-override recovery result:
- Coding task, verification, and session-resume result:

## Terminal, Safety, And Session Review

- [ ] Review normal, narrow, and short terminal layouts with Unicode,
  long paths, CJK, emoji, and combining marks where available.
- [ ] Review monochrome or reduced-color behavior and confirm labels remain
  understandable.
- [ ] Confirm committed transcript history stays in native terminal scrollback.
- [ ] Confirm setup, permissions, pickers, help, and detail surfaces remain
  usable at narrow dimensions.
- [ ] Confirm the UI and public docs say that local tools are not a sandbox.
- [ ] Confirm session inspection/export and diagnostics redact secret-looking
  values while preserving useful audit metadata.

Record:

- Terminal review result:
- Safety wording reviewer:
- Session and redaction result:

## Publication Order And Approval

- [ ] Publish `thndrs-agent 0.1.0` only after its archive review and direct
  approval.
- [ ] Wait for registry availability, then package and clean-install-test
  `thndrs` against the published dependency.
- [ ] Obtain separate approval before publishing `thndrs 0.1.0`.
- [ ] Tag only after both publications and the recorded checks succeed.
- [ ] Confirm this checklist and its evidence contain no secrets, credentials,
  account identifiers, or registry tokens.

Record:

- `thndrs-agent` publication approval/result:
- Registry availability evidence:
- `thndrs` package/install evidence:
- `thndrs` publication approval/result:
- Tag approval/result:
