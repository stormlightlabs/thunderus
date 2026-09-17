---
name: decompose
description: Cut an idea, a spec, or a finding into an epic and the sub-issues that implement it. Use for /decomp, /decompose, or when work is understood well enough to file but has not been filed.
---

# Decompose

Turn something understood into something claimable. The input is an idea file,
a spec, or a finding from a review; the output is one epic and the sub-issues
under it. A spec is decomposed after it merges to `edge`, so the issues cite a
document the worker that claims them can read.

`github-board` performs the writes. This skill decides what to write.

## Before filing anything

Every sub-issue needs a stop rule that someone could check without judgment. If
you cannot write one, you have found the boundary of what is understood, and
filing anyway produces an issue that a worker will return as `blocked:spec`.

Two ways out. If one decision is missing, take it to `specify` and file after.
If the work is understood but the criteria are awkward, the unit is usually
wrong: a sub-issue that needs three sentences of conditions is two sub-issues.

An issue whose done-condition is a recorded decision rather than a code change
is fine, and should say so.

## The epic

One epic per body of work, labelled `kind:epic` and **no `status:*`**. An epic
is not claimable, so a status on it is a copy of its sub-issues' state with
somewhere to drift.

`.github/ISSUE_TEMPLATE/epic.yml` holds the fields: the goal, the stop rule, the
planned sub-issues, the source, what the epic must not absorb, and its risk. The
form renders in the web UI and nowhere else, so filing through `github-board`
means reproducing those six as headings in the body and setting `kind:epic` by
hand, which the form would otherwise carry. Risk stays in the body where the
dropdown puts it; an epic takes no `risk:*` label, because that label sizes the
blast radius of a change and an epic makes none.

The stop rule is the epic's own, not a restatement of "every sub-issue is
done": that is already implied, and a stop rule that adds nothing tells a run
nothing about when to stop early.

## The sub-issues

One unit of work each, labelled `status:queued` plus one `type:*`, at least one
`area:*`, and one `risk:*`. Never invent a label; the set is
`.github/labels.yml`.

Each carries:

- **Problem**, with evidence. What is wrong now, and how you know.
- **Done when**, checkable without judgment.
- **Not in this issue**, so the next worker knows what is deliberately absent.

Cite the identifier of the idea or spec it came from. A reader who finds the
issue should reach the reasoning, and a reader who finds the document should
reach the work.

Attach each one to the epic as a sub-issue. The relation is what the next stage
reads: `/thunderstorm` asks GitHub for the epic's children and refuses to start
an epic that declares none, so sub-issues that exist only as lines in the epic's
body are work no run can find. `github-board` carries the calls under **File new
work** — `parent_issue_number` on the create, or `sub_issue_write` method `add`
afterwards, which takes the sub-issue's ID rather than its number.

## Depth

One level. A sub-issue that needs sub-issues of its own is an epic, and the
work belongs in a second epic rather than a deeper tree. `implement` refuses an
issue that has children, so a nested one blocks the worker that claims it.

## Sizing

A sub-issue is one claim, one worktree, one pull request. Two changes that must
land together are one issue; two that could land a week apart are two.

Resist filing everything you can see. An epic of twenty is a backlog wearing a
stop rule, and a run over it never ends. What does not fit is a second epic, or
stays in the idea file until it does.

## Then

Report what was filed, with the epic and each sub-issue. The epic is what
`/thunderstorm` takes.

## Do not

- File an epic carrying a status.
- Invent a label.
- Nest beyond one level.
- File an issue whose criteria you cannot state. That is the signal for
  `specify`, not something to file and fix later.
- Claim or start any of it. Filing and working are separate steps for the
  reason that one person filing and immediately claiming reviews nothing.
