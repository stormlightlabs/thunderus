---
name: thunderstorm
description: Run one thunderstorm loop over an epic and its sub-issues. Dispatches work, tracks status, reports, and stops. Use when asked to run an epic, work a run, or drive several sub-issues to completion.
---

# Thunderstorm

One run covers one epic and the sub-issues declared under it. A human
starts every run. Nothing here runs on a schedule.

This skill dispatches and reports. It writes no code and touches no working
tree. See `internal/thunderstorm.md` for the protocol this obeys.

## Start

The argument is an epic's issue number. Read it and its children:

The `github-board` skill's Transport section decides whether this run uses `gh`
or the GitHub MCP tools. Use the same transport for everything below.

```sh
gh issue view <n> --json number,title,body,labels,url
gh issue view <n> --json subIssues
```

Through MCP: `issue_read` method `get`, then method `get_sub_issues`.

Stop and ask when any of these is true:

- The issue has no `kind:epic` label. It is a unit of work; use `/impl` instead.
- It declares no sub-issues.
- Its `Done when` condition is missing or not checkable.
- Any sub-issue is already `status:claimed` by another run.

Restate the stop condition before dispatching anything. A run whose ending you
cannot state is not ready to start.

## Dispatch

Take sub-issues one at a time unless two are genuinely independent and own
non-overlapping files. Two writers in one area produce rework, not throughput.

Each implementer works in its own worktree, on either host. Two sharing a
checkout share one `HEAD`, and the second to create its branch takes the
first's work onto it without git raising anything. The `worktree` skill has the
mechanism.

For each sub-issue:

1. Claim it through the `github-board` skill.
2. Create its worktree through the `worktree` skill.
3. Dispatch an implementer. Give it the issue number, the acceptance criteria,
   the file ownership, and the verification command. Nothing else.
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

## Report and stop

Pause after each sub-issue reaches `status:review` and report:

- what was claimed, and what its pull request number is;
- what verification ran and what it returned;
- what was filed as new work rather than absorbed;
- what remains queued under the epic.

Then stop. The human decides whether the run continues.

## Stop conditions

Any of these ends the run:

- Every sub-issue is terminal: `status:done`, `status:verify`, or
  `status:dropped`.
- A sub-issue hits `status:blocked`.
- An edit pass exhausts its 5 cycles, or the same finding recurs unchanged.
- The work needs a decision the issues do not record.

Report the reason and what remains. Do not open new work to keep a run alive.

## Scope

A worker that finds adjacent work files a new issue under the epic and does
not start it. Self-expanding scope is how a run stops being one.

The epic's `Not in this epic` section is binding. Work named there gets
filed, never absorbed.

## Do not

- Write code, edit files, or enter a worktree.
- Merge, approve, or push to `edge` or `main`.
- Change the protocol mid-run.
- Continue past a stop condition because the remaining work looks small.
