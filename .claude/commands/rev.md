---
description: Standard review pass on a PR or branch
argument-hint: [pr-number|branch]
---

Use the `review` skill in standard mode.

Target: $ARGUMENTS (a PR number, a branch name, or empty for the current branch)

Cover correctness, edge cases, security, concurrency, performance, API
compatibility, test coverage, readability, and complexity. Report every finding
to me. Post them to the pull request only if a signed review pass already ran
against it, in which case report only what survived. Do not edit code.
