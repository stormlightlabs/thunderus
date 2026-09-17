---
description: Adversarial review pass on a PR or branch
argument-hint: [pr-number|branch]
---

Use the `review` skill in adversarial mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current branch)

Assume the standard passes already ran. Look only for what they missed:
security holes, races, unhandled input, violated invariants, tests that cannot
fail, documentation that contradicts the code, and complexity that invites any
of them. Do not repeat a finding an earlier pass raised, whether it is on the
thread or was handed to you with this target.

Name the input, ordering, or state that reaches each failure, and mark a
finding you could not verify `unverified`. Few supported findings beat a long
list. Always post one signed comment under 40 lines, even to record that
nothing was found. Do not edit code.
