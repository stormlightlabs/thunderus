# Base Tool Specification

Core tools available to every Thunderus agent. These operate on the local filesystem and execute in a sandboxed shell. Web search is covered separately in `tools.md`.

## Tool Summary

| Tool       | Purpose                            | Backing Programs               |
| ---------- | ---------------------------------- | ------------------------------ |
| `read`     | Read file contents (text or image) | `cat`, `sed`, `file`           |
| `write`    | Create or overwrite a file         | direct write (no shell)        |
| `edit`     | Apply a surgical diff to a file    | `patch` or internal diff-apply |
| `bash`     | Execute arbitrary shell commands   | `bash`                         |
| `research` | Fetch a URL for documentation      | `curl`                         |

## 1. `read`

Read the contents of a file. Text files return line-numbered content. Image files (jpg, png, gif, webp) return the binary as a base64-encoded attachment for multimodal models.

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

### Implementation

**Text files:**

```bash
# Default: first 2000 lines with line numbers
cat -n "$path" | head -n 2000

# With offset and limit (offset is 1-indexed)
sed -n "${offset},$((offset + limit - 1))p" "$path" | cat -n -
```

The `cat -n` output gives the model line numbers for precise referencing in subsequent `edit` calls. The line number prefix format is: right-justified number, tab, content.

**Image files:**

Detect via extension or `file --mime-type -b "$path"`. If mime type starts with `image/`, read the file as binary and return base64-encoded with the mime type. The runtime attaches this as an image content block (see `providers.md` for per-provider image block format).

**Edge cases:**

- File doesn't exist → return error: `"File not found: {path}"`
- Binary file that isn't an image → return error: `"Binary file, cannot display: {path}"`
- Empty file → return: `"File is empty: {path}"`
- Symlink → follow it. If broken, error.
- Path traversal outside sandbox → reject before execution.

## 2. `write`

Create a new file or overwrite an existing file entirely.

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

### Implementation

```bash
mkdir -p "$(dirname "$path")"
# Write content directly - no shell interpolation, no heredoc.
# Use the runtime's native file write, not echo/cat.
```

**No shell involvement.** Write the bytes directly from the `content` parameter to disk. Shell-based writes risk interpolation bugs and encoding issues.

**Edge cases:**

- Path traversal outside sandbox → reject.
- Writing to a directory path → error.
- Disk full → propagate OS error.

**Return value:** `"Wrote {n} bytes to {path}"`

## 3. `edit`

Apply a surgical edit to an existing file using a unified diff.

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

### Implementation

**Option A - `patch` command:**

```bash
echo "$diff" | patch --no-backup-if-mismatch -u "$path"
```

**Option B - internal diff-apply:**

Parse the unified diff hunks and apply them programmatically. This is more reliable than shelling out to `patch` because:

- No fuzz matching (exact context required → fewer silent misapplies)
- Better error messages (report which hunk failed and why)
- No temp files

**Diff format expected from the model:**

```diff
@@ -10,4 +10,5 @@
 unchanged line
-old line to remove
+new line to add
+another new line
 unchanged line
```

The model should produce minimal diffs - only the changed hunks with enough context lines (3) for unambiguous matching.

**Edge cases:**

- File doesn't exist → error: `"File not found: {path}"`
- Hunk doesn't match → error: `"Patch failed at hunk N: context mismatch at line {line}"`
- Empty diff → no-op, return success.
- Conflict with concurrent edit → the sandbox is single-writer, so this shouldn't happen.

**Return value:** Read-back of the affected line range after applying the patch (using the `read` implementation with offset/limit scoped to the changed region). This lets the model verify its edit.

## 4. `bash`

Execute shell commands in the sandboxed repo environment.

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

### Implementation

```bash
bash -c "$command" 2>&1
```

**Runtime constraints:**

- Working directory: repo root (or last `cd` target within the session, if stateful).
- Timeout: 120 seconds default. Kill with SIGTERM, then SIGKILL after 5s grace.
- Output cap: truncate stdout+stderr at 100KB. Append `"[output truncated at 100KB]"` if hit.
- No network access by default (except when `research` tool is the intended path for web).
- Inherit a minimal `PATH` with standard unix tools.

**Available programs the agent should prefer:**

| Task                       | Preferred Command                        | Notes                                            |
| -------------------------- | ---------------------------------------- | ------------------------------------------------ |
| Find files by name/pattern | `find . -name '*.rs' -type f`            | Recursive, glob-based                            |
| List directory contents    | `ls -la path/`                           | Use `-1` for machine-parseable output            |
| Search file contents       | `rg 'pattern' --type rust`               | ripgrep. Falls back to `grep -rn` if unavailable |
| Fuzzy find files           | `find . -type f \| fzf --filter 'query'` | Non-interactive fzf as a filter                  |
| Stream transform           | `awk '{print $1, $3}' file`              | Column extraction, reformatting                  |
| Stream edit                | `sed -n '10,20p' file`                   | Line range extraction                            |
| Count lines/words          | `wc -l file`                             |                                                  |
| File type detection        | `file --mime-type -b path`               |                                                  |
| Diff two files             | `diff -u a.txt b.txt`                    |                                                  |
| Git operations             | `git log --oneline -20`                  | Full git CLI available                           |
| Build/test                 | `cargo build`, `npm test`, etc.          | Project-specific                                 |

**Return value:**

```json
{
  "exit_code": 0,
  "stdout": "string",
  "stderr": "string"
}
```

Or combined if simpler:

```json
{
  "exit_code": 0,
  "output": "string"
}
```

Use combined `output` (stdout+stderr interleaved via `2>&1`) - separate streams are rarely useful to the model and complicate parsing.

## 5. `research`

Fetch a URL and return its content. Intended for reading documentation, API references, and public technical resources.

### LLM-Facing Schema

```json
{
  "name": "research",
  "description": "Fetch a URL and return its contents. Use this to read API documentation, library references, standards, CLI flag descriptions, or any public technical resource. Do not use for authentication-gated content.",
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

### Implementation

```bash
curl -sS -L --max-time 30 --max-filesize 5242880 \
  -H "Accept: text/html,application/json,text/plain" \
  -H "User-Agent: Thunderus/1.0" \
  "$url"
```

**Post-processing pipeline:**

1. **HTML → text.** If response is HTML, strip tags and extract main content. Use a readability-style extractor (like `readability-cli`, `html2text`, or a Rust equivalent like `scraper`) to pull the article body and skip nav/footer/ads.
2. **JSON → pretty-printed.** If response is JSON, pretty-print with 2-space indent.
3. **Plain text → pass through.**
4. **Truncate** to 50KB of text. Append `"[content truncated at 50KB]"` if hit.

**Constraints:**

- HTTPS only. Reject HTTP URLs (upgrade or error).
- Max response size: 5MB raw, 50KB after text extraction.
- Timeout: 30 seconds.
- No cookies, no auth headers, no session state.
- Reject URLs pointing to private/internal networks (localhost, 10.x, 172.16-31.x, 192.168.x).
- Reject non-text content types (images, video, binaries) - return error with the detected content type.

**Return value:** The extracted text content, or an error string.

## Security Model

All tools execute within a sandbox scoped to the repository root.

| Constraint      | Enforcement                                                                                  |
| --------------- | -------------------------------------------------------------------------------------------- |
| Path traversal  | Resolve all paths to absolute, reject if outside repo root                                   |
| Shell injection | `bash` tool runs user-controlled commands by design - sandbox via cgroup/namespace/container |
| Network access  | Only `research` tool makes outbound requests. `bash` has no network by default.              |
| Secrets         | Never include API keys, tokens, or credentials in tool results returned to the model         |
| File size       | `write` rejects files > 10MB. `read` truncates at 2000 lines (configurable).                 |
| Timeout         | All tools have per-invocation timeouts. No tool can block indefinitely.                      |

## Tool Result Format

All tools return results in a uniform envelope:

```json
{
  "tool_use_id": "string",
  "status": "success" | "error",
  "content": "string"
}
```

- `tool_use_id` matches the ID from the model's tool call (provider-specific - see `providers.md`).
- `status`: `"success"` for normal results, `"error"` for failures.
- `content`: the result text. For `read` with images, this is replaced by the provider's image content block format.

Errors are not exceptions - they're returned as tool results with `status: "error"` so the model can reason about them and retry or try a different approach.
