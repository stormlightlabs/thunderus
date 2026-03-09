# Roadmap

## Part 4

### Milestone 7 - GUI Foundation

Cross-platform desktop application using [Iced.rs](https://iced.rs/) 0.14.

#### Core Integration

- Initialize `thndrs-desktop` crate with `iced` 0.14
- Implement `The Elm Architecture` (TEA) loop
- Connect GUI to `crates/core` for state and message management
- Asynchronous message streaming from `crates/providers`
- Tool execution orchestration via `crates/tools`

#### UI - Components

- Main chat view with scrollable message history
- Multi-line auto-expanding input field
- Theme system (Oxocarbon Dark, JetBrains Mono font)
- Window management (title bar, initial size/position)

### Milestone 8 - GUI Advanced Features

Polishing the desktop experience with interactive elements and deep workspace integration.

#### UI - Content

- Markdown rendering for messages (GFM, tables, task lists)
- Syntax highlighting for code blocks
- Interactive tool call widgets (thinking/results)
- Diff viewer for file edits

#### UI - Navigation

- Sidebar for session history and navigation
- Integrated file explorer tree
- Slash command autocomplete and file picker (@)
- Subtle animations for UI transitions

## Part 5

### Milestone 9 - Settings & Help

User-facing configuration and documentation inside the TUI.

#### UI - Settings

- Show version
- Split pane: sidebar nav (left) + settings content (right)
- Setting groups: General, Appearance, Editor, Keyboard, AI Model, Tools, Privacy
- Toggle switches, select dropdowns
- Save/reset actions
- Settings persist to `~/.thunderus/config.toml` using toml crate

*Reference: `designs/templates/settings.html`*

#### UI - Help

- Show version
- Tabbed nav: Keyboard Shortcuts, Commands, Tips, About, Tutorial
- Shortcut grid (two-column, key + description)
- Slash command list (`/help`, `/clear`, `/model`, `/tokens`)
- Tip box

*Reference: `designs/templates/help.html`*

#### UI - Tutorial / Home

- This is a tab in the Help Screen
- Quick start shortcuts (ctrl+n, ctrl+o, ctrl+r)
- Recent conversations list (numbered, with relative timestamps)
- Tip rotation
- Footer links

*Reference: `designs/templates/tutorial.html`*

## Part 6

### Milestone 10 - MCP Servers

External tool providers via the Model Context Protocol (JSON-RPC 2.0).

*Reference*: `.sandbox/extension_spec.md`

#### Core

- `McpClient` - manages one server connection (stdio subprocess or HTTP POST/SSE)
- JSON-RPC 2.0 request/response/notification types
- Stdio transport: spawn child process, write to stdin, read from stdout
- HTTP transport: POST with `application/json` or `text/event-stream` response handling
- `initialize` / `initialized` handshake with capability negotiation
- `tools/list` -> convert MCP `inputSchema` to `Tool` / `ToolSchema`
- `tools/call` -> route arguments, return `ToolResult`
- `McpManager` - owns all clients, merges tool lists into `get_tool_schemas()`
- Tool namespacing: `mcp__{server}__{tool}` (matches Claude Code convention)
- `execute_tool()` routes `mcp__*` names through the manager

#### Configuration

- User-level: `~/.thunderus/mcp.toml`
- Project-level: `.thunderus/mcp.toml` (overrides user-level by server name)
- `${VAR}` expansion in `command`, `args`, `env`, `url`, `headers`
- Fields: `transport` (stdio|http), `command`, `args`, `env`, `url`, `headers`, `timeout_sec`, `enabled`
- Unresolved env vars produce a warning; server is skipped

#### CLI

- `thunderus mcp list` - configured servers and connection status
- `thunderus mcp add <name> -- <command>` - add stdio server to user-level config
- `thunderus mcp remove <name>` - remove from user-level config
- `thunderus mcp test <name>` - handshake, list tools, disconnect
- `thunderus debug mcp <name>` - raw JSON-RPC traffic

#### TUI

- `/mcp` lists active servers and their tools
- MCP tools render with server name prefix in tool call output

### Milestone 11 - Skills

Markdown-based prompt extensions following the Agent Skills open standard. Compatible with Claude Code, Codex, Gemini CLI, and OpenCode skill directories.

*Reference*: `.sandbox/extension_spec.md`

#### Core

- `SkillMeta` - parsed YAML frontmatter (name, description, user_invocable, disable_model_invocation, allowed_tools, argument_hint)
- `Skill` - meta + raw markdown body + source path
- `SkillRegistry` - discovers and indexes skills from user + project scopes
- `SKILL.md` format: YAML frontmatter + markdown body
- `$ARGUMENTS` / `$0` positional substitution
- `${THUNDERUS_WORKSPACE}` variable expansion
- `!`command`` preprocessing (run shell, inject output)

#### Discovery

- User scope: `~/.thunderus/skills/<name>/SKILL.md`
- Project scope: `.thunderus/skills/<name>/SKILL.md`
- Recursive subdirectory scan (monorepo support)
- Project skills override user skills by name

#### Loading

- On startup: scan directories, parse frontmatter only
- Inject skill name + description summary into system prompt (budget: min(2% context, 16K chars))
- On invocation: load full body, preprocess, substitute, inject as user message

#### Invocation

- User types `/name [args]` -> skill body injected
- Model decides to invoke based on description in system prompt
- `disable-model-invocation: true` restricts to user-only `/name`
- `user-invocable: false` hides from slash menu, model-only

#### TUI

- `/skills` lists available skills with descriptions
- Skill slash commands integrate with existing command system

## Part 7

### Milestone 12 - Web Search

The agent gains web search through Tavily Search.

#### Tools

- `web_search` tool - Tavily Search API integration (`/res/v1/web/search`)
- Response flattening (raw Tavily response → trimmed result for model context)

#### CLI

- `thunderus debug search <query>` - exercise Tavily Search directly, print flattened results
- `TAVILY_API_KEY` env var support

### Milestone 13 - Additional Providers

Full provider coverage as specced.

#### Providers

- Anthropic Messages protocol (separate wire format: content blocks, `x-api-key` auth, `anthropic-version` header)
- Google Generative AI protocol (Gemini: `contents[]` with parts, model-in-URL, uppercase JSON Schema types)
- OpenAI Responses protocol (`input` items, `instructions`, named SSE events)
- Tool calling for all three new protocols
- Streaming for all three new protocols
- `debug provider` works for all providers

#### CLI

- `thunderus sessions` - list saved sessions
- `thunderus resume <id>` - resume a session directly

## Parking Lot

- Fuzzy/autocomplete for slash commands
- We need to unify keybinds and inject into every screen
- Double escape to cancel current request.
  - First escape should show a message that tells user to hit escape again to cancel
      the request.
