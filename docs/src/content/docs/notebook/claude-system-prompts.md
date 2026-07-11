---
title: "Claude Code System Prompts"
Source: https://github.com/Piebald-AI/claude-code-system-prompts
Author: Piebald AI, extracted from Anthropic Claude Code package artifacts
Date: 2026-06-29
Captured: 2026-07-01
Tags: [claude-code, prompts, tools, safety, extracted-corpus]
---

## Summary

The Piebald AI repository presents Claude Code as a prompt-and-tool system made
of many small runtime-interpolated fragments for agents, tools, modes,
classifiers, reviews, summaries, safety checks, and workflow guidance. Treat it
as an extracted design corpus for prompt patterns, not as authoritative Anthropic
product documentation.

## Source Caveats

- The repository is maintained by Piebald AI, not Anthropic.
- Files are extracted reference material from compiled Claude Code package
  output.
- Editing the repository does not change Claude Code behavior.
- Template variables appear literally and are interpolated by Claude Code at
  runtime, so exact session prompts vary.
- The changelog is useful evidence for prompt evolution, but it is still derived
  from packaged artifacts rather than upstream source.

## Key Ideas

- **Prompt fragments over one monolith:** Claude Code behavior is assembled from
  focused fragments: core harness behavior, tool descriptions, subagent prompts,
  slash-command prompts, utility prompts, classifiers, and safety prompts.
- **Tool descriptions carry policy:** File, search, write, shell, web, todo, and
  workflow prompts encode when to use a tool, when not to, and what constraints
  apply.
- **Dedicated tools beat shell:** Search/read/edit prompts repeatedly prefer
  specialized tools over shell commands when a narrower tool fits.
- **Planning is read-only:** Plan mode is explicitly separated from editing and
  forbids file creation/modification while it explores and proposes an approach.
- **Task tracking is conditional:** Todo/task tools are recommended for
  multi-step work and explicitly discouraged for trivial single-step requests.
- **Safety is layered:** Claude Code uses action-care guidance,
  permission/classifier prompts, deny-rule circumvention checks, hook feedback,
  security review, and private-URL warnings.
- **Summaries are operational artifacts:** Conversation, session title,
  transcript chunk, and away-summary prompts preserve user intent, files,
  errors, feedback, and next steps for continuation.
- **Review prompts bias toward verified findings:** Code review and security
  review prompts define scope, evidence bars, false-positive filters, and output
  formats.
- **Communication is user-facing and sparse:** Communication fragments favor
  short progress updates, truthful reporting, direct final summaries, and
  avoiding unnecessary comments or docs.

## Claims & Evidence

| Claim                                                                        | Support                                                                                                                                                                            | Caveat / Confidence                                                                     |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Claude Code behavior is assembled from many prompt fragments.                | The extracted `system-prompts/` corpus contains hundreds of focused files for agents, tools, modes, classifiers, summaries, and workflows.                                         | High for this extracted repo; exact runtime assembly is still conditional.              |
| Tool descriptions are part of the behavioral contract.                       | `tool-description-readfile.md`, `tool-description-grep.md`, `tool-description-edit.md`, `tool-description-write.md`, and others contain usage rules and constraints.               | High.                                                                                   |
| Narrow tools should be preferred over shell for file/search/edit operations. | Grep, PowerShell, and harness fragments all prefer dedicated search/read/edit tools over generic shell use.                                                                        | High; maps to a bounded tool boundary even when shell exists as a fallback.             |
| Planning should be isolated from mutation.                                   | `agent-prompt-plan-mode-enhanced.md` forbids writes, deletes, redirects, temp files, and state-changing commands during planning.                                                  | High.                                                                                   |
| Complex task tracking should be explicit but not universal.                  | Todo/task prompts list complex/multi-step work as use cases and simple conversational tasks as non-use cases.                                                                      | High.                                                                                   |
| Safety policy benefits from separate classifiers.                            | Command-prefix detection, permission classifier, deny-rule circumvention, hook feedback, and security monitor fragments separate action evaluation from the main assistant prompt. | Medium-high; some classifiers assume broader shell/edit access than `thndrs` has today. |
| Session continuity needs structured summaries.                               | Conversation summarization prompt requires user intent, files, errors, user feedback, pending tasks, current work, and next step.                                                  | High.                                                                                   |
| Review prompts should define scope and confidence.                           | Review/security-review prompts constrain the diff scope, require concrete failure/exploit scenarios, and filter low-confidence findings.                                           | High.                                                                                   |
| Prompt evolution is observable in extracted artifacts.                       | The repository includes a large changelog and an extraction/update script for prompt changes across Claude Code versions.                                                          | Medium-high; useful trend evidence, not upstream source control.                        |
| Patterns should be borrowed selectively.                                     | The corpus includes features many smaller harnesses may not have, such as todos, agents, workflows, classifiers, and security monitors.                                            | High.                                                                                   |

## Important Terms

| Term                    | Meaning                                                                                                    |
| ----------------------- | ---------------------------------------------------------------------------------------------------------- |
| Extracted corpus        | Prompt text recovered from packaged application artifacts rather than maintained as upstream source.       |
| Prompt fragment         | A small prompt component included only for a specific tool, mode, command, or runtime condition.           |
| Tool description        | Model-visible contract for when and how to use a tool.                                                     |
| Mode prompt             | A prompt fragment used only when a specific mode, such as plan or review, is active.                       |
| Classifier prompt       | A specialized prompt that labels or evaluates an action, request, or state.                                |
| Hook feedback           | Runtime feedback injected into the prompt after an external hook or policy step.                           |
| Plan mode               | Read-only exploration mode that produces an implementation plan before edits.                              |
| Permission classifier   | Prompted classifier that decides whether an action should be blocked or require confirmation.              |
| Deny-rule circumvention | Using another route, such as shell redirection or scripts, to bypass a blocked edit/write operation.       |
| Security monitor        | Separate evaluator for dangerous autonomous actions, prompt injection, scope creep, and accidental damage. |
| Session summary         | Structured continuation artifact that preserves intent, changes, errors, constraints, and next steps.      |

## Open Questions

- Which prompt-fragment patterns improve behavior without adding plan mode or
  task management?
  - **Recommendation**: Keep prompt fragments small and behavior-specific, adding modes
    or classifiers only when static tool boundaries are not enough.
- Should more tool-specific prompt text move into tool schemas?
  - **Recommendation**: Borrow the pattern of colocating rules with tools and modes, but
    avoid importing a large prompt ecosystem without a matching product workflow.
- How much of the security monitor should be static Rust policy versus
  model-assisted classification?
  - **Recommendation**: Put hard safety boundaries in deterministic policy and use
    model-assisted classification only for advisory review or explanation.
- Should review/security-review be commands, prompt modes, or ordinary prompts
  backed by helper prompt fragments?
  - **Recommendation**: Start reviews as ordinary prompts with helper fragments, then
    promote them to commands only after the output contract becomes stable.
- Which proposed tools are worth adding before the existing tool set becomes
  hard to use?
  - **Recommendation**: Add tools only when they remove repeated brittle workflows
    that cannot be handled cleanly by existing narrow tools.
- Can command/action classifiers be deterministic enough to avoid adding another
  model call?
  - **Recommendation**: Prefer deterministic classifiers for command shape and path
    scope, and avoid extra model calls unless semantic risk cannot be captured statically.
- How should prompt fragments be tested: unit snapshots, prompt-bundle debug
  snapshots, or fixture-lowered provider messages?
  - **Recommendation**: Test fragments at the bundle level first, then add
    provider-lowered fixtures for behavior that depends on provider message shape.
- Should user-facing docs document prompt fragments, or only the behaviors they
  produce?
  - **Recommendation**: Document the user-visible behaviors and keep fragment internals
    in development docs unless users need them for debugging.
- What is the simplest audit trail for prompt changes across releases?
  - **Recommendation**: Track prompt behavior through focused prompt snapshots and
    notable changelog entries rather than trying to preserve every intermediate prompt
    draft.

## Connections

- Related ideas: [prompts](/notebook/prompts/), [fs-traversal](/notebook/fs-traversal/),
  [sessions](/notebook/sessions/), prompt XML syntax, review prompts, and session
  summaries.
- Related sources: Claude Code overview docs, Codex prompt sources, Polytoken
  docs IA, Pi docs IA, and the Piebald AI extracted Claude Code prompt corpus.
- Contradictions or tensions: the corpus is rich, but copying a large mode/tool
  ecosystem would conflict with a minimal harness.
- Conceptual uses: richer prompt bundle, safer write tools, session continuity,
  review commands, safety/reference docs, and prompt-change audit trails.

## Notable Quotes

> "Claude Code doesn't just have one single string"

## Takeaways

- Treat prompt text as a modular system, not a single personality blob.
- Put behavioral rules beside the tools and modes they govern.
- Preserve provenance and caveats when learning from extracted prompt corpora.
- Borrow prompt fragmentation and safety patterns without adding plan mode,
  task management, classifiers, or subagents by default.
