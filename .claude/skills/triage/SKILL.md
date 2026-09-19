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
the same issue is the claim, not the list, under `github-board`'s **Claim**.

Triage does not claim ahead either. A claim reserves work the thread may never
start, which is the stale state the 24-hour rule exists to undo.

## Argument

Every argument is prefixed, because the two numeric ones are otherwise the same
token: `/triage 3` could ask for three threads or for epic 3.

| Form               | Means                                    |
| ------------------ | ---------------------------------------- |
| `threads=<n>`      | Plan for `n` threads. Without it, two.   |
| `#<n>` or `epic:<n>` | Narrow to that epic and its children.  |
| `area:<name>`      | Narrow to issues carrying that label.    |
| Anything else      | Ad-hoc work, under **Ad-hoc work** below. |

A narrowing argument changes which issues are ranked and laid into lanes. It
changes nothing else: read the whole board anyway, and report the repairs, the
in-flight work and the issues no run reaches across all of it. Those are the
parts a narrowed pass would otherwise stop surfacing, and they are the reason
to run a board-wide pass rather than a run.

## Read

Read the whole board every pass, whatever the argument narrows.

One paginated call over the repository's open issues carries almost all of it:
labels, assignees, `parent_issue_url`, and the two summary objects that answer
the buckets and two of the rank rules without a second request per issue. File
ownership is the one thing it does not carry, and that comes from the body of
each epic holding a candidate.

`references/reading-the-board.md` has the commands for both transports, the
fields and what each one decides, and the two MCP tools that cannot replace
the call along with the measurements that rule them out.

## Work no run can reach

An open issue with an empty `parent_issue_url` is under no epic, and
`/thunderstorm` dispatches an epic's children, so no run reaches it however
long it stays queued. It is still claimable and it still ranks. Report these
every pass, before the lanes, because nothing else on the board says they are
going unworked. The other shape a run cannot reach, an epic holding nothing, is
a repair under **Buckets** below.

## Buckets

Set the epics aside first. An epic carries no `status:*` label and is not work,
so it is not bucketed and never ranked. What it contributes is its
`sub_issues_summary` to rank rule 3, and its body to the lanes.

Sort every other open issue into the first row it matches, top down. The order
is the precedence: a claim that has gone stale is a repair before it is
somebody's work in flight.

| Bucket        | Holds                                                        |
| ------------- | ------------------------------------------------------------ |
| Needs repair  | Any shape under **Repairs** below.                           |
| In flight     | `status:claimed` or `status:review`. Another thread has it.  |
| Waiting       | `status:verify`, or `status:blocked` inside its 7 days.      |
| Not claimable | `status:queued` with `blocked_by` above zero, or an assignee. |
| Claimable     | `status:queued`, no assignee, `blocked_by` zero.             |

An issue with no `status:*` label reaches none of these rows, which is why the
first one catches it: it is a repair, not a bucket.

Only the claimable bucket is ranked. Report the rest; they are what tells the
human the board is not the shape they thought.

### Repairs

Each shape breaks a rule another document owns. Report it and name the rule;
the repair itself is a `github-board` write a human authorizes.

| Shape                                             | Rule                      |
| ------------------------------------------------- | ------------------------- |
| `status:claimed` past 24 hours with no pull request | `internal/thunderstorm.md`, Statuses |
| `status:blocked` past 7 days, or with no `blocked:*` reason | the same |
| Two `status:*` labels on one issue, or none       | `github-board`, Status    |
| `kind:epic` carrying `status:*` or `risk:*`       | `decompose`, The epic     |
| `kind:epic` with `sub_issues_summary.total` zero  | no run can start it       |

The first two are ages, and `updated_at` does not measure them: any comment or
label bumps it, so an abandoned claim reads fresh the moment someone comments.
The reference above gives the timeline read that does, and says to report an
age as unverified rather than passing `updated_at` off as an answer.

## Rank

Apply these to the claimable bucket in order, each one breaking the ties the
one above it leaves. State which rule placed the top few, so a reader who
disagrees knows which rule to argue with.

1. **What it unblocks.** The count of open issues in its `blocking` set,
   highest first. Work that nothing waits on scores zero and sorts below work
   that holds something up.
2. **`type:fix` ahead of the rest.** A fix names behavior that is wrong now,
   and everything else is built on top of it.
3. **The epic nearest finishing.** Its epic's `sub_issues_summary`, by
   `completed` against `total`, highest first. An issue whose siblings have
   mostly landed outranks one under an epic nothing has started: finishing an
   epic retires its coordination cost, and starting another adds one. An issue
   under no epic scores zero here and is broken out of by rule 4.
4. **Oldest first.** A stable tiebreaker. The oldest queued issue has already
   lost every ordering before this one.

Risk does not enter the rank. It decides lanes instead, below.

## Lanes

A lane is what one thread works, in sequence. `threads=<n>` says how many;
without it, plan two.

Two issues may sit in different lanes only when they own non-overlapping
files. Ownership lives in the epic body, under the `decompose` skill's
**Recording overlap**, which also asks an epic to name the collisions it has
with other epics. Honor those as written, including an instruction to sequence
a whole epic around one issue: a run works one epic and cannot see them.

Where an epic records no ownership, say so and treat every pair under it as
overlapping. An unrecorded overlap found by two implementers costs a rework,
and holding an issue for one round costs a round.

`area:*` is a warning rather than a verdict, too broad to decide by: `area:tui`
alone is 17 of the 38 queued issues here, so matching on area would hold back
nearly half the board. Name the shared area, then decide on files.

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

Name ad-hoc work in the argument and triage places it in the order, marked
off-board, with what it would displace. File it through `/decomp` before any
thread takes it. Triage does not file it: a command that ranks work should not
also create the work it ranks.

## Report

Under 40 lines, in this order:

| Heading       | Carries                                                    |
| ------------- | ---------------------------------------------------------- |
| Dispatch now  | One block per lane: number, title, `type:*`, `risk:*`, epic. |
| Held          | Claimable but overlapping a lane. Name what it overlaps on. |
| Not claimable | The open blocker, or the assignee.                          |
| In flight     | What the other threads hold, so nothing dispatches over them. |
| No run reaches | The issues under no epic.                                  |
| Needs repair  | The transition each one wants, for a human to authorize.    |

Leave out a heading with nothing under it, and say `status:verify` and
`status:blocked` work is waiting rather than listing it every pass.

Close by naming the rule that placed the first entry and what would change the
order. A plan whose reasoning is not visible cannot be argued with.

## Do not

- Write a file, set a label, claim an issue, or open anything.
- Rank an issue in the in-flight, repair, or not-claimable buckets.
- Fan out two issues whose file ownership is unrecorded.
- Invent a priority signal the board does not carry. The rules above are the
  whole ordering; a fifth one that lives only in a report is unarguable.
