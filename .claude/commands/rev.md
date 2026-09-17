---
description: Standard review pass on a PR or branch
argument-hint: [pr-number|branch]
---

Use the `review` skill in standard mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current branch)

Cover correctness, edge cases, security, concurrency, performance, API
compatibility, test coverage, and readability. Post one comment with all
findings, signed with the model and reasoning level. Do not edit code.
