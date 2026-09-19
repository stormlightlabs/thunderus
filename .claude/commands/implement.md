---
description: Work a GitHub issue on an agent branch and open a PR
argument-hint: [issue number]
---

Use the `implement` skill.

Issue: $ARGUMENTS

Read the issue and its acceptance criteria first. Stop and ask if it has
sub-issues, has no verifiable criteria, or is already claimed. Claim it, work it
on an `agent/` branch in the tree the `worktree` skill gives you, and open a
pull request against `edge`.
