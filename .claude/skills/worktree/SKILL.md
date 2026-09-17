---
name: worktree
description: Create, use, and remove an isolated git worktree for one unit of agent work, with Rust build isolation. Use when starting work on an issue, running parallel agents, or cleaning up after a run.
---

# Worktree

One unit of work gets one worktree, one branch, and one owner. The user's
primary checkout is never an agent's working directory.

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
`agent/git-hygiene`. A session that took a harness-generated branch name and
has no worktree yet renames it to match before the first push; a worktree made
here is born with the right name and needs no rename.

A container is not a substitute for a worktree. It separates the session from
the user's machine; a worktree separates one writer from the next, and that is
the job here. Two implementers in one checkout share one index and one `HEAD`,
so they cannot hold a branch each: the second to create its branch moves `HEAD`
and takes the first's staged work with it, the first's later commits land on
the second's branch, and the first's branch never leaves `origin/edge`. Git
refuses one branch in two worktrees, but that refusal cannot fire here, because
there is only one worktree. So every implementer gets its own, on either host.
A dispatched implementer already declares `isolation: worktree`, so it has one
before this skill is consulted.

A reviewer needs no worktree, only a tree that does not move while it reads,
which is a commit. The `review` skill owns how to read one.

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
