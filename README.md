# Thunderus AI Agent

## Project Structure

```sh
.
├── crates/
│   ├── cli/         # CLI entry point, clap, owo-colors
│   ├── core/        # Core logic, conversation state, message handling
│   ├── memory/      # Memory management, SQLite, embeddings
│   ├── providers/   # LLM provider integrations
│   └── tools/       # Tool definitions and execution
├── designs/         # Design mockups and templates
├── meta/            # Source of truth (prompts, tools, response format)
└── docs/            # Documentation, specs, provider details
```

## Tech Stack

- **TUI**: ratatui
- **CLI**: clap (derive, POSIX-compliant flags) + owo-colors
- **Errors**: thiserror (libs), anyhow (cli)
- **Async**: tokio
- **Database**: tokio-rusqlite
- **Syntax Highlighting**: syntect

## Source of Truth

- `meta/PROMPT.txt` - system prompt (operating mode, guidelines, priorities)
- `meta/RESPONSE.txt` - enforced response format (Intent → Actions → Result → Next)
- `meta/TOOLS.txt` - tool definitions exposed to the model
- `docs/` - provider, model, tool, and memory specs
