---
name: implement
description: Work a GitHub issue end to end on its own branch and open a pull request against edge. Use for /impl, /implement, or when asked to start work on an issue number.
---

# Implement

Take one issue, do the work on its own branch, open a pull request. One issue
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
- The issue has sub-issues. It is what `/storm` takes rather than a unit of
  work; run it, or work its sub-issues one at a time.
- The issue is already `claimed` by someone else.
- The work needs a decision the issue does not record.

Read `CLAUDE.md` and the spec the issue points at. Read the code the change
touches before planning the change.

## Claim it

Assign yourself and move the status label to `claimed`. The claim is what stops
a second run from taking the same issue, so make it before touching a tree.

## Take a working tree

A dispatched implementer already has one: the run created it and handed over the
directory. Use it and skip this section.

A session invoked directly takes its own tree. The `worktree` skill's **Who gets
one** section says which kind, and is the only thing that creates a worktree.

A cloud session holds a container checkout nobody else owns, so it works in that
checkout and creates nothing beside it:

```sh
git fetch origin
git branch -m agent/<n>
```

Rename the harness-supplied branch rather than keeping it or branching afresh.
Every branch an agent pushes carries the `agent/` prefix, which is what tells a
human reading the branch list what an agent owns. The rename also keeps whatever
that branch already carries: `git switch -c agent/<n> origin/edge` matches it
only while the branch sits at `origin/edge`, and otherwise leaves its commits on
a name this session no longer uses. Harness branches get deleted, so that
residue becomes unreachable rather than untidy.

Where there is no branch to rename, that same `git switch -c` creates one.

A local session takes a worktree, because the checkout there is the user's. It
goes outside the repository root so Cargo does not find the parent
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

A change under `cli/renderer/` or `cli/app/` gets a captured frame:

```sh
cargo build -p thndrs && .claude/scripts/tui-capture.sh <scenario>
```

Show the rendered image in your report. A capture nobody looks at has checked
nothing, so put the frame in front of whoever asked before calling the change
verified, and check any claim you make about it against the `.ansi` beside it. Two of the first three observations made from one
of these images were wrong about what the application drew.
`docs/src/content/docs/docs/development/tui-qa.md` carries the rest.

Do not weaken, skip, or delete a test to make a gate pass. If a test is wrong,
say so in the pull request body and explain why.

## Open the pull request

Base the pull request on `edge`.

The title and body become the squash commit, verbatim. Write them to that:

- Title under **53 characters**, as `<type>: <what changed>`. GitHub appends
  ` (#NN)`, and 59 is the limit.
- Body under **20 lines, wrapped at 72 columns**, with no headings. `## What`
  reaches `git log` as the literal characters `## What`.
- `Closes #<n>`, a `Verified with <command>` line naming a command you ran, and
  a `Not covered` line that is not empty.

`commits-and-prs` carries the rest, and
`.claude/scripts/check-commit-message.py --pr` reports both before the merge.

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
- Touch the user's primary checkout on a development machine.
- Expand scope past the claimed issue.
- Leave a worktree behind after the pull request merges.
