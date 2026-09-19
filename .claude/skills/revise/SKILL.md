---
name: revise
description: Address review comments on an open pull request and push the fixes. Use for /edit, /revise, or when asked to respond to review feedback on a PR number.
---

# Revise

Take the review findings on one pull request and fix their causes. One pass per
invocation.

## Read the findings

The `github-board` skill's Transport section decides whether this run uses `gh`
or the GitHub MCP tools. Use the same transport for everything below.

```sh
gh pr view <n> --json title,body,headRefName,baseRefName
gh pr diff <n>
gh pr view <n> --comments
```

Through MCP: `pull_request_read` methods `get`, `get_diff`, then
`get_review_comments` for findings left on lines and `get_comments` for findings
left on the pull request itself. A review pass may use either, so read both.

A first standard pass does not comment on the pull request, so its findings
reach you from whoever invoked this edit pass rather than from the thread. Take
what you are handed as the pass, and read the thread as well: a second or
adversarial pass posts there. Where nothing was handed over and the thread
holds no findings, say so and stop rather than inventing a pass to address.

Collect every finding from the most recent review pass. Earlier passes are
history; do not re-address a finding already resolved unless it recurred.

## Work in the pull request's tree

```sh
git worktree list
```

Use the existing worktree for `agent/<n>` if it is still there. Where there is
none, take what the `worktree` skill's **Who gets one** section gives this pass
as its writer. A dispatched pass gets a worktree, branched from the pull
request's head rather than from `origin/edge`. A cloud session working the pull
request itself checks that head branch out in the container checkout. Never make
these edits in the user's primary checkout on a development machine.

## Decide on each finding

Every finding gets one of three outcomes, and every outcome is recorded in the
reply comment:

| Outcome      | When                                                                |
| ------------ | ------------------------------------------------------------------- |
| Fixed        | The finding is right. Fix the cause, not the symptom.               |
| Not a defect | The finding is wrong. Say why, with the code or test that shows it. |
| Deferred     | Real but out of scope. File an issue and link it.                   |

Disagreeing with a reviewer is allowed and expected. Say so plainly instead of
making a change you believe is wrong.

## Fix causes

Fix the underlying problem rather than the reported symptom. A finding about
one call site that applies to four means fixing four.

Add a test that fails before the fix and passes after. A `blocker` or `high`
finding without a regression test needs a reason in the reply.

Never weaken, skip, delete, or narrow a test to clear a finding. If a test is
wrong, say so and fix it deliberately, with the reason recorded.

## Verify

```sh
cargo fmt
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Run `pnpm --dir docs build` when anything under `docs/` changed.

## Push and reply

Commit with the `commits-and-prs` skill, push to the same branch with
`.claude/scripts/push-verified.sh`, then post one reply comment listing each
finding and its outcome, **under 150 words**. Open it with one line naming the
pass you are answering, what it ran as, and what you ran as:

```text
Answering the second standard pass (claude-opus-5 · high) · claude-sonnet-5 · medium
```

The pass it answers because a first pass posts nothing and this reply is the
only record that it ran. Nothing goes at the bottom.

`Fixed in <sha>` is a complete outcome. A finding needs a sentence only where
you did not act on it, or where the diff does not show what changed.

A reply that names a commit is a claim about the remote, so confirm the push
landed before writing one. `git push` exits zero for a push that carried
nothing.

## Resolve the threads

Resolve each thread you addressed. A thread stays open only while something is
still owed on it.

Resolve with a comment only where one is needed: a finding you are not acting
on and why, a disagreement, or an outcome the diff does not show. A thread whose
fix is visible in the diff, and already named in the reply listing outcomes,
needs no second note saying the same thing.

Two threads are not yours to resolve. One that asks a question or waits on a
decision belongs to whoever answers it. One you opened as a reviewer is never
resolved by you: marking your own finding closed hides whether anyone agreed.

## Stop conditions

- Stop after 5 review-and-edit rounds on one pull request.
- Stop when the same finding arrives twice without its cause having changed.
- Stop when a fix would need a decision the issue does not record.

Each stop is an escalation to the user, not a reason to force a change through.

## Do not

- Approve or merge the pull request.
- Review your own revision. The next review pass is a separate invocation.
- Rebase or force-push without being asked.
