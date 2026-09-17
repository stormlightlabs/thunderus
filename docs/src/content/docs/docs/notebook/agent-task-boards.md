---
title: "Agent Task Boards and Branch Policy"
Author: GitHub documentation, Brittany Ellich, OpenAI harness engineering, agent-skill authors
Date: 2026-06-15
Captured: 2026-09-16
Tags: [github, issues, projects, external-state, branch-protection, merge-queue, release-train, verification]
Sources:
  - https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-sub-issues
  - https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets
  - https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue
  - https://brittany-ellich.offprint.app/a/3mrjj34puva23-108-prs-in-eight-days-accidentally-discovering-loop-engineering
  - https://openai.com/index/harness-engineering/
  - https://github.com/irangareddy/gh-project-board
  - https://release-plz.dev/docs
  - https://gist.github.com/haileyok/6be6e7b24bab608d509b659e07915ba8
---

## Summary

A loop needs a place to keep work that outlives a context window, and a set of
gates that bound what an autonomous run can break. GitHub Issues and Projects
supply the first; rulesets, required checks, and a two-branch release model
supply the second. This note collects what those surfaces actually guarantee,
where they are thin, and how in-repository markdown divides responsibility with
them.

The division that holds up: issues carry *queue state* (what is being worked,
by whom, in what status), repository markdown carries *durable intent* (specs,
plans, protocols, decisions), and branch policy carries *authority* (what is
allowed to merge where). Conflating them produces a board nobody trusts or a
docs tree nobody reads.

## Key Ideas

A board has to survive concurrent writers. Ellich abandoned a single markdown
task table within a day, because parallel agents collided on it, and moved to one
file per task. GitHub issues carry that property already: each issue is an
independently addressable record with its own state, and concurrent updates do
not corrupt a shared document.

Issues are also reachable from an agent without custom integration. The `gh` CLI
covers create, list, edit, comment, label, and close, and Projects v2 fields are
reachable through `gh project` and GraphQL. Several published agent skills exist
to let an agent discover, claim, execute, and complete board items.

- **Sub-issues give real hierarchy:** GitHub supports up to 100 sub-issues per
  parent and 8 levels of nesting, with parent-child progress surfaced in
  Projects views. A spec becomes a parent; its dispatchable units become
  sub-issues. This replaces the checklist-in-a-description pattern that agents
  routinely corrupt.
- **Status belongs in one field:** A loop needs one authoritative status per
  task. Duplicating status across labels, a project field, and a markdown file
  produces divergence. Pick one and let labels carry orthogonal facets such as
  area, risk, or blocked-reason.

A claim has to be visible, so two runs do not pick up the same item. Assignment
plus a status transition covers it, and a human can read the result without
running anything.

Durable intent belongs in the repository. OpenAI's harness-engineering account
states that information living only in chat or external documents is unavailable
during an agent run. Specs, plans, and protocols should be versioned files that
agents can read, verify, and update, reviewed through the same pull requests as
code.

The entry document stays short. The same account describes a monolithic
instruction file failing at scale by crowding context, diluting priority, and
rotting. `AGENTS.md` is a map; depth lives in linked documents and skills read on
demand.
- **Rulesets are the real enforcement layer:** Required status checks, required
  pull requests, linear history, deletion and force-push protection, and
  deployment-environment requirements are configurable per branch pattern.
  Rulesets replace and generalize classic branch protection, and they apply to
  agent-authored pull requests exactly as to human ones.
- **Merge queues exist for exactly this throughput problem:** A queue re-tests
  each pull request against the tip of the target branch plus everything already
  queued, so a branch is not broken by changes that were individually green but
  mutually incompatible. That failure gets likely when many independent agents
  open pull requests against one branch.
- **Two branches separate integration from release:** An integration branch
  accumulates verified change; a release branch reflects what is shipped. The
  split gives autonomous merges somewhere to land that is protected but not
  user-facing, and it makes "what is released" answerable by a ref rather than
  by memory.
- **Mechanical gates outlast prose rules:** Documentation alone does not keep a
  high-throughput agent codebase coherent. Linters, structural tests, and
  remediation-oriented error messages encode the rules in a form that fails a
  build rather than being politely ignored.

## Claims & Evidence

| Claim                                                                  | Support                                                                                                                     | Caveat / Confidence                                                                                            |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Per-item records avoid the write contention that breaks shared task files. | Ellich's single-table board failed within a day under concurrent agents; per-task files fixed it. Issues share that shape. | High.                                                                                                          |
| GitHub sub-issues support decomposition to the depth a loop needs.     | Documented limits of 100 sub-issues per parent and 8 nesting levels, with progress shown in Projects.                       | High; the limits are documented, and they are far above practical need.                                        |
| Agents can operate a Projects v2 board through the CLI without custom infrastructure. | Multiple published agent skills implement discover/claim/execute/complete against Projects v2 using `gh`.             | Medium-high. Projects v2 field updates need GraphQL or `gh project` subcommands; ergonomics are rougher than issues. |
| Rulesets can require checks, reviews, and linear history per branch pattern. | GitHub's available-rules documentation for rulesets.                                                                     | High.                                                                                                          |
| Merge queues prevent semantically incompatible green pull requests from breaking a branch. | Documented queue behavior: re-test against target tip plus queued entries.                                            | High for the mechanism. Value scales with merge rate; at low volume it adds latency for little benefit.        |
| Repository-versioned knowledge outperforms chat-resident knowledge for agents. | OpenAI's harness-engineering essay treats repository docs as the system of record and chat as unavailable context.       | High as a reported lesson from an agent-first codebase.                                                        |
| Short entry instructions beat comprehensive ones.                      | The same essay reports a large `AGENTS.md` crowding context and rotting; a map plus progressive disclosure replaced it.    | High for large projects; small repositories may not hit the ceiling.                                           |
| Release automation can drive tags and changelogs from merge events.    | `release-plz` opens a release pull request from conventional-commit history and tags on merge; artifact builds trigger from the tag. | High for the mechanism; adoption is a separate decision with its own constraints.                       |
| Agent pull requests still need human-meaningful evidence.              | Ellich replaced line-by-line review with outcome-level verification rather than removing verification.                     | Medium. The transferable part is the evidence requirement, not the review removal.                             |

## Board Model

A workable minimum, with one authority per column.

| Layer               | Lives in                              | Owns                                                                        | Written by                          |
| ------------------- | ------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------- |
| Intent              | Repository markdown (specs, plans)    | What should exist and why; acceptance criteria; decisions.                   | Humans and agents, via pull request. |
| Protocol            | Repository markdown                   | Statuses, ordering, claim rules, merge criteria, stop conditions, escalation. | Humans, via pull request.            |
| Queue               | GitHub issues and one project field   | What is actionable now, its status, and its owner.                            | The dispatching layer.               |
| Evidence            | Pull request body and CI              | What changed, what was verified, what was not.                                | The worker.                          |
| Authority           | Rulesets and required checks          | What may merge where.                                                         | Repository administrators.           |

Statuses should be few and mutually exclusive, and every one of them needs a
defined exit. A status with no rule for leaving it becomes a landfill. The
shapes that recur in practice: queued, claimed, in review, awaiting human
verification, blocked, done, abandoned. `Blocked` needs a reason field, and
`awaiting human verification` accumulates by design: it is the human bottleneck
made visible rather than hidden.

## Branch Policy

A two-branch model with distinct jobs:

| Branch | Represents                              | Accepts                                              | Typical gates                                                                          |
| ------ | --------------------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `edge` | Verified but unreleased integration.    | Agent and human pull requests from task branches.    | Required checks, pull request required, no force push, no deletion, linear history.     |
| `main` | The current release; matches the tag.   | Release pull requests from `edge` only.              | Everything `edge` requires, plus human approval and release checks.                     |

The properties worth preserving:

- `main` answers "what is shipped" by inspection, not by recollection. It should
  equal the latest release tag between releases.
- `edge` is where autonomous merges land, so a regression is contained to an
  unreleased branch rather than to users.
- Cutting a release is one reviewed pull request from `edge` to `main`, which is
  also the natural place for a human to read an accumulated diff at
  outcome level rather than per change.
- Rollback is reverting a release pull request, not reconstructing history.
- Both branches restrict deletion and force-push. An autonomous system with
  push access and no such rule is one bad command from an unrecoverable state.

The open question this model does not answer by itself is whether `edge` needs a
merge queue. The queue earns its latency only when the merge rate is high enough
that pull requests routinely go stale against each other.

## Verification Gates

Ordering matters: cheap mechanical gates first, agent review second, human
judgment last and only where the first two cannot reach.

| Gate                    | Catches                                                         | Cost                        |
| ----------------------- | ----------------------------------------------------------------- | --------------------------- |
| Formatting and lint     | Style drift and a large class of mechanical errors.              | Seconds.                    |
| Type and build checks   | Structural breakage across the workspace.                        | Minutes.                    |
| Tests                   | Behavioral regression the suite already covers.                  | Minutes.                    |
| Structural or custom lints | Architecture and layering violations that prose cannot enforce. | Cheap once written.        |
| Reviewer agent          | Intent mismatch, missing cases, unjustified scope.               | Tokens.                     |
| Human verification      | Product judgment, taste, risk acceptance, released behavior.     | The scarce resource.        |

### Review Topology

A reviewer agent is a topology, not a single call. One published skill that has
seen real use runs two reviewers concurrently over the same diff and merges
their output:

| Reviewer    | Brief                                                                                              |
| ----------- | ---------------------------------------------------------------------------------------------------- |
| Standard    | Balanced pass over correctness, edge cases, security, concurrency, performance, API compatibility, tests, readability. |
| Adversarial | Worst-case hunt: security holes, races, unhandled input, broken invariants, weak coverage.           |

Its structural rules are the transferable part:

- **Review logic lives only in the subagents.** The dispatching agent gathers
  the diff, commit log, and metadata, then waits. It does not form its own
  opinion, so it cannot launder its own dispatch decisions into a verdict.
- **No partial feedback.** Nothing is reported until both reviewers return, and
  findings are then deduplicated by severity with their source tagged as
  standard, adversarial, or both.
- **One finding format,** carrying severity, location, the problem, why it
  matters, and a fix direction. Severity runs blocker, high, medium, low, nit.
- **Two modes with different authority.** Reviewing someone else's work is
  read-only. Reviewing work the agent authored permits iteration: fix root
  causes, add tests, re-review.
- **The iteration has explicit stop rules:** a hard cap on cycles, and an exit
  when the same finding recurs twice. These are exactly the stop rules loop
  engineering says must be written down rather than assumed, and they belong to
  the reviewer rather than to the orchestrator.
- **Publishing a review requires consent.** Summaries go to the operator by
  default; posting to a pull request is a separate, permitted act.

The mode distinction maps onto a loop directly. A worker verifying its own
change is in authoring mode and may iterate under the cap. The layer that
verifies a worker's pull request is external and stays read-only.

Two constraints apply. The agent that wrote a change cannot be the only thing
that judged it, because self-evaluation inherits the blind spots that produced
the error. A verifier the agent can edit does not verify: weakening the test is
the cheapest way to pass it.

## Important Terms

| Term              | Meaning                                                                                                     |
| ----------------- | ------------------------------------------------------------------------------------------------------------- |
| Ruleset           | GitHub's configurable rule collection applied to branch or tag patterns; the successor to branch protection.  |
| Required check    | A status check that must pass before a pull request may merge.                                                |
| Merge queue       | A serialized merge mechanism that re-tests each pull request against the target tip plus queued entries.      |
| Sub-issue         | A child issue linked to a parent, forming a tracked hierarchy visible in Projects.                            |
| Integration branch | The protected branch that accumulates verified but unreleased change; `edge` here.                           |
| Release branch    | The protected branch that reflects the shipped release; `main` here.                                          |
| Claim             | The visible assignment plus status transition that gives one run exclusive ownership of a task.               |
| Evidence          | The record in a pull request of what was verified and what was not.                                           |
| Triage inbox      | The queue of items a loop could not resolve, routed to a human rather than dropped.                           |

## Questions for Review

- Which status field is authoritative, and what mechanism prevents a second
  copy of status from appearing?
- What does a worker have to write in a pull request body for outcome-level
  review to be possible?
- Does `edge` need a merge queue at this project's merge rate, or only required
  checks?
- What rule governs a task that sits in `awaiting human verification` past some
  age?
- Which architectural rules in `AGENTS.md` could become failing checks instead
  of prose?
- How is the release cut recorded so that `main`, the tag, and the changelog
  cannot drift apart?
- What happens to an issue whose worktree was abandoned mid-run?

## Takeaways

- Split queue state, durable intent, and merge authority across issues,
  repository markdown, and rulesets. Each one handles the others' job badly.
- Decompose with sub-issues rather than checklists inside a description. Agents
  corrupt shared documents and leave separate records intact.
- Protect both branches against deletion and force-push before granting an
  autonomous system push access.
- Order gates by cost, keep the verifier outside the agent's reach, and spend
  human attention on the judgments no gate can make.
- Related ideas: [loop-engineering](/docs/notebook/loop-engineering/),
  [worktrees](/docs/notebook/worktrees/),
  [harness-engineering](/docs/notebook/harness-engineering/),
  [agents-md](/docs/notebook/agents-md/), [release](/docs/notebook/release/),
  [docs](/docs/notebook/docs/).
