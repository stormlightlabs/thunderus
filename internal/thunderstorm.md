---
name: thunderstorm
last_updated: 2026-09-19
id: 01M2PWX233GKXE5M9SPTNTGN0D
---

# Thunderstorm

Thunderstorm is the development loop for this repository. One run covers one
issue and the sub-issues declared under it. A human starts every run.
Nothing runs on a schedule.

This is the protocol and the reasoning behind it.
`guides/using-thunderstorm.md` (`01M2XBVYTZDXTW42ZSCFEEW0QS`) is the operator's
version: what to type, when, and what to skip.

## What a run is

A run is one pass over one issue and the sub-issues under it: a goal, a stop
rule, and at most five open sub-issues that are the units of work. The run and
the issue are not the same thing, and the difference shows the first time a
budget runs out: the run ends, the issue does not, and the next run picks it up
where this one stopped.

Five is the cap because a run holds every sub-issue it dispatches in one
context. Eleven spends that context on work the dispatcher has not started,
which is how a run loses track of the one it is on.

Above that sits the epic, which a run never takes. It groups issues and holds
what none of them can see from inside: a blocker between two, an ordering
across them, a file both write. Splitting for the cap is what makes an epic
worth filing, because the two halves still share those.

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
them. Two of those differences decide whether a write lands at all:

- The MCP issue update replaces an issue's whole label and assignee set, where
  `gh issue edit` changes only what it names.
- Neither transport exposes the dependency and sub-issue counts the REST issue
  list carries per issue, and the MCP sub-issue read returns each child's whole
  body with no field list, 77,000 to 154,000 characters for one issue here. So a
  pass reading the board rather than writing to it goes to REST, under the
  `triage` skill's `references/reading-the-board.md`.
- Label definitions do not go through MCP. `.github/labels.yml` is applied by
  running `.claude/scripts/sync-labels.py` locally, or by dispatching
  `.github/workflows/labels.yml`, which runs that same script on a runner with
  `issues: write`, the narrowest scope GitHub offers for labels. A run that
  wants a label the manifest does not define is still blocked; the manifest
  changes first.

A cloud container starts with no dependency caches, which
`.claude/hooks/session-start.sh` warms before the session begins. Its own
comments carry the reason for each step, and no step's failure fails the hook.

### Identity

Everything here is authored by the repository owner. Pull requests, commits and
comments all carry one account, and nothing inside a session changes that:
`GH_TOKEN` is a placeholder the outbound proxy substitutes, and the MCP tools
carry their own authorization from the account connected at
`claude.ai/connect-github`. Verified 2026-09-19.

That is the whole rule now. A convention that tried to separate an agent's
commits from the owner's was never checkable and is gone; the work is the
owner's and the log says so.

One tag survives, because it answers a question a reader actually has. A review
comment's first line names the model and reasoning level that pass ran at, so
the record shows which reviewer found what and `models.md` stays checkable
afterwards. `internal/ideas/agent-attribution.md` keeps the design for a
mechanism rather than a tag; nothing plans to build it.

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
for more than 7 days surfaces in the next `/triage` report. Nothing closes
automatically.

Neither an epic nor an issue with sub-issues carries a status or a risk.
Neither is work, so there is nothing to claim and no blast radius to size, and
duplicating a child's state on a parent gives that copy somewhere to drift.

The full label set, including `area:*`, `risk:*`, `type:*`, and `kind:epic`
for the containers that group them, lives in `.github/labels.yml`. Its `retired:`
group names labels this repository has stopped defining, which the sync
deletes: a rename that only adds the new name leaves the old one in the picker,
teaching the next contributor a rule that no longer holds. Apply it with
`.claude/scripts/sync-labels.py`.

## Stages

Work moves through these. The bracketed stage is skipped whenever it would add
nothing.

```text
/r-d            an idea            internal/ideas/
[/spec-ify]     a design           internal/features/<name>/plan.md
/decomp         issues of up to five sub-issues, in an epic if several
[/triage]       which of them to dispatch next, and what runs at once
/storm          one run over one of those issues, dispatching the stages below
/impl           a pull request
/rev, /adv-rev, /edit              review passes and their fixes
a human         squashes it into edge, in GitHub
```

A spec is written only when a sub-issue's stop rule cannot be written without
deciding something first. Most work skips it: a finding, a bug, or a chore
usually arrives with its criteria already stated, and a spec for one of those
is a summary with more words.

`decompose` is where a document becomes claimable work, and it is the only
stage that files issues. `github-board` performs those writes but decides
nothing about what to write.

A run is what carries a sub-issue from queued to merged: `/storm` claims each
one and dispatches the stages under it. An epic is never a run's argument. One issue worked on its own skips the
run and starts at `/impl`.

## Commands

| Command                 | Argument                  | Does                                                                               |
| ----------------------- | ------------------------- | ---------------------------------------------------------------------------------- |
| `/r-d`, `/rubber-duck`  | A topic                   | Design discussion. Writes an entry to `internal/ideas/` on request.                |
| `/spec-ify`, `/specify` | An idea or topic          | One design to `internal/features/<name>/plan.md`. Only when a decision is missing. |
| `/decomp`, `/decompose` | An idea, spec, or finding | Files the issues a run can take, their sub-issues, and an epic if several.         |
| `/triage`               | Thread count or a scope   | Ranks the board and lays the top of it into lanes. Writes nothing.                 |
| `/storm`, `/thunderstorm` | Issue number, then `one` | One run over that issue. Claims each sub-issue, dispatches it, and reports. |
| `/impl`, `/implement`   | Issue number              | Claims the issue, works it on an `agent/` branch, opens a pull request.            |
| `/rev`                  | Branch or PR number       | Standard review pass. Comments only on the second pass.                            |
| `/adv-rev`              | Branch or PR number       | Adversarial review pass. Always comments.                                          |
| `/edit`, `/revise`      | PR number                 | Addresses review comments on that pull request.                                    |
| `/release`              | A version                 | `edge` to `main`, tag, publish. Confirms three times.                              |

An alias is a symlink to its canonical file, so the pair cannot drift.

## Choosing what to run next

A run covers one issue. Several are open at once, sub-issues accumulate under
all of them, and work arrives that belongs to none. `/triage` answers the
question a run cannot: of everything queued, which issues go out now, and which
of those are safe to work at the same time. The `triage` skill holds the
buckets, the ordering and the reading; what follows is why it is shaped that
way.

Nothing is ranked by a priority label, because the board defines none and a
label a human has to keep current is one more thing that drifts from the work it
describes. The order comes from what the board already carries, and the `triage`
skill states it.

One thing is visible only from a pass over the whole board. An issue filed
under no parent is never reached by a run, because a run dispatches an issue's
children; #86 covers the ones outstanding and #87 decides whether the state is
legal at all.

What crosses two issues is no longer that case: it belongs in the epic above
them, which both runs read.

The ranked list goes to chat and is derived again the next time it is asked
for. It is not a document, for the reason under [File
conventions](#file-conventions): a list of pending work is wrong as soon as one
issue closes, and a wrong copy on disk gets read in place of the board.

`triage` writes nothing: no file, no label, no claim. It reports what the board
needs repaired and leaves the repair to a `github-board` call a human asks for.

## Review sequence

Three review passes, each followed by an edit pass. No pass starts before the
previous edit pass finishes. `/rev` costs less than a defect on `edge` and the
passes are cheap to run; what is expensive is a long comment, which is why they
are budgeted below rather than fewer.

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

Every comment opens with one line naming the pass, the commit it read, and the
model and reasoning level it ran at. Nothing goes at the bottom: the harness
appends its own footer there. That opener is what records a first pass, which
posts nothing itself, and what makes the model rule in
[models.md](models.md) checkable afterwards.

```text
Second standard pass on #12 at 4f2a91c · claude-opus-5 · high
```

A comment is budgeted in words, not lines, because GitHub soft-wraps: 200 for a
review, 150 for a reply. The `review` and `revise` skills hold those, the
one-line finding format, the severity table, and the edit pass's stop rules.
Either stop is an escalation.

A reviewer does not edit code. An editor does not approve its own work. Only a
human merges to `edge`, in GitHub, by hand.

That last one is a check rather than a rule, under [Recording a failure
mode](#recording-a-failure-mode). `.claude/settings.json` denies `gh pr merge`,
`gh pr review`, `git merge`, and a push to `edge` or `main`, so a session
cannot merge whatever it has been told. The repository ships no merge script
and no `/merge` command either: a named path is what turns a capability into
the obvious next step, and an agent approving or merging its own work is the
failure the whole review sequence exists to prevent.

The implementer and the reviewer never share a model in one run.

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

A run that discovers a way to be confidently wrong writes down the mechanism
that allowed it. Work through these in order:

1. Make it impossible, or make it fail loudly. A check that runs beats a rule
   that has to be remembered.
2. Where no check is possible, say so in the skill that owns the work, in one
   or two sentences naming what goes wrong and what to compare instead.
3. Delete the prose once a check covers it. Guidance that describes a failure
   something else now catches is read as optional and trains people to skim.

The checks under `.claude/scripts/` exist for that reason, each having replaced
a rule that was broken at least once while written down and believed. The
clearest case is length: a target missed by 28 of 30 merges became a repository
setting, and the prose explaining the arithmetic went with it.

## Worktrees

The `worktree` skill owns all of it, and is the only thing that creates one:
`check-isolation.py` fails CI for any text under `.claude/` that asks the
harness instead. Two rules sit here rather than there. A removal that fails on
uncommitted changes is an escalation, never forced. And a reviewer gets no
worktree: it needs a tree that does not move while it reads, which is a commit
rather than a directory.

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
files named `plan`. That decision is issue 9.

An issue cut from a document cites that document's identifier, so the trail is
readable from either end. This document's is `01M2PWX233GKXE5M9SPTNTGN0D`. What
no document carries is a list of pending work: the board holds that, and a list
of gaps goes stale the moment one closes.

## Layout

| Path                    | Holds                                                          |
| ----------------------- | -------------------------------------------------------------- |
| `.claude/skills/`       | Skills. `.agents/skills` symlinks here.                        |
| `.claude/commands/`     | Slash commands. An alias is a symlink to its canonical file.   |
| `.claude/agents/`       | Subagent definitions for dispatch.                             |
| `.githooks/`            | Git hooks. Enable with `git config core.hooksPath .githooks`.  |
| `internal/`             | Plans, specs, ideas, QA notes. Not published by the docs site. |
| `CLAUDE.md`             | Repository instructions. `AGENTS.md` symlinks here.            |

## Skills

`.claude/skills/` holds one directory per skill, each stating in its own
frontmatter what it owns. That list is not copied here, because a copy drifts
and the directory does not.

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

A claim needs `get_me`. MCP has no `@me`, so the assignee array needs the login
spelled out.

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
