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

## Maintainability risks

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

### TUI acceptance

- [ ] Named full-frame fixtures cover startup, ordinary conversation,
      streaming, running and settled tool groups, diffs, successful checks,
      failures, multiline input, and focused details.
- [ ] Review the representative snapshot matrix at 80×24, 120×32, 160×40,
      and one cramped size. Review a smaller subset across every built-in theme
      for semantic contrast.
- [ ] In a real terminal, inspect hierarchy, content rails, spacing,
      truncation, cursor placement, scrolling, selection, overlay transitions,
      and resize behavior.
- [ ] Confirm active and settled states remain distinguishable without color.
- [ ] Redraw and stale-cell regressions remain covered. Add a focused
      regression when the candidate changes one of those state boundaries.

Record:

- Snapshot review:
- Cross-theme review:
- Terminal and resize review:
- Redraw and stale-cell result:

### Installed application acceptance

Use a clean home directory and disposable workspace. Do not perform real
provider checks in a repository that contains work you care about.

- [ ] `thndrs --version` reports the release version.
- [ ] A first run opens required setup before accepting a coding prompt.
- [ ] No provider or model is silently selected on a clean first run.
- [ ] `thndrs setup` provides the equivalent CLI route.
- [ ] `thndrs doctor` and `thndrs doctor --json` provide redacted, actionable
      diagnostics.
- [ ] A rejected credential preserves the coding prompt and opens focused
      in-app sign-in recovery for the active provider.
- [ ] A rejected environment credential names the variable that takes
      precedence and explains that replacing or unsetting it requires a restart.
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
- [ ] OpenCode Zen and OpenCode Go credential entry, environment overrides,
      rejected-key recovery, and transient service failure work.
- [ ] Each first-class provider completes one bounded coding task, uses approved
      local tools, runs verification, exposes inspectable output, and resumes the
      resulting session.

Record only redacted results:

- Clean-home preparation:
- Setup and diagnostics:
- ChatGPT Codex smoke:
- OpenCode Zen smoke:
- OpenCode Go smoke:
- Terminal review:
- Session and redaction review:

## Failure And Recovery

- A failed dry run or archive review stops the release.
  - Fix the candidate, rerun the common gate, and create new evidence.
- A Cargo publish timeout is not proof of failure.
  - Check crates.io and the registry index before retrying.
- A broken crate version can be yanked, but it cannot be deleted or overwritten.
  Fix it with a new patch release.
- A bad binary archive or checksum must be replaced by a new patch release.
  - Do not silently replace an asset referenced by Homebrew or the installer.
- A bad tap change must be reverted or advanced to a fixed patch release after
  checking whether users can still install the previous version.
- A bad stable installer can point back to the last known-good immutable script;
  preserve the failed version and its incident record.
