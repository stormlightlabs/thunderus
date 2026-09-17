---
title: "Git Worktrees for Parallel Agents"
Author: Git documentation, Addy Osmani, Rust community reports, agent-tooling practitioners
Date: 2026-06-07
Captured: 2026-09-16
Tags: [worktrees, isolation, parallel-agents, cargo, build-cache, loop-engineering, git]
Sources:
  - https://git-scm.com/docs/git-worktree
  - https://addyosmani.com/blog/loop-engineering/
  - https://blog.howardjohn.info/posts/shared-rust-build/
  - https://users.rust-lang.org/t/tool-cargo-worktree-fix-build-isolation-in-git-worktrees/139192
  - https://www.augmentcode.com/guides/git-worktrees-parallel-ai-agent-execution
  - https://majesticlabs.dev/blog/202608/faster-git-worktree-setup-without-breaking-branch-isolation
---

## Summary

A worktree is a second working directory attached to the same repository, with
its own checked-out branch and its own index, sharing one object database and
one history. For agent work it is the isolation primitive: it turns invisible
cross-agent corruption into ordinary git conflicts that existing tooling already
knows how to surface.

Worktrees are the second of Osmani's five loop components. Two agents editing one
checkout produce a working tree that belongs to neither of them, with no record
of which change came from where. Two agents in two worktrees produce two branches
and two diffs.

For a Rust workspace the isolation costs something. The default `target/` layout
and Cargo's build-directory lock turn parallel worktrees into serialized builds,
and a shared `CARGO_TARGET_DIR` makes them destroy each other's caches. That
tradeoff is most of what this note covers.

## Key Ideas

- **Isolation:** Each agent gets a directory where it can edit, build, break,
  and revert without touching another agent's files or the user's own checkout.
  Reviews become one diff per worktree instead of one entangled diff.
- **A worktree is not a clone:** It shares `.git` objects, so creation is cheap
  and no extra fetch is needed. Coupling remains: shared hooks, shared config
  discovery, and shared build caches all cross between trees.
- **Branch exclusivity is enforced:** Git refuses to check out the same branch
  in two worktrees. That constraint is useful; it makes "one branch, one owner"
  mechanical rather than a convention.
- **Decompose by boundary, not by convenience:** Practitioner guidance converges
  on splitting parallel work along domain or feature seams and avoiding two
  agents approaching the same files from different directions. Parallelism only
  saves wall-clock time when the task boundaries are tight enough to avoid
  rework.
- **Worktrees do not clean themselves up:** Removal has to be part of the loop.
  Stale worktrees accumulate branches, directories, build caches, and
  administrative entries under `.git/worktrees`.
- **Harness support varies:** Codex has native per-thread worktree support.
  Claude Code exposes a `--worktree` flag and an `isolation: worktree` setting
  for subagents, which provisions a fresh checkout and cleans it up when the
  agent finishes. Other harnesses need the loop to run `git worktree` itself.
- **The Cargo config trap is real:** Cargo walks up from the build directory
  looking for `.cargo/config.toml`. A worktree created *inside* the main
  checkout finds the parent's config, inherits its settings, and can silently
  build into the parent's `target/`. Put worktrees in a sibling directory
  outside the repository root.
- **One target directory per worktree, shared compiler cache:** The reported fix
  that actually recovers parallelism is an isolated `target/` per worktree, with
  cross-worktree artifact reuse moved to a content-addressed `RUSTC_WRAPPER`
  such as `sccache`. A single writable shared `CARGO_TARGET_DIR` serializes on
  Cargo's build-directory lock and thrashes incremental state as each branch
  unifies a different feature set.
- **Untracked files do not follow:** `.env` files, local configuration, cached
  credentials, and installed dependencies are not in git and therefore not in a
  new worktree. A provisioning step has to copy or link them, and that step is
  also where secrets leak if it is careless.

## Claims & Evidence

| Claim                                                                    | Support                                                                                                                          | Caveat / Confidence                                                                                          |
| ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Worktrees give each agent a genuinely isolated filesystem view.          | `git worktree` semantics: separate working directory and index per tree, shared object store.                                    | High; this is documented git behavior.                                                                       |
| Worktrees convert silent cross-agent corruption into visible conflicts.  | Practitioner guides describe file-level isolation producing per-tree diffs reviewable one at a time.                              | High as a mechanism; it does not prevent two agents from making semantically incompatible changes.           |
| A shared writable Cargo target directory removes the parallelism it was meant to enable. | Cargo locks the build directory, so concurrent invocations serialize; path-independent fingerprints collide on output names. | High; reported repeatedly in Rust tooling issues and confirmed by Cargo's locking behavior.             |
| Nested worktrees can silently build into the parent's `target/`.         | Cargo's upward search for `.cargo/config.toml` finds the parent checkout's configuration when the worktree lives beneath it.      | High; avoided entirely by placing worktrees outside the repository root.                                     |
| `sccache` recovers most of the cost of per-worktree target directories.  | Reported pattern: per-worktree `target/` plus `RUSTC_WRAPPER=sccache` for content-addressed artifact reuse.                       | Medium-high. Hit rates depend on feature-flag stability and toolchain pinning; measure before assuming.       |
| Each new worktree otherwise pays a full cold Rust build.                 | Reported directly by agent-dispatch tooling projects measuring per-worktree build cost.                                           | High for this workspace's size; the absolute cost needs local measurement.                                    |
| Worktree cleanup must be automated.                                      | Consistent practitioner guidance; stale trees accumulate disk and administrative state.                                          | High.                                                                                                        |
| Parallel agents only help when task boundaries are tight.                | Guidance to decompose by domain or feature boundary and avoid overlapping files.                                                  | High as an operational rule; the failure is rework, not corruption.                                          |

## Mechanics

Create a worktree on a new branch, outside the repository root:

```sh
repo_root="$(git rev-parse --show-toplevel)"
task="issue-123-fix-picker"
git -C "$repo_root" worktree add "../thndrs-worktrees/$task" -b "agent/$task" origin/edge
```

Branch from the integration branch, not from whatever the user happens to have
checked out. The worker's baseline should be the branch its pull request will
target.

List, prune, and remove:

```sh
git worktree list
git worktree remove ../thndrs-worktrees/issue-123-fix-picker
git worktree prune
```

`remove` refuses to discard a tree with uncommitted changes without `--force`.
That refusal is a feature for agent loops: it surfaces unreported work instead
of deleting it. Treat a refused removal as a signal to inspect, not as an
obstacle to override.

## Rust and Cargo Specifics

| Concern                | Naive approach                        | Consequence                                                                             | Preferred approach                                                                    |
| ---------------------- | ------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Build directory        | Shared `CARGO_TARGET_DIR`.            | Cargo's build-dir lock serializes builds; feature unification thrashes incremental state. | One `target/` per worktree.                                                             |
| Artifact reuse         | Share the target directory.           | Colliding output paths and invalidated fingerprints.                                      | `RUSTC_WRAPPER=sccache` with a shared read/write cache.                                 |
| Worktree location      | Inside the repository root.           | Inherits the parent `.cargo/config.toml`; may build into the parent's `target/`.          | A sibling directory outside the root.                                                   |
| Toolchain              | Whatever is default.                  | Cache misses and inconsistent MSRV results across trees.                                  | Pin via `rust-toolchain.toml` so every tree resolves identically.                       |
| Disk                   | Unbounded worktrees.                  | Each tree carries a full `target/`, which dominates repository size.                      | Cap concurrent worktrees; remove on merge or abandonment.                                |
| Untracked local files  | Assume they exist.                    | Missing configuration and credentials produce confusing first-run failures.               | An explicit provisioning step, with an allowlist rather than a blanket copy.             |

Symptoms of a shared-target misconfiguration are easy to misread as compiler
bugs: incremental builds that restart from scratch, test binaries that vanish
between runs, and compile errors that do not reproduce. If those appear during
parallel agent work, check target-directory isolation before anything else.

## Operating Rules

- One task, one branch, one worktree, one owner. Git enforces the branch half;
  the protocol has to enforce the rest.
- Branch from the integration branch, and rebase or merge forward rather than
  letting a long-lived tree drift.
- Never run an agent worktree against the user's primary checkout. The primary
  working tree stays user-owned.
- Provision untracked prerequisites explicitly and by allowlist. A blanket copy
  of ignored files spreads credentials into directories nobody is tracking.
- Give each worktree its own build directory and share compilation through a
  content-addressed cache, not through a writable target directory.
- Cap concurrency by machine capacity, not by available tasks. Parallel Rust
  builds, language servers, and test processes exhaust memory quickly.
- Make removal part of the loop's terminal state, and prune administrative
  entries on a schedule.
- Treat "uncommitted changes remain" on removal as an escalation, not a failure
  to force through.

## Important Terms

| Term                   | Meaning                                                                                                      |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| Worktree               | An additional working directory attached to one repository, with its own branch and index.                    |
| Primary working tree   | The original checkout containing `.git`; in this project it is user-owned and off-limits to dispatched agents. |
| Build-directory lock   | Cargo's exclusive lock on a target directory, which serializes concurrent invocations sharing it.              |
| Feature unification    | Cargo resolving one feature set per build; differing sets across branches invalidate shared incremental state. |
| Content-addressed cache | A compiler cache keyed by inputs rather than output path, safe to share across isolated target directories.   |
| Provisioning           | Copying or linking untracked prerequisites into a fresh worktree so it can build and run.                     |
| Prune                  | Removing administrative records of worktrees whose directories no longer exist.                               |

## Questions for Review

- What is the measured cold-build cost of a new worktree for this workspace, and
  what does `sccache` actually recover?
- Which untracked files does a worker genuinely need, and which would be a
  credential leak to copy?
- What is the right concurrency cap for this machine given Rust build memory?
- Who removes a worktree when a run fails midway, and what happens to its
  uncommitted work?
- Should agent worktrees live under a fixed sibling directory, a per-run
  temporary directory, or be delegated to harness-native isolation?
- When does a harness's automatic worktree management conflict with a loop that
  manages worktrees itself?

## Takeaways

- Worktrees answer parallel agent file collisions at the lowest cost, and the
  collisions they prevent leave no other trace.
- In Rust, isolation has to reach the build directory. Cargo's lock cancels the
  parallelism otherwise. Share compilation through `sccache` rather than through
  `target/`.
- Place worktrees outside the repository root so Cargo's search for
  `.cargo/config.toml` cannot reach the parent checkout.
- Cleanup, provisioning, and a concurrency cap belong to the loop that creates
  the worktrees.
- Related ideas: [loop-engineering](/docs/notebook/loop-engineering/),
  [agent-task-boards](/docs/notebook/agent-task-boards/),
  [harness-engineering](/docs/notebook/harness-engineering/),
  [herdr](/docs/notebook/herdr/), [release](/docs/notebook/release/).
