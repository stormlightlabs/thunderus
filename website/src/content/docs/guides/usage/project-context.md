---
title: "Project Context"
---

`thndrs` loads project context from `AGENTS.md` so the assistant can follow
workspace-specific conventions without turning repository files into runtime
configuration.

Project context is guidance. It can explain how the project is organized, which
commands are useful, what style to follow, and what caveats matter. It cannot
grant permissions, change the selected provider or model, enable tools, reveal
secrets, suppress errors, or override direct system, developer, or user
instructions.

## What Loads

On startup, `thndrs` discovers the workspace root from `--cwd`. When the
directory is inside a git repository, the git top level is used. Otherwise the
canonicalized `--cwd` path is used.

The current implementation loads the root `AGENTS.md` from that workspace root
when it exists:

```text
<workspace-root>/AGENTS.md
```

Nested `AGENTS.md` scoping is not active yet. Until it is implemented, put
workspace-wide guidance in the root file and keep path-specific details short
enough that they do not become stale.

## Precedence

Instructions are interpreted in this order:

1. Harness safety policy.
2. Current user prompt.
3. CLI and config choices owned by the user.
4. Applicable `AGENTS.md` guidance.
5. Built-in defaults.

That means `AGENTS.md` can influence how work is done, but it is below the
current request and below the harness boundary. For example, a project file can
recommend `cargo test`, but it cannot require ignoring failed tests or authorize
destructive commands.

## Prompt Behavior

Loaded project context is included in the structured prompt below the stable
harness policy and environment metadata. Each source is rendered with:

- absolute path;
- scope label;
- content hash;
- truncation flag;
- text content when the provider request needs it.

`thndrs` caps `AGENTS.md` content before adding it to the prompt. If the file is
too large, the prompt receives the capped text and the truncation flag is set so
the model can avoid assuming it saw the whole file.

When history reuse is available in a provider, unchanged `AGENTS.md` content may
be represented by metadata instead of resending the full text. When history
reuse is unavailable, `thndrs` includes the active capped text on each turn.

## Session Metadata

Session logs record project-context metadata, not full `AGENTS.md` contents.
The context record includes path, scope, content hash, truncation state, and byte
count. This makes prompt inputs auditable without copying project instructions
into every metadata record.

If a session is resumed, the context metadata tells `thndrs` and the user which
instruction files shaped the run. It does not make historical instructions a
substitute for the files currently on disk.

## Startup Display

The startup screen shows a compact inventory of the run before the first
transcript entry. It is deliberately similar to the kind of boot summary shown
by other local agent tools:

```text
[Runtime] umans | umans-coder | search auto | direct-inline
[Context] /repo/AGENTS.md
[Search] auto: native search when prompt appears current-web dependent; DuckDuckGo HTML fallback via web_search tool; read_url fetches public HTTP(S) and extracts HTML with Lectito
[Skills] inspect-project, release-notes
[Diagnostics] (none)
```

For project context, the important line is `[Context]`: it lists the loaded
instruction source paths. It does not print full `AGENTS.md` text. Skills and
diagnostics are shown separately because they are discovered references, not
project-context instructions.

This display is for inspection. The model-visible prompt remains the source of
truth for what the provider received.

## Self-Knowledge Snapshot

Each prompt also receives a generated `<thndrs_self_knowledge>` block. This is
the model-visible companion to the startup display. It exists so the assistant
can answer questions about the current `thndrs` run without guessing from stale
docs or hard-coded assumptions.

The snapshot is structured around real runtime boundaries:

- identity: app name, version, and broad capabilities
- runtime: workspace, renderer mode, provider name, model, selected search
  mode, provider search header, local DuckDuckGo fallback, Lectito-backed
  `read_url`, and exposed tool names
- references: local documentation entry points plus discovered skill names and
  paths
- prompt context: prompt fragment names and loaded `AGENTS.md` source metadata;
- diagnostics: visible skill/context discovery issues.

The snapshot contains names, paths, hashes, counts, and mode labels. It does
not include full prompt fragments, full `AGENTS.md` contents, user prompt text,
secrets, provider request bodies, or provider-private state.

This means the startup screen and model-visible snapshot overlap but are not the
same surface. The startup screen is for the user to inspect quickly. The
snapshot is compact context for the model.

## Good `AGENTS.md` Content

Good project instructions are short, concrete, and maintained with the code:

- project purpose and important directories;
- build, test, lint, and format commands;
- style conventions that are not obvious from tooling;
- verification expectations for common changes;
- safety notes such as generated files, migrations, or external services;
- known slow or flaky checks and the preferred narrower alternatives.

Avoid long architecture essays, stale command lists, broad personality prompts,
secret material, or instructions that conflict with the user or harness. If a
detail only matters for one task, keep it in a task document and ask the
assistant to read it explicitly.

## Failure Modes

If `AGENTS.md` is missing, `thndrs` runs with built-in defaults and records no
project-context source.

If `AGENTS.md` is unreadable, future diagnostics should make that visible to the
user. The current root loader skips missing files and only loads successfully
read text.

If `AGENTS.md` is oversized, `thndrs` uses the capped content and marks the
source as truncated.

## Related Docs

- [Prompt Assembly](../concepts/prompt-assembly.md)
- [Tool Boundary](../concepts/tool-boundary.md)
- [Session Format](../reference/session-format.md)
- [Security and Permissions](security-and-permissions.md)
