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

The `github-board` skill's Transport section decides whether this run uses `gh`
or the GitHub MCP tools. Use the same transport for everything below.

```sh
gh pr view <n> --json title,body,headRefName,baseRefName,files
gh pr diff <n>
git log --oneline origin/edge..<branch>
```

Through MCP: `pull_request_read` methods `get`, `get_files`, and `get_diff`.
The `git log` line is local either way.

Read the changed files around the diff, not only the diff. Read the issue the
pull request closes. Read `CLAUDE.md` for the rules the change must satisfy.

## Judge against something

A finding needs a source. Rank them:

1. The issue's acceptance criteria.
2. The rules in `CLAUDE.md`: module order, error policy, trait boundaries, the
   documented-invariant requirement for `unwrap`, `expect`, `panic!`.
3. Tests that exist, tests that should exist, and tests that cannot fail.
4. Behavior a user can observe.

Taste alone is a `nit` at most. Model disagreement is not a finding.

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

## Which passes post

A first standard pass does not comment. Its findings go to whoever invoked it,
and the fixes land before anything reaches the thread. Most of what a first
pass finds is resolved within the hour, and a reader scrolling through resolved
findings learns nothing about the change.

Determine which pass this is from the pull request itself. No signed review
findings on it means this is the first.

A second standard pass comments, and only on what survived the first. Something
that outlived a round of fixes is worth a record.

An adversarial pass always comments, including to say it found nothing. It is
the last pass before a human reads the diff, so the record has to show it ran.

Findings go to whoever invoked the pass either way. The question here is only
whether they also reach the pull request.

## Post the findings

Post one comment per pass, not one per finding. End every comment with a
signature naming the model and reasoning level:

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
- Do not repeat a finding already posted and addressed in an earlier pass.
- If the same finding survives two edit passes without the cause changing, stop
  and escalate to the user rather than posting it a third time.
- Cap a review-and-edit cycle at 5 rounds.

## Do not

- Edit files, commit, push, approve, or merge.
- Review a diff you wrote in this same session without saying so in the comment.
- Weaken a test to make a finding go away. That belongs to `/edit`, and it is
  not permitted there either.
