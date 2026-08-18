---
title: "Tools and MCP"
---

This page explains how built-in and external tools are described, dispatched,
executed, and recorded.

## Mental Model

## Responsibilities

Tool dispatch exposes typed tools instead of unrestricted shell strings. Each
built-in registry entry has a stable name, a provider-visible definition, an
executor, and valid example input.

## Built-in Tools

Built-in tools live in `thndrs_core::tools::<tool>` and are registered in
`thndrs_core::tools::registry`. The registry supplies the catalog used by the
agent loop and dispatches validated calls to their executors.

## MCP Tools

MCP tools enter through the external-tool path with namespaced names and
separate discovery and configuration. They share the built-in execution result
shape and audit behavior, but are not registered as built-ins.

## Execution Results

## Auditing and Side Effects

## Boundaries

## Key Types

## Invariants

## Source Map

## Related
