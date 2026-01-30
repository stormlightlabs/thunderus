---
outline: deep
---

# Data Flow

Based on `docs/concepts/system.md` with more implementation detail.

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

## Request Lifecycle

1. **UI captures input** and calls `Agent::process_message()`.
2. **Agent prepares request**: update task context, optionally query memory,
   assemble system prompt and `ChatRequest`.
3. **Provider streams**: tokens and tool calls are emitted as `StreamEvent`s.
4. **Tool calls** are classified; risky calls require approval.
5. **Dispatcher executes** approved tool calls and returns `ToolResult`.
6. **UI renders** the transcript and writes all events to the session log.

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

## Cancellation

Cancellation is cooperative via `CancelToken`. The UI can cancel; providers and agent
check the token during streaming.

## Session Events

Sessions are append-only JSONL. Common events include user/model messages, tool
calls/results, approvals, patches, shell commands, file reads, and memory updates.
