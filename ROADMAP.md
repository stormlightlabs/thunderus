# Roadmap

## Part 1

### Milestone 1 - Skeleton

Ship a binary that starts, renders a TUI, and talks to one provider.

#### CLI

- `thunderus` - launch the TUI
- `thunderus debug provider <provider> --model <model>` - send a hardcoded prompt, print the raw response. Validates auth, endpoint, and serialization for a given provider.
- `--config <path>` global flag
- Config file loading from `~/.thunderus/config.toml` (provider keys, default model, temperature)

#### Providers

- OpenAI Chat Completions protocol implementation (non-streaming)
- Moonshot (Kimi K2.5) as the first concrete provider using the OpenAI Completions protocol
- Request building, response parsing, error mapping
- `debug provider` exercises this end-to-end

#### UI - Welcome Screen

- Terminal window chrome (title bar, controls)
- ASCII logo
- Greeting text
- Suggestion items
- Input area with prompt cursor
- Keybind hints

*Reference: `designs/templates/welcome.html`*

### Milestone 2 - Conversation Loop

Send messages, receive responses, render them in the TUI. The agent's personality and response structure are defined here.

#### Agent Behavior

- System prompt from `meta/PROMPT.txt` - operating mode (Inspect → Change → Verify → Summarize), guidelines, priorities
- Enforced response format from `meta/RESPONSE.txt` - every model turn must produce four sections: **Intent** (1 sentence), **Actions** (bullet list of tool calls), **Result** (what changed), **Next** (best next step or "Done")
- The system prompt and response format are injected as the system message on every API call
- Repo content treated as untrusted instructions (prompt injection defense from `meta/PROMPT.txt`)

#### Providers

- Streaming support (SSE parsing, delta reassembly) for OpenAI Completions protocol
- Zhipu (GLM-5) as second provider on the same protocol via Coding Plan endpoint
- `debug provider` gains `--stream` flag

#### Core

- Message history (in-memory conversation state)
- System prompt assembly: base prompt (`meta/PROMPT.txt`) + response format (`meta/RESPONSE.txt`) + tool definitions (`meta/TOOLS.txt`) → single system message
- Temperature clamping per provider (0.0–1.0 for Moonshot/Zhipu)
- Unsupported field stripping (`logprobs`, `logit_bias`, `n`)

#### UI - Active Conversation

- REPL input line with prompt (`❯`)
- User message display
- Intent section (model's stated plan, parsed from response format)
- Actions section (tool calls, parsed from response format)
- Result section with markdown-ish rendering
- Next section (follow-up suggestions)
- Streaming text rendering (character-by-character as deltas arrive)

*Reference: `designs/templates/chat-active.html`, `meta/RESPONSE.txt`*

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

#### UI - File Browser

- Split pane: file tree (left) + file content (right)
- Tree items with folder/file icons, indentation
- Active file highlighting
- Breadcrumb path bar
- Line-numbered source view with syntax highlighting (syntect)

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
- Settings persist to `~/.thunderus/config.toml`

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

- ASCII logo + version
- Quick start shortcuts (ctrl+n, ctrl+o, ctrl+r)
- Recent conversations list (numbered, with relative timestamps)
- Tip rotation
- Footer links

*Reference: `designs/templates/tutorial.html`*

#### CLI

- `thunderus sessions` - list saved sessions
- `thunderus resume <id>` - resume a session directly
