---
name: decompose
description: Cut an idea, a spec, or a finding into an epic where one is needed, the issues a run takes, and the sub-issues that implement them. Use for /decomp, /decompose, or when work is understood well enough to file but has not been filed.
---

# Decompose

Turn something understood into something claimable. The input is an idea file,
a spec, or a finding from a review; the output is the issues a run can take and
the sub-issues under them, inside an epic when the work needs more than one
run. A spec is decomposed after it merges to `edge`, so the
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

## Three shapes, and which one a run takes

| Shape | Label | What it is |
| --- | --- | --- |
| Epic | `kind:epic` | Groups issues. Holds what crosses them. Never dispatched. |
| Issue with sub-issues | none | **What `/storm` takes.** At most five open sub-issues. |
| Sub-issue | none | One claim, one branch, one pull request. `/impl` takes it. |

The run unit is the middle row, and it is identified by having sub-issues and
no `kind:epic`. That is the whole test, so no second label is needed.

An epic is not a bigger version of the middle row. It exists for what a single
run cannot see: a blocker between two issues under it, an ordering across them,
a file two of them both write. Put those in the epic and the runs below it
inherit an order none of them could work out alone.

## The issue a run takes

One per body of work that fits a run, with **no `status:*`** and no `kind:*`. It
is not claimable, so a status on it is a copy of its sub-issues' state with
somewhere to drift, and it takes no `risk:*`, because that label sizes the blast
radius of a change and this makes none.

`.github/ISSUE_TEMPLATE/issue-with-sub-issues.yml` holds the fields: the goal, the stop rule,
the planned sub-issues, the source, and what it must not absorb. The form
renders in the web UI and nowhere else, so filing through `github-board` means
reproducing those five as headings in the body.

The stop rule is its own, not a restatement of "every sub-issue is done": that
is already implied, and a stop rule that adds nothing tells a run nothing about
when to stop early.

## The epic

File one when a body of work needs more than one run, and only then. An epic of
one issue is a heading.

`.github/ISSUE_TEMPLATE/epic.yml` holds its fields. It carries no `status:*`,
no `risk:*`, and no size cap: a container costs a run nothing, because no run
loads it. What it must carry is the part that would otherwise be lost — every
dependency between its children, and every file two of them both write. A run
works one issue and cannot see its siblings, so an epic recording neither
leaves each run to rediscover the ordering or collide.

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

Attach each one as a sub-issue of the issue a run takes. The relation is what the next
stage reads: `/storm` asks GitHub for an issue's children and refuses one that
declares none, so sub-issues that exist only as lines in a body are work no run
can find. `github-board` carries the calls under **File new
work** — `parent_issue_number` on the create, or `sub_issue_write` method `add`
afterwards, which takes the sub-issue's ID rather than its number.

Where one sub-issue cannot start until another lands, record that as a
dependency as well. The sub-issue relation says what the work is part of; it
says nothing about order, and work whose order lives only in prose gets
dispatched out of it. `github-board` carries the calls under **Dependencies**.

A dependency is not `status:blocked`. That label ends a run, and it is for a
block found while working. A sub-issue waiting on a sibling stays
`status:queued`.

## Recording overlap

Two issues can be independent in the dependency graph and still write the same
file, so the body above them carries what the graph cannot: the run unit's body
for its own sub-issues, and the epic's for what crosses its children. Both parts below are
read by `/triage` when it decides what several threads may work at once, and a
parent recording neither is one whose sub-issues all have to be worked in
sequence.

Give the issue a table of each sub-issue against the paths it owns:

```markdown
| Sub-issue | Owns |
| --- | --- |
| #76 assert palette contrast | `cli/renderer/style.rs` |
| #79 remove or bind `--thndrs-pink` | `docs/src/styles/theme.css` |
```

Then name, in prose beside it, every collision with another issue's sub-issues,
and what to do about it. That half matters more than the table: a run works one
issue, so an overlap with a second is invisible to both runs and to every
reviewer reading either one. Say which issue to sequence around which, not
merely that they touch.

Where both sit under one epic, that prose belongs in the epic instead. It is
the only place a reader of either issue is certain to reach.

A path nobody can name yet is a sub-issue whose scope is still open. Say so in
the table rather than leaving the row out, so a later pass treats it as
overlapping instead of as unexamined.

## Depth

Three tiers at most: epic, the issue a run takes, its sub-issues. A sub-issue
that needs sub-issues of its own is a second run unit, filed beside the first
and under the same epic, never nested deeper. `implement` refuses an issue that
has children, so a nested one blocks the worker that claims it.

## Sizing

A sub-issue is one claim, one branch, one pull request. Two changes that must
land together are one issue; two that could land a week apart are two.

**At most five open sub-issues under one run unit.** A run holds every
sub-issue it dispatches in one context, and nine spends that context on work
the dispatcher is not doing yet. Closed sub-issues do not count: they are
finished, and an issue close to done should not have to be split to be worked.

What does not fit becomes a second issue under the same epic, or stays in the
idea file until it does. The epic is where the two then record what they share:
the ordering between them, and the files they both write.

An epic has no cap. Nothing loads it.

## Then

Report what was filed: the epic if there is one, each issue under it, and each
sub-issue. The number `/storm` takes is the middle tier, never the epic.

## Do not

- File an epic or a run unit carrying a status or a risk.
- Invent a label.
- Nest beyond the three tiers.
- File a sixth open sub-issue rather than splitting into a second issue.
- File an epic holding one issue. That is a heading, not a container.
- File an issue whose criteria you cannot state. That is the signal for
  `specify`, not something to file and fix later.
- Claim or start any of it. Filing and working are separate steps for the
  reason that one person filing and immediately claiming reviews nothing.
