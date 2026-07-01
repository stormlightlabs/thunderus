---
Title: Codex Prompt Source Directory
Source: https://github.com/openai/codex/tree/main/codex-rs/prompts/src
Author: OpenAI Codex contributors
Date: 2026-07-01
Captured: 2026-07-01
Tags: [codex, prompts, rust, permissions, review]
---

Codex's Rust prompt crate renders specialized prompt text from typed runtime state, separating
permissions, review, goals, compaction, realtime, and patch-tool instructions.

## Key Ideas

- **Prompt text is code-backed:** Rust modules expose constants and render functions while loading
  markdown/XML templates with `include_str!`.
- **Permissions are generated from real policy:** `permissions_instructions.rs` derives sandbox,
  network, writable-root, denied-read, approval, and approved-prefix text from structured config.
- **Review prompts are target-aware:** Review requests resolve to prompts for uncommitted changes,
  base-branch diffs, commits, or custom instructions.
- **Review output is strict:** The review rubric asks for prioritized, actionable findings and a
  fixed JSON schema.
- **Goals have continuation prompts:** Goal prompts render objective, tokens used, budget, and
  remaining budget into hidden continuation or wrap-up prompts.
- **Compaction is concise and operational:** The compact prompt asks for progress, decisions,
  constraints, remaining work, and references needed to resume.
- **XML is used where structure matters:** Review-exit templates and contextual permission fragments
  use XML-style tags and escape user-controlled content.

## Claims & Evidence

| Claim                                                    | Support                                                                                                                 | Caveat / Confidence                                               |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Runtime policy should generate prompt instructions.      | `PermissionsInstructions::from_permission_profile` renders sandbox and approval text from structured policies.          | High.                                                             |
| Prompt templates should be testable.                     | Codex keeps prompt renderers in Rust modules with unit tests for goals, permissions, review requests, and review exits. | High.                                                             |
| Review behavior benefits from a strict contract.         | The review rubric defines bug criteria, priority fields, code locations, and exact JSON output.                         | High.                                                             |
| XML/text escaping matters for generated fragments.       | Goal prompts escape XML text before rendering user objectives.                                                          | High.                                                             |
| `thndrs` should not copy Codex's full permission system. | `thndrs` currently has a simpler local workspace-contained tool model.                                                  | High; adding a subsystem would violate the simplicity constraint. |

## Important Terms

| Term                     | Meaning                                                                                     |
| ------------------------ | ------------------------------------------------------------------------------------------- |
| Permission profile       | Structured runtime policy for filesystem, network, approvals, and writable roots.           |
| Approval policy          | Rules for when commands or filesystem actions require user approval.                        |
| Review target            | The diff scope being reviewed: uncommitted changes, branch, commit, or custom instructions. |
| Template renderer        | Code that fills prompt templates from structured values.                                    |
| Contextual user fragment | A labeled prompt fragment injected with a specific role and markers.                        |

## Questions for Review

- Which `thndrs` prompt sections should eventually be generated from structured runtime policy?
- Should prompt XML escaping be centralized if more generated XML sections are added?
- Would a review command be useful enough to justify a strict review prompt?

## Connections

- Related ideas: prompt assembly, XML syntax docs, tool boundary, future review workflows.
- Related sources: [prompts](prompts.md), [claude-code-prompts-corpus](claude-code-prompts-corpus.md).
- Tension: Codex's prompt system is mature and policy-rich; `thndrs` should borrow the typed
  rendering discipline without adopting the whole surface.
- Useful application: keep generated prompt text tied to the runtime state that makes it true.

## Open Questions

- Should `thndrs` add tests that validate XML fragment shape beyond snapshot comparison?
- Should review prompt behavior be built in or left to user/project prompts?

## Takeaways

- Generate permission text from actual runtime policy when policy becomes dynamic.
- Keep prompt templates small, typed, and tested.
- Escape user-controlled content in XML-like prompt sections.
