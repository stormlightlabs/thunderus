---
name: implement
description: Work a GitHub issue end to end in an isolated worktree and open a pull request against edge. Use for /impl, /implement, or when asked to start work on an issue number.
---

# Implement

Take one issue, do the work in its own worktree, open a pull request. One issue
per run.

## Read before claiming

The `github-board` skill's Transport section decides whether this run uses `gh`
or the GitHub MCP tools. Use the same transport for everything below.

```sh
gh issue view <n> --json title,body,labels,assignees,url
gh issue view <n> --json subIssues
```

Through MCP: `issue_read` method `get`, then method `get_sub_issues`.

Stop and ask when any of these is true:

- The issue has no acceptance criteria you could verify.
- The issue has sub-issues. It is an epic rather than a unit of work; work its
  sub-issues individually, or run the whole epic with `/thunderstorm`.
- The issue is already `claimed` by someone else.
- The work needs a decision the issue does not record.

Read `CLAUDE.md` and the spec the issue points at. Read the code the change
touches before planning the change.

## Claim it

Assign yourself and move the status label to `claimed`. The claim is what stops
a second run from taking the same issue, so make it before creating a worktree.

## Create the worktree

Outside the repository root, so Cargo does not find the parent
`.cargo/config.toml`:

```sh
git fetch origin
git worktree add ../thndrs-worktrees/<n> -b agent/<n> origin/edge
```

Work only inside that worktree. The user's primary checkout stays untouched.

Each worktree keeps its own `target/`. Do not set a shared `CARGO_TARGET_DIR`:
Cargo locks the build directory and a shared one serializes parallel builds.
Use `RUSTC_WRAPPER=sccache` when it is installed.

## Do the work

Make the smallest change that satisfies the acceptance criteria. Follow the
module order, error policy, and documentation rules in `CLAUDE.md`.

Write the test before or with the change. A change with no test needs a reason
in the pull request body.

Work that turns out to be larger than the issue describes is a signal, not a
license. File the extra work as a new issue and finish what was claimed.

## Verify

Run the narrowest relevant test, then the full gates:

```sh
cargo fmt
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Run `pnpm --dir docs build` when anything under `docs/` changed.

Do not weaken, skip, or delete a test to make a gate pass. If a test is wrong,
say so in the pull request body and explain why.

## Open the pull request

Use the `commits-and-prs` skill for the commit messages and the pull request
body. Base the pull request on `edge`.

Push with `.claude/scripts/push-verified.sh`, which compares the remote ref to
local `HEAD` afterwards. `git push` exits zero for a push that carried nothing,
so its exit code is not evidence that the branch moved.

```sh
gh pr create --base edge --head agent/<n> --title <title> --body-file <file>
# MCP: create_pull_request with base "edge", head "agent/<n>", and the body inline.
```

Move the issue status to `review`.

## Report

State what changed, what you verified and how, what you did not verify, and
anything you filed as a separate issue. Do not claim a check passed without
having run it.

## Do not

- Merge, approve, or push to `edge` or `main`.
- Touch the user's primary checkout.
- Expand scope past the claimed issue.
- Leave the worktree behind after the pull request merges.
