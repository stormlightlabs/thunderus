---
name: thunderstorm
last_updated: 2026-09-19
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

| Host          | GitHub through   |
| ------------- | ---------------- |
| Local         | `gh`             |
| Cloud session | GitHub MCP tools |

Where a worker writes is decided by dispatch rather than host, under
[Worktrees](#worktrees).

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
toolchain a session happens to hold. It installs `freeze` at a pinned version
too, because a container carries no renderer and the frames a capture run posts
are stripped of their color before they reach a pull request. No step's failure
fails the hook, so a missing renderer costs an image rather than a run, and
`.claude/hooks/session-start-test.py` holds that rule in place against stub
toolchains. The hook exits immediately outside the cloud, where a checkout
already has all of this.
`.claude/settings.json` registers it.

### Identity

A cloud session's pull requests and comments are authored by the human whose
account it runs under. GitHub shows no difference between those writes and that
person's own. This repository accepts that and relies on convention instead: no
machine account, no app installation.

Nothing inside a session chooses that account. The container holds `GH_TOKEN`
and `GITHUB_TOKEN` for the REST path `github-board` scopes to dependency edges.
Both are a 14-character placeholder that the outbound proxy substitutes before a
request leaves. The MCP tools carry their own authorization from the account
connected at `claude.ai/connect-github`, which the environment does not set
either.

A machine user's fine-grained PAT in the environment therefore changes nothing,
and the probe below cannot tell you so. Moving identity takes one action,
reconnecting the connector as the machine user. The cost is that the same
connector authorizes a human's own interactive sessions, which would then post
as the machine user too.

Two conventions stand in for an account a query could filter on:

- A review comment ends with a signature naming the model and reasoning level,
  under [Review sequence](#review-sequence).
- A commit from a dispatched agent is authored as
  `Claude <noreply@anthropic.com>`. One from a session a person drove is
  authored by them. Trailers naming the model and the session come from the
  harness where it supplies them, and nothing checks for either. Both cases are
  under the `commits-and-prs` skill's Attribution.

Neither is queryable. Activity feeds, `author:` filters, and branch protection
rules all see `desertthunder`, so telling an agent's writes from a human's means
reading them. Reopen the decision if GitHub attribution has to settle something
a human reading the thread cannot. The same applies if a cloud session gains a
way to point its MCP authorization at an app installation.

Checked on 2026-09-19. `get_me` and `GET /user` both return `desertthunder`, and
`GET /user` returns it with a bogus bearer as well. That last case is what rules
the environment out.

The MCP side is inferred rather than observed. `USE_SHTTP_MCP=true` and the
proxy's bypass for `mcp-proxy.anthropic.com` show the tools reach a remote
server. What authorizes that server was not observed from here.

## Statuses

Status is a label. One status label per issue.

| Label            | Meaning                                           | Leaves when                                      |
| ---------------- | ------------------------------------------------- | ------------------------------------------------ |
| `status:queued`  | Ready to work. No owner.                          | A run claims it.                                 |
| `status:claimed` | A worker owns it and has somewhere to write.      | A pull request opens, or the run abandons it.    |
| `status:review`  | Pull request open. Review passes in progress.     | All three review passes clear.                   |
| `status:verify`  | Merged to `edge`. Waiting on human confirmation.  | A human confirms behavior or files a regression. |
| `status:blocked` | Cannot proceed. Needs a `blocked:*` reason label. | The reason is recorded as resolved.              |
| `status:done`    | Released from `main`.                             | Terminal.                                        |
| `status:dropped` | Abandoned. Reason recorded in a closing comment.  | Terminal.                                        |

`status:claimed` for more than 24 hours with no pull request returns to
`status:queued`, and its worktree is removed if it has one. `status:blocked`
for more than 7 days goes to the triage inbox. Nothing closes automatically.

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
| `/impl`, `/implement`   | Issue number              | Claims the issue, works it on an `agent/` branch, opens a pull request.            |
| `/rev`                  | Branch or PR number       | Standard review pass. Comments only on the second pass.                            |
| `/adv-rev`              | Branch or PR number       | Adversarial review pass. Always comments.                                          |
| `/edit`, `/revise`      | PR number                 | Addresses review comments on that pull request.                                    |

## Review sequence

Three review passes, each followed by an edit pass. No pass starts before the
previous edit pass finishes.

1. `/rev` runs the first pass and reports to whoever dispatched it.
2. `/edit` addresses those findings, which it is handed rather than reading
   from the thread.
3. `/rev` runs again on the revised diff and comments on what survived.
4. `/edit` addresses the second round.
5. `/adv-rev` runs the adversarial pass and always comments.
6. `/edit` addresses the adversarial findings.
7. A human reviews and merges to `edge`.

Which pass is which comes from the dispatch, not from reading the thread. The
`review` skill says why under Which passes post.

The thread still records all three passes even though the first does not
comment on it, because every `/edit` reply names the pass it answers and the
signature that pass ran under. A first pass leaves its trace in the reply to
it, which is also what makes the model rule in `internal/models.md` checkable
after the fact.

Reviews post from whichever account runs them, which for a Claude cloud session
is `desertthunder`, under [Identity](#identity). Every comment ends with a
signature naming the model and its reasoning level, so the record shows which
reviewer produced which finding.

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

Severity is one of `blocker`, `high`, `medium`, `low`, `nit`. An adversarial
finding that could not be reproduced or traced is marked `unverified` and stays
below `high`. A posted comment holds one line per finding, most severe first,
and stays under 40 lines.

An edit pass stops after 5 cycles, or when the same finding appears twice
without the underlying cause changing. Either stop is an escalation.

## Branches

| Branch      | Holds                                   | Accepts                            |
| ----------- | --------------------------------------- | ---------------------------------- |
| `main`      | The current release. Matches the tag.   | Release pull requests from `edge`. |
| `edge`      | Merged work that is not released yet.   | Pull requests from `agent/*`.      |
| `agent/<n>` | One sub-issue. One owner.               | Pushes from its worker.            |

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

Four checks here exist for that reason: the commit-message hook, the commit
report in CI, `push-verified.sh`, and `check-isolation.py`. Each replaced a rule
that had been broken at least once while written down and believed.

## Worktrees

A worktree, where the work takes one, is created outside the repository root so
Cargo does not find the parent `.cargo/config.toml`:

```sh
git worktree add ../thndrs-worktrees/<issue> -b agent/<issue> origin/edge
```

Each worktree keeps its own `target/`. Share compilation through
`RUSTC_WRAPPER=sccache`, never through a shared `CARGO_TARGET_DIR`: Cargo locks
the build directory, so a shared target directory serializes the builds it was
meant to parallelize.

Remove the worktree when the run ends. A removal that fails because of
uncommitted changes is an escalation, not something to force.

Worktrees can only be made via the `worktree` skill, and `check-isolation.py`
fails CI for any text under `.claude/` that asks the harness instead. What no
check covers is in that skill's **Who gets one** section.

### Who gets one

Every dispatched subagent gets one, on either host, and the run creates it
before dispatching. A session working an issue itself takes one on a development
machine, where the checkout is the user's. On a cloud session it works in the
container checkout, which belongs to nobody else. The `worktree` skill's
**Who gets one** section holds the reasoning.

Two implementers in one checkout produce a failure nothing reports. They share
one index and one `HEAD`, so the second to create its branch moves `HEAD` for
both. The first's staged work then lands in the second's commit, and every
commit it makes afterwards lands on the second's branch.

`push-verified.sh` does not catch that. It compares each push against its own
branch, and both pushes have one and both land. The only trace is that script
naming a branch the run never claimed. An index lock collision is rarer and
louder.

A reviewer gets none. It writes nothing into the tree, so what it needs is a
tree that does not move while it reads, which is a commit rather than a
directory. The `review` skill owns that discipline: fetch the branch, read
through `git show <commit>:<path>`, and name the commit. Name the pull request
with it. A branch is deleted when its pull request merges and the commit goes
unreachable with it, so a review that cites a SHA alone is unreadable by the
time anyone goes back to it.

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

`.claude/scripts/check-frontmatter.py` checks the whole tree and runs in CI,
where `--since` also compares each identifier against the pull request's base
commit. The tree alone cannot show an identifier that changed, and a changed one
orphans every issue citing it.

The name rule is waived for `internal/features/*/plan.md` and `tasks.md` until
their scheme is decided, because a name matching its filename would give five
files named `plan`. That decision is issue 9. Nothing else is waived: once one
of those files carries a block, its date and its identifier answer to the check
like any other.

An issue cut from a document cites that document's identifier — this one is
`01M2PWX233GKXE5M9SPTNTGN0D` — so the trail is readable from either end. What
no document carries is a list of pending work: the board holds that, and a list
of gaps goes stale the moment one closes. Two bullets in the list this document
replaced were already false within hours of being written.

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
| `implement`       | One issue, on one branch, to one pull request.               |
| `review`          | Standard and adversarial review passes.                      |
| `revise`          | Addressing findings on a pull request.                       |
| `github-board`    | The only writer of issue state.                              |
| `worktree`        | Who needs one, provisioning, build isolation, removal.       |
| `release`         | `edge` to `main`, tag, changelog, publish.                   |
| `rubber-duck`     | Design discussion and idea files.                            |
| `specify`         | One design, when issues need a decision made first.          |
| `decompose`       | Cutting an idea or spec into an epic and its sub-issues.     |
| `writing-docs`    | Prose in the docs site, `internal/`, and chat.               |
| `commits-and-prs` | Commit messages, pull request bodies, changelog entries.     |

Subagents for dispatch live in `.claude/agents/`: `implementer`, `reviewer`,
`adversarial-reviewer`, and `reviser`, one per role the run dispatches.

Each definition's `tools:` line is an allowlist, so a role reaches GitHub only
through the tools it names. A cloud run is the case that exposes this: the
transport there is the GitHub MCP tools, and an agent whose list omits them
cannot claim an issue, open a pull request, or post a finding, however well the
server is connected to the session around it. What each role needs follows from
what its skill tells it to do.

| Role                   | Reaches GitHub for                                              |
| ---------------------- | --------------------------------------------------------------- |
| `implementer`          | Reading the issue, claiming it, opening the pull request, filing found work |
| `reviewer`             | Reading the pull request and its issue, posting from the second pass |
| `adversarial-reviewer` | The same, and it always posts                                    |
| `reviser`              | Reading findings, replying, resolving and reopening threads, filing a deferral |

`get_me` is not optional for a claim. MCP has no `@me`, so the assignee array
needs the login spelled out.

Give a role the tool that undoes each tool it has. A reviser that can resolve a
thread and not reopen one turns a mistyped thread id into a question that no
longer looks like it is waiting on anybody, and the pass that made it cannot
take it back.

A role also needs the Bash its skill calls for. `.claude/settings.json` carries
the allowlist, and a run that has to stop for a prompt nobody is there to
answer stalls rather than fails, which is the harder shape to read afterwards.

Editing a definition mid-run does not reliably reach the next dispatch. A
changed `tools:` line was live within the session; a removed `isolation` was
not, and took a further dispatch to take effect. Treat a definition change as
something the next session gets, and unblock the run in front of you by moving
the ground rather than the definition.
