---
Title: Claude Code Prompt Corpus
Source: https://github.com/Piebald-AI/claude-code-system-prompts
Author: Piebald AI, extracted from Claude Code package artifacts
Date: 2026-07-01
Captured: 2026-07-01
Tags: [claude-code, prompts, extracted-corpus, tool-descriptions]
---

## Summary

The Piebald AI repository presents Claude Code prompts as a large extracted corpus of small
runtime-interpolated fragments for agents, tools, modes, classifiers, reviews, summaries, and
workflow guidance.

## Source Caveats

- The repository is maintained by Piebald AI, not Anthropic.
- Files are extracted reference material from compiled Claude Code package output.
- Editing the repository does not change Claude Code behavior.
- Template variables appear literally and are interpolated by Claude Code at runtime.

## Key Ideas

- **The corpus is fragment-heavy:** `system-prompts/` contains many small markdown files for agent
  prompts, command prompts, tool descriptions, classifiers, safety checks, summaries, and workflow
  guidance.
- **Tool descriptions carry behavior:** Read, write, search, bash, web, todo, and workflow prompts
  include when to use a tool, when not to, and what constraints apply.
- **Safety is layered:** The corpus includes command-prefix detection, permission/classifier prompts,
  hook feedback, security review, and private-URL warnings.
- **Plan and review modes are explicit:** Planning, code review, security review, and verification
  prompts have distinct instructions and expected output.
- **Session continuity is a prompt concern:** Away summaries, conversation summaries, and title
  generation prompts preserve state for later continuation.
- **The changelog is part of the evidence:** The repo tracks prompt changes over Claude Code
  versions, making the corpus useful for studying prompt evolution.
- **Extraction tooling matters:** `tools/updatePrompts.js` documents that this is a derived corpus,
  not the source of truth for Claude Code behavior.

## Claims & Evidence

| Claim                                                         | Support                                                                                                                       | Caveat / Confidence                                  |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Claude Code behavior is assembled from many prompt fragments. | The `system-prompts/` directory contains hundreds of focused files.                                                           | High for this extracted corpus.                      |
| Tool descriptions are part of the model contract.             | Many files are named `tool-description-*` and include usage guidance.                                                         | High.                                                |
| Safety is distributed across multiple prompts.                | The corpus includes bash prefix detection, permission-related prompts, web private URL warnings, and security review prompts. | Medium-high; exact runtime inclusion is conditional. |
| Prompt evolution is observable.                               | The repository includes a large `CHANGELOG.md` for extracted prompt changes.                                                  | High for the repo.                                   |
| `thndrs` should borrow patterns selectively.                  | The corpus includes features `thndrs` does not have, such as todos, agents, workflows, and classifiers.                       | High.                                                |

## Important Terms

| Term              | Meaning                                                                                              |
| ----------------- | ---------------------------------------------------------------------------------------------------- |
| Extracted corpus  | Prompt text recovered from packaged application artifacts rather than maintained as upstream source. |
| Tool description  | Prompt text attached to a tool that teaches when and how to use it.                                  |
| Classifier prompt | A specialized prompt that labels or evaluates an action, request, or state.                          |
| Hook feedback     | Runtime feedback injected into the prompt after an external hook or policy step.                     |
| Mode prompt       | A prompt fragment used only when a specific mode, such as plan or review, is active.                 |

## Questions for Review

- Which safety rules belong in `thndrs` tool descriptions rather than base fragments?
- Should future `thndrs` modes have dedicated prompt fragments or remain ordinary user requests?
- What prompt changes are worth tracking in project notes versus tests and snapshots?

## Connections

- Related ideas: prompt assembly, action safety, XML syntax, review prompts, session summaries.
- Related sources: [claude-system-prompts](claude-system-prompts.md), [codex-prompts](codex-prompts.md).
- Tension: the corpus is rich, but copying a large mode/tool ecosystem would conflict with
  `thndrs` minimalism.
- Useful application: make each prompt rule live as close as possible to the tool or mode it governs.

## Open Questions

- Should `thndrs` eventually move more tool-specific prompt text into tool schemas?
- What is the simplest audit trail for prompt changes across releases?

## Takeaways

- Treat prompt text as a modular system, not a single personality blob.
- Put tool behavior in tool descriptions where practical.
- Preserve provenance and caveats when learning from extracted prompt corpora.
