---
description: Run one loop over an issue and its sub-issues
argument-hint: [issue number] [one]
---

Use the `thunderstorm` skill.

Issue: $ARGUMENTS (an issue number, then `one` to stop after the first
sub-issue reaches `status:review`)

Stop and ask if it declares no sub-issues, has more than five open ones, has no
checkable stop rule, or holds one another run has claimed. Read its milestone's
description first where it has one: the order and the file collisions that
cross its sibling issues live only there. Restate the
stop condition before dispatching anything.

Claim each sub-issue, give it a worktree, dispatch an implementer, then run the
review sequence over its pull request. Report each one as it lands and take the
next unless a stop condition fires. Do not review a diff yourself and do not
merge.

End by naming every pull request the run opened and the command that merges it.
