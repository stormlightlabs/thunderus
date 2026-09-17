---
name: thunderstorm
last_updated: 2026-09-17
id: 01M2PWX233GKXE5M9SPTNTGN0D
---

# Thunderstorm

Thunderstorm is the development loop for this repository. One run covers one
epic and the sub-issues declared under it. A human starts every run. Nothing
runs on a schedule.

## What a run is

A run is one pass over one epic. The epic is the issue you file: a goal, a stop
rule, and the sub-issues that are the units of work. The two are not the same
thing, and the difference shows the first time a budget runs out: the run ends,
the epic does not, and the next run picks it up where this one stopped.

Run state lives in the issues, so a run survives a killed session and any
harness can resume it.

A run ends when every sub-issue reaches a terminal state, the budget is spent,
or a worker escalates. A worker that finds adjacent work files a new issue and
does not start it in this run.

## Where a run happens

A run works from a local checkout or from a cloud session on the web. The
protocol is the same; the transport to GitHub is not.

| Host          | GitHub through   | Worktrees                                              |
| ------------- | ---------------- | ------------------------------------------------------ |
| Local         | `gh`             | As described under [Worktrees](#worktrees).            |
| Cloud session | GitHub MCP tools | One ephemeral container, one issue. Skip the worktree. |

The `github-board` skill picks the transport and owns the differences between
them. Two are load-bearing:

- The MCP issue update replaces an issue's whole label and assignee set, where
  `gh issue edit` changes only what it names.
- Label definitions do not go through MCP. `.github/labels.yml` is applied by
  running `.claude/scripts/sync-labels.py` locally, or by dispatching
  `.github/workflows/labels.yml`, which runs that same script on a runner with
  `issues: write`, the narrowest scope GitHub offers for labels. A run that
  wants a label the manifest does not define is still blocked; the manifest
  changes first.

A cloud container starts with no Cargo registry, no `target/`, and no
`docs/node_modules`, so `.claude/hooks/session-start.sh` warms the first and
third before the session begins. It also runs `rustup update stable`, because
the image pins whatever stable was current when it was built while CI installs
the current one, and a check that passes against the older compiler can still
fail on CI. The 1.88 job is what guards compatibility, not the age of the
toolchain a session happens to hold. The hook exits immediately outside the
cloud, where a checkout already has all of this.
`.claude/settings.json` registers it.

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

An epic carries no status. It is not work, so there is nothing to claim, and
duplicating its sub-issues' state on the parent gives that copy somewhere to
drift.

The full label set, including `area:*`, `risk:*`, `type:*`, and `kind:epic` for
the issues runs work through, lives in `.github/labels.yml`. Its `retired:` group
names labels this repository has stopped defining, which the sync deletes: a
rename that only adds the new name leaves the old one in the picker, teaching
the next contributor a rule that no longer holds. Apply it with
`.claude/scripts/sync-labels.py`.

## Stages

Work moves through these. The bracketed stage is skipped whenever it would add
nothing.

```text
/r-d            an idea            internal/ideas/
[/spec-ify]     a design           internal/features/<name>/plan.md
/decomp         an epic and sub-issues
/thunderstorm   one run over that epic, dispatching the stages below
/impl           a pull request
/rev, /adv-rev, /edit              review passes and their fixes
a human         merges to edge
```

A spec is written only when a sub-issue's stop rule cannot be written without
deciding something first. Most work skips it: a finding, a bug, or a chore
usually arrives with its criteria already stated, and a spec for one of those
is a summary with more words.

`decompose` is where a document becomes claimable work, and it is the only
stage that files issues. `github-board` performs those writes but decides
nothing about what to write.

A run is what carries a sub-issue from queued to merged: `/thunderstorm` claims
each one and dispatches the stages under it. One issue worked on its own skips
the run and starts at `/impl`.

## Commands

| Command                 | Argument                  | Does                                                                               |
| ----------------------- | ------------------------- | ---------------------------------------------------------------------------------- |
| `/r-d`, `/rubber-duck`  | A topic                   | Design discussion. Writes an entry to `internal/ideas/` on request.                |
| `/spec-ify`, `/specify` | An idea or topic          | One design to `internal/features/<name>/plan.md`. Only when a decision is missing. |
| `/decomp`, `/decompose` | An idea, spec, or finding | Files one epic and the sub-issues under it.                                        |
| `/thunderstorm`         | Epic issue number         | One run over the epic. Claims each sub-issue, dispatches it, and reports.          |
| `/impl`, `/implement`   | Issue number              | Claims the issue, works it in a worktree, opens a pull request.                    |
| `/rev`                  | Branch or PR number       | Standard review pass. Posts findings as comments.                                  |
| `/adv-rev`              | Branch or PR number       | Adversarial review pass. Posts findings as comments.                               |
| `/edit`, `/revise`      | PR number                 | Addresses review comments on that pull request.                                    |

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

| Branch      | Holds                                   | Accepts                            |
| ----------- | --------------------------------------- | ---------------------------------- |
| `main`      | The current release. Matches the tag.   | Release pull requests from `edge`. |
| `edge`      | Merged work that is not released yet.   | Pull requests from `agent/*`.      |
| `agent/<n>` | One sub-issue. One worktree. One owner. | Pushes from its worker.            |

Every branch an agent pushes carries the `agent/` prefix, including work that
has no issue behind it. Name that case for the work rather than the issue it
lacks: `agent/git-hygiene`, not a harness-generated name. The prefix is what
tells a human reading the branch list which branches an agent owns, and a cloud
session that accepted whatever name its harness supplied breaks that.

Delete the task branch when its pull request merges.

A push is finished when the remote ref matches local `HEAD`, not when `git push`
exits zero. The two come apart: with a detached `HEAD` the branch has not moved,
so git finds no ref to update, prints `Everything up-to-date` and succeeds while
carrying nothing. A pre-push hook cannot catch that, because with no ref to
update git never runs one. `.claude/scripts/push-verified.sh` pushes and then
compares, and refuses outright when `HEAD` is detached.

## Recording a failure mode

A run that discovers a way to be confidently wrong writes down the mechanism,
not the apology. The order matters:

1. Make it impossible, or make it fail loudly. A check that runs beats a rule
   that has to be remembered.
2. Where no check is possible, say so in the skill that owns the work, in one
   or two sentences naming what goes wrong and what to compare instead.
3. Delete the prose once a check covers it. Guidance that describes a failure
   something else now catches is read as optional and trains people to skim.

Three checks here exist for that reason: the commit-message hook, the commit
report in CI, and `push-verified.sh`. Each replaced a rule that had been broken
at least once while written down and believed.

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

| Path                    | Holds                                                          |
| ----------------------- | -------------------------------------------------------------- |
| `.claude/skills/`       | Skills. `.agents/skills` symlinks here.                        |
| `.claude/commands/`     | Slash commands.                                                |
| `.claude/agents/`       | Subagent definitions for dispatch.                             |
| `.claude/scripts/`      | Helper scripts. `sync-labels.py` also runs in CI.              |
| `.claude/hooks/`        | Session hooks. Dependency warming for cloud sessions.          |
| `.githooks/`            | Git hooks. Enable with `git config core.hooksPath .githooks`.  |
| `.claude/settings.json` | Hook and permission configuration. Tracked.                    |
| `internal/`             | Plans, specs, ideas, QA notes. Not published by the docs site. |
| `internal/ideas/`       | Output of rubber-duck sessions.                                |
| `internal/features/`    | One directory per feature track, holding its `plan.md`.        |
| `CLAUDE.md`             | Repository instructions. `AGENTS.md` symlinks here.            |

## Skills

| Skill             | Owns                                                         |
| ----------------- | ------------------------------------------------------------ |
| `thunderstorm`    | One run: claims, dispatches, reports, stops. Writes no code. |
| `implement`       | One issue, in one worktree, to one pull request.             |
| `review`          | Standard and adversarial review passes.                      |
| `revise`          | Addressing findings on a pull request.                       |
| `github-board`    | The only writer of issue state.                              |
| `worktree`        | Provisioning, build isolation, removal.                      |
| `release`         | `edge` to `main`, tag, changelog, publish.                   |
| `rubber-duck`     | Design discussion and idea files.                            |
| `specify`         | One design, when issues need a decision made first.          |
| `decompose`       | Cutting an idea or spec into an epic and its sub-issues.     |
| `writing-docs`    | Prose in the docs site, `internal/`, and chat.               |
| `commits-and-prs` | Commit messages, pull request bodies, changelog entries.     |

Subagents for dispatch live in `.claude/agents/`: `implementer`, `reviewer`,
and `adversarial-reviewer`.

## What is not done yet

Pending work lives on the board, not here. A list of gaps in a document goes
stale the moment one is closed, and nobody reviews a document to find out what
changed: two bullets in the list this replaced were already false within hours
of being written.

Issues that come from this document cite `01M2PWX233GKXE5M9SPTNTGN0D` so the
trail back is readable from either end.
