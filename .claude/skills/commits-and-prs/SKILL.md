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

The pull request title and body become the squash commit, verbatim. This
repository merges with `squash_merge_commit_title=PR_TITLE` and
`squash_merge_commit_message=PR_BODY`, so there is one text to get right and
one place to get it right: the pull request. A branch's own commit messages are
discarded at merge and are checked for shape alone.

| Text                   | Target        | Where the number comes from      |
| ---------------------- | ------------- | -------------------------------- |
| Pull request title     | 53 characters | 59 allowed, less the ` (#NN)`    |
| Pull request body      | 20 lines      | It is the commit body            |
| Pull request comment   | 100 words     | A reply, not a report            |
| Review comment         | 200 words     | Ten findings; see `review`       |
| Edit reply             | 150 words     | Ten outcomes; see `revise`       |

The body is a commit body, so **wrap it at 72 columns** and use no headings:
`## What` reaches `git log` as the literal characters `## What`. One shape, at
one size.

Comments are counted in words because GitHub soft-wraps them. A 40-line comment
there is 400 words, which is how the old line targets were met and missed at
once. `writing-docs` carries that rule for everything else.

`.claude/scripts/check-commit-message.py --pr --title-file <f> --body-file <f>`
reports the title and the body, and CI runs it on every edit to either.

## Commit messages

```text
<type>: <what changed, imperative, lowercase, under 60 characters>

<why it changed, wrapped at 72 characters>
```

Types used here: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`.

`.claude/scripts/check-commit-message.py` checks the shape: the type, the
60-character subject, the blank line, and the 72-column body. Fenced blocks,
trailers, and unbreakable strings such as URLs are exempt from the column limit.

It runs in two places. Enable the hook locally once with
`git config core.hooksPath .githooks`, and it rejects a branch message while
that message is still in the editor. CI runs the same script in `--pr` mode
over the title and body, which is the text that lands, and reports without
failing: both stay editable until the merge, so naming a problem is worth more
than blocking on it.

Length is reported and rejected by neither, under [Length](#length).
## Authorship

Every commit here is authored by the repository owner. Agents work under their
direction and the log says so; nothing tries to separate a commit an agent
produced from one the owner typed, because the distinction was never checkable
and the record is not improved by a convention that only sometimes holds.

Leave the trailers a harness writes exactly as it wrote them, and add none by
hand. Name no model in a subject or a body. The message describes the change,
not what produced it.

A review comment is the exception, and the only one: its first line names the
model and reasoning level the pass ran at, under the `review` skill. That is a
claim about which reviewer found what, which `internal/models.md` needs and
which a reader of the thread can use.

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

There is no second, longer form. A body with headings reaches `git log` with
its `##` characters intact, and four headings over a six-line body is a form
rather than a description: a reviewer reads the headings, finds a sentence
under each, and learns less than the one paragraph would have told them. A
change too large to describe in twenty lines wants a document in `internal/`
and a link to it.

Requirements:

- `Verified` names actual commands and actual results. "Tests pass" without the
  command is not verification. If a check was not run, say so.
- `Not covered` is required and may not be empty. Write `Nothing` only when you
  have looked for gaps and found none. One line is a complete answer.
- Any test that was changed, removed, or narrowed gets a line explaining why.
- Twenty lines, wrapped at 72 columns. It is the commit body, and the reader
  is skimming `git log` for one change among hundreds.

Do not restate the diff. Do not recount the path you took to the change: the
dead ends, the thing you tried first, the file you read. A reviewer is deciding
about the code in front of them.

## Pull request comments

A comment is a reply in a conversation. A hundred words is already long for
one, and GitHub soft-wraps, so count words rather than lines.

The harness appends its own footer. That is the harness's line, not a
signature; add none of your own.

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
- Sign a commit body as a model, or name one in it.
- Write a pull request body with headings. It is the commit message.
