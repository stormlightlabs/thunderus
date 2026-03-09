# Base Tool Specification

This document covers the five base tools always available in Thunderus runtime:

- `read`
- `write`
- `edit`
- `bash`
- `research`

Memory tools are documented in `docs/memory.md`.

## Tool Summary

| Tool       | Purpose                                           | Runtime Function   |
| ---------- | ------------------------------------------------- | ------------------ |
| `read`     | Read file text, image bytes, or directory entries | `execute_read`     |
| `write`    | Create/overwrite file content                     | `execute_write`    |
| `edit`     | Apply unified-diff edits to existing files        | `execute_edit`     |
| `bash`     | Execute shell commands from repo sandbox path     | `execute_bash`     |
| `research` | Fetch HTTPS URLs and extract readable content     | `execute_research` |

## 1. `read`

Read a file or directory under the sandbox path.

### LLM-Facing Schema

```json
{
    "name": "read",
    "description": "Read the contents of a file. Returns line-numbered text for text files, or an image attachment for image files. Defaults to the first 2000 lines. Use offset and limit to paginate large files.",
    "parameters": {
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Path to the file (relative to repo root or absolute)."
            },
            "offset": {
                "type": "integer",
                "description": "Line number to start reading from (1-indexed).",
                "minimum": 1
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of lines to read.",
                "minimum": 1
            }
        },
        "required": ["path"]
    }
}
```

### Runtime Behavior

- Resolves and validates path with sandbox checks.
- If path is a directory:
    - Returns a sorted line list (`▶` for directory, `⌸` for file).
- If path is an image (`jpg/jpeg/png/gif/webp` or inferred image MIME):
    - Returns metadata and base64 payload.
- If path is a text file:
    - Returns line-numbered content.
    - Default `offset=1`, default `limit=2000`, max `limit=2000`.
    - Appends `[... N more lines]` when truncated.

### Error Cases

- Missing path argument.
- Path outside sandbox.
- File not found.
- Invalid UTF-8/non-image binary content.
- `offset` beyond file length.

## 2. `write`

Create or overwrite a file.

### LLM-Facing Schema

```json
{
    "name": "write",
    "description": "Write content to a file. Creates the file if it doesn't exist, or overwrites it entirely if it does. Creates parent directories as needed.",
    "parameters": {
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Path to the file (relative to repo root or absolute)."
            },
            "content": {
                "type": "string",
                "description": "The full file contents to write."
            }
        },
        "required": ["path", "content"]
    }
}
```

### Runtime Behavior

- Resolves and validates path with sandbox checks.
- Rejects writing to directory paths.
- Creates parent directories as needed.
- Writes content directly via Rust I/O (no shell interpolation).
- Rejects content over 10 MB.

### Success Format

`Wrote {bytes} bytes to {path}`

## 3. `edit`

Apply a unified diff patch to an existing file.

### LLM-Facing Schema

```json
{
    "name": "edit",
    "description": "Apply a surgical edit to an existing file. Provide a unified diff patch. The file must already exist.",
    "parameters": {
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Path to the file to edit."
            },
            "diff": {
                "type": "string",
                "description": "A unified diff patch to apply. Use standard unified diff format with @@ hunk headers."
            }
        },
        "required": ["path", "diff"]
    }
}
```

### Runtime Behavior

- Resolves and validates path with sandbox checks.
- Requires existing, non-directory target file.
- Uses internal diff application logic (exact context matching, no fuzz mode).
- On success, returns changed-line readback excerpt when hunk ranges are present.

### Error Cases

- Missing `path` or `diff`.
- File not found.
- Context mismatch while applying patch.

## 4. `bash`

Execute shell commands at sandbox working directory.

### LLM-Facing Schema

```json
{
    "name": "bash",
    "description": "Execute a bash command in the repository sandbox. Use for: running builds, tests, git operations, file searches, and any task the other tools don't cover. Commands run with the repo root as the working directory.",
    "parameters": {
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The shell command to run."
            }
        },
        "required": ["command"]
    }
}
```

### Runtime Behavior

- Executes `bash -c <command>`.
- Working directory is the current sandbox path.
- Timeout is 120 seconds.
- Stdout/stderr are combined.
- Output is truncated at 100 KB.

### Return Behavior

- Exit code `0`: success result with command output.
- Non-zero exit: error result prefixed with `Command exited with code {n}`.

## 5. `research`

Fetch a URL for documentation/reference reading.

### LLM-Facing Schema

```json
{
    "name": "research",
    "description": "Fetch a URL and return its contents. For HTML pages, extracts article content and metadata (title, author, date) using readability analysis. Use this to read API documentation, library references, standards, CLI flag descriptions, or any public technical resource. Do not use for authentication-gated content.",
    "parameters": {
        "type": "object",
        "properties": {
            "url": {
                "type": "string",
                "description": "The URL to fetch. Must be HTTPS."
            }
        },
        "required": ["url"]
    }
}
```

### Runtime Behavior

- Accepts HTTPS only.
- Rejects obvious private/internal hosts (`localhost`, loopback, RFC1918 IPv4 ranges).
- Uses `reqwest` client with 30-second timeout.
- Accept header: `text/html,application/json,text/plain`.
- Content handling:
    - JSON: pretty-print when valid JSON.
    - HTML: extract readable article content (Lectito/readability), fallback to tag-stripping extraction.
    - Plain text: return as-is.
- Rejects binary media content types (image/video/audio/octet-stream).
- Truncates output at 50 KB.

## Sandbox And Security Model

Path safety:

- All file tools use `ToolContext::resolve_path`.
- Paths are normalized and validated against sandbox root.
- Non-existent targets are validated using canonical parent paths to prevent `..` escapes.

Execution safety:

- `bash` has timeout and output cap.
- `research` has scheme/host/content-type restrictions and timeout.

Operational note:

- Network availability for `bash` is environment-dependent. Runtime code itself does not add URL allow/deny checks for arbitrary shell commands.

## Uniform Result Envelope

Tool runtime emits:

```json
{
  "status": "success" | "error",
  "content": "string",
  "tool_use_id": "string (optional)"
}
```

The conversation loop attaches `tool_use_id` when relaying a result back to the provider protocol.
