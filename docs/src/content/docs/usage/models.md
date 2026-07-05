---
title: "Models"
---

`thndrs` selects the provider from the model id.

- Umans models such as `umans-coder` use the Umans provider.
- OpenCode Go models use the documented `opencode-go/<model-id>` form, for
  example `opencode-go/kimi-k2.7-code`.
- ACP agents use `acp:<name>`, where `<name>` is a configured
  `[acp_agents.<name>]` entry.
  For example, `thndrs --model acp:codex` launches the configured stdio ACP
  agent instead of a built-in provider.

The default model remains `umans-coder`.

See [ACP](/usage/acp/) for ACP configuration, permission prompts, diagnostics,
and supported capabilities.
