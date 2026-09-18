---
name: worktree
description: Decide whether a unit of agent work needs its own git worktree, then create, use, and remove it with Rust build isolation. Use when starting work on an issue, running parallel agents, or cleaning up after a run.
---

# Worktree

One unit of work gets one branch and one owner. Whether it also gets its own
worktree is decided under [Who gets one](#who-gets-one). This skill is the only
thing that creates one: no agent definition declares `isolation`, so nothing
provisions a worktree before this skill is consulted and nothing lands one
inside the repository root.

## Who gets one

What a worktree separates is one writer from the next, so the question is
whether this work has a next writer.

| Writer                    | Tree                                          |
| ------------------------- | --------------------------------------------- |
| A dispatched subagent     | Its own worktree, made before it is sent.     |
| A local session, directly | Its own worktree. The checkout is the user's. |
| A cloud session, directly | A branch in the container checkout.           |

A subagent always has one. The session that dispatched it is still sitting in
the checkout, and a second subagent may be sent while the first works, so a
subagent is never the only writer even when it is the only implementer. Two of
them in one checkout share one index and one `HEAD`, so they cannot hold a
branch each: the second to create its branch moves `HEAD` and takes the first's
staged work with it, the first's later commits land on the second's branch, and
the first's branch never leaves `origin/edge`. Git refuses one branch in two
worktrees, but that refusal cannot fire here, because there is only one
worktree.

The run creates it before dispatching, not the subagent after arriving. A
subagent that has to make its own tree has already been handed a directory, and
which one it got is the thing nobody can see afterwards.

A session working an issue itself on a development machine takes one too. The
checkout there is the user's working tree and an agent is never its writer.

The one case with no worktree is a cloud session working an issue itself. The
container checkout belongs to nobody else, there is no subagent beside it, and a
second tree inside the repository root only costs: it is untracked, so the
checkout reads dirty and the stop hook asks for a locked second checkout to be
committed, and Cargo can reach the parent `.cargo/config.toml` and build into
the parent `target/`. That session still owns its branch — rename a
harness-supplied branch name to `agent/<issue>` before the first push.

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

Removal is part of the run, not cleanup for later. A run that took no worktree
deletes only its branch, once the pull request has merged:

```sh
git worktree remove ../thndrs-worktrees/<issue>
git branch -d agent/<issue>
git worktree prune
```

`remove` refuses to discard uncommitted changes. Treat that refusal as an
escalation: inspect what is there and report it. Do not pass `--force` to get
past work the run never reported.

## Inspect

```sh
git worktree list
```

A worktree whose issue is no longer `status:claimed` is stale. Remove it after
confirming it holds nothing uncommitted.
