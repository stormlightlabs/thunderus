---
title: Claude Code system prompt corpus
Source: https://github.com/Piebald-AI/claude-code-system-prompts
Author: Piebald AI, extracted from Anthropic Claude Code package artifacts
Date: 2026-06-29
Captured: 2026-06-29
Tags: [claude-code, prompts, tools, safety, agent-harness, thndrs]
---

## Summary

The repository shows Claude Code as a prompt-and-tool system made of many small
conditional prompt fragments, not one monolithic prompt, with separate guidance
for tool descriptions, planning, review, summarization, safety classification,
and utility agents.

## Source Caveats

- The repository is maintained by Piebald AI, not Anthropic.
- Its `CLAUDE.md` says the files are extracted reference material, not source
  that changes Claude Code behavior.
- The README says the prompts are extracted from compiled Claude Code package
  output and include runtime template variables, so exact session prompts vary.
- Treat this as a design corpus for patterns, not as authoritative Anthropic
  product documentation.

## Key Ideas

- **Prompt fragments over a single prompt:** Claude Code behavior is assembled
  from many focused fragments: core harness behavior, tool descriptions,
  subagent prompts, slash-command prompts, utility prompts, and safety prompts.
- **Tool descriptions carry policy:** File, search, write, shell, web, task,
  and plan tools all encode when to use the tool, when not to use it, and what
  safety constraints matter.
- **Dedicated tools beat shell:** Search/read/edit prompts repeatedly prefer
  specialized tools over shell commands when a narrower tool fits.
- **Planning is read-only:** Plan mode is explicitly separated from editing and
  forbids file creation/modification while it explores and proposes an approach.
- **Task tracking is conditional:** Todo/task tools are recommended for
  multi-step work and explicitly discouraged for trivial single-step requests.
- **Safety is layered:** Claude Code uses action-care guidance, permission
  classifiers, deny-rule circumvention checks, hook feedback, and security
  monitor prompts rather than relying only on the main coding prompt.
- **Summaries are operational artifacts:** Conversation, session title,
  transcript chunk, and away-summary prompts preserve user intent, files,
  errors, feedback, and next steps for continuation.
- **Review prompts bias toward verified findings:** Code review and security
  review prompts define scope, evidence bars, false-positive filters, and output
  formats.
- **Communication is user-facing and sparse:** The communication fragments favor
  short progress updates, truthful reporting, direct final summaries, and
  avoiding unnecessary comments or docs.

## Claims & Evidence

| Claim                                                                        | Support                                                                                                                                                                            | Caveat / Confidence                                                                     |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Claude Code has many prompt pieces rather than one static system prompt.     | README lists 500+ prompt strings across tool descriptions, subagents, utilities, and system fragments.                                                                             | High for this extracted repo; exact runtime assembly is still conditional.              |
| Tool descriptions are part of the behavioral contract.                       | `tool-description-readfile.md`, `tool-description-grep.md`, `tool-description-edit.md`, `tool-description-write.md`, and others contain usage rules and constraints.               | High.                                                                                   |
| Narrow tools should be preferred over shell for file/search/edit operations. | Grep, PowerShell, and harness fragments all prefer dedicated search/read/edit tools over generic shell use.                                                                        | High; maps directly to `thndrs` current read-only tool boundary.                        |
| Planning should be isolated from mutation.                                   | `agent-prompt-plan-mode-enhanced.md` forbids writes, deletes, redirects, temp files, and state-changing commands during planning.                                                  | High.                                                                                   |
| Complex task tracking should be explicit but not universal.                  | Todo/task prompts list complex/multi-step work as use cases and simple conversational tasks as non-use cases.                                                                      | High.                                                                                   |
| Safety policy benefits from separate classifiers.                            | Command-prefix detection, permission classifier, deny-rule circumvention, hook feedback, and security monitor fragments separate action evaluation from the main assistant prompt. | Medium-high; some classifiers assume broader shell/edit access than `thndrs` has today. |
| Session continuity needs structured summaries.                               | Conversation summarization prompt requires user intent, files, errors, user feedback, pending tasks, current work, and next step.                                                  | High.                                                                                   |
| Review prompts should define scope and confidence.                           | Review/security-review prompts constrain the diff scope, require concrete failure/exploit scenarios, and filter low-confidence findings.                                           | High.                                                                                   |

## Important Terms

| Term                    | Meaning                                                                                                    |
| ----------------------- | ---------------------------------------------------------------------------------------------------------- |
| Prompt fragment         | A small prompt component included only for a specific tool, mode, command, or runtime condition.           |
| Tool description        | Model-visible contract for when and how to use a tool.                                                     |
| Plan mode               | Read-only exploration mode that produces an implementation plan before edits.                              |
| Permission classifier   | Prompted classifier that decides whether an action should be blocked or require confirmation.              |
| Deny-rule circumvention | Using another route, such as shell redirection or scripts, to bypass a blocked edit/write operation.       |
| Security monitor        | Separate evaluator for dangerous autonomous actions, prompt injection, scope creep, and accidental damage. |
| Session summary         | Structured continuation artifact that preserves intent, changes, errors, constraints, and next steps.      |

## Open Questions

- Which prompt-fragment patterns can improve `thndrs` without adding plan mode
  or task management?
- Should LSP be implemented through installed language servers, a Rust crate,
  or deferred until the read-only file tools feel insufficient?
- How much of the security monitor should be static Rust policy versus
  model-assisted classification?
- Should review/security-review be commands, prompt modes, or ordinary prompts
  backed by helper prompt fragments?
- Which proposed tools are worth adding before write-capable tools exist?
- Can command/action classifiers be deterministic enough to avoid adding another
  model call?
- How should prompt fragments be tested: unit snapshots, prompt-bundle debug
  snapshots, or fixture-lowered provider messages?
- Should user-facing docs document prompt fragments, or only the behaviors they
  produce?

## Connections

- Related ideas: [prompts](prompts.md), [fs-traversal](fs-traversal.md),
  [sessions](./sessions.md), and [specs/v1](../specs/v1.md).
- Related sources: Claude Code overview docs, Codex manual outline, Polytoken
  docs IA, Pi docs IA.
- Contradictions or tensions: the Claude corpus contains plan/task managers, but
  `thndrs` should not inherit them unless a separate product decision changes.
- Useful applications: richer prompt bundle, safer write tools, future session
  persistence, review commands, and v1 safety/reference docs.

## Notable Quotes

> "Claude Code doesn't just have one single string"

## Takeaways

- Keep `thndrs` prompt assembly modular: base prompt plus focused fragments.
- Put behavioral rules beside the tools and modes they govern.
- Borrow prompt fragmentation and safety patterns without adding plan mode or
  task management.
