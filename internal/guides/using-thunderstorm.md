---
name: using-thunderstorm
last_updated: 2026-09-19
id: 01M2XBVYTZDXTW42ZSCFEEW0QS
---

# Using thunderstorm

For the person running it, not the agent obeying it. Every rule here is stated
somewhere under `.claude/`; this says what to type and when.

`internal/thunderstorm.md` is the protocol. Read that when you want to know why
something is the way it is. Read this when you want to get work done.

## The whole loop

```text
/r-d      talk it through        -> internal/ideas/<name>.md
/decomp   file it                -> an issue with sub-issues
/triage   decide what is next    -> a ranked list, in chat
/storm    run it                 -> pull requests, reviewed
you       merge in GitHub        -> edge
```

The middle two are optional. Filing work you already understand skips `/r-d`.
Running the only thing that is queued skips `/triage`.

## Three shapes, and the one a run takes

| Shape | Looks like | What you do with it |
| --- | --- | --- |
| Epic | `kind:epic` | Nothing directly. It groups issues. |
| Issue with sub-issues | has children, no `kind:epic` | **`/storm <n>`** |
| Sub-issue | has no children | `/impl <n>` for one on its own |

`/storm` takes the middle row and refuses the other two. That is the only
distinction worth holding in your head: an epic is a folder, an issue with
sub-issues is a run, a sub-issue is a pull request.

An issue holds **at most five open sub-issues**, because a run keeps every one
of them in a single context and loses the thread past that. When work outgrows
five, it becomes two issues under one epic.

## What to type

**Starting from nothing.** `/r-d <topic>` to think it through, which writes an
idea file. Then `/decomp <that file>` to file issues from it. Then `/storm <n>`.

**Starting from an issue you already filed.** `/storm <n>` if it has
sub-issues, `/impl <n>` if it does not.

**Not sure what to work on.** `/triage`. It reads the whole board, ranks it,
and ends by printing the command to type next. It writes nothing, so running it
costs you nothing but the time.

**A pull request came back with findings.** The run handles this itself. You
only step in when it stops and asks.

## What a run does, so you know when it has gone wrong

`/storm <n>` claims a sub-issue, gives it a worktree, dispatches an
implementer, and opens a pull request. Then three review passes with a fix pass
after each. Then it reports and moves to the next sub-issue.

It stops on its own when every sub-issue is terminal, one hits
`status:blocked`, an edit pass runs out of rounds, or the work needs a decision
the issues do not record. Add `one` to the argument — `/storm 75 one` — to stop
after the first sub-issue instead.

If it stops for any other reason, that is the bug.

## Merging is yours

Nothing under `.claude/` merges a pull request. `settings.json` denies
`gh pr merge`, `gh pr review`, `git merge`, and pushes to `edge` and `main`, so
an agent cannot merge or approve its own work even when told to.

Merge in GitHub, squash. The pull request title and body become the commit
verbatim, so what you see in the merge box is what lands in `git log`. Then move
the issue to `status:verify` yourself; nothing does it for you.

## When it feels like too much ceremony

Most of it is skippable and the loop still works:

- One small fix, already understood: `/impl <n>`. No epic, no run, no triage.
- Something you want to think about: `/r-d`. It writes a file and files nothing.
- A board you have lost track of: `/triage`. It answers and changes nothing.

The full sequence is for work that spans several pull requests and that you do
not want to hold in your head. It is not the price of entry.

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
