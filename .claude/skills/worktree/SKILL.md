---
name: worktree
description: Decide whether a unit of agent work needs its own git worktree, then create, use, and remove it with Rust build isolation. Use when starting work on an issue, running parallel agents, or cleaning up after a run.
---

# Worktree

One unit of work gets one branch and one owner. Whether it also gets its own
worktree is decided under [Who gets one](#who-gets-one), and this skill is the
only thing in the repository that makes one. Two other things can: an
`isolation` key in a definition under `.claude/agents/`, and an `isolation`
setting on a dispatch. Both place the worktree inside the repository root, so
this repository uses neither.

## Who gets one

What a worktree separates is one writer from the next, so the question is
whether this work has a next writer.

| Writer                    | Tree                                          |
| ------------------------- | --------------------------------------------- |
| A dispatched subagent     | Its own worktree, made before it is sent.     |
| A local session, directly | Its own worktree. The checkout is the user's. |
| A cloud session, directly | A branch in the container checkout.           |

A subagent always has one. The session that dispatched it still sits in the
checkout, and it can send a second subagent while the first works. So a subagent
is never the only writer, even when it is the only implementer.

Two of them in one checkout share one index and one `HEAD`, so neither can hold
a branch. The second to create its branch moves `HEAD` for both. The first's
staged work lands in the second's commit, its later commits land on the second's
branch, and its own branch never leaves `origin/edge`. Git refuses one branch in
two worktrees, but that refusal needs two worktrees to fire.

The run creates it before dispatching rather than leaving the subagent to. A
subagent making its own would place it relative to whatever directory it started
in, and that directory is recorded nowhere the run can read afterwards.

`.claude/.gitignore` catches a worktree that lands at `.claude/worktrees/`
anyway, so the checkout stays clean. Nothing catches the rest: the key, the
dispatch setting, and the directory a subagent writes into are rules a reader
has to follow. Delete this paragraph for whichever of them a check later covers.

A session working an issue itself on a development machine takes one too. The
checkout there is the user's working tree and an agent is never its writer.

The one case with no worktree is a cloud session working an issue itself, where
the container checkout belongs to nobody else. A second tree there costs three
things and buys nothing. It is untracked inside the repository root, so the
checkout reads dirty. The stop hook then asks for a locked second checkout to be
committed. Cargo can reach the parent `.cargo/config.toml` and build into the
parent `target/`. That session still owns its branch: rename a harness-supplied
name to `agent/<issue>` before the first push.

A reviewer needs no worktree, only a tree that does not move while it reads,
which is a commit. The `review` skill owns how to read one.

## Create

Always outside the repository root. A worktree created inside it inherits the
parent `.cargo/config.toml`, and Cargo can then build into the parent `target/`:

```sh
git fetch origin
git worktree add ../thndrs-worktrees/<issue> -b agent/<issue> origin/edge
```

Branch from `origin/edge`, not from whatever the user has checked out. The
baseline should match the branch the pull request will target.

Work with no issue behind it still branches under `agent/`, named for the work:
`agent/git-hygiene`. A worktree made here is born with the right name and needs
no rename.

## Build isolation

Each worktree keeps its own `target/`. Do not set a shared `CARGO_TARGET_DIR`.
Cargo takes an exclusive lock on the build directory, so a shared one serializes
the builds it was supposed to parallelize, and differing feature resolution
across branches invalidates the shared incremental state.

Share compilation through a content-addressed cache instead, when it is
installed:

```sh
command -v sccache >/dev/null && export RUSTC_WRAPPER=sccache
```

Symptoms of a shared-target misconfiguration look like compiler bugs: builds
that restart from scratch, test binaries that vanish between runs, and errors
that do not reproduce. Check target isolation before anything else.

## Provisioning

Untracked files do not follow a worktree. Copy only what the build needs, by
name:

```sh
for file in .thndrs/config.toml; do
  test -f "$file" && install -D "$file" "../thndrs-worktrees/<issue>/$file"
done
```

Never copy ignored files as a group. That is how credentials end up in
directories nobody is tracking.

## Concurrency

Cap concurrent worktrees at 2 on a development machine. Parallel Rust builds,
language servers, and test processes exhaust memory before they exhaust CPU.
Raise the cap only after measuring.

## Remove

Removal is part of the run, not cleanup for later:

```sh
git worktree remove ../thndrs-worktrees/<issue>
git branch -d agent/<issue>   # a run that took no worktree runs this line alone
git worktree prune
```

Run the middle line once the pull request has merged. `git worktree remove`
fails with `not a working tree` where there was none, so a run working in a
container checkout skips the first and third.

`remove` refuses to discard uncommitted changes. Treat that refusal as an
escalation: inspect what is there and report it. Do not pass `--force` to get
past work the run never reported.

## Inspect

```sh
git worktree list
```

A worktree whose issue is no longer `status:claimed` is stale. Remove it after
confirming it holds nothing uncommitted.
