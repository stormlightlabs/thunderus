---
name: review
description: Review a pull request or branch and post findings as comments. Runs a standard pass or an adversarial pass. Use for /rev, /adv-rev, code review, PR review, or when asked to check a diff for defects before merge.
---

# Review

Review a diff and post findings. Do not edit code during a review pass. The
`/edit` command owns changes.

## Pick the target and the pass

The argument is a pull request number, a branch name, or empty. Empty means the
current branch.

| Pass        | Command    | Brief                                                                                                       |
| ----------- | ---------- | ----------------------------------------------------------------------------------------------------------- |
| Standard    | `/rev`     | Correctness, edge cases, security, concurrency, performance, API compatibility, test coverage, readability. |
| Adversarial | `/adv-rev` | Worst case only: security holes, races, unhandled input, violated invariants, tests that cannot fail.       |

An adversarial pass assumes the standard passes already ran. It looks for what
they would miss, not for the same findings again.

### What the adversarial pass owes

The standard passes read the change as written. The adversarial pass reads it
as hostile input, an unlucky interleaving, or a caller who ignores the
documentation would.

- Name the input, the ordering, or the state that produces the failure. A
  worst case with no path to it is not a finding.
- Say how far you got: reproduced with a test you ran, traced through the
  code, or suspected. Mark the third kind `unverified` and keep it out of the
  `blocker` and `high` rows.
- Prefer a few findings you can support to a list you cannot. One proven race
  changes the diff. Six guesses cost the next editor a day.
- Read the tests for what they do not assert. A test that passes against the
  old behavior and the new one covers neither.

### Complexity

Both passes weigh complexity, because a defect is cheaper to prevent than to
find. Ask of each change:

- Is this harder than the problem it solves? Name the simpler version and what
  it would cost.
- Does it add a trait, a layer, or a configuration point that one concrete
  helper would cover? `CLAUDE.md` asks for traits only at real boundaries.
- How many things must a reader hold at once to know this is correct? A branch
  nested inside a closure inside a retry is three.
- Does it repeat something the codebase already does, under a new name?
- Could a check replace the care this asks of the next person to touch it?

Report complexity the way you report a defect: what it costs, and what to do
instead. "Simpler would be better" without a concrete alternative is not a
finding. A change that is merely longer than you would have written it is not
either.

## Gather context first

Read the changed files around the diff, not only the diff. Read the issue the
pull request closes. Read `CLAUDE.md` for the rules the change must satisfy.

Four rules govern how a pass reads, and `references/reading-the-diff.md` gives
the commands and the reasons:

- Use the transport the `github-board` skill's Transport section names, `gh` or
  the GitHub MCP tools, for the whole pass.
- Fetch the branch first. A shallow cloud clone usually does not have it.
- Pin the head commit once and review that commit, so every finding describes
  one state of the tree.
- Do not read the change with `Read`, `Grep`, or `Glob`, and write nothing
  inside the repository. Both reach whatever branch the checkout sits on.

## Judge against something

A finding needs a source. Rank them:

1. The issue's acceptance criteria.
2. The rules in `CLAUDE.md`: module order, error policy, trait boundaries, the
   documented-invariant requirement for `unwrap`, `expect`, `panic!`.
3. Tests that exist, tests that should exist, and tests that cannot fail.
4. Behavior a user can observe.

Taste alone is a `nit` at most. Model disagreement is not a finding.

### Prose in the diff

A diff touching `docs/`, `internal/`, `README.md`, `CHANGELOG.md`, or
`.claude/` is a change under review like any other. Judge its prose against the
`writing-docs` skill.

| What you find                                    | Severity |
| ------------------------------------------------ | -------- |
| Documentation contradicts the code it describes  | `high`   |
| User-visible behavior changed, documents did not | `medium` |
| Past a soft limit or a length target             | `low`    |
| Writing tells, heading case, term drift          | `nit`    |

Length is a finding when a reader reaches the answer only by scrolling past
something that repeats the code, the tests, or another page. Name that part and
what it repeats. "Too long" on its own is not a finding.

## Finding format

One line per finding:

```text
<severity> · <path>:<line> — <problem> → <why it matters> → <fix direction>
```

| Severity  | Means                                                                |
| --------- | -------------------------------------------------------------------- |
| `blocker` | Merging causes data loss, a crash, a security hole, or a regression. |
| `high`    | Wrong behavior in a case the change is supposed to handle.           |
| `medium`  | Wrong behavior in an unhandled case, or a missing test for one.      |
| `low`     | Works, but will cause a defect later.                                |
| `nit`     | Style or naming inside the repository's conventions.                 |

State what breaks and with what input. A finding that cannot name a failing
case is speculation; drop it or mark it `nit`.

## Keep the report readable

A report is prose in this repository, so the `writing-docs` length targets
cover it too.

- One line per finding, most severe first. No preamble, and no closing section
  restating the lines above it.
- Keep a comment under 40 lines. Past ten findings, give the ten that matter
  and say how many `nit` rows you left out.
- Cite `<path>:<line>` instead of quoting the diff back at its author.
- Group repeats. One finding naming every site beats one finding per site.

## Which passes post

Findings go to whoever invoked the pass, in full, every time. The question here
is only which passes also comment on the pull request.

| Pass            | Comments on the pull request                            |
| --------------- | ------------------------------------------------------- |
| First standard  | No.                                                     |
| Second standard | Yes, on what survived the first, or to say nothing did. |
| Adversarial     | Yes, including to say it found nothing.                 |

A first pass does not comment because most of what it finds is fixed within the
hour, and a reader scrolling through resolved findings learns nothing about the
change. What outlived a round of fixes is the part worth recording. An
adversarial pass is the last thing before a human reads the diff, so the record
has to show it ran.

The invoker says which pass this is. `/rev` takes it as an argument and the
sequence in `.claude/skills/thunderstorm/SKILL.md` fixes it at dispatch. Do not
infer it from the thread, which answers the question wrongly in three
directions: a pass that found nothing leaves nothing behind, an `/edit` reply
is signed and finding-shaped, and a comment can be edited or deleted after the
fact. Where no pass is named, this is a first pass. Open each comment by naming
the pass that wrote it, which is what makes a second-pass comment evidence that
a first pass ran.

A pass that posts nothing still owes its findings to the invoker, because they
are the next `/edit` pass's only input. An orchestrator hands them to `/edit`
and to the second pass both: "what survived the first" is not something the
second pass can work out, having never seen the first.

## Post the findings

Post one comment per pass, not one per finding. Open it with the commit the
pass read and the pull request it belongs to:

```text
Reviewed at <commit> on #<n>.
```

Both, not the commit alone. The branch is deleted when its pull request merges
and the commit goes unreachable with it, so a review citing only a SHA is
unreadable by the time anyone goes back to it.

End every comment with a signature naming the model and reasoning level:

```text
— <model-id> · <reasoning-level>
```

```sh
gh pr comment <n> --body-file <file>
# MCP: add_issue_comment with issue_number set to the pull request number.
```

Ask before posting when no pull request is open, and never post to a repository
the user did not name.

## Stop conditions

- Report `No findings` rather than inventing something to justify the pass.
- Do not repeat a finding an earlier pass raised and an edit pass addressed,
  whether it was posted to the thread or handed to you by your invoker.
- If the same finding survives two edit passes without the cause changing, stop
  and escalate to the user rather than posting it a third time.
- Cap a review-and-edit cycle at 5 rounds.

## Do not

- Edit files, commit, push, approve, or merge.
- Review a diff you wrote in this same session without saying so in the comment.
- Weaken a test to make a finding go away. That belongs to `/edit`, and it is
  not permitted there either.
