---
description: Rank the board into a dispatch plan for one or more threads
argument-hint: [thread count] [epic number|area] [ad-hoc work]
---

Use the `triage` skill.

Scope: $ARGUMENTS (a thread count, an epic number or `area:*` to narrow to,
and any ad-hoc work not yet filed; empty plans two threads over the whole
board)

Read the board through `github-board`. Bucket every open issue, rank only the
claimable ones, and lay the top of that order into lanes that do not overlap in
the files they own. Say which rule placed the first entry.

Report to me and write nothing: no file, no label, no claim. Name anything the
board needs repaired rather than repairing it. Ad-hoc work I name is placed in
the order and marked off-board; it goes through `/decomp` before any thread
takes it.
