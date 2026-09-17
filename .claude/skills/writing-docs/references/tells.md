# Writing tells

Catalog of AI writing behaviors, with examples taken from this repository's own
documentation before it was revised.

## Structure tells

### Bold-first bullets as a template

Every bullet opening with a bolded phrase turns a list into a form. Use the
pattern only when the bolded span is a term the sentence then defines.

Before:

```text
- **Isolation is the whole point:** Each agent gets a directory where it can
  edit, build, and break without touching another agent's files.
- **A worktree is not a clone:** It shares `.git` objects, so creation is cheap.
```

After:

```text
- Each agent gets a directory where it can edit, build, and break without
  touching another agent's files.
- A worktree shares the repository's object store, so creating one is cheap and
  needs no extra fetch.
```

### Ceremonial endings

A takeaways or summary section that restates the document adds nothing. End on
the last real point, or on a list of links.

### Point-then-restate-then-restate

Saying the same thing at bullet level, paragraph level, and section level reads
as padding. State it once, at the level where it belongs.

### Recurring three-part rhythm

Three parallel clauses, three-item lists, and three-sentence paragraphs used
repeatedly become audible. Vary the count.

## Phrase tells

| Avoid                                  | Use                                               |
| -------------------------------------- | ------------------------------------------------- |
| the whole point, the point is          | State the thing itself.                           |
| load-bearing                           | Name what depends on it.                          |
| worth noting, worth stating, it's worth | Note it, or cut it.                              |
| crucially, importantly, fundamentally  | Cut. If it matters, its position shows that.      |
| that said, having said that            | Cut, or use "but".                                |
| this is where X comes in               | Introduce X directly.                             |
| at its core, essentially, ultimately   | Cut.                                              |
| the key insight                        | State the insight.                                |
| leverage, utilize                      | use                                               |
| serves as                              | is                                                |
| a rich tapestry, delve, navigate the   | Cut and rewrite plainly.                          |
| in the realm of, when it comes to      | Cut.                                              |
| seamless, robust, powerful, elegant    | Name the property: the timeout, the limit, the failure behavior. |

## Word precision

These words carry technical meaning in this repository. Using them loosely
costs the reader the ability to trust them anywhere.

| Word       | Reserve for                                                        |
| ---------- | ------------------------------------------------------------------ |
| bounded    | A limit that the text states: a count, a size, a timeout, a depth. |
| contract   | A formal API, protocol, schema, or compatibility guarantee.        |
| boundary   | An edge or separation between two named things.                    |
| invariant  | A property the code maintains and a test checks.                   |
| guarantee  | Something the system enforces, not something it tries to do.       |
| safe       | A stated threat or failure mode that is prevented.                 |
| minimal    | A comparison against something specific.                           |

## Rhetorical tells

- Negative reframes: "not a convenience, but a requirement."
- Negative countdowns: "Not caching. Not batching. Isolation."
- Self-answered questions: "So what breaks? Everything downstream."
- Standalone fragments for emphasis: "Every time."
- Manufactured stakes: turning a local design choice into a turning point.
- Defending a minor claim against an objection nobody raised.

One of these in a document can be a voice. Two or more is a pattern.

## Formatting tells

- Em dashes used repeatedly in one section for emphasis. Use a comma, a colon,
  or a new sentence.
- Tables used to hold prose. A table cell holding two sentences should be a
  paragraph.
- Lists disguised as paragraphs beginning "First," "Second," "Third."
- Semicolons joining independent points repeatedly. Use separate sentences.
- Decorative Unicode, arrows, and emphasis marks in technical prose.

## Repository-specific

- Notebook pages keep title-case section headings as a house format. Do not
  convert them to sentence case; deslop the prose under them instead.
- Confidence caveats in a claims table are facts. Preserve them verbatim while
  editing style.
- Do not add a "Related" or "Takeaways" section unless the page already had
  one and it carries links rather than restatement.
