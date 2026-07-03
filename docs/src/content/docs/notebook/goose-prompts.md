---
title: "Goose Prompt Directory"
Source: https://github.com/aaif-goose/goose/tree/main/crates/goose/src/prompts
Author: AAIF / Goose contributors
Date: 2026-07-01
Captured: 2026-07-01
Tags: [goose, prompts, coding-agent, prompt-fragments]
---

## Summary

Goose keeps its prompt surface as small markdown templates for agent identity, planning,
compaction, subagents, recipes, app generation, permissions, and tiny-model behavior.

## Key Ideas

- **Prompt roles are split by job:** The directory separates the main system prompt from planner,
  compaction, subagent, recipe, and session-name prompts.
- **The base system prompt is short:** It identifies Goose, lists active extensions and their
  instructions, warns when too many tools are enabled, and asks for Markdown responses.
- **Extensions carry dynamic tool context:** The system prompt renders enabled extensions and their
  instructions rather than baking every tool rule into one static prompt.
- **Planning is a separate prompt:** The planner prompt produces either clarifying questions or a
  step-by-step executor plan, with the reminder that the executor will only see the final plan.
- **Compaction is operational:** The compaction prompt asks for user intent, technical concepts,
  files, errors, pending tasks, current work, and next steps so a later agent can continue.
- **Subagents are bounded:** The subagent prompt emphasizes scope, tool efficiency, turn limits, and
  concise completion reporting.
- **Tiny-model behavior is explicit:** The tiny-model prompt gives concrete shell-command syntax and
  examples, making the desired interaction pattern easy to follow.

## Claims & Evidence

| Claim                                                                    | Support                                                                                                              | Caveat / Confidence                                                  |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Goose favors focused prompt templates over one monolith.                 | Directory files include `system.md`, `plan.md`, `compaction.md`, `subagent_system.md`, `recipe.md`, and app prompts. | High.                                                                |
| Dynamic tool context belongs near the runtime data.                      | `system.md` renders active extensions and extension instructions conditionally.                                      | High.                                                                |
| Planning and compaction are treated as separate tasks.                   | `plan.md` and `compaction.md` define specialized outputs and context requirements.                                   | High.                                                                |
| Subagent prompts can reduce tool sprawl by emphasizing scope and limits. | `subagent_system.md` tells the agent to use the minimum tools needed and stay within assigned scope.                 | Medium-high; only relevant if the product has child-agent workflows. |
| Small-model prompts benefit from concrete examples.                      | `tiny_model_system.md` shows how to run commands and when not to.                                                    | Medium; useful pattern, not a required feature.                      |

## Important Terms

| Term       | Meaning                                                                                                |
| ---------- | ------------------------------------------------------------------------------------------------------ |
| Extension  | Goose mechanism that adds tools and context to the active prompt.                                      |
| Planner    | A specialized prompt that converts a user request into clarifying questions or an executor-ready plan. |
| Compaction | A prompt that summarizes a long session for continuation after context pressure.                       |
| Recipe     | A reusable instruction package generated from prior conversation context.                              |
| Subagent   | A bounded child agent prompt with its own task instructions and tool limits.                           |

## Questions for Review

- Which Goose prompt roles are useful as separate product features, and which
  should remain external inspiration?
  - Treat compaction and session naming as plausible future features, and leave subagents, recipes, and app generation as external inspiration.
- How can dynamic tool/context instructions stay visible without importing
  Goose's extension system?
  - Render dynamic tool and context sections from local runtime data rather than adopting an extension framework.
- What is the smallest useful continuation summary for a coding-agent session?
  - Preserve user intent, changed files, tool failures, constraints, and the next concrete step.

## Connections

- Related ideas: prompt assembly, session persistence, compaction, tool catalog rendering.
- Related sources: [prompts](prompts.md), [sessions](./sessions.md), [pi-prompts](pi-prompts.md).
- Tension: Goose supports extensions and subagents; smaller harnesses should add
  those only when the local workflow needs them.
- Conceptual use: keep prompt fragments focused and put runtime-specific context
  in generated sections.

## Open Questions

- Should a harness add a compaction prompt only after session persistence is stable?
  - **Recommendation**: Add compaction only after sessions can persist enough
    structured context to verify what was preserved and what was dropped.
- Should future "plan" behavior be a mode, a slash command, or just ordinary user prompting?
  - **Recommendation**: Keep runtime context generated and visible, and split
    specialized prompts only when the workflow is real and repeated.

## Takeaways

- Keep the base prompt short and let runtime context be rendered explicitly.
- Separate specialized prompt jobs only when the product actually has that workflow.
- Use concrete operational instructions for small or constrained models.
