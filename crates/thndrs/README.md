# thndrs

`thndrs` is a terminal coding assistant for working directly in a project.
Start it from your repository, describe the work you want done, and keep the
conversation alongside your code.

## Install

```sh
cargo install --locked thndrs
```

Run setup from the repository you want to work in, then start the assistant:

```sh
cd path/to/project
thndrs setup
thndrs
```

Local tools run with the permissions of the user who started `thndrs` without
a sandbox.

The application supports durable sessions, local tools, MCP, ACP, and multiple
provider adapters. See the [docs](https://thndrs.stormlightlabs.org/)
for provider setup, configuration, sessions, and troubleshooting.

`thndrs` is an experimental application. Its CLI, configuration, provider, session,
and tool behavior may evolve during the pre-v1 release line.
