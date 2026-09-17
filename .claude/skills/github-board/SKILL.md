---
name: github-board
description: Read and write thunderstorm board state on GitHub issues, through gh locally or the GitHub MCP tools in a cloud session. Use when claiming an issue, changing its status label, filing a sub-issue, recording a block, or reading what is queued.
---

# GitHub board

The only skill permitted to write board state. Every status change goes through
the operations here so one mechanism owns the transitions.

## Transport

Two transports reach the same board. Pick by what the session has:

| Session          | Transport               | How to tell                        |
| ---------------- | ----------------------- | ---------------------------------- |
| Local checkout   | `gh`                    | `command -v gh` succeeds.          |
| Cloud (web) run  | GitHub MCP tools        | `CLAUDE_CODE_REMOTE=true`, no `gh` |

Check once at the start of a run and use that transport throughout. Never
report a board change made through one transport as if it came from the other.

Two differences decide correctness, so read them before the first write:

- `gh issue edit` applies a **delta**: `--remove-label` and `--add-label` change
  only the labels named. The MCP `issue_write` update applies a **replacement**:
  the `labels` array becomes the issue's entire label set, and so does
  `assignees`. Read the current labels first and send the full intended set,
  or the `type:*`, `area:*`, and `risk:*` labels are silently dropped.
- The MCP surface cannot define labels. It has no create, update, or delete for
  a label, only `get_label`. Applying `.github/labels.yml` is local-only work;
  see [Label definitions](#label-definitions).

## Read

| Operation            | `gh`                                             | MCP                                          |
| -------------------- | ------------------------------------------------ | -------------------------------------------- |
| Queue                | `gh issue list --label "status:queued" --json number,title,labels,assignees` | `list_issues` with `labels: ["status:queued"]` |
| One issue            | `gh issue view <n> --json number,title,body,labels,assignees,state,url` | `issue_read` method `get`                    |
| Sub-issues           | `gh issue view <n> --json subIssues`             | `issue_read` method `get_sub_issues`         |
| Labels alone         | `gh issue view <n> --json labels`                | `issue_read` method `get_labels`             |

A parent issue carries the `run` label. Its sub-issues are the units of work.
Do not treat a parent as claimable.

## Status

Status is a label and exactly one may be set. Changing status means removing the
old label in the same call that adds the new one:

```sh
gh issue edit <n> --remove-label "status:queued" --add-label "status:claimed"
```

Through MCP the same transition is read-then-replace, because the write is a
replacement:

1. `issue_read` method `get_labels` for issue `<n>`.
2. Drop the old `status:*` entry, add the new one, keep every other label.
3. `issue_write` method `update` with the complete `labels` array.

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

Through MCP, `@me` has no equivalent: call `get_me` for the login, then send it
in the `assignees` array of an `issue_write` update, together with the full
label set from the transition above. One update call does both.

Re-read the issue after claiming. If the assignee is not the expected account,
another run took it first; release the claim and pick a different issue.

## Block

```sh
gh issue edit <n> --remove-label "status:claimed" \
  --add-label "status:blocked" --add-label "blocked:decision"
gh issue comment <n> --body "<what is needed to unblock, and from whom>"
```

Through MCP: `issue_write` update carrying the full label set with
`status:blocked` and the reason, then `add_issue_comment`.

`blocked:*` reasons: `decision`, `spec`, `upstream`, `flake`. A block with no
reason label is not a block, it is an abandoned issue.

## File new work

Work found mid-run goes in a new issue, never into the one being worked:

```sh
gh issue create --title <title> --body-file <file> \
  --label "status:queued" --label "type:fix" --label "area:tui" --label "risk:low"
```

Through MCP: `issue_write` method `create` with the same labels in the `labels`
array. Attach it to a parent in the same call with `parent_issue_number`, or
afterwards with `sub_issue_write` method `add`, which takes the sub-issue's ID
rather than its number.

Link it from the originating issue with a comment. Do not start it in this run.

## Label definitions

The label set lives in `.github/labels.yml` and is applied with
`.claude/scripts/sync-labels.sh --apply`. That script needs `gh` and cannot run
in a cloud session, because no MCP tool creates or edits a label definition.

A cloud run that needs a label the repository does not define is blocked, not
free to invent one. Report the missing label and stop; a local run applies the
manifest.

## Rules

- One writer at a time. Two runs editing one issue produce a board nobody trusts.
- Do not close an issue to express any state other than `dropped`. `status:done`
  is set at release.
- Do not edit an issue body written by a human. Add a comment instead.
- Do not invent labels. The set lives in `.github/labels.yml`.
- Report what changed. A status transition nobody announced is a transition
  nobody can question.
