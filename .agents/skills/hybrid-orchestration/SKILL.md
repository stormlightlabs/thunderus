---
name: hybrid-orchestration
description: Orchestrate feature work from Codex or Pi with one or two Herdr-managed worker panes using GPT-5.6 Terra at high effort or Luna at xhigh effort in Thndrs, while capturing and fixing evidenced usability or efficiency problems in the thndrs agent harness. Use when the user asks for hybrid orchestration, or asks to delegate project work to thndrs instances, or explicitly wants feature development paired with agent-harness improvement.
---

# Hybrid Orchestration

Use Codex or Pi as the parent orchestrator and the interactive `thndrs` TUI as the worker.

## Establish control

Verify that Herdr manages the parent session before issuing control commands:

```sh
test "${HERDR_ENV:-}" = 1
```

If the check fails, explain that hybrid orchestration requires a Herdr-managed session and stop.
Use at most two thndrs workers. Keep the user's focus unchanged, and close only panes or tabs
created by this workflow. Start new sessions to prevent context bloat and token usage.

Define the feature deliverable before starting workers. Note that Thndrs & Pi follow the same
AGENTS.md so no need to include that in prompts. Treat harness improvement as a bounded second
responsibility for the parent/orchestrator only.

## Select workers and models

Start with one worker. Add a second only for independent work that materially benefits from concurrency.

| Work                                                                      | Model and effort                      |
| ------------------------------------------------------------------------- | ------------------------------------- |
| Bounded implementation, focused tests, mechanical investigation           | `chatgpt-codex/gpt-5.6-terra`, `high` |
| Difficult diagnosis, architecture, adversarial review, ambiguous failures | `chatgpt-codex/gpt-5.6-luna`, `xhigh` |

Keep one writer in the shared checkout. A second worker must own non-overlapping files or remain read-only.
Do not run duplicate broad checks in different panes.

Resolve the Cargo-installed executable once for delegated workers. Do not run those workers through
`cargo run` or a binary under this workspace's `target` directory. Worker sessions must survive `cargo clean`:

```sh
hybrid_thndrs_bin="$(command -v thndrs || true)"
test -n "$hybrid_thndrs_bin" || {
  echo "hybrid orchestration requires a Cargo-installed thndrs on PATH" >&2
  exit 1
}
case "$hybrid_thndrs_bin" in
  "$PWD"/target/*)
    echo "hybrid orchestration will not use a workspace target binary" >&2
    exit 1
    ;;
esac
```

If this check fails, stop and ask the user to install or update `thndrs`.

## Create worker panes

Create or reuse an existing background tab in the current Herdr workspace and preserve the working directory:

```sh
herdr tab create --workspace "$HERDR_WORKSPACE_ID" --cwd "$PWD" --label THNDRS --no-focus
```

Read `.result.root_pane.pane_id` from the response. If a second worker is justified, split that
pane and read `.result.pane.pane_id` from its response:

```sh
herdr pane split <root-pane-id> --direction right --cwd "$PWD" --no-focus
```

Treat IDs as opaque. Never infer them from layout or examples.

Herdr does not currently expose `thndrs` as an `agent start --kind` value. Run each worker TUI directly in
a pane and observe its visible status.

## Delegate

Give each worker one explicit deliverable, file ownership, constraints, and a focused verification target.
Require a concise final response with these sections:

```text
Result
Files changed
Verification
Harness observations
```

In `Harness observations`, require only friction directly experienced during the run. Each observation must
include evidence or reproduction, impact, and the smallest plausible improvement. Require `None` when there
was no concrete friction; do not reward speculative suggestions.

Start a Terra worker TUI like this:

```sh
herdr pane run <pane-id> env THNDRS_REASONING_EFFORT=high \
  "$hybrid_thndrs_bin" --cwd "$PWD" \
  --model chatgpt-codex/gpt-5.6-terra --websearch none
```

For Luna, change the model to `chatgpt-codex/gpt-5.6-luna` and the environment value to `xhigh`.

Wait until the TUI reports that its composer is editable, then send the task text and `Enter`
separately. Do not match the `❯` glyph because the shell prompt may still be present in the
pane's history while `cargo` or thndrs starts:

```sh
herdr pane wait-output <pane-id> --match 'Idle · Editable' --source visible --timeout 30000
herdr pane send-text <pane-id> '<bounded task and reporting contract>'
herdr pane send-keys <pane-id> enter
```

If the prompt does not appear, read the visible pane. Setup, authentication, or startup errors require
parent or user action. Do not type through an unexpected screen. Prompt both workers before waiting when
they are independent. Otherwise, finish and integrate the first worker before starting the second.

Steer an active worker by writing the refinement into its composer and sending the platform chord:

```sh
herdr pane send-text <pane-id> '<refinement of the active task>'
case "$(uname -s)" in
  Darwin) herdr pane send-keys <pane-id> super+enter ;;
  *)      herdr pane send-keys <pane-id> ctrl+enter ;;
esac
```

The thndrs TUI enables the enhanced keyboard protocol needed to represent `super+enter`. Confirm the
composer clears and the active turn reflects the refinement. If it remains visible, inspect the pane and
report the failed steering attempt; do not send plain `Enter`, because that queues a follow-up instead.

## Observe and integrate

Wait for the submitted turn to become active, then wait for its terminal status. `wait-output`
searches the current snapshot before polling, so observing the active state prevents a stale
`Complete` label from a previous turn from satisfying the second command:

```sh
herdr pane wait-output <pane-id> \
  --regex '(Thinking|Waiting for permission)' \
  --source visible --timeout 5000
herdr pane wait-output <pane-id> \
  --regex '(?m)^\\s*(Complete|Failed|Stopped)\\s+·\\s+Editable' \
  --source visible --timeout 60000
```

Very short turns may finish before the first wait observes `Thinking`. In that case, inspect the visible
pane and accept a terminal status only when the transcript contains the current task's response. If either
wait times out, inspect progress once with `herdr pane process-info --pane <pane-id>` and a bounded
`herdr pane read`; continue waiting only when the process is making progress. After completion, read the
recent TUI transcript:

```sh
herdr pane read <pane-id> --source recent-unwrapped --lines 160
```

The TUI remains open after a turn. Send follow-up prompts through the same `send-text` then `send-keys enter`
sequence while its context is useful. If alternate-screen history omits the final report, ask the worker for
a shorter summary in the same pane.

Do not accept a worker's success claim as verification. Inspect every changed file, reconcile overlapping
assumptions, and run the smallest relevant check yourself.

## Improve the harness from evidence

Triage each reported harness observation against the transcript and code:

1. Reproduce or otherwise confirm the friction.
2. Identify the shared cause rather than patching the worker prompt around it.
3. The parent orchestrator fixes it in this cycle only when it is concrete, repository-owned, bounded, and does not jeopardize the requested feature.
4. Run the narrowest relevant automated check, then exercise the affected state in a fresh `cargo run` thndrs instance built from the modified checkout.
5. Report issues that are too broad, risky, or unconfirmed.

Good signals include lifecycle ambiguity, missing diagnostics, unnecessary context or token use, repeated tool work,
permission dead ends, poor error recovery, and unclear worker output. Model disagreement, stylistic preference, or an
unverified idea is not a harness defect.

Keep thndrs workers on the assigned feature. The parent orchestrator owns confirmed harness fixes instead of
changing the worker assignment or delegating the fix. Wait for the active feature writer to settle before editing
shared files or building the workspace.

Create a fresh background pane for live verification and run the modified checkout:

```sh
herdr pane split <root-pane-id> --direction down --cwd "$PWD" --no-focus
herdr pane run <verification-pane-id> env THNDRS_REASONING_EFFORT=high \
  cargo run -p thndrs -- --cwd "$PWD" \
  --model chatgpt-codex/gpt-5.6-terra --websearch none
```

Read `.result.pane.pane_id` from the split response and use it as `<verification-pane-id>`. Wait for
`Idle · Editable`, reproduce the original flow, and inspect the affected state and its transition at a relevant
terminal size. For rendering or interaction changes, include the failure, cancellation, narrow-terminal, or recovery
state implicated by the evidence. A successful build does not verify the product experience. This live instance
verifies the harness fix; do not give it another implementation task or start a recursive orchestration cycle. Close
only the verification pane after capturing the result.

When the delegated change warrants review, use the second worker as a fresh read-only Luna/xhigh reviewer.
Ask it to check the feature and any harness fix together, prioritizing correctness and consequential usability
failures. The parent orchestrator owns all resulting edits and final verification.

## Finish

Close only the worker panes or tab created by this workflow once their output is captured and no follow-up
is needed. Summarize:

- the feature outcome and verification;
- the thndrs worker model(s) used;
- harness friction observed, including `none` when applicable;
- harness fixes made, focused checks, and the `cargo run` state exercised;
- confirmed issues left for user direction.
