---
outline: deep
---

# Trajectories & Inspector

Trajectories capture "why" a decision was made: they link memory entries and conclusions
back to the evidence in the event log and diff history. The inspector view renders this
chain in the UI.

## What a Trajectory Provides

- A chronological trace of relevant events.
- Links to the patches or file changes that influenced a decision.
- A place to surface confidence and risk classification.

## Interactive Inspector

The inspector UI makes trajectories interactive: filtering by tool usage, jumping to
diffs, and drilling into the evidence chain. Depth of evidence depends on what is
captured in the session log.

For the underlying event log and flow details, see [Data Flow](/development/data-flow).
