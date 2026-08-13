---
title: "Skills"
---

`thndrs` can discover Agent Skills and expose their metadata to the model.

[Skills](https://agentskills.io/home) are reusable instruction packages for
specific workflows. They are not loaded wholesale on startup.

## Discovery

A skill is a directory with a required `SKILL.md` file. `thndrs` discovers skills
from configured skill directories and project-local skill locations when they are
available.

Built-in discovery checks these project directories:

- `.thndrs/skills`
- `.agents/skills`
- `.claude/skills`
- `.codex/skills`
- `.pi/skills`
- `.pi/agent/skills`

It also checks the same names under the user's home directory. `.agents/skills`
matches the Agent Skills convention; `.claude`, `.codex`, and `.pi` are
compatibility locations for existing local skill packages. When more than one
skill has the same name, the first matching skill in discovery-root order is
used. Inspect the selected and ignored paths with `thndrs skills doctor`.

At discovery time, only this set of compact metadata is used:

- name
- description
- source label
- path
- optional license, compatibility, metadata, allowed-tools, and references
  fields
- discovery diagnostics for malformed skill files

The full `SKILL.md` body and any referenced files are not part of the startup inventory.
They should be loaded only when the skill is relevant to the current task.

## Prompt Exposure

Discovered skills appear in two places:

- the startup screen, as a compact `[Skills]` list for the user;
- the model-visible self-knowledge snapshot, as names, sources, and paths.

The regular prompt also includes available skill metadata so the assistant can
decide when a skill might apply. This follows progressive disclosure: route from
small metadata first, then read the skill instructions when the task needs them.

## Activate or Read a Skill

A skill can become visible in the transcript in two ways:

- **User activation:** Run `/skills`, browse or filter the loaded skills, select
  one, and press Enter. thndrs loads its instructions and declared references
  into the current model context.
- **Agent read during a run:** After the agent successfully reads the `SKILL.md`
  of a discovered skill with `read_file_range`, thndrs adds the same compact
  notice/transcript message.

The transcript will show you the skill name, the full skill package's estimated
token cost, its estimated share of the currently available context, and the source
path. This includes declared references, so an agent that reads only part of a skill
may use less context than the notice estimates. The estimates are guidance,
not a reservation, and can change as the context working set changes.

Activation records are stored in session metadata. When you restore a session, thndrs
shows the record but does not silently reload instruction text that is no longer available.

Activate or read the skill again if its instructions are needed in the restored session.

## Skill Shape

`SKILL.md` should start with YAML frontmatter. `name` and `description` are the
important routing fields. The name should be stable and match the skill
directory. The description should say what the skill does and when to use it.

Optional frontmatter fields are preserved as metadata when present:

- `license`
- `compatibility`
- `metadata`
- `allowed-tools`
- `references`

Good skills keep the main instruction file short. Conditional detail belongs in
focused reference files under the skill directory, and reusable material belongs
in assets or scripts. Avoid recursive reference chains.

Common optional directories are `scripts/`, `references/`, and `assets/`.
Scripts are not executed automatically.

## Loading References

Skill instructions are loaded progressively. Startup discovery reads only
metadata. The full `SKILL.md` body is loaded only when the skill is activated
for the current task.

Reference files are loaded on demand from paths relative to the skill root. A
skill can declare default activation references in frontmatter, for example
`references: [workers, pages, d1]`. Reference paths must be non-empty relative
paths that stay inside the skill directory.
Reference traversal is bounded by depth, total bytes, per-file bytes, file
count, and cycle detection. When traversal stops early, `thndrs` surfaces a
diagnostic instead of silently expanding more context.

Activated skill names, content hashes, byte counts, and loaded reference paths
are recorded in session metadata.

## Boundaries

Skills are guidance, not permissions. A skill can recommend tools or workflow
steps, but it cannot grant filesystem access, enable network access, override
user instructions, suppress errors, or bypass the harness safety model.

`allowed-tools` is preserved as metadata when present. It is not a substitute for
the runtime permission boundary.

## Diagnostics

Malformed skills are skipped and surfaced as diagnostics. Diagnostics are shown
compactly so users can fix local skill packages without turning broken metadata
into prompt noise. Duplicate names are expected when compatibility roots overlap;
they are resolved silently and listed by `thndrs skills doctor`.

## Further Reading

- [Project Context](/docs/usage/project-context/)
- [Prompt Assembly](/docs/concepts/prompt-assembly/)
- [Tools](/docs/usage/tools/)
- [Security and Permissions](/docs/usage/security-and-permissions/)
