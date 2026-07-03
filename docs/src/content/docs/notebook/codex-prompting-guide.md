---
title: "Codex Prompting Guide"
Source: https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide
Author: Noah MacCallum (OpenAI), Brian Fioca (OpenAI)
Date: 2026-07-02
Captured: 2026-07-02
Tags: [codex, prompts, coding-agent, tools, compaction]
---

## Summary

OpenAI's Codex guide argues that Codex performance depends as much on harness prompt/tool design as on model choice: prompts should emphasize autonomy, efficient exploration, correct tool use, and concise user updates, while tools should match trained patterns and preserve model-visible state.

## Key Ideas

- **Start from Codex-style operating rules:** The recommended starter prompt centers bias to action, persistence, root-cause fixes, existing-code reuse, restrained clarification, and verification after behavior changes.
- **Remove fragile rollout chatter for older Codex models:** The guide warns that prompting for upfront plans or preambles can cause premature stopping before GPT-5.3 Codex's `phase` support.
- **Use project instructions as ordered context:** `AGENTS.md` files are loaded from global/root/deeper directories and injected before the user prompt, with later path-specific instructions overriding earlier ones.
- **Tool design is a performance lever:** Codex is trained strongly around `apply_patch`, shell commands, and plan updates; custom tools work best when names, arguments, and output shapes are semantically clear and close to familiar command behavior.
- **Parallel reads need explicit prompting:** When parallel tool calls are enabled, the prompt should tell the model to think through needed files first, batch independent reads/searches, and only sequence calls when later targets are unknowable.
- **Long-running sessions need compaction/state preservation:** Responses compaction carries prior state forward in fewer tokens; GPT-5.3 Codex assistant items also need `phase` metadata persisted and replayed.
- **Preambles should feel like pairing, not logs:** For GPT-5.3 Codex, updates should acknowledge, orient, and report impact/next steps in short natural language at a modest cadence.

## Claims & Evidence

| Claim                                                                                      | Support                                                                                                                                                            | Caveat / Confidence                                            |
| ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| Prompt snippets for autonomy, exploration, tool use, and frontend quality are high-impact. | The migration section calls these the most critical snippets when updating a harness prompt.                                                                       | High for Codex-family models; other models may need evals.     |
| Prompting old Codex models for upfront plans/status can hurt rollouts.                     | The guide explicitly says to remove upfront plan/preamble/status prompting because it can cause abrupt stops before rollout completion.                            | High for older Codex; GPT-5.3 Codex changes this with `phase`. |
| Dedicated non-terminal tools can work if they stay in distribution.                        | The tools section says terminal-like tools work best when names, arguments, and outputs resemble underlying commands, and prompt directives can steer tool choice. | Medium-high; still requires tuning.                            |
| Parallel-call behavior improves with explicit batching instructions.                       | Codex CLI adds a system snippet telling the model to plan needed reads, call `multi_tool_use.parallel`, and avoid sequential reads unless necessary.               | High when the provider supports parallel calls.                |
| Tool output truncation should keep beginning and end.                                      | The guide recommends about 10k tokens, approximated as bytes/4, with middle truncation.                                                                            | Medium; exact budget depends on provider and tool shape.       |
| GPT-5.3 Codex requires preserving assistant `phase`.                                       | The guide says dropping phase metadata in reconstructed history can significantly degrade performance.                                                             | High for GPT-5.3 Codex via Responses API.                      |

## Important Terms

| Term                 | Meaning                                                                                                            |
| -------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `apply_patch`        | Codex's preferred patch-editing tool format, available as a Responses tool or custom grammar-backed freeform tool. |
| `update_plan`        | A TODO/plan tool with hygiene rules: multi-step only, one in-progress item, reconcile all items before final.      |
| Compaction           | Responses API flow that replaces a long context with a compacted item that preserves key prior state.              |
| Preamble             | A short assistant update sent with tool calls to orient the user about progress and intent.                        |
| `phase`              | GPT-5.3 Codex assistant-item metadata: `null`, `commentary`, or `final_answer`; it must be persisted and replayed. |
| In-distribution tool | A custom tool whose name, arguments, and output resemble patterns the model already handles well.                  |

## Questions for Review

- What four prompt areas does the guide call most critical when migrating a harness?
- Why can upfront plans or status-update instructions hurt older Codex rollouts?
- How should a harness prompt steer parallel exploration?
- What makes a custom tool more likely to work well with Codex?
- What metadata must be preserved for GPT-5.3 Codex assistant items?

## Connections

- Related ideas: prompt assembly, `AGENTS.md` precedence, tool boundary, transcript/session replay, compaction.
- Related sources: `docs/internal/notes/prompts.md`, `docs/internal/notes/codex-prompts.md`, `docs/internal/notes/agents-md.md`, `docs/internal/notes/sessions.md`.
- Contradictions or tensions: thndrs exposes typed tools rather than Codex's exact terminal/apply_patch surface; borrowing the behavioral prompt lessons is cheaper than copying the whole Codex tool stack.
- Useful applications: tighten thndrs prompt fragments around autonomy, efficient exploration, batching, preserving user changes, and concise non-loggy progress updates.

## Open Questions

- Would a canonical diff/freeform `apply_patch` tool outperform the current structured `write_patch` for Codex-like models?
- Should thndrs add first-class compaction or keep relying on transcript tail projection until context limits become a concrete problem?
- Do prompt changes measurably reduce slow starts or overexploration in thndrs sessions?

## Notable Quotes

> "Default expectation: deliver working code, not just a plan."

> "Think first. Before any tool call, decide ALL files/resources you will need."

## Takeaways

- Keep prompt instructions action-oriented, but bounded: inspect enough, act, verify, and stop when blocked.
- Tool instructions should prefer narrow typed tools, batch independent reads, and keep output shapes distinct and predictable.
- Do not add rich preamble/status behavior without matching provider support for preserving commentary/final-answer state.
