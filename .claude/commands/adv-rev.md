---
description: Adversarial review pass on a PR or branch
argument-hint: [pr-number|branch]
---

Use the `review` skill in adversarial mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current branch)

Assume the standard passes already ran. Look only for what they missed:
security holes, races, unhandled input, violated invariants, and tests that
cannot fail. Do not repeat earlier findings. Post one signed comment. Do not
edit code.
