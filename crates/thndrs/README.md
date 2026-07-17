# thndrs

`thndrs` is an LLM-powered pair programmer.

## Install

```sh
cargo install --locked thndrs
```

Run `thndrs` from the repository you want to work in:

```sh
cd path/to/project
thndrs
```

On a fresh install, the application opens required setup before it accepts a
coding prompt. Choose a provider and authenticate there, or run `thndrs setup`
for the equivalent CLI workflow.

Local tools run with the permissions of the user who started `thndrs`.

Use a container, VM, or OS-level sandbox when isolation matters.

The [docs](https://thndrs.stormlightlabs.org/) cover provider setup,
configuration, sessions, diagnostics, and tool safety.

`thndrs` is experimental pre-1.0 software. Its CLI, configuration, provider,
session, and tool behavior may evolve during the pre-1.0 release line.

## License

`thndrs` is licensed under the Apache License, Version 2.0. See
[`LICENSE`](./LICENSE) for the full text.
