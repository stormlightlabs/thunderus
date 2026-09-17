---
name: rubber-duck
description: Think through a design, feature, or problem in conversation before any code or spec exists, and record the result as an idea file. Use for /r-d, /rubber-duck, ideation, design discussion, or working out an approach.
---

# Rubber duck

Design discussion. No implementation, no branch, no pull request.

## How to talk

Argue with the idea. Agreement that arrives without resistance is not useful
here.

- Say what you would do and why, then say what would change your mind.
- Check claims against the repository before repeating them. A wrong fact
  stated confidently costs more than a slow answer.
- Name the tradeoff instead of listing options. Recommend one.
- When a decision rests on something unknown, find out rather than assume. Read
  the code, run the command, check the branch.
- Say when a premise is wrong, including a premise the user supplied.
- Do not restate what was already settled. Move to what is not.

Research claims that carry weight. Cite the source and its confidence. A
practitioner blog post and a documented API limit are not the same kind of
evidence, and the difference belongs in the answer.

## What to write down

The conversation is not the artifact. When the discussion settles, or when the
user asks, write an entry to `internal/ideas/`.

```sh
python3 .claude/scripts/ulid.py
```

```markdown
---
name: <short-kebab-case-name>
last_updated: <YYYY-MM-DD>
id: <ULID>
---

# <Title>

## Problem

What is wrong now, with evidence.

## Decisions

What was settled, and why. One line each.

## Open

What is not settled, and what would settle it.

## Sources

Links, with what each supports.
```

Record decisions, not the discussion that produced them. Keep the reasoning
that would be expensive to reconstruct and drop the rest.

Use the `writing-docs` skill for the prose.

## Moving on

An idea becomes work when it has acceptance criteria someone could verify.
At that point file it as an issue, or as an epic with sub-issues, and
link the idea file. Until then it stays an idea.

## Do not

- Write implementation code.
- Create branches, worktrees, or pull requests.
- File issues without being asked.
- Expand the idea file into a specification. A spec is a separate document with
  a separate review.
