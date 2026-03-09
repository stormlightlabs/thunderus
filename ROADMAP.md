# Roadmap

## Part 2

### Milestone 3 - Tools

The agent can call tools, the runtime executes them, results feed back. Tool definitions follow `meta/TOOLS.txt`.

#### Tools

- Tool calling flow: model requests → runtime executes → result injected as next message
- Tool definitions serialized from `meta/TOOLS.txt` into the provider's tool format (see `docs/base-tools.md`)
- `read` tool - `cat -n` / `sed` for text, base64 for images
- `write` tool - direct file write, parent directory creation
- `edit` tool - unified diff application
- `bash` tool - sandboxed shell execution (120s timeout, 100KB output cap)
- `research` tool - URL fetching, HTML-to-text extraction, 50KB cap
- Tool result envelope (`status`, `content`)

#### Providers

- Tool/function calling serialization for OpenAI Completions protocol
- Multi-turn tool loop (model calls tool → runtime returns result → model continues)

#### UI - Tool Execution

- Tool call rows: name, args, status indicator (spinner → checkmark)
- Collapsible tool output
- Diff rendering for `edit` tool (red/green lines)
- Bash output display
- Loading state with task progress list (spinning/done/pending indicators)

*Reference: `designs/templates/tools.html`, `designs/templates/loading.html`*

### Milestone 4 - File Browser & Syntax Highlighting

Navigate the workspace visually, view files with highlighting.

#### Core

- File tree walker (respects `.gitignore`)
- syntect integration for syntax highlighting
- Fuzzy file finder triggered by `@` (nucleo for substring/fuzzy match against workspace file paths)
- Slash command implementation `/`

#### UI - File Browser

- Split pane: file tree (left) + file content (right) - should look like unix tree
- Recursively rendered tree items with folder/file icons, indentation
- Active file highlighting
- Breadcrumb path bar
- Line-numbered source view with syntax highlighting (syntect)
- `@` fuzzy finder overlay: text input with ranked file matches, enter to open

*Reference: `designs/templates/files.html`*

#### UI - Debug

- `/debug chat` shows a long, scrollable chat
- `/debug files` shows a long, scrollable file tree

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

### Milestone 8 - Web Search

The agent gains web search through Tavily Search.

#### Tools

- `web_search` tool - Tavily Search API integration (`/res/v1/web/search`)
- Response flattening (raw Tavily response → trimmed result for model context)

#### CLI

- `thunderus debug search <query>` - exercise Tavily Search directly, print flattened results
- `TAVILY_API_KEY` env var support

### Milestone 9 - Additional Providers

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
