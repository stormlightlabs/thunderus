---
title: "Adding a Tool"
---

This guide covers the changes required to add a built-in, model-visible tool.

## Before You Start

MCP tools use the external-tool path and do not need a built-in registry entry.
Use this guide only when the tool ships with thndrs.

## Define the Tool

Add a `thndrs_core::tools::<name>` module with module documentation and a
provider-visible `definition()`.

## Parse and Validate Input

Define the input shape and reject invalid requests before execution. Include a
valid example input for registry and schema tests.

## Implement Execution

Implement `execute_request()` and return errors for recoverable failures.

## Register the Tool

Add the stable name, definition function, executor, and example input to
`thndrs_core::tools::registry`.

## Record Side Effects

Tools that write files or launch processes return structured side-effect
metadata. Add session tests that assert the resulting audit records.

## Test the Tool

Cover the schema, input parsing, successful execution, and failures with focused
unit tests. Add prompt or tool-catalog snapshots when the provider-visible
definition changes.

## Update Public Documentation

Update the [tool reference](/docs/reference/tools/) when the model-visible name,
fields, behavior, or examples change.

## Run the Checks

Run the focused tool and session tests first, then the repository's standard
format, Clippy, and workspace test commands.
