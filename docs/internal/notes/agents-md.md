---
Title: AGENTS.md for thndrs
Author: AGENTS.md project, Aider, OpenCode, Gemini CLI docs, recent AGENTS.md research
Date: 2026-06-28
Captured: 2026-06-28
Tags: [agents-md, context, coding-agent, instructions, harness, safety]
Sources: >
  - https://agents.md/
  - https://aider.chat/docs/config/aider_conf.html
  - https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/configuration.md
  - https://opencode.ai/docs/agents/
  - https://arxiv.org/abs/2601.20404
  - https://arxiv.org/abs/2602.11988
  - https://arxiv.org/abs/2606.15828
---

## Summary

`AGENTS.md` should be treated as scoped, repo-owned operating instructions for
the harness: useful enough to load automatically, but never powerful enough to
override user instructions, tool permissions, or safety limits.

## Key Ideas

- **README for agents:** The AGENTS.md site frames the file as a predictable
  place for build commands, tests, conventions, and project-specific warnings
  that would clutter a human README.
- **Plain Markdown, no schema:** The format intentionally has no required fields.
  Harnesses should not depend on frontmatter, headings, or machine-readable keys
  to make the file useful.
- **Nested scope matters:** The site recommends nested `AGENTS.md` files for
  monorepos and says the closest file to the edited file takes precedence.
- **Explicit prompts still win:** The AGENTS.md FAQ says user chat instructions
  override file instructions. For `thndrs`, system/developer/harness policies
  also outrank repository text.
- **Harnesses vary:** Aider exposes a `read:` config mechanism that can include
  `AGENTS.md`; Gemini CLI can be configured to use `AGENTS.md` as its context
  filename; OpenCode has richer agent config/permissions outside AGENTS.md. The
  common lesson is to separate repository guidance from tool permissions.
- **Shorter is safer:** Recent research is mixed: one study reports AGENTS.md can
  reduce runtime/output tokens, while another finds context files can hurt task
  success and increase cost when they add unnecessary requirements. A smell
  study reports common problems like context bloat, lint leakage, skill leakage,
  and conflicting instructions.

## Claims & Evidence

| Claim                                                 | Support                                                                                                                                                                                                                  | Caveat / Confidence                                                |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| AGENTS.md is intentionally simple and interoperable.  | agents.md describes it as an open format and standard Markdown with no required fields.                                                                                                                                  | High.                                                              |
| Nested files should be supported.                     | agents.md recommends per-package files and says the nearest file in the tree takes precedence.                                                                                                                           | High. Need define behavior when a task spans multiple subtrees.    |
| AGENTS.md should not be treated as a permission file. | agents.md describes project guidance, while OpenCode puts permissions in separate config keys with allow/ask/deny.                                                                                                       | High. Keep permissions in harness/config, not repo prose.          |
| Harnesses should expose loaded context.               | Pi notes stress visible context; agents.md positions the file as explicit instructions.                                                                                                                                  | High for `thndrs` UX.                                              |
| AGENTS.md quality matters.                            | Research on configuration smells found frequent context bloat, lint leakage, skill leakage, and conflicting instructions; another paper found context files can hurt performance when they add unnecessary requirements. | Medium-high. Findings are recent and may vary by agent/model/task. |
| Good AGENTS.md files can improve efficiency.          | One study found AGENTS.md associated with lower median runtime and reduced output token consumption while maintaining comparable task completion behavior.                                                               | Medium. Small sample and not a guarantee for every repo.           |

## Important Terms

| Term             | Meaning                                                                                                                    |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Root AGENTS.md   | Repository-level instruction file, usually at workspace root.                                                              |
| Nested AGENTS.md | Instruction file inside a package/subdirectory with narrower scope.                                                        |
| Scope            | The subtree where a nested AGENTS.md applies.                                                                              |
| Precedence       | Rule for resolving conflicts: system/developer policies, then user prompt, then closest AGENTS.md, then broader AGENTS.md. |
| Context bloat    | Overly long or irrelevant instructions that consume tokens and distract the model.                                         |
| Lint leakage     | Instructions listing too many commands/checks, causing agents to run irrelevant checks.                                    |
| Skill leakage    | Tool- or agent-specific instructions that confuse other agents.                                                            |

## How Harnesses Implement It

Common patterns:

- Load a project instruction file automatically at session start.
- Let users configure extra read-only instruction files.
- In monorepos, use nearest-file scoping rather than one global instruction blob.
- Treat the content as instruction context, not executable configuration.
- Keep permission and tool policy in separate harness config.
- Show or log which context files were loaded so behavior is explainable.

Observed examples:

- agents.md: root plus nested `AGENTS.md`; closest file wins; user prompt
  overrides.
- Aider: `.aider.conf.yml` supports `read:` files, so users can include
  `AGENTS.md` or other convention files as read-only context.
- Gemini CLI: agents.md FAQ says Gemini can be configured to use `AGENTS.md` as
  the context file name.
- OpenCode: agent prompts and permissions live in config/Markdown agent files;
  permission policy is explicit (`allow`, `ask`, `deny`) and separate from prose
  instructions.

## Open Questions

- Should alpha load only root AGENTS.md, or should nested support ship with the
  first real provider?
- What is the default size cap for AGENTS.md content?
- Should `thndrs` warn when a nested AGENTS.md exists but no target file is known
  yet?
- Should AGENTS.md be reloaded automatically when it changes during a session?

## Connections

- Related ideas: visible context from Pi, scoped project layout from monorepos,
  read-only traversal tools from [fs-traversal](./fs-traversal.md), release
  gates from [release](./release.md).
- Related sources: [pi](./pi.md), [herdr](./herdr.md),
  [ui-patterns](./ui-patterns.md), [providers/umans](./providers/umans.md).
- Contradictions or tensions: AGENTS.md can reduce repeated explanation, but it
  can also add irrelevant constraints and token cost.
- Useful applications: root context loading in alpha; nested scoped loading and
  session audit trail by v1.

## Open Questions

- Whether to lint AGENTS.md for common smells or only document guidance.
- Whether to support compatibility symlinks such as `AGENT.md` -> `AGENTS.md`.
- Whether to offer a `thndrs init-agents` scaffold command after v1.
- How to display conflicting nested instructions without overwhelming the TUI.

## Notable Quotes

> "Think of AGENTS.md as a README for agents"
