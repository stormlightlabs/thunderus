---
name: release
description: Cut a release from edge to main, tag it, and publish. Use when asked to cut a release, ship a version, tag, or publish thndrs to crates.io.
---

# Release

A release is one reviewed pull request from `edge` to `main`, a tag on `main`,
and a publish. `main` matches the released tag between releases.

Every step here is outward-facing. Confirm with the user before the pull
request, before the tag, and before the publish. Approval for one is not
approval for the next.

## Preconditions

Stop unless all of these hold:

- `edge` is green on the required checks.
- No issue sits at `status:review` with an open pull request meant for this
  release.
- The working tree is clean. `cargo package` refuses a dirty tree, and forcing
  past that publishes files nobody reviewed.

```sh
gh run list --branch edge --limit 1
git status --short
```

Through MCP: `actions_list` method `list_workflow_runs` with
`workflow_runs_filter.branch` set to `edge`.

## Version

Both crates share a version. Decide it from the changes since the last tag, not
from habit:

| Change                                          | Bump    |
| ----------------------------------------------- | ------- |
| Breaking API or behavior change before 1.0      | minor   |
| New behavior, backward compatible               | minor   |
| Fixes and internal work only                    | patch   |

```sh
git log --oneline "$(git describe --tags --abbrev=0)"..edge
```

Update `version` in `crates/thndrs/Cargo.toml` and
`crates/thndrs-agent/Cargo.toml`, then `cargo update --workspace` so
`Cargo.lock` matches.

## Changelog

Move `## Unreleased` entries into a new `## <version>` section with the date.
Leave an empty `## Unreleased` behind.

Write for a user of `thndrs`: name the behavior that changed, not the module.
Use the `writing-docs` skill.

## Verify

Run what CI runs, before asking for the pull request:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
pnpm --dir docs build
cargo package -p thndrs-agent --locked
```

## Pull request

```sh
gh pr create --base main --head edge --title "release: v<version>" --body-file <file>
# MCP: create_pull_request with base "main", head "edge", and the body inline.
```

The body lists the user-visible changes and names the verification that ran.
This is where a human reads the accumulated `edge` diff at outcome level, so it
is the last point at which the release can be stopped cheaply.

A human merges it. Do not merge this pull request.

## Tag and publish

After the merge, from `main`:

```sh
git checkout main && git pull
git tag "v<version>"
git push origin "v<version>"
```

Publish the library before the binary, because the binary depends on it:

```sh
cargo publish -p thndrs-agent --locked
cargo publish -p thndrs --locked
```

A publish cannot be undone. A version number cannot be reused. Confirm the
version and the tag before running either command.

## After

Move every issue at `status:verify` that shipped in this release to
`status:done` through the `github-board` skill.

Confirm `main`, the tag, and the published version all name the same version. If
they disagree, say so rather than reconciling quietly.
