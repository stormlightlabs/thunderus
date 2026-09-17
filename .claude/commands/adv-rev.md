---
description: Adversarial review pass on a PR or branch
argument-hint: [pr-number|branch]
---

Use the `review` skill in adversarial mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current branch)

Assume the standard passes already ran. Look only for what they missed:
security holes, races, unhandled input, violated invariants, tests that cannot
fail, and complexity that invites any of them. Do not repeat earlier findings.
Always post one signed comment, even to record that nothing was found. Do not
edit code.
