# Dependency relations over REST

The one board operation neither transport performs. The `github-board` skill's
**Dependencies** says when to record one; this holds the calls.

## Neither transport reaches them

The GitHub MCP server exposes no dependency tool: `sub_issue_write` writes
hierarchy and nothing writes `blocked_by`. The transport table already assumes a
cloud session cannot fall back to `gh`, and the current image carries no `gh`
binary at all, so this is the one board operation that goes to the REST API
directly.
It is the only place this skill reaches past the transport table, and it
reaches it for dependency relations alone. Every board write still goes through
`gh` or MCP.

Reading is where that stops being the whole story. The `triage` skill reads the
board through the same REST path, for the dependency and sub-issue counts the
issue list carries and neither transport exposes. Its
`references/reading-the-board.md` holds those calls. A read that goes around
the transport table costs nothing a write would, and the rule above is about
writes.

A cloud container has `GH_TOKEN` and `GITHUB_TOKEN` in the environment. Use one
of them; do not print either, and do not pass a token on a command line where it
lands in shell history.

## Read

```sh
api=https://api.github.com/repos/<owner>/<repo>/issues
gh_api() {
  curl -sS \
    -H "Authorization: Bearer $GH_TOKEN" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" "$@"
}

gh_api "$api/<n>/dependencies/blocked_by"   # what <n> waits for
gh_api "$api/<n>/dependencies/blocking"     # what waits for <n>
```

Both return an array of issue objects, empty when there is no relation. Read
`blocked_by` before claiming anything: an issue whose blockers are still open is
not claimable, whatever its status label says.

## Write

```sh
gh_api -X POST -H "Content-Type: application/json" \
  "$api/<blocked-number>/dependencies/blocked_by" \
  -d '{"issue_id": <blocker-id>}'
```

`201` is success. Removing one is the same path with `-X DELETE` and no body.

Two things make this fail in ways the error message only half explains:

- **The body takes an issue ID, the path takes an issue number.** They are
  different values, and an issue's ID is global where its number is per
  repository, so a number sent as `issue_id` is not the issue you meant. Get the
  ID from the `id` field of `issue_write` method `create`, or from
  `gh_api "$api/<n>"` piped through `jq .id`.
- **`Content-Type: application/json` is required.** Without it the request fails
  `415` even though the body is valid JSON and every other header is right. The
  `Accept` header does not cover this.

## Verify

The relation is stored once and projected both ways, so reading it back from the
other end is a real check rather than an echo:

```sh
gh_api "$api/<blocker>/dependencies/blocking" | jq -r '.[].number'
```

Report the graph you wrote, in both directions. A dependency nobody announced is
an ordering nobody can question, and a missing one is invisible until a run
dispatches into it.
