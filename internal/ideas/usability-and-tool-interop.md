---
name: usability-and-tool-interop
last_updated: 2026-09-18
id: 01M2TDS2FCYDM9XBGF3YCZ0CMD
---

# Usability, dogfooding, and talking to other tools

## Problem

The non-interactive surface has never been used against a real repository, and
it shows. Running the 0.1.1 binary against this workspace found three defects
in the first ten minutes, none of them on the board.

`thndrs run --jsonl` needs six flags. `--model` is one of them, but it lives on
`Cli` rather than the subcommand and is not `global`, so the diagnostic
`JSONL runs require an exact --model route` is followed by
`error: unexpected argument '--model' found` when a caller does what it says.
The remaining five surface one at a time, so a caller finds them through five
failed invocations. Two of them, `--evidence-max-bytes` and
`--resource-max-bytes`, are range-checked in `validate_jsonl_request` and then
bound to `_limits` and dropped.

`thndrs skills doctor` reports all twelve skills in this repository as
duplicates. `.agents/skills` is a symlink to `.claude/skills`, which `CLAUDE.md`
requires so that harnesses following the AGENTS.md convention find them.
`default_skill_dirs` scans both roots without canonicalizing, so the convention
the repository documents produces a wall of false positives in the command meant
to validate it.

`thndrs doctor` never mentions ChatGPT Codex. `collect_credential_statuses` is a
hardcoded array of the two API-key routes, so the only subscription-priced
provider, the one Quick Start lists first, is invisible to the command that
answers "am I set up?".

Separately, this repository's own workflow cannot run under `thndrs`. The
workflow is `.claude/commands/`, and `COMMANDS` in `cli/app/commands.rs` is a
static array of builtins.

## Decisions

### ACP is already built in both directions, and neither one reaches the CLIs

`thndrs acp serve` exposes the harness as an ACP agent, and `core/acp/` is a
complete client with a registry installer. Nothing about the tool-interop goal
calls for more ACP work.

The clients are editors. Claude Code and Codex are each reachable as an ACP
*agent* through an adapter, which is the direction `thndrs` already consumes
through `model = "acp:<name>"`; `usage/acp.md` configures `codex-acp` that way.
Pi and opencode are assumed to sit on the same side of the protocol, but that
was not verified and nothing here should rest on it. So `acp serve` buys Zed,
not those four.

### The interop target is both directions, in that order

The maintainer delegating out of Claude Code comes first, because it needs only
the `run --jsonl` repairs. Other people's agents reaching `thndrs` comes second,
because it needs an MCP server and a tagged release, and a server surface is
worth nothing while nobody can install the binary.

### The subprocess surface is the near-term interop surface

The protocol those tools speak as clients is MCP, and `rmcp` is pinned to
`features = ["client", ...]`. An MCP tool wrapping a whole agent run has no
streaming story beyond progress notifications, so a subprocess call to
`thndrs run --jsonl` gives the caller a readable event stream sooner and needs
no new protocol surface.

`thndrs review --range BASE..HEAD --jsonl` is already the right shape: one
required flag, structured output. `run --jsonl` should match it.

`--evidence-max-bytes` and `--resource-max-bytes` keep their names and get wired
to the artifact retention cap and the request byte cap they already describe.
Deleting them would be the smaller change, but the caps are real and a machine
caller delegating an unbounded job is the case they exist for.

### Mire is an optional tool, not a plugin

`internal/features/skills/plan.md` already rejects a generic plugin layer and
names what to reach for instead: a skill, a slash command, an existing CLI, or
an MCP server. Mire is the third of those, reached through the first. Nothing in
`thndrs` needs a plugin concept, an extension registry, or a Mire dependency.

Optionality falls out of discovery. `mire skill path` writes its bundled skill
into `~/.agents/skills/mire/`, which `thndrs` already scans, so the skill exists
for people who installed Mire and does not for people who did not. What is
missing is a way for that skill to declare the binary it needs, so it stays out
of the catalog when Mire is absent rather than being offered and failing. The
skills plan already anticipates metadata declaring local requirements; that is
where the mechanism belongs, and it needs a spec before an issue.

### Mire and thndrs already share a directory, and thndrs rejects what it finds

`installed_path` in Mire writes to `~/.agents/skills/mire/SKILL.md`, and
`default_skill_dirs` already scans `~/.agents/skills`. The two tools were built
to the same convention and meet without configuration.

`thndrs` then drops the skill. `validate_name` requires the frontmatter `name`
to equal the parent directory, and Mire ships `name: mire-review` in `mire/`.
The failure is fatal rather than advisory: the discovery test asserts the
inventory is empty on a mismatch. The same rule rejects at least one
Anthropic-shipped skill, so it is stricter than the ecosystem it is trying to be
compatible with.

Relaxing that rule is the whole integration. Mire owns the review artifact,
anchoring, refresh, and human disposition; `thndrs` owns the reviewer. The
handoff is Mire's existing `mire context` and `mire notes apply` contract driven
through `run_shell`.

### Review already loads skills; authority is what separates it from Mire

`thndrs review` runs through the ordinary agent loop. `run_command` sets
`ToolAuthority::ReadOnly` and calls `headless::run_prompt_capture`, which builds
an `App` through `App::from_cli` and assembles the turn through the shared
`runtime::interactive` path. Skills are discovered, their metadata reaches the
prompt as `available_skills`, and their roots are granted as extra read roots.

Activation needs no special tool. A skill becomes active when the model reads
its `SKILL.md` with `read_file_range`, which `record_skill_read` notices after
the fact. `read_file_range` is in the read-only set, so a review can already
load and follow a house-style review skill.

What a review cannot do is write. `is_read_only_tool` excludes `run_shell`, and
Mire's contract is two shell commands: `mire context` to read the revision and
`mire notes apply` to land findings. So Mire's skill is discoverable under
`thndrs review` and unusable there.

That is the correct boundary, not a defect. A review hands the model no write
tool, which is the property that makes it safe for another agent to call. The
command itself is not free of effects: it shells out to Git and, unless the
caller passes `--ephemeral`, records a session like any other run. Mire's loop
belongs under `thndrs run`, where shell and full authority already exist.

### The finding schemas are close enough to map, with one real gap

`ReviewFinding` carries `severity`, `title`, `evidence`, and
`location { path, start_line, end_line }`. A Mire note needs `file`, a line on a
named side, `end_line`, `severity`, `kind`, and `body`. Severity is a subset of
Mire's. `title` and `evidence` concatenate into `body`. `kind` defaults honestly
to `defect`, because the review prompt already asks only for defects.

The gap is the side. `FindingLocation` has no old/new discriminator, so a
finding on deleted code cannot be placed. That is a field, not a redesign.

### Priority holds for lndrs

Lndrs stays. The shift is that a run of small usability fixes lands first. They
are almost all `risk:low`, they remove the reasons not to reach for the tool,
and the interop goal depends on the same surface they repair.

## Open

- Whether Mire's writeback should ever move inside `thndrs review`, with
  `thndrs` performing the write under its own validation while the model stays
  read-only. Settled by whether the skill route produces fabricated note
  schemas or skipped revision re-reads; if it does not, the answer is no.
- What a skill's declaration of a required local binary looks like. Belongs to
  the skills feature track and needs a spec, not an issue.

## Sources

- `stormlightlabs/mire`, `crates/cli/skills/mire/SKILL.md`, for the note
  contract, the severity and kind vocabularies, and the revision-conflict rule
  this file maps `ReviewFinding` onto. The install path is `installed_path` in
  the same crate.
- The [ACP registry](https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json),
  which `core/acp/registry.rs` reads, for which agents exist and on which side
  of the protocol they sit.

Work filed from this idea is tracked under #64.
