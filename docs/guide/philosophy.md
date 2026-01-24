---
outline: deep
---

# Philosophy

Thunderus is not a chat UI. It is a harness: a workbench that makes agentic work
predictable, inspectable, and safe to execute inside real repositories.

## Design Principles

### 1) Harness > Chat

Thunderus is built around an interactive TUI that foregrounds plans, diffs, and
approvals instead of a stream of chat messages. The TUI is the core product.

### 2) Shell-First, With Guardrails

Anything you can do in the shell is valid, but every command is gated by approval
modes and sandbox policies. This keeps the system flexible without being unsafe.

### 3) Mixed-Initiative Collaboration

You and the agent share control. If you type, the agent pauses. If you edit, it
reconciles the new state before continuing. This prevents the agent from acting
on stale assumptions.

### 4) Diff-First Editing

All edits are surfaced as diffs before they land in your repo. You can accept,
reject, or modify changes in a controlled loop.

### 5) Teaching Through Transparency

Thunderus exposes "why" and "how" via trajectories, inspector views, and event
logs. The goal is not just correctness, but understanding.
