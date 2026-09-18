---
name: lndrs
last_updated: 2026-09-18
id: 01M2S3GAM20S0VNJ4A10MBBWR0
---

# Lndrs

Lndrs is an orchestration engine. It reads ready work from Arc Lightning,
dispatches each unit to a worker it supervises, records what the worker settled,
and writes the result back. It has a control protocol, and its interface is a
client of that protocol rather than the program itself.

The idea this comes from is `01M2RY8QAPD62KMAMTPPKT9HZZ`, in
`../../ideas/lndrs-orchestration.md`. It decided that the engine precedes the
interface, that lndrs owns work rather than panes, that workers are driven over
ACP where they speak it, that Herdr is a process backend rather than a rival,
and that the work graph is Arc Lightning. It left four questions open: how lndrs
reaches Arc Lightning, which orchestration verbs ACP carries, what isolation
lndrs owns, and what starts a run. This document answers those and fixes the
vocabulary the issues use.

## Three state models

Three state machines meet in a run, and collapsing any two of them is the
mistake this section exists to prevent. They answer different questions and
change at different times.

| Model              | Owner        | Question it answers          |
| ------------------ | ------------ | ---------------------------- |
| `TaskStatus`       | Arc Lightning | What is true of the work?    |
| `InstanceLifecycle`| `thndrs-agent` | What is the worker doing?   |
| Run stage          | Lndrs        | Where in the run is this unit? |

Arc Lightning holds `pending`, `in_progress`, `parked`, `completed`, and
`cancelled`, moved by `start`, `park`, `unpark`, `complete`, and `cancel`. That
is the durable record of the work and it outlives every run.

`thndrs-agent` holds `starting`, `ready`, `running`, `waiting_permission`,
`stopping`, `completed`, `failed`, and `cancelled`, with a checked `transition`.
That is one supervised process and it dies with the worker.

A unit can be `in_progress` in Arc Lightning while its worker is `running`,
then still `in_progress` while no worker exists at all, because the run moved
on to review. The two are not views of one value.

### Run stages are not task statuses

Thunderstorm's `status:claimed`, `status:review`, and `status:verify` describe
where a unit sits inside a run. They are not properties of the work, and Arc
Lightning should not learn them. It is a general planning tool, and teaching it
one repository's review sequence makes that sequence a schema migration.

Lndrs owns run stages and keeps them in the run record. Arc Lightning sees
`start` when a unit is claimed and `complete` or `park` when it settles.
Everything between is the run's business.

The board projection derives from both. A unit that is `in_progress` in Arc
Lightning with a run stage of `review` projects to `status:review`. Nothing
stores the projection.

### The vocabulary issues use

A **unit** is one Arc Lightning task that a run dispatches. A **run** is one
pass over a set of units, with a budget and a stop rule. A **worker** is one
supervised instance working one unit. A **settlement** is the typed result a
worker returns, which is `SettledInstanceResult` and nothing else. Issues use
these five words and do not introduce synonyms for them.

## Reaching Arc Lightning

Lndrs calls the `arcl` binary and parses its JSON. It does not link
`arcl-core`, and it does not wait for an MCP server.

The reason is where the operations live today. `arcl-core` holds
`domain` and `plan` only, described in its own module docs as "domain records
and transport-independent application inputs". The operations that answer a
query or move a task sit in `crates/arcl-cli/src/app/`, and `crates/` carries
no `arcl-mcp`. Linking `arcl-core` would import the vocabulary and leave lndrs
to reimplement every operation against `arcl-store`, which is the same as
forking the tool.

The CLI is a supported machine interface rather than scraped output. Read
commands emit `Envelope<T> { format_version: u8, data: T }`, so the payload is
versioned and lndrs can refuse a `format_version` it does not know. Three
commands carry the dispatch loop: `arcl ready` lists actionable leaf tasks in
deterministic order, `arcl next` returns the first one, and `arcl context`
returns the bounded packet for a task.

This rules out two things. Lndrs does not compute readiness; when a unit looks
ready and Arc Lightning disagrees, Arc Lightning is right. And lndrs does not
reach into `.arcl/arcl.db`, so a schema change is not a lndrs bug.

The cost is a process per call. That is acceptable at the rate a run dispatches
and it is not acceptable inside a render loop, so the interface reads the run
record rather than polling `arcl`. If the cost becomes real, `arcl-mcp` is the
replacement and the seam was chosen so that swapping it changes one module.

## What ACP carries

ACP carries worker identity. Lndrs carries the graph.

This follows Zed, which is the only ACP client with a published answer. It
transmits subagent information through message metadata under
`SUBAGENT_SESSION_INFO_META_KEY`, handing each child its parent's `SessionId`
and a depth counter, and keeps depth policy inside the client, where
`MAX_SUBAGENT_DEPTH` defaults to 1. Identity travels; policy does not.

Lndrs attaches one `_meta` object to `session/new`, holding the run identifier,
the unit identifier, and the delegation depth. A worker that understands it can
report against the right unit. A worker that ignores it behaves normally, which
is what makes an unmodified third-party agent usable.

Everything else stays out of the protocol. Budgets are `InstanceBounds` and
`DelegationBudget`, enforced by the supervisor that owns the child process, not
requested of the child. A worker cannot raise its own ceiling by declining to
implement a field.

### The verbs ACP does not have

ACP is session-scoped and a run outlives its sessions, so four operations have
no ACP expression: list the runs, read a run's units and their stages, cancel a
run rather than a session, and subscribe to run events.

These belong to the lndrs control protocol, which is a separate surface from
ACP. It is newline-delimited JSON over a Unix socket or named pipe, addressed
by dotted method names, in the shape Herdr already proved: a request carrying
an id, a method, and params, answered by that id and a result. Lndrs is an ACP
client downward and a control
server upward, and the two do not share a vocabulary.

One property of Herdr's design is worth copying and one is worth fixing.
Subscriptions there begin when the request is accepted and do not replay what
came before, so a client that attaches late cannot reconstruct the run. Lndrs
events carry a monotonic sequence number per run and a subscriber names the
sequence it has seen, because the interface must be able to attach to a run
already in progress and show what happened.

### Why the seam survives a repository move

Arc Lightning, Mire, and Stormbuffer are separate repositories today and may not
stay that way. This decision holds either way, which is most of why it is the
right one.

A monorepo would not make linking possible. The obstacle is Arc Lightning's own
layering, where the operations sit in `arcl-cli` and `arcl-core` holds records,
and its roadmap schedules the extraction as a later milestone. Moving the
directory does not move the code, so lndrs would shell out from inside one
repository exactly as it does from outside.

Calling a sibling tool through versioned JSON is also how this family already
composes. Arc Lightning emits a versioned envelope, Mire exchanges bounded
context and findings with local agents as JSON, and Stormbuffer answers
`sbuf context` under a byte budget. Lndrs is one more caller of that pattern.

### Work that lands in Arc Lightning

Lndrs is the first program to drive Arc Lightning as a work queue, and four
things it needs are missing. Each is a property of the work record rather than
of a run, so each is a change to Arc Lightning rather than something lndrs
works around.

The issues are filed in this repository with the rest of the track, because one
board holding one run is the point and a second board is a second thing to
reconcile. What differs is where the change lands. An issue below is worked in
a checkout of `stormlightlabs/arclightning`, opens a pull request against that
repository, and is reviewed by its conventions, while its thunderstorm status
label stays on the issue here. Anyone cutting these issues marks them so a
worker knows which tree to open before it starts.

Shipping is the other difference. Lndrs consumes these through the installed
`arcl` binary, so a merged change is not a usable one. Lndrs declares the
minimum `arcl` version it requires and the `format_version` it parses, and
refuses to start against an older pair with a message naming both. An issue
here is done when Arc Lightning has released the change, not when its pull
request merges.

Claiming a unit has to be one operation. `arcl next` returns a task and
`arcl task start` moves it, so two runs can read the same task before either
moves it and both dispatch it. The fix is one call that returns a unit and
claims it in the same transaction. Arc Lightning already takes this shape
elsewhere: `arcl task handoff` is documented as leaving a resume note and
parking the task atomically.

A claim needs an owner and an expiry. `PlanningTask` carries no owner,
assignee, or lease field, so `in_progress` cannot say who holds the unit or
whether that holder is still alive. Thunderstorm's rule that a claim older than
24 hours returns to the queue needs both. Ownership belongs to the work, not
the run, because the case that matters is a run that died.

Settlement should keep its structure. `handoff` and `evidence` are `String`,
and a worker settles with `SettledInstanceResult`, whose `SemanticEvidence` and
`ChangedPath` are typed. Writing them as prose loses what a later run could
check. Arc Lightning should accept a structured settlement beside the Markdown,
keeping the Markdown as the readable form it already is.

The interface needs a change feed. There is no watch or subscribe command,
so a client learns the graph changed by running `arcl ready` again. That is a
process per tick. A feed of record changes lets the interface follow a run
without polling.

Building against the CLI is what surfaces requirements like these, which is an
argument for the seam rather than against it. Extracting operations into
`arcl-core` before a consumer exists would extract the wrong ones.

## The provisioning hook

Worktrees are settled by the `worktree` skill and cover source. Ports,
per-worker databases, and environment variables pointing at shared resources
are not covered, and lndrs does not decide them. They are project-specific, and
a policy guessed here is a policy to work around.

Lndrs offers one hook and no policy. It runs after the worktree exists and
before the worker starts, because that is the only moment when the working
directory is real and nothing has used it yet. A single point is deliberate:
two hooks invite an ordering question that every project answers differently.

The hook is an executable named in configuration. It receives the run
identifier, the unit identifier, and the worktree path in the environment, and
it writes environment assignments to stdout, which lndrs merges into the
worker's environment. A project that needs a free port allocates one and prints
it. A project that needs a database creates one and prints its URL. A project
that needs neither configures no hook and pays nothing.

A non-zero exit fails the unit before the worker starts, and the unit is
`parked` in Arc Lightning with the hook's stderr as the reason. A failure here
is a provisioning failure, not a work failure, and the run must not report it
as a worker that could not do the job.

Lndrs has no teardown hook in this design. Releasing a port or dropping a
database is the same problem as removing a worktree, which the `worktree` skill
owns, and splitting cleanup across two owners is how a cleanup gets skipped.

## What starts a run

A run has a trigger, and a human invocation is one trigger rather than the only
one. The bounded loop in `loop-engineering` remains the default and the
documented shape. An unattended trigger is supported, and it carries three
requirements the attended path does not.

A budget is mandatory. An attended run can be stopped by the person watching
it; an unattended run has no such stop, so a run with no budget is refused
rather than started with a default.

An escalation target is a person. A unit that blocks reaches a named human
through a configured channel. A run that escalates into a log nobody reads is
an unattended run that failed quietly, which is the failure mode the bounded
argument exists to prevent.

An unresolved previous run blocks the next one. If the last run over the same
unit set left anything `parked` or left a worker unsettled, the trigger refuses
and says what is outstanding. This is what keeps an unattended loop from
compounding a mistake across firings.

These three are requirements of the trigger, not of the run, so the same run
mechanics serve both paths and only the entry point differs.

## What this does not cover

The interface. The idea defers it until the capture loop in
`../tui-verification/plan.md` can show a reviewer a frame, and nothing here
changes that. The control protocol is specified so the interface has something
to be a client of, and the interface is a separate track with a separate spec.

The Herdr backend. The idea decided that process backends sit behind a trait
with an embedded PTY and a Herdr implementation, and that the choice between
them is measurable. Measuring it is work this spec does not schedule, and the
first engine can supervise ACP workers with no pane rendering at all.

The GitHub projection. Arc Lightning's repository-native mode already defines a
three-state reconciliation that refuses to resolve a conflict by picking a side,
and projecting a run onto issues should reuse it rather than invent a second
sync. That is its own decision and it is not made here.

Review and memory. Mire covers review, holding anchored findings and exchanging
them with agents as JSON, and Stormbuffer covers retrieval under a budget. Both
are callers a run will want and neither is designed in here. The run record and
the control protocol are specified to leave room for them, which is as far as
this document goes.

Arc Lightning's own roadmap. `arcl-mcp`, the desktop app, and the migration to
the capture and spec vocabulary are that project's work and this track does not
schedule them. The four changes above are the exception, and they are tracked
here because lndrs is what blocks on them. The atomic claim blocks first: until
it exists, a run dispatches one unit at a time.
