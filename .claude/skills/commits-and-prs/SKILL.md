---
name: commits-and-prs
description: Write commit messages and pull request descriptions for this repository. Use when committing, opening a pull request, or asked to write a PR body, release note, or changelog entry.
---

# Commits and pull requests

Write for the person reading `git log` in a year with no memory of this work.

Use the `writing-docs` skill for the prose. Everything here is in addition to
it.

## Commit messages

```text
<type>: <what changed, imperative, lowercase, under 60 characters>

<why it changed, wrapped at 72 characters>
```

Types used here: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`.

`.claude/scripts/check-commit-message.py` checks the shape: the type, the
60-character subject, the blank line, and the 72-column body. Fenced blocks,
trailers, and unbreakable strings such as URLs are exempt from the column limit.

It runs in two places and only one of them blocks. Enable the hook locally once
with `git config core.hooksPath .githooks`, and it rejects a message while that
message is still in the editor, where fixing it costs a keystroke. CI runs the
same script over a pull request's commits with `--warn`: the violations appear
as annotations and in the job summary, and the job passes anyway, because the
only way to correct a pushed message is to rewrite history that someone may
already have pulled. A rule worth a rebase is a rule worth catching at the
hook.

The subject says what changed. The body says why, and only when the why is not
obvious from the diff. A one-line commit is correct when the change explains
itself.

Good:

```text
fix: collapse nested if in instance percentage check

Clippy's collapsible_if fires under -D warnings, which has failed CI
on every run since 2026-08-18.
```

Do not write:

- `fix: fix bug`, `chore: updates`, or any subject that could describe any
  commit.
- A body that restates the diff line by line.
- A list of every file touched. That is what the diff is for.
- Attribution to a model or tool in the subject or body. The trailers under
  [Attribution](#attribution) carry that.

One commit does one thing. A commit that needs "and" in its subject is two
commits.

## Attribution

A commit written from an agent session is authored as
`Claude <noreply@anthropic.com>`, so the log says plainly which commits a person
wrote and which an agent did. The Verified badge is a separate matter: it tracks
a cryptographic signature, not the author address, and signing is out of scope
here.

Such a commit ends with the trailers the harness supplies:

```text
Co-Authored-By: <model> <noreply@anthropic.com>
Claude-Session: <session url>
```

The session link is the useful half: it is the only way back to the reasoning
behind a change once the branch is merged. Nothing else in the message names a
model. The subject and body describe the change, not what produced it.

## Pull request bodies

```markdown
Closes #<issue>

## What

One paragraph. What changed and what behavior it produces.

## Why

The problem this solves, with evidence. Link the issue or the idea file.

## Verification

What was run, and what the result was. Name the commands.

## Not covered

What this does not do, what was not tested, and what was deferred with a link.
```

Requirements:

- `Verification` names actual commands and actual results. "Tests pass" without
  the command is not verification. If a check was not run, say so.
- `Not covered` is required and may not be empty. Write `Nothing` only when you
  have looked for gaps and found none.
- Any test that was changed, removed, or narrowed gets a line explaining why.
- Keep the body under roughly 40 lines. Longer means the change is too large or
  the spec belongs in `internal/`.

## Changelog

Add to `## Unreleased` in `CHANGELOG.md` under `Added`, `Changed`, `Fixed`, or
`Removed`. Write for a user of `thndrs`, not for a contributor: name the
behavior that changed, not the module that changed.

Skip the changelog for internal refactors, test-only changes, and documentation
that does not describe a behavior change.

## Do not

- Pad a body to look thorough.
- Claim a check ran when it did not.
- Sign a commit body as a model. A review comment carries a signature; a commit
  carries the trailers under [Attribution](#attribution) and nothing else.
