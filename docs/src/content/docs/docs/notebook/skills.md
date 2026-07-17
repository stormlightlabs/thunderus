---
title: "Agent Skills Specification"
Source: https://agentskills.io/specification.md
Author: Agent Skills project
Date: Unknown
Captured: 2026-07-01
Tags: [agent-skills, specification, prompts, tool-use, progressive-disclosure]
---

## Summary

An Agent Skill is a directory-level instruction package whose required `SKILL.md`
frontmatter makes it discoverable, while optional scripts, references, and assets
are loaded only when the active task needs them.

## Key Ideas

- **A skill is a directory, not just a prompt:** The minimum shape is a directory
  containing `SKILL.md`; scripts, reference docs, assets, and other files are
  optional supporting material.
- **`SKILL.md` has a strict frontmatter contract:** The file must start with YAML
  frontmatter and then Markdown instructions. `name` and `description` are
  required; `license`, `compatibility`, `metadata`, and `allowed-tools` are
  optional.
- **Names are constrained for stable discovery:** A skill name is 1-64
  characters, must match the parent directory, and is limited to lowercase
  letters, numbers, and single hyphens.
- **Descriptions drive activation:** The description should say both what the
  skill does and when to use it, with concrete keywords that help an agent select
  the skill.
- **Progressive disclosure is the core design:** Agents initially see only small
  metadata, then load the full `SKILL.md` after activation, and read scripts,
  references, or assets only as needed.
- **Keep the main instruction file small:** The spec recommends keeping
  `SKILL.md` under 500 lines and moving detailed material into focused referenced
  files.
- **References should stay shallow:** File references should be relative to the
  skill root and kept one level deep from `SKILL.md` to avoid long chains.
- **Validation exists:** The `skills-ref` reference library can validate
  frontmatter and naming conventions with `skills-ref validate ./my-skill`.

## Claims & Evidence

| Claim                                                  | Support                                                                                              | Caveat / Confidence |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- | ------------------- |
| Skills are meant to be self-contained packages.        | The directory structure includes `SKILL.md` plus optional `scripts/`, `references/`, and `assets/`.  | High.               |
| Metadata is intentionally compact.                     | Startup discovery uses only the `name` and `description`, estimated around 100 tokens.               | High.               |
| The description is part of routing, not decoration.    | The spec says it should describe what the skill does, when to use it, and include keywords.          | High.               |
| Large skills should be split into on-demand resources. | Progressive disclosure separates instructions from resources and recommends focused reference files. | High.               |
| Tool pre-approval is not portable yet.                 | `allowed-tools` is marked experimental and support may vary.                                         | High.               |

## Important Terms

| Term                   | Meaning                                                                                 |
| ---------------------- | --------------------------------------------------------------------------------------- |
| Skill                  | A directory containing a required `SKILL.md` and optional supporting files.             |
| `SKILL.md`             | The required skill entry point with YAML frontmatter and Markdown instructions.         |
| Frontmatter            | YAML metadata used for discovery, validation, and optional compatibility/tool metadata. |
| Progressive disclosure | Loading only the amount of skill context needed for the current task.                   |
| Reference file         | Focused supplemental documentation under `references/`, read on demand.                 |
| Asset                  | Static reusable material such as templates, images, data files, or schemas.             |

## Questions for Review

- What two fields are required in every `SKILL.md` frontmatter block?
  - Require `name` and `description` before a skill can be discovered or loaded.
- Why should a skill description include "when to use it" language?
  - Put activation criteria in the description so routing can happen without loading the whole skill.
- What is the difference between `SKILL.md`, `references/`, and `assets/`?
  - Keep `SKILL.md` as routing and core instructions, `references/` as on-demand detail, and `assets/` as reusable static material.
- When should detailed instructions move out of `SKILL.md`?
  - Move detail out when it is conditional, lengthy, or only relevant to specific variants of the workflow.
- Why does shallow file referencing matter for agent context management?
  - Keep references shallow so agents can load only task-relevant context without recursive instruction chasing.

## Connections

- Related ideas: prompt fragments, tool descriptions, slash-command prompts,
  project-local instruction files.
- Related sources: [prompts](/docs/notebook/prompts/), [codex-prompts](/docs/notebook/codex-prompts/),
  [goose-prompts](/docs/notebook/goose-prompts/), [pi-prompts](/docs/notebook/pi-prompts/).
- Tension: Skills can package rich workflows, but skill infrastructure should
  wait until repeated workflows justify the extra loader, validation, and
  precedence rules.
- Conceptual use: the prompt system can borrow the spec's progressive
  disclosure shape now: keep base instructions small, put specialized edit or
  workflow rules near the tool/command that needs them, and avoid always loading
  every reference.

## Open Questions

- Should a harness support repo-local skill discovery, or are `AGENTS.md` plus
  built-in prompt fragments enough?
  - **Recommendation**: Borrow progressive disclosure and strict metadata concepts now, but defer repo-local skill discovery until repeated workflows justify loader, validation, and precedence rules.
- If skills are added, should `allowed-tools` be ignored until the harness has a
  real permission system?
  - **Recommendation**: Ignore `allowed-tools` as enforcement until a real permission model exists, but preserve it as advisory metadata if useful.
- What validation would be needed to prevent malformed skill metadata from
  becoming model-visible prompt noise?
  - **Recommendation**: Validate required metadata, relative references, file size caps, and unsupported fields before any skill content becomes model-visible.

## Takeaways

- Treat reusable agent behavior as a small, discoverable package with strict
  metadata and on-demand detail.
- Put activation keywords and task boundaries in the description.
- Keep main instructions short; move detailed references, scripts, and assets
  behind explicit file references.
