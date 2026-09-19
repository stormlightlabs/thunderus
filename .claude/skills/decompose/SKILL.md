---
name: decompose
description: Cut an idea, a spec, or a finding into the issues a run takes, the sub-issues that implement them, and a milestone where the work needs more than one run. Use for /decomp, /decompose, or when work is understood well enough to file but has not been filed.
---

# Decompose

Turn something understood into something claimable. The input is an idea file,
a spec, or a finding from a review; the output is the issues a run can take and
the sub-issues under them, grouped by a milestone when the work needs more than
one run. A spec is decomposed after it merges to `edge`, so the
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

| Shape | What it is |
| --- | --- |
| Milestone | Groups issues. Holds the order and what crosses them. Not an issue. |
| Issue with sub-issues | **What `/storm` takes.** At most five open sub-issues. |
| Sub-issue | One claim, one branch, one pull request. `/impl` takes it. |

An issue that has sub-issues is the run unit. That is the whole test, and no
label carries it.

Grouping is a milestone because a milestone cannot be dispatched by mistake: it
is not an issue, so there is no rule to remember about not running it. Its
description takes markdown, so the order and the crossings live there, and it
reports its own progress.

## The issue a run takes

One per body of work that fits a run, with **no `status:*`**. It is not
claimable, so a status on it is a copy of its sub-issues' state with somewhere
to drift, and it takes no `risk:*`, because that label sizes the blast radius of
a change and this makes none.

`.github/ISSUE_TEMPLATE/issue-with-sub-issues.yml` holds the fields: the goal, the stop rule,
the planned sub-issues, the source, and what it must not absorb. The form
renders in the web UI and nowhere else, so filing through `github-board` means
reproducing those five as headings in the body.

The stop rule is its own, not a restatement of "every sub-issue is done": that
is already implied, and a stop rule that adds nothing tells a run nothing about
when to stop early.

## The milestone

File one when a body of work needs more than one run. A milestone holding a
single issue is a heading.

Its description takes markdown and carries two things a run cannot see from
inside one issue: the order its issues run in, and what crosses them — a
dependency between two, or a file both write. A run works one issue and never
loads its siblings, so a milestone recording neither leaves each run to
rediscover the ordering or collide.

Record a dependency as a GitHub dependency as well. The description is what a
reader gets; `blocked_by` is what GitHub enforces on close.

Where a plan document under `internal/features/` tracks the same work, name the
milestone in its frontmatter as `milestone: <url>`, so either side of the link
reaches the other. `check-frontmatter.py` checks the shape.

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
for its own sub-issues, and the milestone's for what crosses its issues. Both
parts below are read by `/triage` when it decides what several threads may work
at once, and work recording neither has to be taken in sequence.

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

Where both sit in one milestone, that prose belongs in its description
instead. It is the one place a reader of either issue is certain to reach.

A path nobody can name yet is a sub-issue whose scope is still open. Say so in
the table rather than leaving the row out, so a later pass treats it as
overlapping instead of as unexamined.

## Depth

Two tiers of issue: the one a run takes, and its sub-issues. A sub-issue that
needs sub-issues of its own is a second run unit, filed beside the first and in
the same milestone, never nested deeper. `implement` refuses an issue that
has children, so a nested one blocks the worker that claims it.

## Sizing

A sub-issue is one claim, one branch, one pull request. Two changes that must
land together are one issue; two that could land a week apart are two.

**At most five open sub-issues under one run unit.** A run holds every
sub-issue it dispatches in one context, and nine spends that context on work
the dispatcher is not doing yet. Closed sub-issues do not count: they are
finished, and an issue close to done should not have to be split to be worked.

What does not fit becomes a second issue in the same milestone, or stays in the
idea file until it does. The milestone is where the two then record what they
share: the ordering between them, and the files they both write.

A milestone has no cap. Nothing loads it.

## Then

Report what was filed: the milestone if there is one, each issue in it, and
each sub-issue. The number `/storm` takes is an issue with sub-issues.

## Do not

- File a run unit carrying a status or a risk.
- Invent a label.
- Nest beyond two tiers of issue.
- File a sixth open sub-issue rather than splitting into a second issue.
- Open a milestone for one issue.
- File an issue whose criteria you cannot state. That is the signal for
  `specify`, not something to file and fix later.
- Claim or start any of it. Filing and working are separate steps for the
  reason that one person filing and immediately claiming reviews nothing.
