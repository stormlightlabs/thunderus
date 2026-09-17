---
name: thunderstorm
last_updated: 2026-09-17
id: 01M2PWX233GKXE5M9SPTNTGN0D
---

# Thunderstorm

Thunderstorm is the development loop for this repository. One run covers one
parent issue and the sub-issues declared under it. A human starts every run.
Nothing runs on a schedule.

## What a run is

The parent issue is the run. Its sub-issues are the units of work. Run state
lives in the issues, so a run survives a killed session and any harness can
resume it.

A run ends when every sub-issue reaches a terminal state, the budget is spent,
or a worker escalates. A worker that finds adjacent work files a new issue and
does not start it in this run.

## Where a run happens

A run works from a local checkout or from a cloud session on the web. The
protocol is the same; the transport to GitHub is not.

| Host          | GitHub through   | Worktrees                          |
| ------------- | ---------------- | ---------------------------------- |
| Local         | `gh`             | As described under [Worktrees](#worktrees). |
| Cloud session | GitHub MCP tools | One ephemeral container, one issue. Skip the worktree. |

The `github-board` skill picks the transport and owns the differences between
them. Two are load-bearing:

- The MCP issue update replaces an issue's whole label and assignee set, where
  `gh issue edit` changes only what it names.
- No MCP tool defines a label. `.github/labels.yml` is applied by running
  `.claude/scripts/sync-labels.sh` locally, or by dispatching
  `.github/workflows/labels.yml`, which runs that same script on a runner with a
  token scoped to labels. A run that wants a label the manifest does not define
  is still blocked; the manifest changes first.

A cloud container starts with no Cargo registry, no `target/`, and no
`docs/node_modules`, so `.claude/hooks/session-start.sh` warms the first and
third before the session begins. It exits immediately outside the cloud, where
a checkout already has them. `.claude/settings.json` registers it.

## Statuses

Status is a label. One status label per issue.

| Label            | Meaning                                           | Leaves when                                      |
| ---------------- | ------------------------------------------------- | ------------------------------------------------ |
| `status:queued`  | Ready to work. No owner.                          | A run claims it.                                 |
| `status:claimed` | A worker owns it and a worktree exists.           | A pull request opens, or the run abandons it.    |
| `status:review`  | Pull request open. Review passes in progress.     | All three review passes clear.                   |
| `status:verify`  | Merged to `edge`. Waiting on human confirmation.  | A human confirms behavior or files a regression. |
| `status:blocked` | Cannot proceed. Needs a `blocked:*` reason label. | The reason is recorded as resolved.              |
| `status:done`    | Released from `main`.                             | Terminal.                                        |
| `status:dropped` | Abandoned. Reason recorded in a closing comment.  | Terminal.                                        |

`status:claimed` for more than 24 hours with no pull request returns to
`status:queued` and its worktree is removed. `status:blocked` for more than 7
days goes to the triage inbox. Nothing closes automatically.

The full label set, including `area:*`, `risk:*`, `type:*`, and the `run` marker
for parent issues, lives in `.github/labels.yml`. Apply it with
`.claude/scripts/sync-labels.sh`.

## Commands

| Command                | Argument           | Does                                                               |
| ---------------------- | ------------------ | ------------------------------------------------------------------ |
| `/r-d`, `/rubber-duck` | A topic            | Design discussion. Writes an entry to `internal/ideas/` on request. |
| `/impl`, `/implement`  | Issue number       | Claims the issue, works it in a worktree, opens a pull request.    |
| `/rev`                 | Branch or PR number | Standard review pass. Posts findings as comments.                  |
| `/adv-rev`             | Branch or PR number | Adversarial review pass. Posts findings as comments.               |
| `/edit`, `/revise`     | PR number          | Addresses review comments on that pull request.                    |

## Review sequence

Three review passes, each followed by an edit pass. No pass starts before the
previous edit pass finishes.

1. `/rev` posts findings as pull request comments.
2. `/edit` addresses them.
3. `/rev` runs again on the revised diff.
4. `/edit` addresses the second round.
5. `/adv-rev` runs the adversarial pass.
6. `/edit` addresses the adversarial findings.
7. A human reviews and merges to `edge`.

Reviews post from whichever account runs them: Claude, Codex, or the
repository owner. Every comment ends with a signature naming the model and its
reasoning level, so the record shows which reviewer produced which finding.

```text
— claude-opus-5 · high
```

A reviewer does not edit code. An editor does not approve its own work. Only a
human merges to `edge`.

The implementer and the reviewer never share a model in one run. See
[models.md](models.md) for which model runs which role.

### Finding format

```text
<severity> · <path>:<line> — <problem> → <why it matters> → <fix direction>
```

Severity is one of `blocker`, `high`, `medium`, `low`, `nit`.

An edit pass stops after 5 cycles, or when the same finding appears twice
without the underlying cause changing. Either stop is an escalation.

## Branches

| Branch      | Holds                                    | Accepts                        |
| ----------- | ---------------------------------------- | ------------------------------ |
| `main`      | The current release. Matches the tag.    | Release pull requests from `edge`. |
| `edge`      | Merged work that is not released yet.    | Pull requests from `agent/*`.  |
| `agent/<n>` | One sub-issue. One worktree. One owner.  | Pushes from its worker.        |

Every branch an agent pushes carries the `agent/` prefix, including work that
has no issue behind it. Name that case for the work rather than the issue it
lacks: `agent/git-hygiene`, not a harness-generated name. The prefix is what
tells a human reading the branch list which branches an agent owns, and a cloud
session that accepted whatever name its harness supplied breaks that.

Delete the task branch when its pull request merges.

## Worktrees

One sub-issue gets one worktree, created outside the repository root so Cargo
does not find the parent `.cargo/config.toml`:

```sh
git worktree add ../thndrs-worktrees/<issue> -b agent/<issue> origin/edge
```

Each worktree keeps its own `target/`. Share compilation through
`RUSTC_WRAPPER=sccache`, never through a shared `CARGO_TARGET_DIR`: Cargo locks
the build directory, so a shared target directory serializes the builds it was
meant to parallelize.

Remove the worktree when the run ends. A removal that fails because of
uncommitted changes is an escalation, not something to force.

## File conventions

Every file under `internal/` starts with YAML frontmatter:

```yaml
---
name: short-kebab-case-name
last_updated: YYYY-MM-DD
id: <ULID>
---
```

Generate the identifier with `.claude/scripts/ulid.py`. The identifier never
changes once assigned. Update `last_updated` when the content changes.

## Layout

| Path                | Holds                                                        |
| ------------------- | ------------------------------------------------------------ |
| `.claude/skills/`   | Skills. `.agents/skills` symlinks here.                      |
| `.claude/commands/` | Slash commands.                                              |
| `.claude/agents/`   | Subagent definitions for dispatch.                           |
| `.claude/scripts/`  | Helper scripts. `sync-labels.sh` also runs in CI.            |
| `.claude/hooks/`    | Session hooks. Dependency warming for cloud sessions.        |
| `.claude/settings.json` | Hook and permission configuration. Tracked.              |
| `internal/`         | Plans, specs, ideas, QA notes. Not published by the docs site. |
| `internal/ideas/`   | Output of rubber-duck sessions.                              |
| `CLAUDE.md`         | Repository instructions. `AGENTS.md` symlinks here.          |

## Skills

| Skill             | Owns                                                        |
| ----------------- | ----------------------------------------------------------- |
| `thunderstorm`    | One run: claims, dispatches, reports, stops. Writes no code. |
| `implement`       | One issue, in one worktree, to one pull request.            |
| `review`          | Standard and adversarial review passes.                     |
| `revise`          | Addressing findings on a pull request.                      |
| `github-board`    | The only writer of issue state.                             |
| `worktree`        | Provisioning, build isolation, removal.                     |
| `release`         | `edge` to `main`, tag, changelog, publish.                  |
| `rubber-duck`     | Design discussion and idea files.                           |
| `writing-docs`    | Prose in the docs site, `internal/`, and chat.              |
| `commits-and-prs` | Commit messages, pull request bodies, changelog entries.    |

Subagents for dispatch live in `.claude/agents/`: `implementer`, `reviewer`,
and `adversarial-reviewer`.

## Not wired up yet

- `edge` does not exist on the remote. Worktrees branch from it, pull requests
  target it, and CI already has a push trigger for it. Nothing dispatches until
  it is created and protected.
- `.github/labels.yml` has never been applied. Run
  `.claude/scripts/sync-labels.sh --apply`, or dispatch the Labels workflow.
- The repository has no issues, so no run has a parent to work from.
- There are no tags. The release skill reads the previous version with
  `git describe --tags`, which fails until the first tag exists.
- OpenCode Go and Cursor have no role assignments in [models.md](models.md).
- No harness triggers a run automatically, by design. Runs start when a human
  invokes one.
