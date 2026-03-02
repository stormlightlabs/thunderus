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

#### UI - File Browser

- Split pane: file tree (left) + file content (right)
- Tree items with folder/file icons, indentation
- Active file highlighting
- Breadcrumb path bar
- Line-numbered source view with syntax highlighting (syntect)
- `@` fuzzy finder overlay: text input with ranked file matches, enter to open

*Reference: `designs/templates/files.html`*

## Part 3

### Milestone 5 - Web Search

The agent gains web search through Brave Search.

#### Tools

- `web_search` tool - Brave Search API integration (`/res/v1/web/search`)
- Response flattening (raw Brave response → trimmed result for model context)

#### CLI

- `thunderus debug search <query>` - exercise Brave Search directly, print flattened results
- `BRAVE_API_KEY` env var support

## Part 4

### Milestone 6 - Memory

Persistent agent memory across conversations.

#### Core

- SQLite database per workspace (`~/.thunderus/memory/workspaces/{hash}.db`)
- Global memory database (`~/.thunderus/memory/global.db`)
- `memories` + `embeddings` tables (packed float32 BLOBs)
- Embedding via local model (all-MiniLM-L6-v2, 384 dimensions)
- Brute-force cosine similarity search with SQL metadata pre-filtering
- Deduplication (>0.95 similarity → update existing)
- Decay on startup (archive memories unaccessed for 90+ days)

#### Tools

- `memory_store` tool - embed + insert
- `memory_recall` tool - vector search + metadata filter

#### Automatic

- Implicit recall on each user message (top 3, similarity > 0.5, injected into system prompt)

#### CLI

- `thunderus debug memory recall <query>` - test recall directly
- `thunderus debug memory stats` - row counts, DB size, model info

### Milestone 7 - Additional Providers

Full provider coverage as specced.

#### Providers

- Anthropic Messages protocol (separate wire format: content blocks, `x-api-key` auth, `anthropic-version` header)
- Google Generative AI protocol (Gemini: `contents[]` with parts, model-in-URL, uppercase JSON Schema types)
- OpenAI Responses protocol (`input` items, `instructions`, named SSE events)
- Tool calling for all three new protocols
- Streaming for all three new protocols
- `debug provider` works for all providers

## Part 5

### Milestone 8 - Settings & Help

User-facing configuration and documentation inside the TUI.

#### UI - Settings

- Split pane: sidebar nav (left) + settings content (right)
- Setting groups: General, Appearance, Editor, Keyboard, AI Model, Tools, Privacy
- Toggle switches, select dropdowns
- Save/reset actions
- Settings persist to `~/.thunderus/config.toml` using toml crate

*Reference: `designs/templates/settings.html`*

#### UI - Help

- Tabbed nav: Keyboard Shortcuts, Commands, Tips, About
- Shortcut grid (two-column, key + description)
- Slash command list (`/help`, `/clear`, `/model`, `/tokens`)
- Tip box

*Reference: `designs/templates/help.html`*

## Part 6

### Milestone 9 - Session Management

Resume, list, and manage conversations.

#### Core

- Conversation persistence to SQLite (messages, tool calls, metadata)
- Session listing (title, timestamp, message count)
- Session resume (reload history, restore context)

#### UI - Tutorial / Home

- Version
- Quick start shortcuts (ctrl+n, ctrl+o, ctrl+r)
- Recent conversations list (numbered, with relative timestamps)
- Tip rotation
- Footer links

*Reference: `designs/templates/tutorial.html`*

#### CLI

- `thunderus sessions` - list saved sessions
- `thunderus resume <id>` - resume a session directly
