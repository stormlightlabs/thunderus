---
name: github-board
description: Read and write thunderstorm board state on GitHub issues, through gh locally or the GitHub MCP tools in a cloud session. Use when claiming an issue, changing its status label, filing a sub-issue, recording a block or a dependency, or reading what is queued.
---

# GitHub board

The only skill permitted to write board state. Every status change goes through
the operations here so one mechanism owns the transitions.

## Transport

Two transports reach the same board. Pick by what the session has:

| Session         | Transport        | How to tell                         |
| --------------- | ---------------- | ----------------------------------- |
| Cloud (web) run | GitHub MCP tools | `CLAUDE_CODE_REMOTE=true`           |
| Local checkout  | `gh`             | `gh auth status` succeeds otherwise |

`CLAUDE_CODE_REMOTE` decides first and on its own. A cloud image that happens to
carry `gh` almost certainly carries no token with it, and a run that picks `gh`
on the strength of the binary alone fails on its first write, or worse, decides
it is local and tries to create a worktree. Where the variable is unset, `gh`
needs a working credential, not merely a place on `PATH`.

Check once at the start of a run and use that transport throughout. Never
report a board change made through one transport as if it came from the other.

Two differences decide correctness, so read them before the first write:

- `gh issue edit` applies a **delta**: `--remove-label` and `--add-label` change
  only the labels named. The MCP `issue_write` update applies a **replacement**:
  the `labels` array becomes the issue's entire label set, and so does
  `assignees`. Read the current labels first and send the full intended set,
  or the `type:*`, `area:*`, and `risk:*` labels are silently dropped.
- Label definitions do not go through MCP. The upstream server keeps label
  writes in a `labels` toolset that is not enabled by default, so `get_label`
  may well be the only label tool a session has. Treat the workflow as the way
  to apply the manifest even where `label_write` is present: one path that
  records what changed beats two that disagree. See
  [Label definitions](#label-definitions).

## Read

| Operation    | `gh`                                                                         | MCP                                            |
| ------------ | ---------------------------------------------------------------------------- | ---------------------------------------------- |
| Queue        | `gh issue list --label "status:queued" --json number,title,labels,assignees` | `list_issues` with `labels: ["status:queued"]` |
| One issue    | `gh issue view <n> --json number,title,body,labels,assignees,state,url`      | `issue_read` method `get`                      |
| Sub-issues   | `gh issue view <n> --json subIssues`                                         | `issue_read` method `get_sub_issues`           |
| Labels alone | `gh issue view <n> --json labels`                                            | `issue_read` method `get_labels`               |

Three shapes. An epic carries `kind:epic`, groups issues and is never
dispatched. An issue with sub-issues and no `kind:epic` is what a run takes, at
most five of them open at once. A sub-issue is the unit of work.

Neither of the first two is ever claimed: no owner, no `status:*`, no `risk:*`,
because their state is whatever their children say. A run is a pass over the
middle one, which outlives the runs that work it. The `decompose` skill's
**Three shapes** carries the rest.

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
4. Read the labels again and compare them with the set you sent.

The write carries no condition, so a label added between steps 1 and 3 is
removed by step 3 and nothing notices. Keep the window to one read followed
immediately by one write, and let step 4 catch what still slipped through:
report the difference rather than writing again, because a second replacement
races the same way.

Never add a status label without removing the previous one. Two status labels on
one issue make the board unreadable and the loop will pick the wrong transition.

| From             | To               | When                                          |
| ---------------- | ---------------- | --------------------------------------------- |
| `status:queued`  | `status:claimed` | A run takes the issue and starts work on it.  |
| `status:claimed` | `status:review`  | A pull request opens.                         |
| `status:claimed` | `status:queued`  | The run abandons it. Remove any worktree too. |
| `status:review`  | `status:verify`  | The pull request merges to `edge`.            |
| `status:verify`  | `status:done`    | The change ships in a release from `main`.    |
| any              | `status:blocked` | Add a `blocked:*` label saying why.           |
| any              | `status:dropped` | Close with a comment giving the reason.       |

Every row but one is a write this skill performs. The `status:review` row is
not: a person merges the pull request in GitHub and moves that label, and no
skill, script or command here does either. Report that a pull request is ready
and stop.

## Claim

A `status:queued` issue has no owner, so an issue that already carries an
assignee is not claimable, whoever put them there. Read it first and move on if
anyone holds it.

A claim is a status transition followed by an assignment, in that order:

```sh
gh issue edit <n> --remove-label "status:queued" --add-label "status:claimed"
gh issue edit <n> --add-assignee @me
```

The order is what makes an interrupted claim safe. A session killed between the
two calls leaves `status:claimed` with no assignee, which the 24-hour rule
returns to the queue. Assigning first would leave an owned issue still reading
`status:queued`, which the next claimant takes as free.

Through MCP, `@me` has no equivalent: call `get_me` for the login and send it in
the `assignees` array of an `issue_write` update, together with the full label
set from the transition above. One update call does both, and because
`assignees` replaces, it sends exactly `[me]`.

Re-read after claiming. The claim succeeded only when `assignees` is exactly
your login and nothing else. Two `gh` runs that claim at once both succeed at
`--add-assignee`, which adds rather than replaces, so each finds itself present
and each believes it won; comparing against the whole list is what tells them
apart.

Losing the race means removing your own assignment and nothing else. Do not
change the status label: `status:claimed` belongs to the run that won it, and
moving the issue back to `status:queued` hands the work in progress to a third
run. If both runs back off, the issue is left claimed with no assignee, which
the 24-hour rule already covers.

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

## Dependencies

A sub-issue says what an issue is part of. A dependency says what it has to wait
for, and the two are different relations: #31 is a sub-issue of #26 and blocked
by #28 and #29 at the same time. Recording the second one is what stops a run
dispatching a harness before the thing it starts exists, and GitHub enforces it
by refusing to close an issue whose blockers are open.

A dependency is an ordering known when the issues are filed. It is not
`status:blocked`, which stops a run (see the `thunderstorm` skill's stop
conditions) and belongs to a block discovered while working. An issue waiting
on a sibling stays `status:queued` and keeps its place in the dispatch order.

### Neither transport performs them

The GitHub MCP server exposes no dependency tool: `sub_issue_write` writes
hierarchy and nothing writes `blocked_by`, and a cloud image carries no `gh`
binary to fall back to. So this is the one board operation that goes to the
REST API directly, and it is the only place this skill reaches past the
transport table. Every other board write still goes through `gh` or MCP.

`references/dependencies.md` holds the read, write, and verify calls, and the
token rule that goes with them. `triage`'s `references/reading-the-board.md`
holds the read path for the counts the issue list carries and neither transport
exposes.

## Label definitions

The label set lives in `.github/labels.yml`. Applying it needs a token that may
write labels, so it runs in one of two places:

| Session | How                                                                        |
| ------- | -------------------------------------------------------------------------- |
| Local   | `.claude/scripts/sync-labels.py` for a dry run, `--apply` to make changes. |
| Cloud   | Dispatch `.github/workflows/labels.yml`, which runs the same script.       |

From a cloud session that means `actions_run_trigger` method `run_workflow`,
`workflow_id` `labels.yml`, `ref` `edge`, and `inputs` `{"apply": "true"}`.
Dispatch on `edge` and nothing else: the job refuses any other ref, because a
dispatch runs the script as it exists on the ref it names, and only the default
branch has been through review. Omitting `apply` gives the dry run. Watch the result with
`actions_list` method `list_workflow_runs` and read the job log before claiming
the labels changed: the script verifies its own work and exits non-zero when the
final state does not match the manifest.

One thing the workflow will not do is invent a label. A run that needs a label
the manifest does not define is blocked. Add it to `.github/labels.yml` in a
pull request, then sync.

## Rules

- One writer at a time. Two runs editing one issue produce a board nobody trusts.
- Do not close an issue to express any state other than `dropped`. `status:done`
  is set at release.
- Do not edit an issue body written by a human. Add a comment instead.
- Do not invent labels. The set lives in `.github/labels.yml`.
- Report what changed. A status transition nobody announced is a transition
  nobody can question.
