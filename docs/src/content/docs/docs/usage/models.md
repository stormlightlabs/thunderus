---
title: "Models"
---

## Recommended Providers

- ChatGPT Codex models use the `chatgpt-codex/<model-id>` form, for example
  `chatgpt-codex/gpt-5.6-sol`, `chatgpt-codex/gpt-5.6-terra`, or
  `chatgpt-codex/gpt-5.6-luna`.
  - The integration is ChatGPT-backed.
  - Selecting a GPT-5.6 model from `/model` opens the reasoning-effort picker;
    - `/reasoning` opens it again later.

The picker shows reasoning controls only when the selected model supports them.
`reasoning_effort` and `reasoning_summary` can also be set through configuration
or `THNDRS_` environment variables.

## Other

- OpenCode Zen models use the `opencode/<model-id>` form, such as
  `opencode/big-pickle`.
- OpenCode Go models use the documented `opencode-go/<model-id>` form, for
  example `opencode-go/kimi-k2.7-code`.
- ACP agents use `acp:<name>`, where `<name>` is a configured
  `[acp_agents.<name>]` entry.
  For example, `thndrs --model acp:codex` launches the configured stdio ACP
  agent instead of a built-in provider.

Big Pickle is listed by OpenCode as free for a limited time. During that free
period, OpenCode documents that collected data may be used to improve the model;
do not use it for confidential material unless that is acceptable.

Use `--model <id>`, `THNDRS_MODEL`, or `model = "<id>"` in TOML config to
override the setup selection.

See [ACP](/docs/usage/acp/) for ACP configuration, permission prompts, diagnostics,
and supported capabilities.
