---
title: "Quick Start"
---

## Set Up a Provider

`thndrs` requires an authenticated provider before it can start a coding turn.
Run setup from the repository you want to work in and choose a provider:

```sh
thndrs setup

# to go right to chatgpt/codex setup:
thndrs setup --provider chatgpt-codex
```

## Running the TUI

Run from a repository:

```sh
thndrs
```

To point `thndrs` at a different workspace:

```sh
thndrs --cwd /path/to/repo
```

When developing from a checkout, replace `thndrs` in these commands with
`cargo run -p thndrs --`.

## First Prompt

Type a prompt in the bottom prompt line and press Enter. The transcript shows
your message, assistant output, reasoning updates, tool activity, and final
status.
