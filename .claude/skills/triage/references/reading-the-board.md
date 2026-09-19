# Reading the board

The commands a triage pass reads with, and why each one rather than the
obvious alternative. The `triage` skill's **Read** section says what to read;
this says how.

## One call for almost everything

One call carries almost the whole pass:

```sh
gh api "repos/<owner>/<repo>/issues?state=open&per_page=100" --paginate
```

In a cloud session there is no `gh` binary, so the same path goes through
`curl` with the token and headers `github-board` sets up under
**Dependencies**. Page in both cases: `gh issue list` stops at 30 by default
and this board passed that long ago.

Per issue that response carries `labels`, `assignees`, `updated_at`,
`parent_issue_url`, `sub_issues_summary` and `issue_dependencies_summary`. The
last two are what make a second call unnecessary:

| Field                                  | Answers                          |
| -------------------------------------- | -------------------------------- |
| `issue_dependencies_summary.blocked_by` | Blockers still open. Zero is claimable. |
| `issue_dependencies_summary.blocking`   | Open issues waiting on this one. Rank rule 1. |
| `sub_issues_summary`                    | `total` and `completed` on an epic. Rank rule 3. |
| `parent_issue_url`                      | Whether the issue sits under an epic at all. |

`blocked_by` counts the blockers still open where `total_blocked_by` counts
every one, so an issue whose blockers have all closed reads as claimable with
nobody resolving the edge. The `dependencies/blocked_by` endpoint answers the
same question one issue at a time; do not spend it on a number already in hand.

File ownership is the one thing the list does not carry. It lives in an epic's
body, read one epic at a time and only for epics with a candidate in the
ranked set:

```sh
gh api "repos/<owner>/<repo>/issues/<epic>" --jq .body
```

Neither MCP tool replaces the list call. `issue_read` method `get_sub_issues`
returns every child's whole body and takes no field list, 77,000 to 154,000
characters for one epic here, and `list_issues` takes a `fields` list but omits
both summary fields, so it cannot answer the buckets.

## The clock on a stale claim

The 24-hour rule on `status:claimed` and the 7-day rule on `status:blocked`
need the moment the label moved. `updated_at` is not that moment: any comment,
assignee change, or label bumps it, so a claim abandoned a week ago reads fresh
the moment somebody comments on it.

```sh
gh api "repos/<owner>/<repo>/issues/<n>/timeline" \
  --jq '[.[] | select(.event=="labeled")] | last'
```

Read the open pull requests once for which claims have one. Both are per-issue
calls, so make them only for issues already carrying `status:claimed` or
`status:blocked`. Where the timeline is unavailable, report the age as
unverified rather than passing `updated_at` off as an answer.
