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
`--resource-max-bytes`, are validated and then discarded at
`crates/thndrs/src/headless.rs:421`.

`thndrs skills doctor` reports all twelve skills in this repository as
duplicates. `.agents/skills` is a symlink to `.claude/skills`, which `CLAUDE.md`
requires so that harnesses following the AGENTS.md convention find them.
Discovery scans both roots without canonicalizing, so the convention the
repository documents produces a wall of false positives in the command meant to
validate it.

`thndrs doctor` never mentions ChatGPT Codex. `collect_credential_statuses` at
`crates/thndrs/src/cli/commands/doctor.rs:140` is a hardcoded array of the two
API-key routes, so the only subscription-priced provider, the one Quick Start
lists first, is invisible to the command that answers "am I set up?".

Separately, this repository's own workflow cannot run under `thndrs`. The
workflow is `.claude/commands/`, and the slash-command list is a static array of
builtins at `crates/thndrs/src/cli/app/commands.rs:14`.

## Decisions

### ACP is already built in both directions, and neither one reaches the CLIs

`thndrs acp serve` exposes the harness as an ACP agent, and `core/acp/` is a
complete client with a registry installer. Nothing about the tool-interop goal
calls for more ACP work.

Claude Code, Codex, pi, and opencode are not ACP clients. Each is reachable as
an ACP agent through an adapter, which is the direction `thndrs` already
consumes through `model = "acp:<name>"`. Editors are the clients. So
`acp serve` buys Zed, not those four.

### The subprocess surface is the interop surface

The protocol those tools speak as clients is MCP, and `rmcp` is pinned to
`features = ["client", ...]`. Adding an MCP server is not the cheapest route to
the goal, because an MCP tool wrapping a whole agent run has no streaming story
beyond progress notifications. A subprocess call to `thndrs run --jsonl` gives
the caller a readable event stream and needs no new protocol surface.

`thndrs review --range BASE..HEAD --jsonl` is already the right shape: one
required flag, structured output. `run --jsonl` should match it.

An MCP server becomes worth building when the target is other people's agents
reaching `thndrs`, rather than this maintainer delegating out of Claude Code.
That question is open.

### Mire and thndrs already share a directory, and thndrs rejects what it finds

`mire skill path` installs its bundled skill to `~/.agents/skills/mire/SKILL.md`
(`crates/cli/src/skill.rs:36`). `default_skill_dirs` at
`crates/thndrs/src/core/skills.rs:198` already scans `~/.agents/skills`. The two
tools were built to the same convention and meet without configuration.

`thndrs` then drops the skill. `validate_name` requires the frontmatter `name`
to equal the parent directory, and Mire ships `name: mire-review` in `mire/`.
The failure is fatal rather than advisory: the discovery test asserts the
inventory is empty on a mismatch. The same rule rejects at least one
Anthropic-shipped skill, so it is stricter than the ecosystem it is trying to be
compatible with.

Relaxing that rule is the whole integration. Mire owns the review artifact,
anchoring, refresh, and human disposition; `thndrs` owns the reviewer. The
handoff is Mire's existing `mire context` and `mire notes apply` contract driven
through `run_shell`, with no Mire-specific code in `thndrs`.

### Review already loads skills; authority is what separates it from Mire

`thndrs review` runs through the ordinary agent loop.
`crates/thndrs/src/core/review.rs:133` sets `ToolAuthority::ReadOnly` and calls
`headless::run_prompt_capture`, which builds an `App` through `App::from_cli`
(`crates/thndrs/src/cli/app.rs:1408`) and assembles the turn through the shared
path in `crates/thndrs/src/runtime/interactive.rs:399`. Skills are discovered,
their metadata reaches the prompt as `available_skills`, and their roots are
granted as extra read roots.

Activation needs no special tool. A skill becomes active when the model reads
its `SKILL.md` with `read_file_range`, which `record_skill_read`
(`crates/thndrs/src/cli/app/agent_lifecycle.rs:668`) notices after the fact.
`read_file_range` is in the read-only set, so a review can already load and
follow a house-style review skill.

What a review cannot do is write. `is_read_only_tool`
(`crates/thndrs/src/core/tools.rs:320`) excludes `run_shell`, and Mire's
contract is two shell commands: `mire context` to read the revision and
`mire notes apply` to land findings. So Mire's skill is discoverable under
`thndrs review` and unusable there.

That is the correct boundary, not a defect. `thndrs review` is a pure function:
a diff in, one validated structured artifact out, no side effects. That property
is exactly what makes it safe for another agent to call. Mire's loop belongs
under `thndrs run`, where shell and full authority already exist.

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

- Is the interop target this maintainer delegating out of Claude Code, or other
  agents reaching `thndrs`? The first needs only the `run --jsonl` repairs. The
  second needs an MCP server and a tagged release. Nothing else in this file
  depends on the answer.
- Whether Mire's writeback should ever move inside `thndrs review`. Review
  already loads skills, so the open question is not skills but authority: see
  the decision above.
- Whether the discarded `--evidence-max-bytes` and `--resource-max-bytes` should
  be wired to the artifact and stdin caps or deleted. Settled by whether a
  machine caller has a reason to set them per run.

## Sources

- `crates/thndrs/src/headless.rs:380-428`, the JSONL flag gauntlet and the
  discarded limits.
- `crates/thndrs/src/core/skills.rs:194-211`, `548-558`, discovery roots and the
  fatal name rule.
- `crates/thndrs/src/cli/commands/doctor.rs:140-153`, the hardcoded credential
  list.
- `crates/thndrs/src/core/review.rs:74-104`, the finding schema.
- `stormlightlabs/mire`, `crates/cli/src/skill.rs:36` and
  `crates/cli/skills/mire/SKILL.md`, the install path and the note contract.
- `docs/src/content/docs/docs/usage/acp.md`, which configures `codex-acp` as an
  agent `thndrs` drives, confirming the direction those tools occupy.

## Filed

- #56 skill name rule drops mismatched skills
- #57 symlinked `.agents/skills` duplicates every skill
- #58 JSONL flag gauntlet and the misdirecting `--model` diagnostic
- #59 the two discarded byte limits
- #60 `doctor` omits ChatGPT Codex
- #61 review findings have no diff side
- #62 `run --resume`
- #63 `.claude/commands` support
