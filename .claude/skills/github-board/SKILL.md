---
name: github-board
description: Read and write thunderstorm board state on GitHub issues through gh. Use when claiming an issue, changing its status label, filing a sub-issue, recording a block, or reading what is queued.
---

# GitHub board

The only skill permitted to write board state. Every status change goes through
the commands here so one mechanism owns the transitions.

## Read

```sh
gh issue list --label "status:queued" --json number,title,labels,assignees
gh issue view <n> --json number,title,body,labels,assignees,state,url
gh issue view <n> --json subIssues
```

A parent issue carries the `run` label. Its sub-issues are the units of work.
Do not treat a parent as claimable.

## Status

Status is a label and exactly one may be set. Changing status means removing the
old label in the same call that adds the new one:

```sh
gh issue edit <n> --remove-label "status:queued" --add-label "status:claimed"
```

Never add a status label without removing the previous one. Two status labels on
one issue make the board unreadable and the loop will pick the wrong transition.

| From             | To                | When                                           |
| ---------------- | ----------------- | ---------------------------------------------- |
| `status:queued`  | `status:claimed`  | A run takes the issue and creates a worktree.  |
| `status:claimed` | `status:review`   | A pull request opens.                          |
| `status:claimed` | `status:queued`   | The run abandons it. Remove the worktree too.  |
| `status:review`  | `status:verify`   | The pull request merges to `edge`.             |
| `status:verify`  | `status:done`     | The change ships in a release from `main`.     |
| any              | `status:blocked`  | Add a `blocked:*` label saying why.            |
| any              | `status:dropped`  | Close with a comment giving the reason.        |

## Claim

A claim is an assignment plus a status transition, in that order:

```sh
gh issue edit <n> --add-assignee @me
gh issue edit <n> --remove-label "status:queued" --add-label "status:claimed"
```

Re-read the issue after claiming. If the assignee is not the expected account,
another run took it first; release the claim and pick a different issue.

## Block

```sh
gh issue edit <n> --remove-label "status:claimed" \
  --add-label "status:blocked" --add-label "blocked:decision"
gh issue comment <n> --body "<what is needed to unblock, and from whom>"
```

`blocked:*` reasons: `decision`, `spec`, `upstream`, `flake`. A block with no
reason label is not a block, it is an abandoned issue.

## File new work

Work found mid-run goes in a new issue, never into the one being worked:

```sh
gh issue create --title <title> --body-file <file> \
  --label "status:queued" --label "type:fix" --label "area:tui" --label "risk:low"
```

Link it from the originating issue with a comment. Do not start it in this run.

## Rules

- One writer at a time. Two runs editing one issue produce a board nobody trusts.
- Do not close an issue to express any state other than `dropped`. `status:done`
  is set at release.
- Do not edit an issue body written by a human. Add a comment instead.
- Do not invent labels. The set lives in `.github/labels.yml`; apply it with
  `.claude/scripts/sync-labels.sh`.
- Report what changed. A status transition nobody announced is a transition
  nobody can question.
