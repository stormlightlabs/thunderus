---
outline: deep
---

# TUI Overview

Thunderus ships a set of TUI components (header, sidebar, transcript, footer) and
an event system for input handling. The TUI runs from the CLI today, while some
panels and advanced flows are still evolving.

## Core Areas (Planned)

- **Header**: Session identity, status, and mode indicators.
- **Sidebar**: Memory hits, plan steps, and navigation sections.
- **Transcript**: The main conversation and action log.
- **Footer**: Input box, hints, and approval prompts.

## Approval Flow

Approval prompts are the guardrail for all actions that touch the filesystem,
network, or shell. The UI surfaces the action, the risk class, and the current
approval mode before anything runs.

## Planned: Inspector View

The inspector is designed to explain "why the agent believes X" by linking memory
entries to evidence in the session log and diffs. The view exists in the UI state
model, but the end-to-end integration is still in progress.
