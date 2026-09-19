---
name: decompose
description: Cut an idea, a spec, or a finding into a parent issue and the sub-issues that implement it. Use for /decomp, /decompose, or when work is understood well enough to file but has not been filed.
---

# Decompose

Turn something understood into something claimable. The input is an idea file,
a spec, or a finding from a review; the output is one parent issue and the
sub-issues under it. A spec is decomposed after it merges to `edge`, so the
issues cite a document the worker that claims them can read.

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

## The parent issue

One parent per body of work, labelled `kind:parent` and **no `status:*`**. A
parent is not claimable, so a status on it is a copy of its sub-issues' state
with somewhere to drift.

`.github/ISSUE_TEMPLATE/parent.yml` holds the fields: the goal, the stop rule,
the planned sub-issues, the source, and what the parent must not absorb. The
form renders in the web UI and nowhere else, so filing through `github-board`
means reproducing those five as headings in the body and setting `kind:parent`
by hand, which the form would otherwise carry. A parent takes no `risk:*`
label, because that label sizes the blast radius of a change and a parent makes
none.

The stop rule is the parent's own, not a restatement of "every sub-issue is
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

Attach each one to the parent as a sub-issue. The relation is what the next
stage reads: `/storm` asks GitHub for an issue's children and refuses one
that declares none, so sub-issues that exist only as lines in the parent's body
are work no run can find. `github-board` carries the calls under **File new
work** — `parent_issue_number` on the create, or `sub_issue_write` method `add`
afterwards, which takes the sub-issue's ID rather than its number.

Where one sub-issue cannot start until another lands, record that as a
dependency as well. The sub-issue relation says what the work is part of; it
says nothing about order, and a parent whose order lives only in prose gets
dispatched out of it. `github-board` carries the calls under **Dependencies**.

A dependency is not `status:blocked`. That label ends a run, and it is for a
block found while working. A sub-issue waiting on a sibling stays
`status:queued`.

## Recording overlap

Two issues can be independent in the dependency graph and still write the same
file, so the parent's body carries what the graph cannot. Both parts below are
read by `/triage` when it decides what several threads may work at once, and a
parent recording neither is one whose sub-issues all have to be worked in
sequence.

Give the parent a table of each sub-issue against the paths it owns:

```markdown
| Sub-issue | Owns |
| --- | --- |
| #76 assert palette contrast | `cli/renderer/style.rs` |
| #79 remove or bind `--thndrs-pink` | `docs/src/styles/theme.css` |
```

Then name, in prose beside it, every collision this parent has with another
parent's sub-issues, and what to do about it. That half matters more than the
table: a run works one parent, so an overlap with a second is invisible to both
runs and to every reviewer reading either one. Say which issue to sequence
around which, not merely that they touch.

A path nobody can name yet is a sub-issue whose scope is still open. Say so in
the table rather than leaving the row out, so a later pass treats it as
overlapping instead of as unexamined.

## Depth

One level. A sub-issue that needs sub-issues of its own is a parent, and the
work belongs in a second parent filed beside this one rather than in a deeper
tree. `implement` refuses an issue that has children, so a nested one blocks the
worker that claims it.

## Sizing

A sub-issue is one claim, one branch, one pull request. Two changes that must
land together are one issue; two that could land a week apart are two.

**At most five open sub-issues under one parent.** A run holds every sub-issue
it dispatches in one context, and a parent of nine spends that context on work
the dispatcher is not doing yet. Closed sub-issues do not count: they are
finished, and a parent close to done should not have to be split to be worked.

What does not fit is a second parent, or stays in the idea file until it does.
Two parents that share a goal are two parents; there is no tier above them, and
the goal lives in both stop rules rather than in a container issue.

## Then

Report what was filed, with the parent and each sub-issue. The parent's number
is what `/storm` takes.

## Do not

- File a parent carrying a status or a risk.
- Invent a label.
- Nest beyond one level.
- File a sixth open sub-issue under a parent rather than splitting it.
- File an issue whose criteria you cannot state. That is the signal for
  `specify`, not something to file and fix later.
- Claim or start any of it. Filing and working are separate steps for the
  reason that one person filing and immediately claiming reviews nothing.
