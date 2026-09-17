---
name: enforcing-workflow-rules
last_updated: 2026-09-17
id: 01M2RH6SW2KZHH0V3BRW0NVMKT
---

# Enforcing the workflow's own rules

## Problem

The workflow states rules and checks almost none of them.
`internal/thunderstorm.md` and `.claude/skills/github-board/SKILL.md` write
theirs in the imperative. A few are prohibitions, such as "Do not invent
labels". Most are bare obligations: "One writer at a time", or "Remove the
worktree when the run ends".

Neither file uses the word "must", so no keyword count finds these rules. Two
drafts of this file reported different totals and neither survived a recount.

Two workflows exist, `ci.yml` and `labels.yml`. Neither is triggered by an
issue event and neither checks issue state, though `labels.yml` reads issues to
count what carries a label.

Every run reads the board to pick its next issue and writes the transition
back. Status is the state, and `github-board` says two status labels on one
issue make the loop "pick the wrong transition". That is silent misrouting, not
a visible failure, and nothing would notice it.

The gap is not theoretical. Over 2026-09-17, building the cloud path, the rules
that had a check were the ones that held:

| What went wrong                      | Caught by                        |
| ------------------------------------ | -------------------------------- |
| Five commit messages over the limits | The commit-msg hook, locally     |
| Doubled blank lines from an edit     | `cargo fmt` in CI                |
| A parse failure reported as success  | The label sync test, once it ran |
| Two false claims that a push landed  | Nothing, until CI showed no run  |
| A false claim that a rename was done | An adversarial reviewer          |

The first row overstates its check. The hook grades what a committer writes
locally. GitHub composes a squash message server-side and appends ` (#NN)`, so
neither the hook nor the CI `commits` job sees the message that lands on
`edge`. Two of the three squashes merged since the hook shipped exceed the
59-character subject limit: `3af907e` at 62, `f51217e` at 65.

The last two rows are the argument against enforcement as the whole answer.
Both were assertions made without the verification they implied: a push
reported from an exit code rather than from the remote, and a repository-wide
claim made from a search of three directories. No check catches a confident
statement about work that was not done. `push-verified.sh` exists because of
the first, but it was written afterwards and caught nothing; a missing CI run
did. A reviewer caught the second.

## Decisions

Enforcement is three levels, and the answer differs per rule: prevention makes
a state impossible, detection names it, blocking refuses to continue. Only
blocking creates pressure to bypass, and conflating the three is how a rule
that deserved a whisper ends up stopping work.

Enforce the board invariants, at detection. They are machine-checkable without
judgment and they fail silently today. Take them from their definitions: the
Statuses table in `thunderstorm.md`, and the Claim and Rules sections of
`github-board`.

Restating them here does not work. A draft of this file listed four from memory
and had already lost three. It dropped that an epic has no owner as well as no
status, that an issue carrying an assignee is not claimable, and that a claim
succeeds only when `assignees` is exactly the claimant. `thunderstorm.md` names
the failure. A duplicated state gives the copy somewhere to drift.

The short form of an invariant misfires. "`status:claimed` has an assignee"
reads like one. `github-board` creates that state deliberately: it orders the
label before the assignment, so a session killed between the two calls leaves
the issue claimed by nobody. `thunderstorm.md` already disposes of it.
Claimed for more than 24 hours with no pull request returns to the queue.

A check written on the assignee would report every claim in flight, so the 24
hours is the thing to measure.

A board check does not block a merge. The board and the code are separate
systems; a mislabelled issue is not a reason to stop a green pull request.

Do not enforce the process rules: a worktree per concurrent writer, the review
sequence, the model assignments. No session's worktree is visible from outside
it. The other two do reach the record. `thunderstorm.md` requires every review
comment to end with a signature naming the model and its reasoning level, and
`internal/models.md` repeats that. An implementer's model arrives on the pull
request in a `Co-Authored-By:` trailer.

Those artifacts record what the session said about itself. A check over them
reaches the claim and never the fact, and a run that skipped a pass is the
least likely to report it. The two false push claims above are the same
failure, which is why a reviewer caught them and no check did.

A guard that misfires is worse than the rule written down. The commit-msg hook
shipped rejecting `git commit -v` on its own diff and refusing `git merge`
outright, both found in adversarial review. The only way past either was
`--no-verify`, which also skips the check for the next message, so a false
positive does not cost one commit, it costs the rule.

Some rules should be deleted instead of checked. `github-board` says "Do not
close an issue to express any state other than `dropped`". Every merge
contradicts it. A pull request body carrying `Closes #N` closes its issue on
merge, which is what the keyword is for. Issue #8 is closed now, labelled
`status:review`, by that route.

Honouring that rule would mean banning closing keywords, or reopening issues by
hand after every merge. A check written from it would fire on every issue the
workflow completes. The rule is gone from `github-board`, which now says the
`status:*` label carries the status and open-or-closed does not.

Shipping a check does not oblige deleting the prose. Step 3 of Recording a
failure mode says to delete guidance once a check covers it, because a rule
stated twice trains people to skim. That holds where the check prevents or
blocks. A detection check only names what it finds, in a comment someone has to
read. The prose is still what tells a run what to do, so step 3 applies at
prevention and blocking and not at detection.

Enforcement and review are not substitutes. Checks catch drift; the adversarial
pass catches claims. Dropping either leaves a class of failure uncovered, and
the class review covers is the more expensive one.

## Open

Whether board violations actually occur. One issue has been through a full
pass. #8 went `queued` to `claimed` to `review` with pull request #10 open, and
conformed at every step. It ended closed at `status:review` without reaching
`verify`, by the auto-close above, which these decisions treat as correct.

That is one clean pass. If twenty issues pass through without a violation the
check is ceremony and should be deleted rather than kept for its own sake.
Nothing counts completed passes today, so the twenty cannot be observed until
the count has somewhere to live.

What level a board check should sit at if violations turn out to be common
rather than rare. Detection assumes someone reads the comment. Settled by the
first violation that reaches a merge unnoticed: if the comment was posted and
nobody acted on it, detection is the wrong level.

Whether a pre-push hook should carry `push-verified.sh`. The reason recorded
for keeping it self-invoked is wrong. `internal/thunderstorm.md` and the
script's own header say git runs no pre-push hook when the push has no ref to
update.

On git 2.43.0 it does. A no-op push runs the hook with empty stdin and prints
`Everything up-to-date`. A hook that refuses on empty stdin fails the push
with exit 1.

The detached-HEAD case does not arise by default either. Under
`push.default=simple` git refuses with `You are not currently on a branch`.
Reaching `Everything up-to-date` from a detached HEAD needed
`push.default=matching`. A hook is worth trying here. Both places recording the
old reason now state the verified behaviour instead.

## Sources

- `internal/thunderstorm.md` (`01M2PWX233GKXE5M9SPTNTGN0D`) and
  `.claude/skills/github-board/SKILL.md`, for the rules stated in the
  imperative, the claim that two status labels misroute the loop, the claim
  order, the 24-hour rule, and step 3 of Recording a failure mode.
- #1 and #2, for the failures in the table and the checks added in response.
- The adversarial review on #1, for the commit-msg hook defects: a verbose
  commit graded on its appended diff, and a merge commit rejected for the
  message git writes itself.
- `.github/workflows/`, for the two workflows that exist and the absence of any
  trigger on an issue event.
- #8 and #10, for the one completed pass and for the auto-close.
- git 2.43.0, for the pre-push behaviour, reproduced against a scratch remote
  rather than recalled.
