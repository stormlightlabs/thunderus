# Skills

Skills remain bounded instructions and reference material. Metadata may declare
compatibility and local requirements, but it cannot grant authority, enable
tools, install dependencies, or weaken run policy. Loading stays progressive
and bounded.

The existing orchestration integrations remain skill-based:

- `herdr` controls panes, tabs, workspaces, commands, and agent processes in
  the Herdr terminal multiplexer.
- `orchestrate` teaches Codex to coordinate bounded Pi work in Herdr.
- `hybrid-orchestration` teaches Codex or Pi to coordinate one or two
  Herdr-managed `thndrs` workers and capture evidenced harness problems.

These skills own delegation policy, worker count, task assignment, progress
inspection, and final verification. Herdr owns terminal placement and process
visibility. Each `thndrs` worker owns only its local run, session, authority,
workspace, and result. The repository-owned hybrid skill lives at
`.agents/skills/hybrid-orchestration/SKILL.md`; the generic skills come from the
user's agent environment.

Distribution waits for an explicit trust and supply-chain design. Prefer a
skill, slash command, existing CLI, or MCP server over a generic plugin layer.
