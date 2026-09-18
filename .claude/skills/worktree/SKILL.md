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

What a worktree separates is one writer from the next. Two implementers in one
checkout share one index and one `HEAD`, so they cannot hold a branch each: the
second to create its branch moves `HEAD` and takes the first's staged work with
it, the first's later commits land on the second's branch, and the first's
branch never leaves `origin/edge`. Git refuses one branch in two worktrees, but
that refusal cannot fire here, because there is only one worktree. So a worktree
is load-bearing exactly when two writers are live at once.

| Host  | Writers live | Worktree                              |
| ----- | ------------ | ------------------------------------- |
| Local | Any          | One each. The checkout is the user's. |
| Cloud | One          | None. Work in the container checkout. |
| Cloud | Two or more  | One each, created here.               |

On a development machine every implementer gets one, whether it is alone or
not. The checkout is the user's working tree and an agent is never its writer.

A cloud container is already a checkout nobody else owns, so a run dispatching
one implementer at a time works in it directly. A second worktree there buys no
isolation and costs two things: it is untracked inside the repository root, so
the checkout reads dirty and the stop hook asks for a locked second checkout to
be committed, and Cargo can reach the parent `.cargo/config.toml` and build into
the parent `target/`.

Concurrency brings the worktree back. The moment a run dispatches a second
implementer, each gets its own, created here and rooted outside the repository,
because the argument above applies to a container exactly as it applies to a
laptop. One writer is the condition, not the host.

A session working in the container checkout still owns its branch. Rename a
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
