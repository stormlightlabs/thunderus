---
title: "Installation"
---

## Requirements

- Rust toolchain.
- A terminal with Unicode support.
- `UMANS_API_KEY` or `OPENCODE_GO_KEY` for LLM integration.
- `fd` and `rg` for the read-only repository tools.

## API Key

Set the provider API key in the environment:

```sh
export UMANS_API_KEY=sk-...
# or
export OPENCODE_GO_KEY=sk-...
```

Secrets are not accepted through CLI flags.
