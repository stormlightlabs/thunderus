# Lifecycle Hooks

`AgentEvent` remains the observational record of a run: `Started`,
`ToolStarted`, `ToolFinished`, `Finished`, and the other variants describe what
happened. Lifecycle hooks are the complementary control mechanism. They run at
named boundaries before execution crosses them and may return only the
decisions that boundary supports.

The API should be typed, provider-neutral, and deterministic. The
`thndrs-agent` crate owns hook inputs, outcomes, ordering, and dispatch. The
`thndrs` application invokes hooks at application-owned session boundaries and
owns configuration, trust, process execution, and command-backed handlers.
Provider wire payloads do not enter the public hook API.

## Reference model

Codex and Claude Code organize hooks as lifecycle event, matcher, and handler
rules. Both support command handlers with structured input and decisions at
points such as pre-tool use and stop; their broader catalogs also cover
sessions and compaction. Claude additionally supports HTTP, prompt, agent, and
MCP handlers.

Pi exposes an in-process TypeScript extension API. Its handlers can transform
input and context, replace provider requests, block tool calls, and modify tool
results. Transformations compose in extension load order, which is the most
useful model for a hook that changes data another hook may inspect.

Polytoken makes the operational choices explicit: global and project hooks,
declared ordering, blocking versus fire-and-forget execution, per-event result
types, deadlines, and fail-closed behavior for blocking failures.

thndrs should adopt the shared lifecycle, matching, structured protocol, and
trust concepts without copying each product's handler implementation. Its event
stream handles passive observation. Hooks cover boundaries where a handler can
change whether or how thndrs proceeds.

Reviewed references, current on 2026-08-13:

- [Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Claude Code hooks](https://code.claude.com/docs/en/hooks)
- [Pi extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)
- [Polytoken hook engineering](https://docs.polytoken.dev/harness-engineering/hooks/)

## Initial hook points

A turn begins when thndrs accepts one user input and ends when its agent run
settles as finished, failed, or cancelled. One turn may contain several model
requests and tool calls. A session contains any number of turns and ends only
when the durable conversation is explicitly closed.

| Hook point      | Runs                                                                                | Allowed outcome                                        |
| --------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `session_start` | After durable context is loaded and before a new or resumed session becomes active  | Continue, add bounded context, or reject activation    |
| `turn_start`    | After user input is accepted and before compaction, model, or tool work begins      | Continue, add bounded context, or reject the turn      |
| `pre_model`     | Before each provider-neutral model request is lowered for the selected provider     | Continue, add bounded context, or stop the turn        |
| `pre_tool_use`  | After a tool call is normalized and before permission or execution                  | Continue, deny with a reason, or replace arguments     |
| `post_tool_use` | After execution and evidence capture, before the result returns to the model        | Keep or replace the model-facing projection            |
| `pre_compact`   | After the source range is selected and before a compaction request is built         | Continue, deny, or add bounded compaction instructions |
| `post_compact`  | After a candidate summary is parsed and before review or commit                     | Accept, reject, or replace the candidate summary       |
| `stop`          | When normal completion checks propose ending a turn, after queued steering is read  | Finish or continue with bounded context                |
| `turn_end`      | Once after the result and any session write settle, before the terminal event       | Acknowledge                                             |
| `session_end`   | Before an active session is explicitly closed                                       | Continue or reject closure                             |

Each hook receives a typed, event-specific input rather than a generic mutable
run object. Stable run, session, and tool-call identifiers are included where
they exist. A hook may not change the selected provider, model, workspace,
authority, tool name, tool-call identifier, actual tool status, captured
evidence, or user-facing display result.

`session_start`, `turn_start`, `pre_model`, and `stop` context additions enter
the normal context ledger with hook provenance and the same size and reduction
rules as other context. `stop` continuation has a dedicated finite budget so a
hook cannot keep a turn alive forever.

`stop` and `turn_end` are separate boundaries. `stop` runs only when the agent
would otherwise finish normally and may continue the current turn. `turn_end`
runs exactly once after any finished, failed, or cancelled turn has settled. It
receives the terminal outcome but cannot change it, reopen the turn, or alter
the durable record. A `turn_end` failure is recorded and reported without
changing the settled outcome. Turn cancellation does not skip finalization;
handlers run through the bounded process-settlement path.

`pre_compact` cannot change the selected source range, protected facts, recovery
handle, model, or review policy. A replacement from `post_compact` goes through
the same schema, source-range, protected-fact, and review checks as a generated
candidate. Rejection preserves the current context.

## Tool-call ordering

For every provider and execution surface, one shared path must:

1. Parse and normalize the provider tool request.
2. Run matching `pre_tool_use` hooks in effective configuration order. Each
   later hook sees the previous valid argument replacement. A denial is
   terminal.
3. Reparse and validate the final arguments against the active tool schema and
   current authority.
4. Ask the existing application permission adapter about the effective
   request. A hook can narrow or deny a request, but cannot grant permission or
   bypass authority.
5. Execute through the existing application execution adapter, capture the
   actual output and evidence, and emit the observational tool events.
6. Run matching `post_tool_use` hooks over the model projection. Pass the final
   projection through normal context limits before returning it to the
   provider.

A hook denial becomes an explicit failed tool result attributed to that hook;
it does not pretend the tool executed. A failure after a tool has executed
cannot undo its side effects. The run must preserve and report the actual tool
outcome even if a `post_tool_use` hook prevents that result from reaching the
model.

The current `ToolPermissionHook` and `ToolExecutionHook` types are application
adapters, not lifecycle hooks. Rename them to callback or adapter terminology,
retaining compatibility aliases if public callers need them, before reserving
the hook name for this API.

## Ordering, failures, and observation

Hooks compose sequentially: global configuration first, then trusted project
configuration, preserving declaration order within each scope. This is
deliberate. Parallel execution makes transformations ambiguous.

An outcome that leaves the current boundary ends its chain. Rejection, denial,
turn stop, and stop continuation do not run later handlers at that point.
Argument, context, compaction-candidate, and model-projection changes remain in
the chain, so each later matching handler receives the previous validated
value. All matching `turn_end` handlers run so one failed finalizer does not
prevent later finalizers from observing the settled turn.

Decision-capable hooks fail closed. A timeout, cancellation, non-zero exit,
malformed response, invalid outcome for the current point, or invalid argument
rewrite blocks the guarded transition and produces a bounded diagnostic. A
`pre_tool_use` failure returns a failed tool result. A failure at another turn
or compaction point fails that operation, then proceeds through `turn_end` with
the failed outcome. A `session_start` failure leaves the session inactive, and
a `session_end` failure leaves an explicit close uncommitted. Because
`turn_end` runs after settlement, its failures are diagnostic and do not change
the terminal result. Process shutdown still settles resources and records the
hook failure rather than allowing a hook to hold the process open.
Cancellation terminates an active command hook and waits only for a bounded
process-settlement period.

Hook invocation and outcomes are observable through bounded, redacted agent
events carrying the hook name, scope, point, duration, and outcome class. They
never include command output, secrets, or full tool arguments by default.
Emitting those events does not recursively invoke hooks.

## Handlers and command-backed hooks

The public API accepts application-provided handlers without requiring a child
process. HTTP, MCP, prompt, model, agent, and embedded-language implementations
plug into this handler boundary and use the same typed requests, outcomes,
ordering, limits, and failure rules. Their transport and authority belong to
their application adapters rather than the lifecycle API.

The first configured adapter runs local commands. Each `[[hooks]]` entry in the
existing global or project TOML configuration has a unique name, one hook
point, an executable and argument vector, a timeout, and an optional list of
exact tool names for tool hooks. Omitted tool names match every tool. The
executable is invoked directly, without an implicit shell.

Commands receive a versioned JSON document on stdin and return one bounded JSON
outcome on stdout. The document contains the hook point, stable identifiers,
configuration scope, workspace, and only the event-specific data required by
that point. Provider credentials, provider wire payloads, unrelated
environment variables, and an unbounded transcript are excluded. stderr is
bounded and redacted for diagnostics; stdout and runtime have explicit limits.

Global hook executables resolve independently of the active project. A global
definition cannot use a relative executable from an untrusted workspace.
Global hooks otherwise run with the thndrs process authority, so status and
errors must not imply process isolation that does not exist.

Project hook configuration may be discovered and shown, but remains inactive
until `HOOK-5` integrates it with project trust. Trust applies to the exact
project configuration hash and is inspectable and revocable. A changed
configuration returns its hooks to the blocked state.

## Boundaries

Passive notifications consume `AgentEvent`; they do not run through a second
fire-and-forget hook system. Hook outcomes can narrow or reject an action but
cannot bypass tool permissions, workspace containment, provider adapters, or
project trust.
