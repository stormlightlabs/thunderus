---
name: commit-and-pr-length
last_updated: 2026-09-18
id: 01M2S96YAWBQX8Y53FFQF3GCZE
---

# Commit and pull request length

## Problem

Messages written from agent sessions are long. The targets existed before this
file and nothing measured them, so nobody knew by how much.

Measured on `edge` at `de489a8`, over the 17 squash merges it carries:

| Measure                         | Target   | Found                     |
| ------------------------------- | -------- | ------------------------- |
| Merged commit body              | 15 lines | 15 to 385, median 53      |
| Merged bodies over that target  | —        | 16 of 17                  |
| Commits per merged branch       | —        | median 3, max 31          |
| Pull request description        | 40 lines | median 47.5 lines         |

Reproduce the first three with `.claude/scripts/check-commit-message.py
--warn --range <base>..<head>`, which reports each body and the projected
squash size. The fourth came from the pull request API over the 18 pull
requests open or merged on 2026-09-18; their median body is also 385 words,
which is the top of the 200-to-400 band the sources below give for a large
change, for descriptions that are mostly documentation edits.

Two causes, both structural. Neither is a writer choosing to ramble.

**A squash sums the branch.** GitHub builds the merged body by concatenating
every commit on the branch, each under a `* subject` bullet. Three commits at
the 15-line target merge as a 48-line commit, and the median branch here is
three. The hook graded one message at a time, so every message could pass while
the merged result ran to four figures of words. The 385-line outlier is #1 at
31 commits.

**The suffix spends part of the subject.** GitHub appends ` (#NN)` server-side.
Four squash subjects on `edge` exceed the 59-character limit, and all four had
titles of 57 to 59 characters before the suffix. Not one author wrote an
overlong title. Three further overlong subjects predate squash merging and
carry no suffix; those are genuine.

A pull request description never reaches `git log` at all. It is read once, in
review, and then only the commits survive.

## Decisions

Report length, enforce nothing. The three enforcement levels are prevention,
detection, and blocking, and only blocking creates pressure to bypass. Length
is a judgement a script cannot make: a body over the target is usually padding,
but sometimes a change earns the room. A hook that blocks on that teaches
`--no-verify`, which also skips the shape checks, so the check that cannot be
certain would cost the one that can. Shape stays an error, because a missing
type is not a judgement call.

Budget the title at 53 characters rather than blaming the author. 59 allowed,
less six for ` (#NN)`. Seven past #99, which the budget absorbs.

Project the squash before the merge. The sum is the number that matters and it
is invisible until someone clicks merge, so CI prints it. The fix is to edit
the message in GitHub's merge box, which is free and takes one edit; the
alternative, landing fewer commits, is a real constraint on how work is split
and should not be forced by a message-length rule.

Scale the pull request body to its form. Four headings and their blank lines
cost six lines before a word is written, which is most of the reason a small
change should skip them: a reviewer reads four headings, finds a sentence under
each, and learns less than one paragraph would have told them. Short form 20
lines, headings 40. The 40 is where the description target already sat.

Targets, with where each number comes from:

| Text                 | Target        | Source                              |
| -------------------- | ------------- | ----------------------------------- |
| Pull request title   | 53 characters | 59 allowed, less ` (#NN)`           |
| One commit body      | 15 lines      | `writing-docs`, unchanged           |
| Merged body          | 15 lines      | It is one commit like any other     |
| Pull request body    | 20 or 40      | Short form, or headings on a big diff |
| Pull request comment | 10 lines      | It is a reply, not a report         |

## Open

Whether 15 lines survives contact with a branch that legitimately needs five
commits. Five at target project 80 lines, and the answer is either a squash
message written by hand every time or a target that admits the sum. Settled
by the first branch where the merge-box edit is skipped because it felt like
ceremony.

Whether the 20-line short form holds for a change with real verification to
report. This file's own pull request used the headings form and fit 40 with
nothing cut, which is one data point and the easy direction.

## Sources

- `origin/edge` at `de489a8`, read with `git log --format='%H%n%B%x00'`: every
  number in the Problem table. Measured in this container on 2026-09-18, not
  recalled. The body counts come from `measure()` in
  `.claude/scripts/check-commit-message.py`, which excludes the harness
  trailers and counts blank lines between paragraphs.
- The 17 squash subjects on `edge`, compared against themselves with the
  ` (#NN)` suffix stripped: the finding that every overlong squash subject had
  a compliant title.
- `de489a8` and `b726f1a`: the two squash body shapes. The first carries three
  `* subject` bullets from a three-commit branch, the second none, which is how
  a single-commit branch merges.
- [The 50/72 rule](https://deviq.com/practices/50-72-rule/) and [a summary of
  Tim Pope's 2008 formatting
  post](https://www.w3tutorials.net/blog/git-commit-messages-50-72-formatting/):
  the 50-character subject and 72-column body convention, and that it came from
  Linux kernel practice. This repository uses 60, which predates this file.
  Neither source sets a body length, which is the gap the 15 lines fills.
- [Pull request description
  length](https://willowvoice.com/blog/how-to-write-good-pull-request-description)
  and [pull request best
  practices](https://blog.codacy.com/pull-request-best-practices): under 150
  words for a focused change, 200 to 400 depending on size, and that structure
  matters more than volume. Read on 2026-09-18. The claim that clear
  descriptions cut review time by 40 per cent appears in the first and is not
  reproduced here, so it is not load-bearing for any decision above.
- `.claude/skills/writing-docs/SKILL.md`: the existing 15-line commit body and
  40-line description targets, which this file did not invent and only one of
  which it changed.
