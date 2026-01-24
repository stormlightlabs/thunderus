---
outline: deep
---

# TUI Overview

Thunderus ships a set of TUI components (header, sidebar, transcript, footer) and
an event system for input handling. The TUI runs from the CLI and supports
approvals, tool execution, and diff review in the main loop.

## Core Areas

- **Header**: Session identity, status, and mode indicators.
- **Sidebar**: Memory hits, plan steps, and navigation sections.
- **Transcript**: The main conversation and action log.
- **Footer**: Input box, hints, and approval prompts.

## Approval Flow

Approval prompts are the guardrail for all actions that touch the filesystem,
network, or shell. The UI surfaces the action, the risk class, and the current
approval mode before anything runs.

## Inspector View

The inspector is designed to explain "why the agent believes X" by linking memory
entries to evidence in the session log and diffs. Evidence detail depends on what
has been captured in the session history.
