# Maintaining this skill

Update the skill when real use produces a reusable lesson. The goal is better future decisions, not a history of every correction.

## Evidence that merits an update

Update when one of these occurs:

- the user explicitly corrects hierarchy, spacing, color, chrome, copy, motion, or interaction behavior;
- the same review finding appears in more than one state or task;
- implementation reveals that a rule conflicts with the renderer or terminal behavior;
- a reference harness or primary technical source establishes a useful pattern that thndrs adopts;
- a rule repeatedly produces generic, over-engineered, or visually weak work.

Do not update for personal guesses, a single temporary workaround, incidental code cleanup, or dimensions copied from one screenshot.

## Update method

1. State the reusable lesson in one sentence.
2. Locate the narrowest owning section.
3. Replace or qualify stale guidance before adding a new rule.
4. Put workflow-critical guidance in `SKILL.md`; put visual, technical, or verification detail in the matching reference.
5. Search the skill for duplicate or conflicting advice and consolidate it.
6. Keep the main skill succinct. Split detail into a direct, one-level reference only when another task will need it.
7. Validate with `quick_validate.py` and check the edited Markdown for placeholders and broken relative links.
8. Mention the learned rule and edited file in the task handoff.

Do not maintain a changelog or feedback ledger. Git history already records revisions, and the active guidance should be readable without reconstructing its evolution.

## Resolve conflicts

Use this priority order:

1. explicit current user feedback;
2. current thndrs product behavior and architecture;
3. observed behavior in a real terminal;
4. current primary documentation for Ratatui, Crossterm, and reference harnesses;
5. general TUI heuristics.

When new evidence only applies to one theme, terminal, width, or interaction state, scope the rule to that condition instead of weakening a sound default.
