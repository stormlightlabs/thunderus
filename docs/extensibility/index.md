---
outline: deep
---

# Extensibility

Thunderus is built to be extended without compromising safety. Today, the main
extension mechanism is the Skills system. Plugin hosting and additional runtime
hooks are not implemented yet.

## Skills

Skills are self-contained capability bundles defined in `SKILL.md` files. The
agent can load them on demand based on task intent or explicit invocation.

Key properties:

- Skills live under `.thunderus/skills/`.
- Each skill declares its name and description in frontmatter.
- Skills can be toggled via profile configuration.

## Plugins & Extension Points

Not implemented yet:

- Tool and command extension traits.
- Runtime plugin host support (WASM and Lua).
- A built-in extensions registry for optional tools.
- An MCP client for loading external tool schemas.
