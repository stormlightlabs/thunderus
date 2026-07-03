---
title: "Harness Engineering"
Source: https://openai.com/index/harness-engineering/
Author: Ryan Lopopolo, OpenAI
Date: 2026-02-11
Captured: 2026-07-03
Tags: [codex, harness-engineering, coding-agent, agent-legibility, architecture]
---

## Summary

OpenAI's harness engineering essay argues that agent-first software development
shifts engineering effort away from hand-writing code and toward designing the
repositories, tools, feedback loops, and constraints that let coding agents do
reliable work.

## Key Ideas

- **Humans steer, agents execute:** The team intentionally built an internal
  product with no manually-written code so their scarce human resource became
  intent-setting, review, environment design, and feedback-loop construction.
- **Missing capability is the failure signal:** When Codex struggled, the
  useful question was not how to prompt harder, but what tool, abstraction,
  documentation, or enforceable rule was missing from the agent's environment.
- **Application state must be agent-legible:** UI state, browser behavior, logs,
  metrics, traces, and deployment feedback were exposed to Codex so it could
  reproduce failures, validate fixes, and reason about runtime behavior without
  relying on human QA.
- **Repository knowledge is the system of record:** A short `AGENTS.md` acts as
  a map, while durable knowledge lives in structured, versioned repository docs
  that agents can discover, verify, and update.
- **Legibility beats hidden context:** Information that lives only in chat,
  Google Docs, or people's heads is unavailable to an agent run; important
  decisions need to be encoded into repo-local artifacts.
- **Architecture and taste need mechanical enforcement:** Documentation alone
  cannot keep a high-throughput agent codebase coherent, so the team used custom
  linters, structural tests, and remediation-oriented error messages to enforce
  boundaries and quality rules.
- **High throughput changes merge tradeoffs:** With cheap follow-up corrections
  and expensive human waiting, short-lived PRs, minimal blocking gates, and
  agent-to-agent review became more practical than conventional slow review
  processes.
- **Entropy still accumulates:** Agents copy existing patterns, including bad
  ones, so the system needs recurring cleanup tasks and encoded "golden
  principles" to prevent drift from compounding.

## Claims & Evidence

| Claim                                                                   | Support                                                                                                                 | Caveat / Confidence                                                                  |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| An agent-first team can ship a substantial product without manual code. | OpenAI reports an internal beta built from an empty repo into roughly a million-line codebase over five months.         | Medium-high; this is an internal case study, not independently benchmarked.          |
| Environment design determines agent effectiveness.                      | Early progress slowed because the repo lacked tools, abstractions, and structure; fixes focused on adding capabilities. | High as a reported project lesson; exact tooling needs vary by codebase.             |
| Agent-legible feedback loops reduce human QA bottlenecks.               | Codex could drive per-worktree app instances, inspect DOM/screenshots, query logs/metrics/traces, and validate changes. | High for UI and service work when local observability is available.                  |
| Monolithic instruction files fail at scale.                             | The essay says a large `AGENTS.md` crowded context, diluted priority, rotted, and resisted mechanical verification.     | High for large projects; small repos may not need a large docs system.               |
| Mechanical constraints preserve coherence better than prose alone.      | Custom linters and structural tests enforced layer boundaries, logging, naming, file size, and reliability rules.       | High; rules still need human judgment to choose and maintain.                        |
| Agent throughput makes some normal merge gates counterproductive.       | The team accepted minimal blocking gates and cheap follow-up fixes because agent output exceeded human review capacity. | Medium; this tradeoff depends heavily on tests, blast radius, rollback, and product. |
| Autonomous development loops do not generalize automatically.           | The essay explicitly says end-to-end feature work depends on that repository's structure and tooling.                   | High.                                                                                |
| Continuous cleanup is required for fully agent-generated systems.       | The team moved from manual weekly cleanup to recurring Codex tasks that detect deviations and open refactor PRs.        | High for pattern-replicating agents; exact cleanup cadence is contextual.            |

## Important Terms

| Term                      | Meaning                                                                                                         |
| ------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Harness engineering       | Designing the tools, prompts, repository structure, feedback loops, and constraints around coding agents.       |
| Agent legibility          | Making code, docs, app state, and operational signals accessible and understandable to an agent while it runs.  |
| Repository knowledge base | Versioned in-repo documentation and generated artifacts used as the source of truth for agent work.             |
| Progressive disclosure    | Letting agents start from a compact map and follow links to deeper context only when a task needs it.           |
| Taste invariant           | A mechanically enforceable quality rule that captures human judgment about architecture, style, or reliability. |
| Garbage collection        | Recurring cleanup work that prevents agent-amplified technical debt and stale patterns from compounding.        |

## Questions for Review

- What changes when engineers stop being the primary code writers and become
  designers of agent environments?
- Why did OpenAI treat failures as missing capabilities rather than prompt
  effort problems?
- What kinds of runtime signals did the team expose to Codex to reduce the human
  QA bottleneck?
- Why is a short `AGENTS.md` used as a map instead of a comprehensive manual?
- How do custom linters and structural tests help an agent-generated codebase
  stay coherent?
- Why can higher agent throughput justify different merge and review tradeoffs?
- What risks remain even after an agent can drive features end-to-end?

## Connections

- Related ideas: agent scaffolding, repository-local memory, progressive
  disclosure, tool affordances, structural tests, self-review, observability as
  context.
- Related sources:
  [codex-prompting-guide](./codex-prompting-guide.md),
  [codex-prompts](./codex-prompts.md), [agents-md](./agents-md.md),
  [skills](./skills.md), [sessions](./sessions.md).
- Contradictions or tensions: the essay values agent-readable in-repo
  reimplementation over opaque dependencies, but that can conflict with normal
  maintainability instincts that prefer battle-tested libraries.
- Useful applications: use short maps instead of giant instruction files, expose
  executable feedback loops to agents, turn repeated review comments into
  lintable rules, and run small recurring cleanup tasks before drift spreads.

## Open Questions

- Which parts of OpenAI's result depend on Codex-specific model behavior versus
  general agent-harness design?
- How should teams decide when reimplementing a dependency for agent legibility
  is worth the long-term maintenance cost?
- What merge-gate minimum is required before high-throughput agent follow-up
  stops being reckless?
- How does architectural coherence evolve over years in a codebase where agents
  write all production code?
- Which human judgments are best encoded as docs, which as linters, and which
  should stay as explicit human review?

## Notable Quotes

> "Humans steer. Agents execute."

---

> "Give Codex a map, not a 1,000-page instruction manual."

## Takeaways

- Agent-first development moves leverage into environment design: tools,
  documentation, observability, review loops, and enforceable constraints.
- Agents need legible systems, not just more instructions; anything important
  should be discoverable, versioned, and preferably mechanically checked.
- High-throughput agent codebases need continuous garbage collection because
  agents amplify both good and bad existing patterns.
