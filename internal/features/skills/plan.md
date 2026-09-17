# Skills

Skills remain bounded instructions and reference material. Metadata may declare
compatibility and local requirements, but it cannot grant authority, enable
tools, install dependencies, or weaken run policy. Loading stays progressive
and bounded.

Orchestration remains skill-based. Repository-owned skills live in
`.claude/skills`, which `.agents/skills` symlinks to so every harness following
the AGENTS.md convention finds them.

`thunderstorm` owns delegation policy, worker count, task assignment, progress
inspection, and stop conditions. `github-board` owns issue state. `worktree`
owns isolation. Each worker owns only its local run, session, authority,
workspace, and result. See `internal/thunderstorm.md`.

Generic skills such as `herdr` and `orchestrate` still come from the user's
agent environment rather than this repository.

Distribution waits for an explicit trust and supply-chain design. Prefer a
skill, slash command, existing CLI, or MCP server over a generic plugin layer.
