---
title: "Umans Provider"
---

## Models

The Umans integration includes:

- `umans-coder` - the recommended route, currently backed by Kimi K2.7-Code.
- `umans-kimi-k2.7` - hard coding tasks with always-on reasoning.
- `umans-glm-5.2` - GLM 5.2 and the largest Umans context window.
- `umans-flash` - a fast model for short context, summaries, and interactive work.

Umans can change the model behind `umans-coder`; use model metadata or `/model`
to inspect the current catalogue.

## Authentication

Create an API key in the Umans dashboard under API Keys. `thndrs` does not
accept API keys in CLI flags. You can either export `UMANS_API_KEY` for a
process-local credential or run the guided login flow:

```sh
thndrs setup --provider umans

# or use a process-local credential:
export UMANS_API_KEY=sk-...

# or, in an interactive terminal:
thndrs login umans
```

The guided flow asks whether the key belongs in the global store
(`~/.thndrs/credentials.env`) or the current project store
(`.thndrs/credentials.env`). Project credentials are added to Git's local
exclude file. Keys are never written to TOML, sessions, prompt inspection,
snapshots, or diagnostics.

## Messages API

The provider client uses the Anthropic-compatible Umans Messages API:

- Base URL: `https://api.code.umans.ai`
- Endpoint: `POST /v1/messages`
- API key header: `x-api-key`
- Version header: `anthropic-version: 2023-06-01`

## Streaming Events

Provider streaming output is normalized into app events. Assistant text,
reasoning, tool calls, completion, and errors are kept as distinct event types
so the transcript can render them separately.

Extended thinking is a provider/model capability. For models that support the
toggle, `thndrs` lowers `On` and `None` to the Messages API `thinking` object;
`Auto` leaves Umans' model default unchanged. Models with always-on reasoning
only expose `Auto` in the picker.

## Tool Schemas

Local read-only tools are sent as provider-native tool schemas. Tool
descriptions stay short and focus on purpose, safety limits, and truncation
behavior.

## Error Mapping

Provider errors are mapped into actionable transcript errors and return the
prompt draft to a usable state.

- Authentication failures point to `UMANS_API_KEY`/`thndrs login umans`. If
  `UMANS_API_KEY` is an environment override, replace or unset it before login.
  - A stored key cannot take precedence over it.
- rate limits suggest checking Umans usage
- network and server failures can be retried
