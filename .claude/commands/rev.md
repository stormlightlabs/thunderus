---
description: Standard review pass on a PR or branch
argument-hint: [pr-number|branch] [first|second]
---

Use the `review` skill in standard mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current
branch, then `first` or `second` for which pass this is; without it, first)

Cover correctness, edge cases, security, concurrency, performance, API
compatibility, test coverage, readability, and complexity. Prose the diff
changes counts: judge it against the `writing-docs` skill, length targets
included.

Report every finding to me, one line each, most severe first. A first pass
posts nothing to the pull request. A second posts what survived the first, or
says nothing did. Keep a posted comment under 200 words, opening with one line
naming the pass, the commit, and the model you ran at. Do not edit code.
