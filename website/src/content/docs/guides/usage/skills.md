---
title: "Skills"
---

`thndrs` can discover Agent Skills and expose their metadata to the model.
Skills are reusable instruction packages for specific workflows. They are not
loaded wholesale on startup.

## Discovery

A skill is a directory with a required `SKILL.md` file. `thndrs` discovers
skills from configured skill directories and project-local skill locations when
they are available.

At discovery time, only compact metadata is used:

- name;
- description;
- source label;
- path;
- optional license, compatibility, metadata, and allowed-tools fields;
- discovery diagnostics for malformed skill files.

The full `SKILL.md` body and any referenced files are not part of the startup
inventory. They should be loaded only when the skill is relevant to the current
task.

## Prompt Exposure

Discovered skills appear in two places:

- the startup screen, as a compact `[Skills]` list for the user;
- the model-visible self-knowledge snapshot, as names, sources, and paths.

The regular prompt also includes available skill metadata so the assistant can
decide when a skill might apply. This follows progressive disclosure: route from
small metadata first, then read the skill instructions when the task needs them.

## Skill Shape

`SKILL.md` should start with YAML frontmatter. `name` and `description` are the
important routing fields. The name should be stable and match the skill
directory. The description should say what the skill does and when to use it.

Good skills keep the main instruction file short. Conditional detail belongs in
focused reference files under the skill directory, and reusable material belongs
in assets or scripts. Avoid recursive reference chains.

## Boundaries

Skills are guidance, not permissions. A skill can recommend tools or workflow
steps, but it cannot grant filesystem access, enable network access, override
user instructions, suppress errors, or bypass the harness safety model.

`allowed-tools` is preserved as metadata when present. It is not a substitute for
the runtime permission boundary.

## Diagnostics

Malformed skills are skipped and surfaced as diagnostics. Diagnostics are shown
compactly so users can fix local skill packages without turning broken metadata
into prompt noise.

## Related Docs

- [Project Context](project-context.md)
- [Prompt Assembly](../concepts/prompt-assembly.md)
- [Tool Boundary](../concepts/tool-boundary.md)
