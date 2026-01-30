---
outline: deep
---

# System

Thunderus is a TUI-first agent harness. The UI owns the event loop, spawns the agent,
and renders a streaming transcript. The agent orchestrates model calls, tool execution,
approvals, and (optional) memory retrieval, emitting `AgentEvent`s back to the UI.

## Runtime Flow

```text
User Input
   |
   v
TUI (event loop)
   |
   | spawn agent task
   v
Agent::process_message
   |  (optional)
   +--> MemoryRetriever -> system context
   |
   +--> Provider::stream_chat
          | tokens / tool_calls / done
          v
      AgentEvent stream ---------------------> TUI transcript + session log
```

Key points:

- The UI owns cancellation, pause, and drift handling.
- The agent streams tokens and tool events; the UI renders them and persists session events.
- Memory retrieval (if configured) adds a system-context block before the model call.

## Tool Execution Path

```text
ToolCall (from model)
   |
   v
Risk classify + ApprovalGate
   |
   +--> ApprovalProtocol (TUI prompt) -> user decision
   |
   v
Profile path access (read/write policy)
   |
   v
SessionToolDispatcher
   |  logs tool call/result
   |  tracks read history
   |  enqueues patches (for patch tool)
   v
ToolRegistry -> builtin tool implementation
```

Guardrails in the path:

- Approval modes gate risky or blocked actions before any tool runs.
- Profile path access can deny or require approval for specific paths.
- User-owned files are blocked from writes until re-read and reconciled.
- Shell commands are classified and may be blocked pre-execution.

## Session + Memory

```text
Session (JSONL)
  - UserMessage
  - ModelMessage
  - ToolCall / ToolResult
  - Approval
  - Patch / FileRead / etc.

Memory Retriever
  - queries memory store
  - returns relevant chunks
  - used to augment system context
```

Sessions provide replayable, auditable histories. Memory retrieval is optional and is
used to inject relevant facts into the model’s system prompt for the current turn.

## TUI/Event Loop

```text
Crossterm + Ratatui
   |
   +--> input events
   +--> agent events
   +--> approval requests
   +--> drift notifications
```

The TUI multiplexes input, agent stream, approvals, and drift checks to keep the
agent responsive while preserving user control.
