---
description: Run one loop over a parent issue and its sub-issues
argument-hint: [issue number] [one]
---

Use the `thunderstorm` skill.

Parent issue: $ARGUMENTS (an issue number, then `one` to stop after the first
sub-issue reaches `status:review`)

Stop and ask if it declares no sub-issues, has more than five open ones, has no
checkable stop rule, or holds a sub-issue another run has claimed. Restate the
stop condition before dispatching anything.

Claim each sub-issue, give it a worktree, dispatch an implementer, then run the
review sequence over its pull request. Report each one as it lands and take the
next unless a stop condition fires. Do not review a diff yourself and do not
merge.

End by naming every pull request the run opened and the command that merges it.
