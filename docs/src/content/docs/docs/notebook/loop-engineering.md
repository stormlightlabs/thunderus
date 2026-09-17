---
title: "Loop Engineering"
Author: Addy Osmani, O'Reilly Radar, Brittany Ellich, Anthropic engineering
Date: 2026-06-07
Captured: 2026-09-16
Tags: [loop-engineering, orchestration, subagents, automation, verification, worktrees, external-state]
Sources:
  - https://addyosmani.com/blog/loop-engineering/
  - https://www.oreilly.com/radar/loop-engineering/
  - https://brittany-ellich.offprint.app/a/3mrjj34puva23-108-prs-in-eight-days-accidentally-discovering-loop-engineering
  - https://brittany-ellich.offprint.app/a/3mrpzcvrhke23-ai-as-a-tool-for-the-mental-load
  - https://www.anthropic.com/engineering/multi-agent-research-system
  - https://claude.com/blog/multi-agent-coordination-patterns
---

## Summary

Loop engineering is the practice of designing the control system that prompts,
verifies, and stops a coding agent, instead of being the person who types the
next prompt. Addy Osmani named the pattern in June 2026 and Peter Steinberger
compressed it into a sentence: stop prompting agents and start designing the
loops that prompt them. The leverage moved from prompt phrasing to loop
architecture: what triggers a run, which agents run in what topology, who checks
the result, and what condition ends the loop.

A loop does not make an agent smarter. It removes the human from the position of
scheduler, context carrier, and clipboard, and replaces that human with durable
artifacts: a board, a state file, a skill, a verifier, and a stop rule.

## Key Ideas

- **Trigger, topology, verifier, stop rule:** Osmani's essay frames four
  questions for any autonomous run. What fires it, which agents run in what
  shape, who checks the output, and what makes it quit. Prompt text is a detail
  inside that structure.
- **Five components plus external state:** The essay's inventory is automations
  (scheduled triggers), worktrees (isolation), skills (codified project
  knowledge in `SKILL.md`), plugins and connectors (MCP access to trackers and
  APIs), and subagents (independent verification). The sixth, added explicitly,
  is external state: the agent forgets, the repo does not.
- **Maker and checker are different agents:** An agent grading its own work
  inherits its own blind spots. The recommended topology uses a different agent,
  often a different model or instruction set, to verify against tests and
  skills. Anthropic describes the same orchestrator-worker separation for
  research agents.
- **Three layers, one authority each:** Ellich's implementation splits the
  system into a protocol (a markdown rulebook of statuses, ordering, and merge
  criteria), a loop (reads the board, runs `gh`, dispatches work, writes no
  code), and workers (write code in isolated worktrees, cannot touch the board
  or the user). No layer can reach into another.

Ellich started with a single markdown task table and abandoned it within a day,
because concurrent agents collided on the same file. One file per task, with
frontmatter for status, removed the contention. The same reasoning favors GitHub
issues over a shared checklist.

Stop rules have to be written down. "All tests pass and lint is clean" is a stop
rule. "Until it looks done" is not. Claude Code's `/goal` runs until a verifiable
condition holds, and self-paced `/loop` runs without an interval and terminates
when no actionable work remains.

Her loop also keeps a learnable-facts file with confirmation counts. A recurring
problem that crosses a threshold stops being a note and becomes its own task.
Without that cap, the memory file grows into a second unread instruction manual.

- **Review changed shape rather than disappearing:** She dropped line-by-line PR
  review in favor of outcome-level verification, citing a team of two or three
  with high trust, and batched releases once or twice a day instead of shipping
  continuously. The 108 pull requests in eight days, against a baseline of 5-10
  per week, measures throughput rather than quality; production changes still
  averaged about two per day.
- **The bottleneck relocates:** With review automated, the human constrains spec
  definition and verification instead. Ellich describes her role shifting toward
  product management and QA. Osmani states the same risk differently:
  comprehension debt grows as generated code outruns understanding.

Ellich's earlier essay on household mental load is the root of the engineering
one. The burden she describes is not doing tasks. It is noticing, anticipating,
and remembering what needs doing. A loop takes over noticing and remembering
while the human keeps preferences, decisions, and approval. That division is the
part that transfers to software development.

## Claims & Evidence

| Claim                                                                   | Support                                                                                                                            | Caveat / Confidence                                                                                                                  |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| Loop design has replaced prompt phrasing as the main lever.             | Osmani's essay and its O'Reilly republication; the framing spread quickly through practitioner writing in mid-2026.                | Medium-high. It is a widely adopted practitioner claim, not a measured result.                                                       |
| A layered protocol/loop/worker split prevents agents from corrupting shared state. | Ellich's loop never modifies a working tree and workers never modify the board; the separation was adopted after failures.         | High as a reported design lesson from one small team.                                                                                |
| One task per file removes board write contention.                      | The single-table board was abandoned after one day due to collisions; per-task files with frontmatter replaced it.                 | High for concurrent agents; a single-writer loop would not hit this.                                                                  |
| Autonomous loops can raise PR throughput by an order of magnitude.      | 108 PRs in eight days versus a stated baseline of 5-10 per week.                                                                   | Medium. PR count is an activity metric; the same period produced roughly two production changes per day.                              |
| Separate verifier agents catch what self-grading misses.                | Osmani prescribes maker/checker separation; research on self-evaluation finds models rate their own output higher than independent judges do. | High for the direction; separate verification reduces but does not eliminate shared-model blind spots.                        |
| Dropping line-by-line review is viable at small scale with high trust.  | Ellich removed mandatory review and batched releases, citing a 2-3 person team.                                                    | Low-to-medium transferability. The essay presents this as a local tradeoff, not a general recommendation.                             |
| External state is required for loops that outlive a context window.     | Osmani names external state as the sixth component; state files and boards carry progress between runs.                            | High. Model context resets between runs regardless of harness.                                                                       |
| Human verification remains mandatory.                                   | Osmani: unattended loops make unattended mistakes, and shipping code you confirmed works is still the job.                         | High, and stated as the essay's central warning rather than a footnote.                                                              |

## Loop Anatomy

A loop is legible when each of these four is a separate, named artifact rather
than an implicit habit.

| Element   | Question it answers        | Typical implementation                                                                             |
| --------- | -------------------------- | -------------------------------------------------------------------------------------------------- |
| Trigger   | What starts a run?         | Schedule, CI failure, issue label change, webhook, or a self-paced session loop with no interval.  |
| Topology  | Which agents, in what shape? | Orchestrator plus workers; explorer, implementer, verifier chains; fan-out across isolated trees. |
| Verifier  | Who decides it worked?     | A second agent with different instructions, plus mechanical gates: tests, lint, type checks.       |
| Stop rule | When does it quit?         | An explicit, checkable predicate, plus a token or iteration budget and an escalation path.         |

The canonical daily shape described in both the essay and Ellich's account:
triage runs on a schedule, reads yesterday's failures and open work, writes
findings to durable state, opens an isolated worktree per actionable item,
dispatches a drafting agent and then a reviewing agent, opens a pull request
through a connector, and routes anything it could not resolve to a human inbox
rather than dumping raw output.

## Orchestration Patterns

| Pattern                  | Shape                                                               | Use when                                                                                       |
| ------------------------ | ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Single agent             | One context, sequential turns.                                      | The task fits one context window and needs continuity more than parallelism.                   |
| Orchestrator-worker      | A lead decomposes, workers run isolated, the lead aggregates.        | Work splits cleanly and workers do not need to talk to each other.                             |
| Maker-checker            | One agent produces, a second with different instructions verifies.   | Output correctness is not mechanically decidable by tests alone.                                |
| Pipeline                 | Explorer to implementer to verifier, each with its own model tier.   | Cost matters and early stages are cheap to run wide.                                            |
| Board-driven loop        | A protocol layer dispatches from durable external state.             | Work outlives a session and multiple runs must not collide.                                     |

Anthropic's multi-agent research write-up supplies the constraint that makes
orchestrator-worker safe: workers do not communicate with each other, each
receives a self-contained task description and output format, and the
orchestrator sees results rather than intermediate reasoning. Context isolation
is the point. The lead keeps a high-level view while each worker carries its own
local load.

## Bounded Loops

The published accounts describe loops whose trigger is a schedule and whose stop
rule is a predicate: run every morning, keep going until no actionable work
remains. That shape maximizes throughput and it is also the shape that produces
the failure modes the same authors warn about. Unattended mistakes multiply
because the run is unattended. Cognitive surrender is available because the
human is not in the path.

A bounded loop keeps the structure and removes the schedule. The trigger is a
human starting a named unit of work. The scope is that unit and nothing else.
The stop rule is structural rather than predicate-based: the loop ends when
every child of the unit has reached a terminal state.

| Property   | Unattended loop                                   | Bounded loop                                                       |
| ---------- | ------------------------------------------------- | -------------------------------------------------------------------- |
| Trigger    | Schedule, webhook, or self-pacing.                | Explicit human invocation on one work unit.                         |
| Scope      | Whatever triage finds.                            | The unit and its declared children.                                 |
| Stop rule  | A predicate over repository state.                | All children terminal, or budget exhausted, or an escalation fires. |
| Discovery  | The loop enqueues new work for itself.            | The loop files new work and does not execute it this run.           |
| Resumption | Next scheduled fire.                              | The human starts the next run.                                      |
| Failure    | Compounds until someone notices.                  | Bounded by the unit; the run reports and stops.                     |

The trade is real and should be stated plainly. A bounded loop cannot produce
the throughput numbers in the source accounts, because a human gates every run.
What it buys is that the human stays in the position the same accounts say the
human must occupy anyway: defining intent and confirming outcomes. It removes
the scheduler from the human's job without removing the human from the loop.

Two properties make the bounded shape work in practice:

- **Scope containment.** A worker that discovers adjacent work files it as a new
  item and does not act on it. Self-expanding scope is the mechanism by which a
  bounded run becomes an unbounded one.
- **Externalized run state.** If the unit of work *is* the external state, a run
  is resumable by construction. Killing the session loses nothing, because
  nothing that matters lived in the session.

The second property is what makes a work-tracking item, rather than a state
file, the natural unit. A state file has to be written by the loop and can
diverge from reality. A tracked item with a status field is already the record.

## Failure Modes

- **Unattended mistake multiplication:** An unsupervised loop that is wrong is
  wrong repeatedly and in parallel. Verifier agents reduce the rate; they do not
  bound the blast radius. Branch protection, tests, and batched releases do.
- **Comprehension debt:** Generation speed and understanding move in opposite
  directions. The cost is deferred, not avoided, and it is paid during the next
  incident.
- **Cognitive surrender:** The failure is accepting outputs without judgment
  because the loop is comfortable. Osmani's closing advice is to build the loop
  as someone who intends to remain the engineer rather than the person who
  presses go.
- **Self-graded success:** An agent that writes both the change and its test can
  satisfy the test by weakening it. Verification that the agent can edit is not
  verification.
- **Activity mistaken for progress:** PR count, task count, and loop iterations
  are cheap to inflate. Merged-to-`edge` and shipped-to-release counts are not.
- **Board rot:** Statuses that no run ever clears accumulate until the board
  stops describing reality. The protocol needs a rule for stale and abandoned
  items, not only for new ones.

## Important Terms

| Term            | Meaning                                                                                                     |
| --------------- | ------------------------------------------------------------------------------------------------------------- |
| Loop            | The control system that triggers, dispatches, verifies, and stops agent runs without per-turn human prompting. |
| Protocol        | The written rulebook a loop obeys: statuses, ordering, ownership, merge criteria, and stop conditions.        |
| Worker          | An agent that produces work in isolation and reports a structured result; it cannot modify the board.         |
| External state  | Durable storage outside any context window that carries progress between runs.                                |
| Stop rule       | An explicit, checkable predicate that ends a loop, paired with a budget and an escalation path.               |
| Maker/checker   | Producing and verifying assigned to different agents to avoid shared blind spots.                             |
| Comprehension debt | The growing gap between the code that exists and the code the humans understand.                           |
| Triage inbox    | The human-facing queue a loop routes anything it could not resolve into.                                       |

## Questions for Review

- Which parts of this project's current work actually have a checkable stop
  rule, and which are only ever "done" by human judgment?
- What is the smallest external state that would let a run resume after a
  context reset without re-deriving everything?
- Where does maker/checker separation earn its token cost here, and where do
  mechanical gates already cover the risk?
- What is the blast radius of a wrong autonomous change in this repository, and
  which gate bounds it?
- How would a stale or abandoned task be detected, and by whom?
- If review becomes outcome-level rather than line-level, what evidence must a
  pull request carry to make that judgment possible?

## Takeaways

- Name the trigger, topology, verifier, and stop rule as separate artifacts. A
  loop that leaves any of the four implicit is a human scheduler with extra
  steps.
- Separate authority by layer: the layer that plans does not write code, and the
  layer that writes code does not update the board.
- Keep run state in files the repository owns. Context windows do not persist
  across runs.
- A bounded loop gives up the throughput in these accounts and keeps the human
  in the path. Where the unit of work is itself durable external state, a run is
  resumable and the session holds nothing worth keeping.
- Related ideas: [harness-engineering](/docs/notebook/harness-engineering/),
  [worktrees](/docs/notebook/worktrees/),
  [agent-task-boards](/docs/notebook/agent-task-boards/),
  [skills](/docs/notebook/skills/), [agents-md](/docs/notebook/agents-md/),
  [context-control](/docs/notebook/context-control/),
  [herdr](/docs/notebook/herdr/).
