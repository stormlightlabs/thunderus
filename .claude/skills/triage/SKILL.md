---
name: triage
description: Read the whole board and rank what to dispatch next, as lanes that several threads can work at once. Use for /triage, for deciding what to work across epics, or before dispatching more than one thread.
---

# Triage

One pass over the whole board. It produces an order to dispatch in and a plan
for running several issues at once. It reads everything and writes nothing.

`/thunderstorm` runs one epic. This chooses among them, and among the issues
that belong to no epic at all.

## The list is re-derived, never stored

Report to chat. Do not write a file, post the list to an issue, or set a label.
A list of pending work is stale the moment one issue closes, and a stale copy
outranks the board in the next reader's attention. `internal/thunderstorm.md`
states that rule under **File conventions**.

Two threads that both run this reach the same order, because they read the same
board and apply the rules below in the same sequence. What stops them working
the same issue is the claim, not the list. See `github-board` under **Claim**.

Writing nothing is also why triage does not claim ahead. A claim reserves work
the thread may not start, and a claim nobody is working is the stale state the
24-hour rule exists to undo.

## Read

Use the transport `github-board` picks, and read in this order:

1. Every open issue, with labels, assignees, and `updated_at`.
2. Which issues each `kind:epic` holds.
3. The body of each epic that has a candidate under it, for file ownership.
4. `blocked_by` for the candidates that survive the buckets, and not for the
   rest. It is one call per issue, and an issue already ruled out needs none.

Step 1 through MCP is `list_issues` carrying a `fields` list that omits `body`.
Steps 2 and 3 do not go through MCP at all, for a reason that only appears at
this size: `issue_read` method `get_sub_issues` returns every child's full body
and takes no field list, which on this board is 77,000 to 154,000 characters for
one epic. Six of them is the whole pass spent on text triage does not read. Go
to REST with the token `github-board` uses under **Dependencies**, and filter
before the response is read:

```sh
gh_api "$api/<epic>/sub_issues?per_page=100" \
  | jq -r '.[] | select(.state=="open") | .number'
gh_api "$api/<epic>" | jq -r .body
```

Read an epic's body one epic at a time, and only for epics with a candidate.

## Work no run can reach

Two shapes go unworked because no `/thunderstorm` run can find them. Report
both, every pass, before the lanes:

- An open issue under no epic. `/thunderstorm` dispatches an epic's children,
  so an issue with no parent is never reached by a run, however long it has
  been queued. It is still claimable, and it still ranks.
- An epic declaring no sub-issues. A run refuses to start one, so the epic sits
  open holding nothing. It is not work and it does not rank; say it needs
  children or closing.

## Buckets

Sort every open issue into exactly one bucket before ranking anything. Ranking
an issue nobody can claim spends the reader's attention on work that is not
available.

| Bucket        | Holds                                                       |
| ------------- | ----------------------------------------------------------- |
| In flight     | `status:claimed` or `status:review`. Another thread has it.  |
| Needs repair  | A claim past 24 hours with no pull request, a block past 7 days, or two status labels. |
| Not claimable | `status:queued` with an open blocker, or with an assignee.   |
| Claimable     | `status:queued`, no assignee, every blocker closed.          |

Only the claimable bucket gets ranked. Report the other three; they are what
tells the human the board is not what they thought.

## Rank

Apply these to the claimable bucket in order, each one breaking the ties the
one above it leaves. State which rule placed the top few, so a reader who
disagrees knows which rule to argue with.

1. **What it unblocks.** The count of open issues in its `blocking` set,
   highest first. Work that nothing waits on scores zero and sorts below work
   that holds something up.
2. **`type:fix` ahead of the rest.** A fix names behavior that is wrong now,
   and everything else is built on top of it.
3. **The epic nearest finishing.** An issue whose siblings are already merged
   or in review outranks one under an epic nothing has started. Finishing an
   epic retires its coordination cost; starting another adds one.
4. **Oldest first.** A stable tiebreaker. The oldest queued issue has already
   lost every ordering before this one.

Risk does not enter the rank. It decides lanes instead, below.

## Lanes

A lane is what one thread works, in sequence. The argument says how many
threads; without one, plan two.

Two issues may sit in different lanes only when they own non-overlapping files.
Read ownership from the epic body, where `decompose` records it as a table of
sub-issue against the paths it owns. Epics filed before that convention carry
no such table; where one is missing, say so and treat the pair as overlapping.
An unrecorded overlap found by two implementers costs a rework, and holding an
issue for one round costs a round.

An epic body also records the collisions it has with other epics, in prose
beside the table. Those are the ones worth the read: a run works one epic and
cannot see them, which is most of why this command exists. Honor them as
written, including an instruction to sequence a whole epic around one issue.

`area:*` is the coarse fallback and a warning rather than a verdict. Most of
this board carries `area:tui`, so an area match alone would serialize nearly
everything. Name the shared area, then decide on files.

Two issues never share a fan-out:

- Either carries `risk:high`. It touches released behavior, data, or security,
  and it wants a thread and a review sequence to itself.
- Either has sub-issues. `implement` refuses an issue with children, so it is
  an epic that `/thunderstorm` takes, not a lane entry.

Three issues per lane is enough. Past that the plan is a backlog, and the board
already holds one.

## Ad-hoc work

Work that is not filed cannot be ranked, because no other thread can see it. A
thread dispatched on an unfiled item holds something invisible, and the next
thread picks the same work up with nothing to warn it.

Name ad-hoc work in the argument and triage places it in the order, marked as
off-board, with what it would displace. File it through `/decomp` before any
thread takes it. Triage does not file it: `decompose` decides what to write and
`github-board` writes it, and a command that ranks work should not also be
creating the work it ranks.

## Report

Under 40 lines, in this order:

1. **Dispatch now.** One block per lane, each entry giving the number, the
   title, `type:*`, `risk:*`, and the epic it sits under.
2. **Held.** Claimable, but overlapping something already in a lane. Name what
   it overlaps on.
3. **Not claimable.** Name the open blocker or the assignee.
4. **In flight.** What the other threads hold, so nothing is dispatched over
   them.
5. **Needs repair.** The transition each one wants. A human or a later
   `github-board` call makes it.

Close by naming the rule that placed the first entry and what would change the
order. A plan whose reasoning is not visible cannot be overridden.

## Do not

- Write a file, set a label, claim an issue, or open anything.
- Rank an issue in the in-flight, repair, or not-claimable buckets.
- Fan out two issues whose file ownership is unrecorded.
- Invent a priority signal the board does not carry. The rules above are the
  whole ordering; a fifth one that lives only in a report is unarguable.
