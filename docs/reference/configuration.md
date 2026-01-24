---
outline: deep
---

# Configuration

Thunderus uses a single `config.toml` file containing one or more named profiles.
If the file is missing, the CLI creates it from the built-in example and exits
so you can edit it.

## Top-Level Fields

- `default_profile`: The profile name used when none is specified on the CLI.
- `profiles`: A table of named profiles.

## Profile Fields

Each `[profiles.<name>]` entry includes the following fields.

### Identity & Paths

- `name` (string): Profile display name.
- `working_root` (absolute path): Root directory for session work.
- `extra_writable_roots` (array of absolute paths): Additional writable roots.

### Approval & Sandbox

- `approval_mode`: One of `read-only`, `auto`, `full-access`.
- `sandbox_mode`: One of `policy`, `os`, `none`.
- `allow_network` (bool): Legacy shortcut for enabling network access.

### Provider

```
[profiles.<name>.provider]
provider = "glm" | "gemini"
api_key = "..."
model = "..."
base_url = "..." # optional
```

Provider adapters are still being integrated. Treat provider execution as
**Planned** until the adapter for your provider is fully wired.

### Workspace Sandbox

```
[profiles.<name>.workspace]
roots = []
include_temp = true
allow = []
deny = []
```

- `roots`: Additional workspace roots used for sandbox allowances.
- `include_temp`: Whether `/tmp` is considered allowed.
- `allow`: Explicitly allowed paths.
- `deny`: Explicitly denied paths.

### Network Sandbox

```
[profiles.<name>.network]
enabled = false
allow_domains = []
```

### Memory

```
[profiles.<name>.memory]
enable_vector_search = false
vector_model = "all-MiniLM-L6-v2"
vector_dims = 384
vector_fallback_threshold = -3.0
```

Vector search is optional and defaults to lexical-only behavior. If your build
has not enabled vector search, treat these fields as **Planned**.

### Skills

```
[profiles.<name>.skills]
enabled = true
skills_dir = "/abs/path/to/skills" # optional
auto_discovery = true
```

Skills can be toggled or restricted per profile. If skills are disabled, the
agent will not auto-load skill definitions. `skills_dir` overrides the default
`.thunderus/skills` location.

### Options

- `options` (table): Additional key-value pairs for provider or runtime tuning.

## Example

```
# Default profile to use when no profile is specified
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/path/to/workspace"
extra_writable_roots = []
approval_mode = "auto"
sandbox_mode = "policy"
allow_network = false

[profiles.default.provider]
provider = "glm"
api_key = "your-api-key-here"
model = "glm-4.7"
```
