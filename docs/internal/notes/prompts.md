---
Title: Coding Agent Prompt Structure
Sources:
  - https://github.com/openai/codex/
  - https://github.com/sst/opencode/
  - https://github.com/block/goose/blob/
  - https://github.com/Aider-AI/aider/
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
the user's turn-specific request.

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

| Claim                                                          | Support                                                                                                                                               | Caveat / Confidence                                   |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Prompt assembly should be layered.                             | Codex uses base instructions plus contextual fragments; Goose uses a `PromptManager`; OpenCode has prompt variants and typed prompt schemas.          | High.                                                 |
| Project instructions are context, not enforcement.             | Claude Code memory docs describe instruction files as loaded guidance; Codex describes `AGENTS.md` as scoped repo guidance below direct instructions. | High.                                                 |
| Tool and edit constraints should live near the base prompt.    | Codex and OpenCode include tool usage and editing constraints in base instructions; Aider's edit-format prompts define exact diff output rules.       | High.                                                 |
| The first prompt structure can be simpler than Codex/OpenCode. | `thndrs` has one provider, a small tool set, and no plugin/MCP/subagent system yet.                                                                   | High; avoid importing unnecessary framework concepts. |
| Prompt logs should record what context was sent.               | Session research showed durable context metadata matters for audit and resume; prompt assembly should expose this as structured metadata.             | Medium-high.                                          |

## Important Terms

| Term             | Meaning                                                                                              |
| ---------------- | ---------------------------------------------------------------------------------------------------- |
| Base prompt      | Stable model-facing instructions for identity, tone, safety, and work style.                         |
| Context fragment | A labeled piece of model context such as `AGENTS.md`, environment metadata, or loaded file excerpts. |
| Prompt bundle    | Internal structured representation of all parts before lowering to provider messages.                |
| Prompt lowering  | Conversion from structured prompt bundle into provider-specific chat/messages format.                |
| Turn request     | One user submission plus selected model, tools, context, and runtime settings.                       |
| Edit format      | The model-visible contract for how edits are proposed or applied.                                    |

## Practical Shape for `thndrs`

Start with a small internal `PromptBundle` rather than a string-concatenation
helper:

```text
PromptBundle
- base: identity, concise coding-agent behavior, output style
- policy: no raw shell, bounded tools, workspace containment
- environment: cwd/root, current date/timezone, model, search mode
- project_context: AGENTS.md metadata and text when loaded
- tool_catalog: read-only tool names, schemas, limits, truncation behavior
- transcript_tail: recent user/assistant/reasoning/tool entries
- user_turn: current prompt text and attachments later
```

Recommended alpha order:

1. Base coding-agent identity.
2. Harness policy and tool boundary.
3. Environment metadata.
4. Loaded `AGENTS.md` context.
5. Tool catalog.
6. Relevant transcript tail.
7. Current user prompt.

Rules for alpha:

- Keep provider-specific lowering in the concrete Umans client path for now.
- Store prompt-bundle metadata in session JSONL: model, search mode, context
  sources, context hashes, tool catalog version, and truncation state.
- Do not persist full provider raw requests by default; they can contain prompt
  text, repo content, and secrets.
- Treat `AGENTS.md` and future prompt files as guidance only. Harness policy and
  direct user/developer instructions stay above project guidance.
- Keep edit instructions minimal until safe file operations exist.

## Questions for Review

- Should `thndrs` expose the base prompt through `--print-prompt` or wait until
  v1 inspect/export?
- Should the current date be exact per run or rounded for cache stability?
- Should prompt construction include the full transcript tail or a projected
  model-visible view that omits UI-only status entries?
- Should read-only tool schemas be included in prompt text, provider-native tool
  definitions, or both for Umans?

## Connections

- Related ideas: session JSONL should record prompt metadata; AGENTS.md notes
  define project-context precedence; fs traversal notes define tool boundaries.
- Related sources: [sessions](./sessions.md), [agents-md](./agents-md.md),
  [fs-traversal](./fs-traversal.md), [providers/umans](./providers/umans.md).
- Contradictions or tensions: Rich prompts improve behavior, but long prompts
  burn context and hide policy in prose. Keep the structure explicit and small.
- Useful applications: prompt assembly, search integration, session persistence,
  v1 inspect/export.

## Open Questions

- Does Umans support provider-native tool schemas closely enough that text tool
  descriptions can stay minimal?
- How much of the tool catalog should be repeated every turn versus cached by
  the provider?
- Should `AGENTS.md` text be included every turn or only when the context hash
  changes and the provider supports history reuse?

## Takeaways

- Build a structured prompt bundle first, then lower it to provider messages.
- Keep base identity, policy, environment, project context, tool catalog,
  transcript, and user turn as separate pieces.
- Make prompt metadata durable without storing full raw provider payloads by
  default.
