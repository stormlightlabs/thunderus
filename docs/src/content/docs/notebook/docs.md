---
title: "Documentation Internals Notes"
Author: "Synthesized from public project docs"
Date: "2026-07-03"
Captured: "2026-07-03"
Tags: ["docs", "internals", "contributors"]
Sources:
  - https://rust-analyzer.github.io/book/contributing/architecture.html
  - https://rustc-dev-guide.rust-lang.org/overview.html
  - https://ratatui.rs/concepts/rendering/under-the-hood/
  - https://aider.chat/docs/repomap.html
  - https://opencode.ai/docs/
  - https://diataxis.fr/
---

## Summary

Good internals documentation gives readers a stable mental model and gives contributors a safe path into the codebase without turning the public docs into a source listing.

## Key Ideas

- **Separate reader concepts from contributor mechanics:** Public concepts should explain behavior and vocabulary; development pages should explain source ownership, invariants, and change workflows.
- **Document boundaries before modules:** The most useful internals docs explain which layer owns state, I/O, data conversion, testing, or user-visible behavior before listing files.
- **Use source maps as anchors, not content:** A code map is useful when it points readers to entry points, API boundaries, invariants, and tests.
- **Keep research notes below curated docs:** Notebook material can preserve investigation context, but stable conclusions should be promoted into concepts, reference, or development pages.
- **Prefer exact contracts in reference:** Schemas, commands, environment variables, config keys, and tool interfaces belong in reference pages rather than prose-heavy architecture docs.

## Claims & Evidence

| Claim | Support | Caveat / Confidence |
| --- | --- | --- |
| Architecture docs work best when they start with a high-level model. | rust-analyzer opens its architecture chapter with a bird's-eye view, entry points, and code map before drilling into modules. | High; this pattern maps well to thndrs because the app has clear boundaries: app state, prompt assembly, provider loop, tools, sessions, renderer. |
| Invariants are more valuable than file summaries. | rust-analyzer repeatedly calls out architecture invariants such as API boundaries, absent dependencies, and cancellation behavior. | High; thndrs should document what must stay true around tool containment, append-only sessions, provider ownership, and renderer row models. |
| Deep contributor books should be distinct from user docs. | The Rust compiler development guide is a contributor handbook, not a user manual. | High; thndrs is smaller, so this should be a Development section rather than a separate book. |
| TUI internals benefit from mechanism-level concept docs. | Ratatui's rendering docs explain the buffer/render/flush flow instead of only listing APIs. | High; thndrs can use the same style for direct inline rendering, live regions, transcript blocks, and snapshots. |
| AI coding tool docs often expose internal concepts when they affect outcomes. | Aider documents repository maps and edit formats because those internals shape user-visible agent behavior. | Medium; thndrs should do this for prompt assembly, project context, tool boundaries, sessions, and provider behavior. |
| A clear information architecture prevents docs from becoming a junk drawer. | Diataxis separates tutorials, how-to guides, reference, and explanation. | High; thndrs can keep Getting Started, Usage, Concepts, Reference, Development, Providers, and Notebook as separate jobs. |

## Important Terms

| Term | Meaning |
| --- | --- |
| Concept page | An explanatory page that teaches a stable mental model or behavior. |
| Development page | Contributor-facing documentation for architecture, invariants, source ownership, and change workflow. |
| Reference page | Exact, lookup-oriented documentation for schemas, commands, configuration, and APIs. |
| Notebook page | A research or synthesis note that may be useful later but is not yet a polished contract. |
| Invariant | A property the codebase intentionally preserves across changes. |
| Boundary | A point where ownership changes, such as app state to renderer, agent loop to provider, or tool dispatcher to filesystem. |

## Patterns From Similar Projects

| Project | Useful Pattern | Application to thndrs |
| --- | --- | --- |
| rust-analyzer | Bird's-eye view, entry points, code map, architecture invariants, cross-cutting concerns. | Expand Architecture around the turn lifecycle and document invariants per subsystem. |
| Rust compiler dev guide | Contributor-first handbook organized by subsystem and development task. | Keep thndrs contributor internals in Development, not mixed into Usage. |
| Ratatui | Mechanism-level concept docs for rendering internals. | Explain the row model, live region, backend boundary, wrapping, cursor placement, and snapshots. |
| Aider | User-facing docs for internals that affect agent results. | Keep prompt assembly, project context, tools, sessions, and provider behavior visible in Concepts. |
| OpenCode | Product docs organized around usage, config, tools, agents, SDK/server/plugin surfaces. | Keep usage docs operational and move exact config/tool details to Reference. |
| Diataxis | Separate explanation, how-to, reference, and learning paths. | Use Concepts for explanation, Usage for tasks, Reference for contracts, Development for contributors, Notebook for research. |

## Local Documentation Shape

thndrs should treat `docs/internal` as planning material outside the published site. The published docs should use these layers:

- **Getting Started:** orientation, install, quick start, manual.
- **Usage:** user tasks such as input, keybindings, project context, skills, tools, web search, models, sessions, and security.
- **Concepts:** stable explanations such as prompt assembly, prompt XML, transcript model, tool boundary, and TUI.
- **Providers:** provider-specific behavior and integration boundaries.
- **Reference:** exact CLI, config, environment, tool, and session contracts.
- **Development:** contributor knowledge hub for architecture, workflow, testing, invariants, and source maps.
- **Notebook:** research, comparisons, and synthesis notes that are useful but not yet contractual.

## Page Template For Internals

Use this structure for subsystem internals:

```md
## Why This Exists
## User-Visible Behavior
## Data Flow
## Code Map
## Invariants
## Extension Points
## Failure Modes / Security Notes
## Tests
## Related Reading
```

The template should stay lightweight. Pages should describe the decisions a contributor needs before changing code, not restate every exported symbol.

## Candidate Development Pages

- **Architecture:** one-turn lifecycle, state ownership, module map, cross-cutting invariants.
- **Prompt System:** prompt bundle fields, fragment ordering, self-knowledge snapshot, transcript projection, tests.
- **Agent Loop:** provider streaming, retries, cancellation, tool batches, continuation budgets.
- **Tool Boundary:** workspace containment, typed tools, write audit, shell command handling, truncation.
- **Renderer:** row model, transcript/live split, backend, width behavior, snapshots.
- **Sessions:** append-only JSONL, replayable records, audit metadata, schema evolution.
- **Providers:** provider trait, provider-specific lowering, stream formats, metadata loading.
- **Testing:** unit tests, fixtures, snapshots, no-network provider tests, narrow TUI states.

## Questions For Review

- What is the main job of internals documentation?
  - To teach stable system behavior and contributor-safe change paths, not to mirror source files.
- Where should exact schemas and commands live?
  - Reference pages.
- Where should rough research and investigation notes live?
  - Notebook pages, until a conclusion is stable enough to promote.
- What makes a code map useful?
  - It points to entry points, ownership boundaries, invariants, extension points, and tests.
- When should an internal concept be public?
  - When it changes how users understand behavior, debug outcomes, or use the tool safely.

## Connections

- Related ideas: Diataxis, architecture invariants, contributor handbooks, source maps, API boundaries.
- Related local docs: Architecture, Testing, Prompt Assembly, Tool Boundary, Transcript Model, Session Format.
- Contradictions or tensions: Too much internals detail can make docs stale; too little leaves contributors dependent on oral history.
- Useful applications: Use this note as the rubric when expanding Development pages or promoting Notebook research.

## Open Questions

- Which thndrs internals deserve their own Development pages versus sections inside Architecture?
- Should source code doc comments be generated into public reference pages later, or kept separate from the docs site?
- Which invariants should be enforced by tests before being documented as stable contracts?

## Takeaways

- Internals docs should explain boundaries, flow, and invariants first.
- Keep `docs/internal` private to the repo workflow and publish curated knowledge in the site.
- Promote stable notebook conclusions into Concepts, Reference, or Development instead of expanding the notebook forever.
