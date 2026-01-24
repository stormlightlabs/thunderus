---
outline: deep
---

# Workflows

Thunderus emphasizes repeatable, reviewable workflows. The patterns below are
safe defaults.

## Review-First Editing

1. Ask for a change.
2. Review the proposed diff.
3. Accept or reject the patch.
4. Run tests (or request a test plan).

## Approval-Gated Commands

Shell commands are routed through the approval system. The intent is simple:
read-only commands should be easy to approve, while write or network access
requires explicit opt-in and a clear audit trail.

## Mixed-Initiative Sessions

If you edit files manually, the agent pauses and waits for reconciliation. This
protects you from the agent acting on outdated state.
