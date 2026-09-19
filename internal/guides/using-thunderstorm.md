---
name: using-thunderstorm
last_updated: 2026-09-19
id: 01M2XBVYTZDXTW42ZSCFEEW0QS
---

# Using thunderstorm

What to type and when. `internal/thunderstorm.md` holds the protocol and the
reasoning behind it; this is the operator's side of the same thing.

## The whole loop

```text
/r-d      talk it through        -> internal/ideas/<name>.md
/decomp   file it                -> an issue with sub-issues
/triage   decide what is next    -> a ranked list, in chat
/storm    run it                 -> pull requests, reviewed
you       merge in GitHub        -> edge
```

The middle two are optional. Filing work you already understand skips `/r-d`,
and running the only thing that is queued skips `/triage`.

## The three shapes

| Shape | Looks like | What you do with it |
| --- | --- | --- |
| Milestone | not an issue | Nothing directly. It groups issues. |
| Issue with sub-issues | has children | **`/storm <n>`** |
| Sub-issue | has no children | `/impl <n>` for one on its own |

`/storm` takes the middle row. A milestone is not an issue, so there is no way
to dispatch one and no rule to remember about it.

An issue holds at most five open sub-issues, because a run keeps every one of
them in a single context and loses the thread past that. When work outgrows
five, it becomes two issues in one milestone.

## What to type

Starting from nothing, `/r-d <topic>` thinks it through and writes an idea
file. `/decomp <that file>` files issues from it. Then `/storm <n>`.

Starting from an issue you already filed, `/storm <n>` if it has sub-issues and
`/impl <n>` if it does not.

When you are not sure what to work on, `/triage` reads the whole board, ranks
it, and ends by printing the command to type next. It writes nothing.

When a pull request comes back with findings, the run handles them. You step in
where it stops and asks.

## What a run does

`/storm <n>` claims a sub-issue, gives it a worktree, dispatches an
implementer, and opens a pull request. Then three review passes with a fix pass
after each. Then it reports and moves to the next sub-issue.

It stops when every sub-issue is terminal, one hits `status:blocked`, an edit
pass runs out of rounds, or the work needs a decision the issues do not record.
Add `one` to the argument, as `/storm 75 one`, to stop after the first
sub-issue. A stop for any other reason is a bug.

## Merging is yours

Nothing under `.claude/` merges a pull request. `settings.json` denies
`gh pr merge`, `gh pr review`, `git merge`, and pushes to `edge` and `main`, so
an agent cannot merge or approve its own work even when told to.

Merge in GitHub, squash. The pull request title and body become the commit
verbatim, so what you see in the merge box is what lands in `git log`. Then move
the issue to `status:verify` yourself; nothing does it for you.

## Skipping most of it

A small fix you already understand is `/impl <n>`: no milestone, no run, no
triage. Something you want to think about is `/r-d`, which writes a file and
files nothing. A board you have lost track of is `/triage`, which answers and
changes nothing.

The full sequence is for work spanning several pull requests that you would
rather not hold in your head.

## Milestones

A milestone groups the issues of one body of work. Its description takes
markdown and holds the order they run in and what crosses them: a dependency
between two, a file both write. A run cannot see any of that from inside one
issue.

```sh
gh issue list --milestone "UI Polish"
gh issue edit 75 --milestone "UI Polish"
gh api repos/stormlightlabs/thunderus/milestones          # create or edit
```

There is no `gh milestone` command, so creating one goes through `gh api`.
Where a plan document under `internal/features/` covers the same work, it names
the milestone in its frontmatter.

## The status labels, briefly

`queued` is ready and unowned. `claimed` means a run has it. `review` means a
pull request is open. `verify` means it is on `edge` and you have not confirmed
it. `done` means it shipped from `main`. `blocked` needs a `blocked:*` reason
beside it, and ends a run.

You set `verify` after merging. A run sets the rest.

## Where things are

| Want | Look at |
| --- | --- |
| Why a rule exists | `internal/thunderstorm.md` |
| What a command does | `.claude/commands/<name>.md` |
| How a stage works | `.claude/skills/<name>/SKILL.md` |
| What models run which role | `internal/models.md` |
| Length targets | `.claude/skills/writing-docs/SKILL.md` |
