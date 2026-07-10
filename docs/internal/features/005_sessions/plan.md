---
title: Sessions
status: Absorbed into baseline
captured: 2026-07-10
---

The initial sessions feature is a minimum usability requirement, not a later
feature. Its complete command, lookup, resume, inspection, export, redaction,
and log-reader contract now lives in
[`000_baseline`](../000_baseline/plan.md#session-use-and-inspection), with its
implementation ticket in
[`000_baseline/tasks.md`](../000_baseline/tasks.md#ticket-6-make-sessions-usable).

Future session ideas begin only after the baseline release gate and need a new
feature plan; they must preserve append-only JSONL, derived/rebuildable indices,
exclusive resume locking, renderer-independent inspection, and side-effect-free
replay.
