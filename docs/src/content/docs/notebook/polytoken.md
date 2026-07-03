---
title: "Polytoken Notes"
description: "Notes on Polytoken's local-first coding-agent daemon, sessions, facets, prompt input, work management, and permissions."
sources:
  - https://docs.polytoken.dev
author: Polytoken maintainers
captured: 2026-07-03
tags: [polytoken, coding-agent, tui, sessions, permissions, facets]
---

## Summary

Polytoken presents a local-first coding-agent system where a daemon owns durable
work state and the TUI exposes prompt input, session navigation, permissions,
facets, todos, jobs, and project instructions.

## Key Ideas

- Polytoken is a local-first AI coding-agent daemon. It manages model
  conversations, runs tools, and exposes a CLI/TUI interface.
- Project context comes from `AGENTS.md`. Polytoken reads the file from global
  and project scopes, falls back to `CLAUDE.md` and `GEMINI.md`, and supports
  `@path/to/file.md` references.
- `AGENTS.md` can stay plain Markdown. If the file has `polytoken: true`
  frontmatter, Polytoken renders the body as a template before sending it to the
  model.
- The prompt is a multiline text area with mouse placement, selection/copy,
  local cut/paste behavior, queued prompts while work is running, `@`
  references, `/` commands, and a shared typeahead bar.
- Facets change prompt framing and available tools without changing the model.
  The shipped facets include `execute` and `plan`.
- Plan mode uses the plan facet. It withholds edit and shell tools while the
  model investigates and writes a handoff plan, then asks the user how to
  continue.
- A session is the durable event record on disk. Context is the bounded subset
  of that record sent to the model for a turn.
- `/compact` writes a summary event and lets later turns use the summary instead
  of the full transcript. `/clear` resets model context while keeping session
  events on disk.
- `/rewind` moves to an earlier prompt boundary and drops later events.
  Polytoken avoids rewinding into the middle of a tool call.
- The daemon/TUI split lets users detach and reattach. Closing the TUI does not
  necessarily stop the daemon.
- Larger work is represented in the UI with sidebar state, flagged files,
  todos, and jobs. Jobs can be working, completed, failed, or cancelled.
- Permissions are checked at tool-call time. Polytoken has approval prompts,
  permission modes, file path approvals, shell-command parsing, and secret
  guards.

## Claims & Evidence

| Claim                                                                     | Support                                                                                                                                | Confidence |
| ------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| Polytoken separates durable work from the interface.                      | The introduction calls Polytoken a local-first daemon; the sessions page describes daemon/TUI detaching and reattaching.               | High       |
| Project instructions are based on the cross-agent `AGENTS.md` convention. | The project-context page describes global, project, parent, and fallback instruction files.                                            | High       |
| Facets alter prompt framing and tool access.                              | The prompting and facets pages describe plan/execute facets and custom facet files.                                                    | High       |
| Sessions and model context are separate concepts.                         | The sessions page defines session events on disk and bounded context for the model.                                                    | High       |
| Polytoken treats prompt input as a full application surface.              | The prompting page covers multiline input, mouse placement, queues, references, commands, typeahead, and conversation card navigation. | High       |
| Permission checks combine static rules and interactive approval.          | The permissions page covers approval prompts, modes, path approvals, shell parsing, and secret protections.                            | High       |

## Important Terms

| Term          | Meaning                                                                                |
| ------------- | -------------------------------------------------------------------------------------- |
| Daemon        | The process that owns the active session and work.                                     |
| TUI           | The terminal interface used to read, type, approve, navigate, and detach.              |
| `AGENTS.md`   | Project instruction file loaded into model context.                                    |
| Facet         | Markdown-defined persona that controls prompt framing, tool access, and display color. |
| Plan facet    | Read-only planning facet that writes a handoff plan.                                   |
| Execute facet | Working facet with implementation tools enabled.                                       |
| Session       | Durable ordered event record stored on disk.                                           |
| Context       | Events selected from the session and sent to the model.                                |
| Compaction    | Summary event that reduces the context needed for later turns.                         |
| Clear         | Context reset recorded in the session.                                                 |
| Rewind        | Destructive move to an earlier prompt boundary.                                        |
| Todo          | Model-visible task item with status and dependencies.                                  |
| Job           | Background work tracked outside the foreground prompt/reply loop.                      |

## Review Questions

- How does Polytoken distinguish session history from model context?
- What changes when a user switches facets?
- What does plan mode withhold from the model, and why?
- How does Polytoken handle prompts submitted while work is already running?
- What information can `AGENTS.md` contribute to each turn?
- How do file path approvals and shell-command checks differ?
- Why does detach/reattach require a daemon-owned session?

## Connections

- Related ideas: durable event logs, bounded model context, project instruction
  files, prompt queues, plan/execute modes, approval policies, background jobs.
- Related sources: local notes on sessions, prompts, skills, UI patterns, and
  prompt-renderer ownership.
- Tensions: daemon ownership improves continuity but adds process management;
  facets make modes explicit but can multiply configuration; rewinding is useful
  because it is destructive and bounded.

## Open Questions

- How are session events serialized on disk?
- How are queued prompts ordered relative to tool approvals and background jobs?
- Which permission decisions become durable session events?
- How much of the sidebar state is source-of-truth data versus derived display?
- How does Polytoken test terminal behavior across narrow widths and detach
  boundaries?

## Takeaways

- Polytoken documents sessions as durable event records and context as a bounded
  model input.
- Facets are a concrete mechanism for separating planning from execution.
- The TUI exposes more than chat: prompt editing, typeahead, cards, todos,
  jobs, permissions, and detach/reattach all appear as first-class workflows.
