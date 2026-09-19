---
name: writing-docs
description: Write, revise, and deslop documentation for this repository in plain technical English. Use for the docs site, internal plans and specs, README, CHANGELOG, notebook research notes, error messages, and any prose where AI writing tells, density, or readability matter.
---

# Writing docs

Write like a person choosing words for a reader. Keep every fact, number,
qualifier, and identifier. Remove whatever does not help.

This applies to the chat response as well as the file being edited.

## Soft limits

These are warnings that something needs restructuring, not reasons to delete
information. Exceed one when the content requires it and the result still reads
cleanly.

| Limit                    | Target        | What exceeding it usually means                       |
| ------------------------ | ------------- | ----------------------------------------------------- |
| Line width               | 80 characters | Reflow. Tables and links may exceed.                  |
| Sentence                 | 25 words      | Two ideas in one sentence. Split them.                |
| Paragraph                | 5 sentences   | More than one topic. Split or cut.                    |
| Section before a heading | 40 lines      | The section needs subheadings or is doing too much.   |
| Consecutive bullets      | 9             | The list is a table, or it needs grouping.            |
| Table columns            | 4             | The table is prose in disguise.                       |
| Heading depth            | 3 (`###`)     | The document needs splitting.                         |
| Code block               | 25 lines      | Show the relevant span and link to the source.        |

Prose paragraphs should outnumber tables and lists in any document that
explains something. A page of nothing but tables is a reference, not an
explanation; say which one you are writing.

## Document length

The soft limits above govern a sentence, a paragraph, and a section. None of
them stops a document from growing past what a reader will finish. Match the
length to what a reader has to do with the document, and check it against these
targets.

**Count lines only where something wraps them.** A file wraps at 80, a commit
body at 72, a terminal at its width. Everything GitHub renders — a pull request
body, a comment, an issue — is soft-wrapped, so one paragraph is one line and a
line count measures nothing. Those are counted in words. A comment that passed
the old 40-line cap ran 392 words, which is how the targets below were being
met and missed at the same time.

| Text                             | Unit       | Target |
| -------------------------------- | ---------- | ------ |
| Error message                    | lines      | 2      |
| Pull request title               | characters | 53     |
| Pull request body                | lines      | 20     |
| Review comment                   | words      | 200    |
| Edit reply                       | words      | 150    |
| Any other pull request comment   | words      | 100    |
| Issue body                       | words      | 250    |
| Chat reply                       | lines      | 20     |
| Command or agent definition      | lines      | 40     |
| `SKILL.md`                       | lines      | 200    |
| Docs page, README, internal spec | lines      | 300    |

The pull request body counts in lines because it is a commit body: this
repository merges with `squash_merge_commit_message=PR_BODY`, so the body
reaches `git log` verbatim and is written wrapped at 72 columns. The title is
the subject, and 53 is 59 less the ` (#NN)` GitHub appends. `commits-and-prs`
holds both, and `.claude/scripts/check-commit-message.py --pr` reports them on
every pull request. The review and reply numbers live with `review` and
`revise`, which is where they are read.

A reference page that enumerates a surface, every configuration key or every
flag, grows with that surface and is the usual exception. It still needs
headings a reader can jump between.

A chat reply is held to its target like anything else. A long answer in chat is
a document with no home: nobody can find it again, and the length hides which
sentence held the decision. When a reply wants to exceed the target, write the
file instead and answer with where it went.

A reader looking for one answer should reach it without reading the rest. When
a document passes its target, cut in this order:

1. Anything the code, the tests, or `--help` already states. Link to it.
2. Anything another document states. One fact has one home; every other
   mention links to that home.
3. Background the audience already has.
4. Examples past the first one that shows the shape.

Split only after cutting. A split that sends one reader to two files costs more
than the length did, so split by audience or by task, never by size alone.

Never drop a fact, a number, a limit, or a qualifier to reach a target. A
document whose facts do not fit is more than one document.

A comment is the exception, because it is a reply rather than a record. When a
review comment does not fit, cut a finding and say how many you left out; do
not compress ten findings into denser prose. The reader can ask for the rest,
and the tenth `nit` was never what the length was for.

## Plain technical prose

Use this mode by default.

- Use one term for one thing. Do not cycle through synonyms.
- Prefer short common words and plain verbs. Use the verb, not the
  nominalization: "parse the config," not "perform parsing of the config."
- Put the actor before the action when the actor matters.
- Give each paragraph one topic. Put conditions before the instruction they
  govern.
- State the point before its evidence, once.
- Use active voice when it clarifies responsibility. Keep passive voice when
  the actor is unknown or deliberately backgrounded.
- Name sources, limits, and observable behavior.

## Deslop rules

Read `references/tells.md` for the full catalog and repository-specific
examples.

The tells that appear most in this repository:

- Bold-first bullets used as the default pattern. Use them only when the bolded
  span is a real term being defined.
- `not X, but Y` and its variants, repeated.
- Closing sections that restate what the document already said.
- Words that name a judgment instead of a property: `load-bearing`, `the whole
  point`, `worth noting`, `crucially`, `seamless`, `robust`.
- `bounded` without a stated limit, `contract` for anything that is not a formal
  API or schema, `boundary` for anything that is not an edge.
- Em dashes used repeatedly for emphasis.
- Tricolons, anaphora, and one-line slogans.

One instance of any of these can read as a human voice. Repetition is the tell.

## Repository conventions

- Sentence case for new headings. Existing pages under
  `docs/src/content/docs/docs/notebook/` use title-case section names as a
  house format; keep that format and deslop the prose inside it.
- Notebook pages keep their frontmatter and their section order: summary, key
  ideas, claims and evidence, terms, questions.
- Files under `internal/` need `name`, `last_updated`, and a ULID `id` in
  frontmatter. Generate identifiers with `.claude/scripts/ulid.py`.
- Claims in research notes carry a confidence caveat. Do not strip it while
  editing for style.
- Run `pnpm --dir docs build` after editing anything under `docs/`. It
  validates internal links.

## Editing method

1. Identify the audience, the purpose, and which mode applies.
2. Mark the facts, conditions, numbers, and qualifiers that must survive.
3. State each point once, before its support.
4. Cut preambles, duplication, filler, and template endings.
5. Replace abstractions and nominalizations with concrete nouns and verbs.
6. Check the soft limits and the length target, and restructure where one
   is exceeded.
7. Read for rhythm, parallel lists, and clear pronoun referents.

For a small edit, change the smallest useful span. For a rewrite, preserve every
fact unless the user authorizes substantive changes.

## Check before returning

- Does the first sentence answer instead of announce?
- Does each claim appear once, before its evidence?
- Can any paragraph disappear without losing a fact?
- Is the document inside its length target? If not, what did the extra
  length buy the reader?
- Does every `same`, `this`, and `existing` have a clear referent?
- Are limits, sources, and failure behavior named rather than implied?
- Did one trope recur enough to become visible?
- Does the ending stop, or explain that it has ended?
- Did a style rule damage accuracy or clarity? If so, break the rule.
