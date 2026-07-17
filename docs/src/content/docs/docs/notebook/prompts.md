---
title: "Coding Agent Prompt Structure"
Sources:
  - https://github.com/openai/codex/
  - https://github.com/sst/opencode/
  - https://github.com/block/goose/blob/
  - https://github.com/Aider-AI/aider/
  - https://app.umans.ai/offers/code/docs
  - https://code.claude.com/docs/en/memory
  - https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/overview
  - https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices
Author: OpenAI Codex, OpenCode, Goose, Aider, Anthropic
Date: 2026-06-29
Captured: 2026-06-29
Tags: [coding-agent, prompts, system-prompt, context, tools]
---

## Summary

Public coding-agent prompts converge on layered prompt assembly: base identity,
behavior policy, tool/editing constraints, environment and project context, then
the user's turn-specific request. The durable lesson is the separation of prompt
concerns before provider-specific lowering, not any one harness's exact fragment
set.

## Key Ideas

- **System prompts are assembled, not monolithic:** Codex, Goose, and OpenCode
  build prompts from reusable fragments such as base behavior, permissions,
  collaboration mode, tool inventory, extension instructions, and project
  guidance.
- **Instruction precedence must be explicit:** Codex puts direct system,
  developer, and user instructions above `AGENTS.md`; Claude Code documents
  broad-to-specific memory loading for managed, user, project, local, and
  path-scoped instructions.
- **Context belongs in labeled blocks:** Codex wraps `AGENTS.md` instructions in
  marked contextual user fragments; Goose appends named additional instructions;
  Claude Code recommends structured markdown sections and concise rules.
- **Prompt input is typed before model lowering:** OpenCode models user prompt
  text plus file and agent attachments; its message-shape notes separate stored
  messages, prompt mutators, and turn/request metadata.
- **Editing strategy changes the prompt:** Aider has separate prompt classes for
  unified diffs, whole-file edits, edit blocks, and shell command policy. The
  edit format is part of the model contract.
- **Prompt structure should support caching and replay:** Goose rounds current
  time to improve prompt-cache stability; Codex and OpenCode persist enough
  context metadata to replay or audit model-visible state.

## Claims & Evidence

| Claim                                                       | Support                                                                                                                                               | Caveat / Confidence                                   |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Prompt assembly should be layered.                          | Codex uses base instructions plus contextual fragments; Goose uses a `PromptManager`; OpenCode has prompt variants and typed prompt schemas.          | High.                                                 |
| Project instructions are context, not enforcement.          | Claude Code memory docs describe instruction files as loaded guidance; Codex describes `AGENTS.md` as scoped repo guidance below direct instructions. | High.                                                 |
| Tool and edit constraints should live near the base prompt. | Codex and OpenCode include tool usage and editing constraints in base instructions; Aider's edit-format prompts define exact diff output rules.       | High.                                                 |
| Prompt structure can be simpler than Codex/OpenCode.        | A small harness can have one provider, a small tool set, and no plugin/MCP/subagent system while still preserving typed prompt assembly.              | High; avoid importing unnecessary framework concepts. |
| Prompt logs should record what context was sent.            | Session research showed durable context metadata matters for audit and resume; prompt assembly should expose this as structured metadata.             | Medium-high.                                          |

## Important Terms

| Term             | Meaning                                                                                              |
| ---------------- | ---------------------------------------------------------------------------------------------------- |
| Base prompt      | Stable model-facing instructions for identity, tone, safety, and work style.                         |
| Context fragment | A labeled piece of model context such as `AGENTS.md`, environment metadata, or loaded file excerpts. |
| Prompt bundle    | Internal structured representation of all parts before lowering to provider messages.                |
| Prompt lowering  | Conversion from structured prompt bundle into provider-specific chat/messages format.                |
| Turn request     | One user submission plus selected model, tools, context, and runtime settings.                       |
| Edit format      | The model-visible contract for how edits are proposed or applied.                                    |

## Harness Comparison

- Codex separates rollout/event history from model-visible reconstruction:
  persisted events can feed UI replay, while base instructions, dynamic tools,
  and turn context are recovered separately for model requests.
- OpenCode uses projected Session messages for durable history and treats live
  text/reasoning fragments as ephemeral. Its session spec calls out canonical
  model-visible lowering and policy-filtered tool materialization per turn.
- Goose stores typed conversation parts and can filter content by audience, so
  reasoning or tool content can be visible to the assistant without necessarily
  being visible to the user.
- Aider keeps model messages as role/content pairs and summarizes older history;
  it does not feed UI-only transcript decorations into the model.
- Across these harnesses, tools are represented as structured tool definitions
  or tool request/response parts when the provider/runtime supports them. Prompt
  text still carries operating policy, but does not replace native schemas.

## Open Questions

- How should prompt assembly stay inspectable without growing into a large static prompt?
  - **Recommendation**: Keep prompt assembly as a layered structured bundle and lower it to provider messages only after policy, context, tools, transcript, and user turn are separated.

## Connections

- Related ideas: session logs can record prompt metadata; AGENTS.md notes define
  project-context precedence; fs traversal notes define tool boundaries.
- Related sources: [sessions](/docs/notebook/sessions/), [agents-md](/docs/notebook/agents-md/),
  [fs-traversal](/docs/notebook/fs-traversal/), [providers/umans](/docs/notebook/providers/umans/).
- Contradictions or tensions: Rich prompts improve behavior, but long prompts
  burn context and hide policy in prose. Keep the structure explicit and small.
- Conceptual uses: prompt assembly, provider lowering, search/source metadata,
  session audit, inspect/export.
