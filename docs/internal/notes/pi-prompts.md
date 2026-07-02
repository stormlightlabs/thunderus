---
Title: Pi Prompt Directory
Source: https://github.com/earendil-works/pi/tree/main/.pi/prompts
Author: Earendil Works / Pi contributors
Date: 2026-07-01
Captured: 2026-07-01
Tags: [pi, prompts, slash-commands, coding-agent]
---

## Summary

Pi's `.pi/prompts` directory is a compact set of procedural slash-command prompts for
maintainer workflows, written as explicit checklists with safety rules and expected output.

## Key Ideas

- **Prompts encode workflows, not broad personality:** Files such as `cl.md`, `is.md`, `pr.md`,
  `sa.md`, and `wr.md` describe specific maintainer jobs.
- **Frontmatter gives command metadata:** Each prompt starts with a description and, when useful,
  an argument hint.
- **Steps are concrete and ordered:** The prompts name commands, files, checks, and reporting
  sections rather than asking the model to improvise.
- **Analysis is separated from implementation:** The issue-analysis prompt explicitly says not to
  implement unless asked.
- **Verification beats trust:** Issue and advisory prompts tell the agent not to trust prior
  analysis without independently reading code and checking behavior.
- **GitHub operations are constrained:** PR, issue, advisory, and wrap prompts specify exact `gh`
  commands, state changes, labels, comments, and when to ask before applying.
- **Safety rules are workflow-local:** The security-advisory prompt bans PoC content, browser-cookie
  scraping, unauthorized publication, and CVE requests without explicit approval.

## Claims & Evidence

| Claim                                                      | Support                                                                                                            | Caveat / Confidence                            |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------- |
| Pi prompts are command-oriented.                           | The directory contains short command prompt files for changelog, issue, PR, security advisory, and wrap workflows. | High.                                          |
| Good procedural prompts reduce ambiguity.                  | Prompts list ordered steps, exact files, command examples, and final report structure.                             | High.                                          |
| Analysis and mutation boundaries should be explicit.       | `is.md` says analyze and propose only; `wr.md` handles changelog, commit, push, and issue closing.                 | High.                                          |
| Prompt-level safety should be close to the risky workflow. | `sa.md` places advisory-specific restrictions inside the advisory prompt.                                          | High.                                          |
| This pattern does not require a large base prompt.         | The command prompts carry specialized detail outside the general harness.                                          | Medium-high; depends on slash-command support. |

## Important Terms

| Term                 | Meaning                                                                                          |
| -------------------- | ------------------------------------------------------------------------------------------------ |
| Slash-command prompt | A reusable prompt file invoked for a named workflow.                                             |
| Frontmatter          | YAML metadata that describes the command and expected arguments.                                 |
| Wrap prompt          | Pi's end-to-end completion workflow for changelog, final comment, commit, push, and issue close. |
| Advisory workflow    | A security-publication prompt with strict GitHub and CVSS handling.                              |

## Questions for Review

- Which workflows are common enough to deserve command prompts later?
  - **Recommendation**: Promote only repeated, multi-step workflows with predictable inputs and outputs into command prompts.
- Should workflow-specific safety rules live in command prompts instead of the base fragment?
  - **Recommendation**: Put workflow-specific safety rules in command prompts and keep the base fragment limited to general behavior.
- How much GitHub workflow support belongs in the harness versus project-local
  prompt files?
  - **Recommendation**: Keep GitHub workflow details project-local until the harness has a stable built-in GitHub surface.

## Connections

- Related ideas: project context, prompt XML fragments, GitHub workflow automation.
- Related sources: [pi](pi.md), [prompts](prompts.md), [goose-prompts](goose-prompts.md).
- Tension: command prompts are useful, but prompt-command infrastructure should
  follow repeated workflows rather than one-off cleanup needs.
- Conceptual use: write prompt instructions as concrete steps when the workflow
  is known.

## Open Questions

- Should command prompts be repo-local files, built-in templates, or both?
  - **Recommendation**: Use procedural prompt files only for repeated workflows with clear inputs, ordered steps, safety rules, and expected output.
- What is the minimum metadata needed for a prompt command if this feature is added?
  - **Recommendation**: Require only a name, description, and optional argument hint until command routing needs richer metadata.

## Takeaways

- Put workflow-specific rules near the workflow.
- Keep procedural prompts concrete: inputs, steps, safety limits, and output shape.
- Do not implement command-prompt infrastructure until repeated workflows justify it.
