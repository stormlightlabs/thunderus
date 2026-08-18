---
title: "Providers"
---

This page explains the provider-neutral runtime boundary and the work owned by
each provider adapter.

## Mental Model

## Responsibilities

ChatGPT Codex, OpenCode Zen, and OpenCode Go are concrete clients behind a small
streaming provider trait. Each adapter owns its wire format, authentication,
and conversion into provider-neutral agent events.

## Request Conversion

The agent loop derives one tool catalog from the registry and passes it to each
provider request. Anthropic-compatible routes receive
`name`/`description`/`input_schema` entries. OpenAI-compatible routes convert
the same catalog to function tools at the provider boundary.

## Streaming Event Normalization

## Authentication

## Errors, Retries, and Cancellation

## Boundaries

## Key Types

## Invariants

## Source Map

## Related
