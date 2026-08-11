# Session Context

Session forking starts only at a replayable settled turn and records lineage.
Exports produce deterministic human-review artifacts from semantic session
events. Neither operation copies live runtime state.

Context inspection presents provider-neutral request projections rather than
raw wire payloads. It shows origin, lifecycle, omission, size, budgets, and
compaction. Historical capture remains content-free by default; storing
normalized request projections requires a separate opt-in privacy and
retention decision.
