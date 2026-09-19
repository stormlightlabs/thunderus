---
name: agent-attribution
last_updated: 2026-09-19
id: 01M2VZFXTS1HCMB42NHHZ5DECH
---

# Agent attribution

## Problem

The merged record cannot say whether an agent worked alone. This repository has
401 commits under two author identities, both the repository owner's, and twenty
carry a `Claude-Session` trailer marking them as agent work. All twenty are
authored `Owais <desertthunder.dev@gmail.com>` with committer
`GitHub <noreply@github.com>`.

Some of that is correct, because a commit from a session the owner drove is
authored by the owner. What is wrong is that a dispatched commit lands the same
way. A squash merge takes its author from the pull request submitter and demotes
every branch author to a `Co-Authored-By` trailer, and `/impl` opens the pull
request as the owner, so no branch author survives the merge whatever it said.

Review comments have the same shape. `internal/thunderstorm.md` Review sequence
records that reviews post from whichever account runs them, which for a cloud
session is `desertthunder`, and the only marker separating a model's comment
from a person's is a signature line a model is asked to append.

The trailer convention also names one harness. `Claude-Session` has no
equivalent for Pi or Codex, and `internal/models.md` assigns implementer and
reviewer roles to both.

## Decisions

### A GitHub App rather than a machine user

`internal/thunderstorm.md` Identity records the opposite: no machine account and
no app installation, with conventions standing in. Its reasoning is scoped to a
cloud session, where the container's `GH_TOKEN` is a proxy placeholder and the
MCP tools carry authorization from the account connected at
`claude.ai/connect-github`, so a credential placed in the environment changes
nothing. That section names its own reopen condition, and runs moving to the
operator's machine is one: `gh` there holds a real token, and an installation
token is a real token.

GitHub does not bill for app bot accounts, and a machine user consumes a
license. An app's installation access tokens expire after one hour and can be
scoped to named repositories and to a subset of the permissions the
installation holds, where a personal access token is long-lived and carries its
owner's full access.

The app opens pull requests and comments. It does not push: `origin` here is an
SSH remote, so a branch goes up under the operator's key, and the squash
discards branch authorship anyway. So the app needs Issues and Pull requests at
write, Contents at read, and no webhooks. It is installed on `thunderus` alone
until a second repository runs work.

### Authorship follows supervision, not the harness

A commit from a session the owner drove turn by turn is authored by them, with
the model as a co-author. A commit from a dispatched agent is authored as
`Claude <noreply@anthropic.com>`. The decisions in the first case are the
owner's, and naming the harness there would tell a reader less than it hides.

This is not checkable, and neither was the rule it replaced. A worktree does not
separate the cases, because the `worktree` skill gives one to a local session
working directly as well as to a dispatched subagent. Supervision is decided at
runtime and recorded nowhere the tree can read, so the rule stays a convention
and `internal/thunderstorm.md` Identity is where this repository already accepts
that trade.

### The pull request author is the mechanism

Because a squash merge takes the author from the submitter, opening the pull
request through the app is what puts an agent's name on the permanent record.
Nothing else in the sequence does: branch commits are discarded by the squash,
and comments are not history.

Commit authoring stays as `commits-and-prs` documents it. Under a squash those
authors become co-author trailers, which is the right shape. The app opened the
work, the model wrote it, and the trailers say which model.

### The private key stays on the operator's machine

Neither cloud host can hold it. Claude Code cloud environments warn against
putting secrets in environment variables, because anyone who uses the
environment can read them; the mechanism that would avoid that, an API
credential the agent proxy attaches without the session seeing it, excludes
GitHub, since the GitHub proxy authenticates those requests instead. Codex
cloud encrypts secrets but makes them "only available to setup scripts", and
removes them before the agent phase starts, so a run cannot mint a token when
it needs one.

Running on the operator's machine is settled separately in
`remote-operation.md` (`01M2VZFXVCF6XSDF3PWMQYTWHT`). It leaves one transport
rather than two: a local script mints from the key and `gh` runs as the app, so
no workflow dispatch stands between a session and the board. Label definitions
keep the workflow they already have.

### Claims stay on the human assignee

A GitHub App cannot be an issue assignee. GitHub rejects the write, naming the
bot, and `github-board` defines a claim as a status transition followed by
`--add-assignee @me` with the claim confirmed only when `assignees` is exactly
`[me]`.

Nothing here works around that. A claim is run state that the 24-hour rule
expires, the operator dispatches every run by hand, and one shared identity
would break the read-back that detects a lost race anyway. Attribution on a
claim stays wrong, deliberately, and this paragraph is the record of that
choice.

### A check holds the pull request author

A `pull_request` job fails when a head branch matching `agent/*` carries a pull
request author other than the app. The author rule for commits has been written
down across twenty commits without holding, and `internal/thunderstorm.md`
already states the order: make it impossible or make it fail loudly, and only
then write prose.

### Thunderstorm stays in this repository

Extraction was considered and rejected. No repository needs the workflow, and
correct attribution is not a packaging problem. The coupling is already low
enough that the option stays open: `release`, `worktree`, `implement`, and
`revise` carry every mention of Cargo and this workspace, and the protocol
skills carry at most one each.

A cross-repository view does not require extraction either. A Projects v2 board
is organization-level and holds issues from any repository, so `arclightning`
and `mire` can share one board while the workflow lives here. What is
per-repository is the label set, which `.github/labels.yml` defines and the app
can apply to every installed repository from one place.

## Open

Whether a squash merge of a pull request opened by an app attributes the merge
commit to the app's bot identity. The documented rule is that the author comes
from the submitter, and how that renders for a bot submitter is unverified. If
it holds, the app closes the problem on its own. Settled by opening one
throwaway pull request as the app, squashing it, and reading
`git log --format='%an <%ae>'`.

Whether Projects v2 becomes the cross-repository view. Claude Code cloud
sessions cannot reach it: the GitHub proxy serves only a pinned set of GraphQL
operations, names Projects v2 as unreachable, and returns the same 403 for a
`GH_TOKEN` the caller supplies. Local sessions reach it normally. Settled by
whether any run needs to read the board from a web session.

What a trailer naming the harness looks like for Pi and Codex. Pi sets
`AI_AGENT=pi` in the process environment, which a commit hook can read, so this
may be a check rather than a convention. Settled with the first Pi run that
commits.

## Sources

- `git log` over 401 commits at `52a447b`, for the two author identities and
  the twenty `Claude-Session` trailers.
- [Git commit author shown when squash merging](https://github.blog/changelog/2022-09-15-git-commit-author-shown-when-squash-merging-a-pull-request/),
  for the author coming from the pull request submitter.
- [People who consume a license](https://docs.github.com/en/billing/managing-the-plan-for-your-github-account/about-per-user-pricing),
  for app bots not consuming one.
- [Authenticating as a GitHub App installation](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-as-a-github-app-installation),
  for the one-hour token, the repository and permission scoping, and a webhook
  server being optional.
- [Assigning an agent from a workflow](https://github.com/orgs/community/discussions/186820),
  for GitHub refusing a bot as an assignee. A community thread quoting the
  validation error, not reference documentation.
- [Configure cloud environments](https://code.claude.com/docs/en/cloud-environments),
  read 2026-09-18, for the environment variable warning, API credentials
  excluding GitHub, the GitHub proxy, and the pinned GraphQL operation set.
- [Codex cloud environments](https://developers.openai.com/codex/cloud/environments/),
  for secrets reaching setup scripts only and being removed before the agent
  phase.
