---
name: commits-and-prs
description: Write commit messages and pull request descriptions for this repository. Use when committing, opening a pull request, or asked to write a PR body, release note, or changelog entry.
---

# Commits and pull requests

Write for the person reading `git log` in a year with no memory of this work.
They are skimming for one change among hundreds. Every line you add is a line
they read before they find it.

Use the `writing-docs` skill for the prose. Everything here is in addition to
it.

## Length

Length is the first thing to get right here, because this repository squashes
and the arithmetic is not obvious.

GitHub builds the merged subject from the pull request title and appends
` (#NN)`. It builds the merged body by concatenating every commit on the
branch, each under a `* subject` bullet. So the text that reaches `edge` is the
branch's commit messages added together, and the pull request description never
reaches it at all.

| Text                 | Target        | Where the number comes from         |
| -------------------- | ------------- | ----------------------------------- |
| Pull request title   | 53 characters | 59 allowed, less the ` (#NN)`       |
| One commit body      | 15 lines      | The `writing-docs` commit target    |
| Merged body          | 15 lines      | It is one commit like any other     |
| Pull request body    | 20 lines      | A reviewer reads it once, then never |
| Pull request comment | 10 lines      | It is a reply, not a report         |

Three commits at the 15-line target merge as a 48-line commit. The target is
for the merged message, so either the branch stays short or the squash message
gets written by hand in GitHub's merge box, which is where the last twelve
merges here went wrong: their bodies run 15 to 98 lines, median 49.

Nothing blocks on any of this. `check-commit-message.py` reports the body
target and the projected squash size as advice and fails no run, because a
change sometimes earns the room and no script can tell which one has. Shape is
still an error, since a missing type is not a judgement call. A check that
cannot be certain names what it finds and leaves the decision with the author;
one that blocks on a judgement call only teaches authors `--no-verify`, which
skips the checks that were certain too.

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
same script over a pull request's commits with `--warn`: the findings appear as
annotations and in the job summary, and the job passes anyway, because the only
way to correct a pushed message is to rewrite history that someone may already
have pulled. A rule worth a rebase is a rule worth catching at the hook.

Length is reported by both and rejected by neither, under [Length](#length).
The CI run adds the projected size of the squash, which is the only place that
number appears before someone clicks merge.

The subject says what changed. The body says why, and only when the why is not
obvious from the diff. A one-line commit is correct when the change explains
itself, and most of them do. Reach for a body when the diff cannot say why:
a constraint from outside the repository, a rejected alternative, a bug the
change is answering.

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

A reviewer wants to know what to look at and whether it works. Say that, and
stop. The body is scaffolding for one review; it is not a record, because it
never reaches `git log`.

Most changes need only this:

```markdown
Closes #<issue>

One paragraph: what changed, and what behavior it produces.

Verified with `<command>`: <result>.

Not covered: <what was left, with a link>.
```

Reach for headings when the change is large enough that a reviewer would
otherwise scroll looking for the verification, which in practice means a
diff over roughly 300 lines or one touching more than one crate:

```markdown
## What / ## Why / ## Verification / ## Not covered
```

Four headings over a six-line body is a form, not a description. A reviewer
reads the headings, finds a sentence under each, and learns less than the one
paragraph would have told them.

Requirements, at either size:

- `Verified` names actual commands and actual results. "Tests pass" without the
  command is not verification. If a check was not run, say so.
- `Not covered` is required and may not be empty. Write `Nothing` only when you
  have looked for gaps and found none. One line is a complete answer.
- Any test that was changed, removed, or narrowed gets a line explaining why.
- Keep the body under 20 lines. Longer means the change is too large, or the
  spec belongs in `internal/` with a link from here.

Do not restate the diff. Do not recount the path you took to the change: the
dead ends, the thing you tried first, the file you read. A reviewer is deciding
about the code in front of them.

## Pull request comments

A comment is a reply in a conversation. Ten lines is already long for one.

- Answer the question that was asked. Do not summarize the change again.
- One comment per review pass, not one per finding. The `review` skill sets
  what a pass posts.
- Say what you changed and where. `Fixed in <sha>` beats a paragraph.
- Skip the acknowledgement-only comment. Resolving the thread says it.
- No status tables, no progress checklists, no restating the plan. If a
  reviewer needs the state of the branch, CI is the state of the branch.

## Changelog

Add to `## Unreleased` in `CHANGELOG.md` under `Added`, `Changed`, `Fixed`, or
`Removed`. Write for a user of `thndrs`, not for a contributor: name the
behavior that changed, not the module that changed.

Skip the changelog for internal refactors, test-only changes, and documentation
that does not describe a behavior change.

## Do not

- Pad a body to look thorough. Length reads as effort and costs the reader.
- Narrate the work: what you tried first, what you read, what you ruled out.
  The result is the deliverable.
- Restate in the pull request what the commits already say, or in a comment
  what the pull request already says.
- Claim a check ran when it did not.
- Sign a commit body as a model. A review comment carries a signature; a commit
  carries the trailers under [Attribution](#attribution) and nothing else.
