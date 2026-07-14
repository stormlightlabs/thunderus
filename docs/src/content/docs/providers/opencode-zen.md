---
title: "OpenCode Zen Provider"
---

OpenCode Zen models use the `opencode/<model-id>` form in `thndrs.toml`.

For example:

```toml
model = "opencode/big-pickle"
```

`thndrs` sends Big Pickle requests to the OpenAI-compatible OpenCode Zen chat
completions endpoint and strips the `opencode/` prefix before sending the raw
model id.

## Authentication

Set `OPENCODE_ZEN_KEY` in the process environment or store it with the normal
credential flow:

```sh
thndrs login opencode-zen
thndrs logout opencode-zen
```

You can also run setup and choose OpenCode Zen:

```sh
thndrs setup
thndrs setup --provider opencode-zen
```

Provider keys are not accepted through CLI flags or TOML config. Stored API keys
live in the same managed credential files used by setup and login, and are
redacted from diagnostics, prompt inspection, snapshots, and session metadata.

`OPENCODE_ZEN_KEY` is separate from `OPENCODE_GO_KEY`. OpenCode Zen uses
`opencode/` model ids and `OPENCODE_ZEN_KEY`; OpenCode Go keeps
`opencode-go/` model ids and `OPENCODE_GO_KEY`.

## Big Pickle

OpenCode lists Big Pickle as free for input, output, and cached-read tokens, but
its docs also describe that free access as time-limited. Treat that as a current
provider offer, not a permanent `thndrs` guarantee.

OpenCode also documents a Big Pickle free-period privacy exception: during the
free period, collected data may be used to improve the model. Do not send
private, regulated, or customer-confidential code through Big Pickle unless that
provider behavior is acceptable for your use case.

## Model Discovery

`thndrs` can validate OpenCode Zen credentials with the model-list endpoint and
maps discovered ids back into `opencode/<model-id>` picker entries. Discovery is
not treated as pricing metadata; pricing and privacy caveats come from OpenCode
documentation.
