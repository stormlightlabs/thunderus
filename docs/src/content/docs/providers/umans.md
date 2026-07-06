---
title: "Umans Provider"
---

## Models

`thndrs` supports Umans Code through `umans-coder` and `umans-glm-5.2`.

`umans-coder` is the main Umans coding model. `umans-glm-5.2` is useful when
you want the GLM model path or its larger context behavior.

## Authentication

Set `UMANS_API_KEY` in the environment. `thndrs` does not accept API keys in CLI
flags.

```sh
export UMANS_API_KEY=sk-...
```

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

## Tool Schemas

Local read-only tools are sent as provider-native tool schemas. Tool
descriptions stay short and focus on purpose, safety limits, and truncation
behavior.

## Error Mapping

Provider errors are mapped into transcript errors and return the prompt to a
usable state.

## Model Metadata

`thndrs` can read Umans model metadata from `/v1/models/info` for visible model
capabilities.

## Smoke Test

The live provider smoke test is ignored by default and requires `UMANS_API_KEY`
plus network access.
