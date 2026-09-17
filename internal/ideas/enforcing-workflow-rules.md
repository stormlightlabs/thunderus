---
name: enforcing-workflow-rules
last_updated: 2026-09-17
id: 01M2RH6SW2KZHH0V3BRW0NVMKT
---

# Enforcing the workflow's own rules

## Problem

The workflow states rules as musts and checks almost none of them.
`internal/thunderstorm.md` and `.claude/skills/github-board/SKILL.md` carry
thirteen sentences between them phrased as a prohibition or an obligation. Two
workflows exist, `ci.yml` and `labels.yml`, and neither reads an issue.

The board is the part that matters. Status is the state, and `github-board`
says two status labels on one issue make the loop "pick the wrong transition".
That is silent misrouting, not a visible failure, and nothing would notice it.

The gap is not theoretical. Over 2026-09-17, building the cloud path, the rules
that had a check were the ones that held:

| What went wrong                      | Caught by                        |
| ------------------------------------ | -------------------------------- |
| Five commit messages over the limits | The commit-msg hook, once it ran |
| Doubled blank lines from an edit     | `cargo fmt` in CI                |
| A parse failure reported as success  | The label sync test, once it ran |
| Two false claims that a push landed  | A check that had to be invoked   |
| A false claim that a rename was done | An adversarial reviewer          |

The last two rows are the argument against enforcement as the whole answer.
Both were assertions made without the verification they implied: a push
reported from an exit code rather than from the remote, and a repository-wide
claim made from a search of three directories. No check catches a confident
statement about work that was not done. A reviewer did.

## Decisions

Enforcement is three levels, and the answer differs per rule: prevention makes
a state impossible, detection names it, blocking refuses to continue. Only
blocking creates pressure to bypass, and conflating the three is how a rule
that deserved a whisper ends up stopping work.

Enforce the board invariants, at detection. One `status:*` per issue, an epic
carries none, `status:claimed` has an assignee, `status:blocked` has a
`blocked:*` reason. These are machine-checkable without judgment, they fail
silently today, and the board is what routes every run.

A board check does not block a merge. The board and the code are separate
systems; a mislabelled issue is not a reason to stop a green pull request.

Do not enforce the process rules: a worktree per concurrent writer, the review
sequence, the model assignments. None is observable from outside the session
that performed it, so a check would inspect artifacts a careless run produces
just as well as a careful one. That buys the appearance of rigour and none of
it.

A guard that misfires is worse than the rule written down. The commit-msg hook
shipped rejecting `git commit -v` on its own diff and refusing `git merge`
outright, both found in adversarial review. The only way past either was
`--no-verify`, which also skips the check for the next message, so a false
positive does not cost one commit, it costs the rule.

Enforcement and review are not substitutes. Checks catch drift; the adversarial
pass catches claims. Dropping either leaves a class of failure uncovered, and
the class review covers is the more expensive one.

## Open

Whether board violations actually occur. Six issues and one epic exist as of
this writing, all conforming, none of them worked by a run yet. If twenty
issues pass through without a violation the check is ceremony and should be
deleted rather than kept for its own sake.

What level a board check should sit at if violations turn out to be common
rather than rare. Detection assumes someone reads the comment.

How to make a self-invoked check automatic. `.claude/scripts/push-verified.sh`
prevents exactly the failure you have when you are not thinking about it, and
it only runs when someone remembers to call it. Nothing in git can force it: a
pre-push hook is not run when the push has no ref to update, which is the case
it exists for.

## Sources

- `internal/thunderstorm.md` (`01M2PWX233GKXE5M9SPTNTGN0D`) and
  `.claude/skills/github-board/SKILL.md`, for the rules stated as musts and the
  claim that two status labels misroute the loop.
- #1 and #2, for the failures in the table and the checks added in response.
- The adversarial review on #2, for the commit-msg hook defects: a verbose
  commit graded on its appended diff, and a merge commit rejected for the
  message git writes itself.
- `.github/workflows/`, for the two workflows that exist and the absence of any
  trigger on an issue event.
