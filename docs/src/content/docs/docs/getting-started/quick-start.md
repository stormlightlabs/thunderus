---
title: "Quick Start"
---

## Start

Run `thndrs` from the repository you want to work in:

```sh
thndrs
```

The first launch opens required setup. Choose a provider, authenticate, and
select a model before submitting a coding prompt. `thndrs setup` provides the
guided CLI credential and configuration route; its selected provider supplies
the initial model. Use the TUI `/model` command or configuration to choose a
different model.

```sh
thndrs setup --provider chatgpt-codex
thndrs setup --provider opencode-zen
thndrs setup --provider opencode-go
```

To point `thndrs` at a different workspace:

```sh
thndrs --cwd /path/to/repo
```

For ChatGPT Codex, setup starts browser OAuth by default. In a headless or
remote environment, choose device code explicitly with:

```sh
thndrs login chatgpt-codex --oauth-method device-code
```
