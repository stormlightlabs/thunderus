---
description: Standard review pass on a PR or branch
argument-hint: [pr-number|branch] [first|second]
---

Use the `review` skill in standard mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current
branch, then `first` or `second` for which pass this is; without it, first)

Cover correctness, edge cases, security, concurrency, performance, API
compatibility, test coverage, readability, and complexity. Report every finding
to me. A first pass posts nothing to the pull request. A second posts what
survived the first, or says nothing did. Do not edit code.
