---
title: "Prompt Templates"
---

Prompt templates turn reusable Markdown or MiniJinja files into slash commands.
Typing `/review src/` renders the `review` template and submits the result as a
normal user prompt.

## Locations and precedence

`thndrs` loads `.md` and `.j2` files from two non-recursive directories:

- Global: `~/.thndrs/prompts/`
- Project: `<workspace>/.thndrs/prompts/`

The filename becomes the command name. For example,
`.thndrs/prompts/explain.j2` creates `/explain`.

Project templates replace global templates with the same name. Global
templates replace bundled prompt templates. Application commands such as
`/clear`, `/model`, and `/quit` cannot be replaced by a prompt template.

Each template and rendered prompt is limited to 256 KiB. Rendering also has a
bounded MiniJinja instruction budget. Discovery does not descend into nested
directories.

## Template format

Templates may start with optional YAML frontmatter:

```jinja
---
description: Explain a subsystem to a new contributor
argument-hint: "<path> [audience]"
---
Explain {{ arg1 }} to {{ arg2 | default("a new contributor", true) }}.

Cover its responsibility, main execution path, invariants, and tests.
```

`description` appears in slash-command suggestions. When it is absent, thndrs
uses the first non-empty line of the template. `argument-hint` appears before
the description and should use angle brackets for required input and square
brackets for optional input.

Both `.md` and `.j2` files are rendered by
[MiniJinja](https://docs.rs/minijinja/latest/minijinja/). Undefined variables
are errors, which keeps misspelled or missing arguments from producing an
incomplete prompt.

## Positional arguments

Arguments are split on whitespace. Single or double quotes keep spaces inside
one value:

```text
/explain crates/thndrs "an experienced Rust developer"
```

Templates receive these positional values:

| Variable    | Value                                      |
| ----------- | ------------------------------------------ |
| `args`      | List of positional arguments.              |
| `arg1`      | First positional argument.                 |
| `arg2`…     | Later positional arguments, when supplied. |
| `arguments` | All positional arguments joined by spaces. |

Use `args[0]` when list indexing is clearer than `arg1`.

## Named arguments

Tokens in `key=value` form become named arguments. Quote the complete value
when it contains spaces:

```text
/issue issue=123 audience="release maintainers"
```

A named value is available directly as `{{ issue }}` and through
`{{ named.issue }}`. Names must match `[A-Za-z_][A-Za-z0-9_]*`. The context
names `args`, `arguments`, `named`, and `arg1`-style names are reserved.
Providing the same named argument twice is an error.

This template accepts either a named `issue` or positional input:

```jinja
Analyze {{ issue | default(arguments, true) }}.
```

## Using templates

Type `/` followed by a template name. The existing command picker shows
matching templates with their argument hints and descriptions. Press `Tab` or
`Enter` to complete the selected name, add arguments, then press `Enter` to
render and submit it.

When an agent turn is already running, invoking a template renders it first and
queues the result using the current running-input target. `Ctrl+T` switches that
target between steering and follow-up.

Malformed frontmatter, invalid MiniJinja syntax, unreadable files, and oversized
templates appear in startup diagnostics. Invocation errors remain in the prompt
so the arguments can be corrected and retried.

## Bundled templates

`thndrs` includes these templates:

| Command               | Purpose                                                    |
| --------------------- | ---------------------------------------------------------- |
| `/review`             | Balanced correctness, safety, and maintainability review.  |
| `/adversarial-review` | Hunt subtle, hostile-input, concurrency, and test gaps.    |
| `/issue`              | Independently analyze a bug report or feature request.     |
| `/pr-review`          | Review a pull request with linked-issue and test context.  |
| `/changelog-audit`    | Compare recent user-visible work with changelog entries.   |
| `/commit`             | Draft a Conventional Commit message without changing Git.  |
| `/security-advisory`  | Investigate and draft an advisory without publishing it.   |
| `/wrap`               | Finish and verify the current task within its permissions. |

The review templates separate the balanced and adversarial roles so each can be
used on its own. The remaining workflows are adapted for `thndrs` from the pi
project's prompt collection and avoid repository-specific paths or release
rules.
