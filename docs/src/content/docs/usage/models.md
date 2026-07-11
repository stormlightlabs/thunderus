---
title: "Models"
---

`thndrs` selects the provider from the model id.

- OpenCode Zen models use the `opencode/<model-id>` form. The built-in default
  is `opencode/big-pickle`.
- OpenCode Go models use the documented `opencode-go/<model-id>` form, for
  example `opencode-go/kimi-k2.7-code`.
- ChatGPT Codex models use the `chatgpt-codex/<model-id>` form, for example
  `chatgpt-codex/gpt-5.6-sol`. This provider is ChatGPT-backed and experimental.
- Umans models such as `umans-coder` use the Umans provider.
- ACP agents use `acp:<name>`, where `<name>` is a configured
  `[acp_agents.<name>]` entry.
  For example, `thndrs --model acp:codex` launches the configured stdio ACP
  agent instead of a built-in provider.

Big Pickle is listed by OpenCode as free for a limited time. During that free
period, OpenCode documents that collected data may be used to improve the model;
do not use it for confidential material unless that is acceptable.

Use `--model <id>`, `THNDRS_MODEL`, or `model = "<id>"` in TOML config to select
a different provider route.

See [ACP](/usage/acp/) for ACP configuration, permission prompts, diagnostics,
and supported capabilities.
