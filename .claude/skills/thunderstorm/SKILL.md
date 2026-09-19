---
name: thunderstorm
description: Run one thunderstorm loop over an issue and its sub-issues. Dispatches work, tracks status, reports, and stops. Use for /storm, /thunderstorm, or when asked to run an issue with sub-issues.
---

# Thunderstorm

One run covers one issue and the sub-issues declared under it. A human starts
every run. Nothing here runs on a schedule.

A milestone is not that issue. It groups several of them and holds what
crosses them, and it is not an issue at all, so there is nothing there to
dispatch. Take the issues in a milestone one run each.

This skill dispatches and reports. It writes no code and edits no files; the
one thing it does in the checkout is create and remove the worktrees it hands
out. See `internal/thunderstorm.md` for the protocol this obeys.

## Start

The argument is the number of an issue that has sub-issues. Read it and its
children:

The `github-board` skill's Transport section decides whether this run uses `gh`
or the GitHub MCP tools. Use the same transport for everything below.

```sh
gh issue view <n> --json number,title,body,labels,url
gh issue view <n> --json subIssues
```

Through MCP: `issue_read` method `get`, then method `get_sub_issues`.

Stop and ask when any of these is true:

- It declares no sub-issues. It is a unit of work; use `/impl` instead.
- It has more than five open sub-issues. Split it through `/decomp` first: a
  run holds every sub-issue it dispatches in one context, and past five that
  context is spent on work not yet started.
- Its `Done when` condition is missing or not checkable.
- Any sub-issue is already `status:claimed` by another run.

Restate the stop condition before dispatching anything. A run whose ending you
cannot state is not ready to start.

## Dispatch

Take sub-issues one at a time unless two are genuinely independent and own
non-overlapping files. Two writers in one area produce rework, not throughput.
The `triage` skill's **Lanes** section says what may share a fan-out; a run
taking two at once applies those rules rather than deciding again.

Read each sub-issue's `blocked_by` before claiming it, through the
**Dependencies** section of `github-board`. An issue whose blockers are still
open is not claimable, whatever its status label says, and dispatching one
anyway produces a worker with nothing to build on. Independent in the dependency
graph is not the same as safe to run at once: check the file ownership this
issue records, and its milestone's description for what crosses to a sibling,
before taking two at a time.

Every implementer you dispatch gets its own worktree, on either host, and you
create it before dispatching rather than leaving the subagent to. Two of them
sharing a checkout share one `HEAD`, and the second to create its branch takes
the first's work onto it without git raising anything. Make it through the
`worktree` skill and pass no `isolation` on the dispatch itself. Tell the
implementer the directory it is to work in. That argument and whether the
implementer goes there are the two parts of this no check covers, so read its
report for the paths it touched rather than assuming.

For each sub-issue:

1. Claim it through the `github-board` skill.
2. Give it a working tree through the `worktree` skill.
3. Dispatch an implementer. Give it the issue number, the acceptance criteria,
   the file ownership, the working directory, and the verification command.
   Nothing else.
4. When the pull request opens, move the issue to `status:review`.

Use the model assignments in `internal/models.md`. The implementer and the
reviewer never share a model within one run.

## Review

Do not review the diff yourself. The `review` skill owns that, and the
orchestrator forming its own verdict defeats the separation the sequence exists
to create.

The sequence per pull request is `/rev`, `/edit`, `/rev`, `/edit`, `/adv-rev`,
`/edit`. Then stop. A human merges to `edge`.

Tell each pass which one it is, because nothing else can: a pass that reads the
thread to work it out gets the answer wrong. The first `/rev` reports its
findings to you and posts nothing, so hand them to the `/edit` that follows and
to the second `/rev`. They are the edit pass's only input, and the second pass
cannot say what survived the first without them. From the second pass on, each
pass comments for itself.

## Report and continue

Report each sub-issue as it reaches `status:review`:

- what was claimed, and what its pull request number is;
- what verification ran and what it returned;
- what was filed as new work rather than absorbed;
- what remains queued under the issue.

Then take the next one. A run that stops after every sub-issue to ask makes the
operator the scheduler, which is the job this skill exists to do. Stop only on a
stop condition below, or when the argument was `one`.

End the run by listing every pull request it opened and whether its checks are
green, so the operator sees what is waiting on them in one place. The merge
itself is theirs: nothing here merges, and nothing here tells them a command to
run that would.

## Stop conditions

Any of these ends the run:

- Every sub-issue is terminal: `status:done`, `status:verify`, or
  `status:dropped`.
- The argument was `one` and the first sub-issue reached `status:review`.
- A sub-issue hits `status:blocked`.
- An edit pass exhausts its 5 cycles, or the same finding recurs unchanged.
- The work needs a decision the issues do not record.

Report the reason and what remains. Do not open new work to keep a run alive.

## Scope

A worker that finds adjacent work files a new issue beside this one and does
not start it. Self-expanding scope is how a run stops being one.

The `Not in this issue` section is binding, and so is the milestone's. Work named there gets
filed, never absorbed.

## Do not

- Write code, edit files, or work in an implementer's tree.
- Merge, approve, or push to `edge` or `main`.
- Change the protocol mid-run.
- Continue past a stop condition because the remaining work looks small.
