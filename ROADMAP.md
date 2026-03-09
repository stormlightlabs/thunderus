# Roadmap

## Part 3

### Milestone 5 - Memory

Persistent agent memory across conversations, and session management.

#### Core

- SQLite database per workspace (`~/.thunderus/memory/workspaces/{hash}.db`)
- Global memory database (`~/.thunderus/memory/global.db`)
- `memories` + `embeddings` tables (packed float32 BLOBs)
- Embedding via SQLite Vector extension
- Brute-force cosine similarity search with SQL metadata pre-filtering
- Deduplication (>0.95 similarity → update existing)
- Decay on startup (archive memories unaccessed for 90+ days)
- Conversation persistence to SQLite (messages, tool calls, metadata)
- Session listing (title, timestamp, message count)
- Session resume (reload history, restore context)
- Session deletion
- Log persistence to SQLite (level, timestamp, message)

#### Tools

- `memory_store` tool - embed + insert
- `memory_recall` tool - vector search + metadata filter

#### Automatic

- Implicit recall on each user message (top 3, similarity > 0.5, injected into system prompt)

#### CLI

- `thunderus debug memory recall <query>` - test recall directly
- `thunderus debug memory stats` - row counts, DB size, model info
- Add `dbg` alias to CLI definition (i.e. `thunderus dbg` is equivalent to `thunderus debug`)

#### TUI

- `/history` shows a list of sessions
- `/resume <id>` resumes a session
- `/clear` clears the chat
- `/tokens` shows token usage
- `/model` shows model info
- `/debug memory stats` shows memory stats
- `/debug memory recall <query>` shows memory recall results
- `/debug log <id>` shows a session's logs:
    ex. `[INFO] [runtime] 2024-01-01T00:00:00.000000 | Tool call: memory_recall`

### Milestone 6 - Tool Output Formatting

Polished, tool-aware rendering of tool call results in the chat view.
Currently tool outputs fall through to a generic `CollapsibleSection` with plain muted text.

*Reference*: `designs/templates/chat-active.html`, `designs/templates/tools.html`, `designs/static/styles.css`

#### Tool Argument Display

- Per-tool compact argument formatting in `format_tool_arguments` (`chat.rs`):
    - `read`  -> file path (and `offset..offset+limit` when present)
    - `write` -> file path
    - `edit`  -> file path (extracted from `diff` header `+++ b/{path}`)
    - `bash`  -> command string (truncated at terminal width)
    - `research` -> URL
    - `memory_store` -> kind + first N chars of content
    - `memory_recall` -> query (already handled, keep as-is)
- Remove generic `"Args: key: value | ..."` fallback for known tool names

#### Read Tool Output

- Line-numbered text display matching `cat -n` style used by the tool result envelope
- Muted line numbers (right-aligned), secondary-colored content
- Directory listings: folder icon (▶) in yellow, file icon (⌸) in secondary text
- Image results: render placeholder line `[image: {mime}, {size}]` in muted text
- Truncation indicator when output exceeds display height

#### Write Tool Output

- Single success line: `Wrote {byte_count} bytes -> {path}` in green
- Error line in red when status is error

#### Edit Tool Diff Output

- Add file path header line above diff (cyan, bold) extracted from `+++ b/{path}`
- Existing `DiffView` + `parse_diff` already handle line rendering; wire path header into `draw_expanded_tool_details`

#### Bash Tool Output

- Existing `BashOutputView` handles command + output; enhance with:
    - Truncation indicator (`[{n} lines hidden]`) when output exceeds 50 visible lines
    - Exit code badge after command line: `[exit {code}]` in green (0) or red (non-zero)

#### Research Tool Output

- URL header line (cyan, underlined)
- Body text in secondary color, wrapped
- Truncation indicator when content exceeds display height

#### Generic Fallback

- Unknown/future tools still use `CollapsibleSection` with formatted output
- Ensure `format_tool_output` normalisation (CRLF fix, blank-line collapse, bullet breaks) applies to all paths

#### Height Calculation

- Update `tool_call_expanded_height` in `chat.rs` to account for per-tool layout:
    - `read`: line count (clamped 1-20) + 1 (header)
    - `write`: 1 line
    - `edit`: diff line count (clamped 1-15) + 1 (path header)
    - `bash`: output line count (clamped 1-15) + 2 (command + exit badge)
    - `research`: line count (clamped 1-15) + 1 (URL header)

## Part 4

### Milestone 7 - Settings & Help

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

## Part 5

### Milestone 8 - MCP Servers

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

### Milestone 9 - Skills

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

## Part 6

### Milestone 10 - Web Search

The agent gains web search through Tavily Search.

#### Tools

- `web_search` tool - Tavily Search API integration (`/res/v1/web/search`)
- Response flattening (raw Tavily response → trimmed result for model context)

#### CLI

- `thunderus debug search <query>` - exercise Tavily Search directly, print flattened results
- `TAVILY_API_KEY` env var support

### Milestone 11 - Additional Providers

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
