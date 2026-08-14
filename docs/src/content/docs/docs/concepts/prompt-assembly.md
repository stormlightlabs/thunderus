---
title: "Prompt Assembly"
---

## Prompt Bundle

`thndrs` builds a structured `PromptBundle` before converting it to provider
messages. This keeps prompt construction inspectable and avoids ad hoc string
concatenation.

The rendered prompt uses XML-shaped sections for scanability. See
[Prompt XML Syntax](/docs/concepts/prompt-xml-syntax/) for the tag and CDATA conventions.

## Base Identity

The base identity describes `thndrs` as a concise terminal coding assistant
built in Rust.

## Harness Policy

The harness policy describes the bounded tool boundary, workspace containment,
and the absence of raw shell access.

## Environment Metadata

Environment metadata includes workspace root, rounded current date/timezone,
selected model, and search backend.

## Self-Knowledge Snapshot

The generated `thndrs_self_knowledge` block describes the current harness run in
a compact, model-visible form. It includes:

- app version and broad capabilities
- runtime state: workspace, renderer mode, provider, model, search backend, search
  backend details, URL-reading behavior, and tool names
- major capabilities
- references: local documentation entry points and available skill names and
  locations
- prompt context like prompt fragment names and loaded project-context metadata
- visible diagnostics

This block is metadata only. It does not include full prompt fragment text,
`AGENTS.md` contents, user prompt text, or provider-private state.

## Project Context

Project context includes loaded `AGENTS.md` metadata and text when included.
Metadata includes path, scope, content hash, and truncation state.

`AGENTS.md` is treated as repository guidance, not executable configuration. It
can describe conventions, commands to consider, and project caveats, but it
cannot grant permissions, reveal secrets, change provider/model settings, bypass
tests, disable safety checks, or override direct user/system/developer
instructions.

Instruction precedence is:

1. Harness safety policy.
2. Current user prompt.
3. CLI choices owned by the user.
4. Applicable `AGENTS.md` guidance.
5. Built-in defaults.

The implementation discovers root and nested `AGENTS.md` files. The root is
broad guidance; nested sources are selected when the current turn or a pin
references their scope. Non-applicable nested sources remain inspectable
candidates instead of entering every request. Truncation is visible in source
metadata.

Good `AGENTS.md` files are short and practical: project overview, relevant
build/test commands, style conventions, testing expectations, safety gotchas,
and monorepo navigation notes. Avoid long architecture essays, stale command
lists, tool-specific prompts, conflicting instructions, and instructions to skip
checks or ignore failures.

## Tool Catalog

The tool catalog contains provider-native schemas for local tools. Text
descriptions are intentionally minimal: name, purpose, safety limits, and
truncation behavior.

## Context ledger and transcript tail

Before lowering provider messages, `thndrs` builds a context ledger containing
all candidates, their visibility and lifecycle, token estimates, budget limits,
and selection reasons. Harness material and the current user turn are protected;
pins, applicable instructions, and activated skills are selected before the
ordinary transcript tail. Older settled transcript entries are evicted first
under pressure.

The model-visible transcript tail includes selected user, assistant, reasoning,
and tool entries. UI-only status rows, live-only stream deltas, status-line
metadata, and renderer artifacts are excluded. Approved compaction summaries
can stand in for a closed older prefix while the append-only session retains the
source records.

## User Turn

The user turn is the current submitted prompt text. Attachments are deferred.

## Print Prompt

`--print-prompt` prints the assembled prompt bundle and lowered messages with
secrets redacted, then exits without calling the provider.

## History Reuse

When history reuse is unavailable, `thndrs` includes active size-capped project
instructions and records hash and truncation metadata. Request-bound
By default, `context_snapshot` records and `/context export` provide content-free
accounting and a bounded redacted view of the selected projection. A run started
with `--capture-context-content` may also retain its sanitized, provider-neutral
projection under the recorded capture policy.
