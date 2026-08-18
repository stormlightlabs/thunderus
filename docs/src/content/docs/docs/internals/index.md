---
title: "Architecture"
---

thndrs is organized around an application state machine, an agent runtime, and
a terminal presentation loop. This section explains how those pieces fit
together.

## System Overview

The application accepts input through either the terminal interface or the ACP
server. Both frontends use the agent runtime, which assembles context, streams
requests through a provider, executes tools, and reports events. The terminal
interface projects those events into application state and renders the mutable
part of that state with Ratatui.

## Major Subsystems

- The [runtime](/docs/internals/runtime/) owns interactive application state and
  effects.
- [Context assembly](/docs/internals/context/) constructs the instructions, conversation,
  skills, and tool catalog sent to a provider.
- [Providers](/docs/internals/providers/) translate provider-neutral requests and normalize
  streaming responses.
- [Tools and MCP](/docs/internals/tools/) expose model-visible actions and record
  their side effects.
- The [terminal UI](/docs/internals/terminal-ui/) renders the interactive frontend.
- [Sessions](/docs/internals/sessions/) persist transcripts, metadata, and audit records.
- The [ACP server](/docs/internals/acp/) exposes the runtime through another transport.

## Architectural Boundaries

`thndrs-agent` is the provider-neutral agent library. The `thndrs` application
owns filesystem discovery, session persistence, terminal I/O, and ACP
transport. Provider wire formats stay behind provider adapters rather than
appearing in the library's public API.

## Where to Start

Read the [request lifecycle](/docs/internals/lifecycle/) before following
individual subsystems. Use the [codebase tour](/docs/internals/codebase/) when
you are ready to find the corresponding modules.

## Related

- [Development workflow](/docs/development/workflow/)
- [Testing](/docs/development/testing/)
