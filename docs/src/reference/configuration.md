# Configuration

`thndrs` currently uses CLI flags and environment variables for configuration.
A stable config file is planned for v1 but is not part of the current public
contract.

Current user-controlled settings:

- `--cwd`: workspace root.
- `--model`: completion model.
- `--websearch`: web-search mode.
- `--tick-rate-ms`: event poll interval.
- `--verbose`: diagnostic transcript rows.
- `--mouse` / `--no-mouse`: terminal mouse capture.
- `--no-alt-screen`: draw inline instead of using the alternate screen.
- `--print-prompt`: inspect prompt assembly without contacting the provider.

Secrets are read from environment variables, not config examples. See
[Environment Variables](environment-variables.md).
