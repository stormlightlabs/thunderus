# Environment Variables

## `UMANS_API_KEY`

API key used by the Umans provider. `thndrs` reads this from the environment or
from a workspace `.env` file.

Example:

```sh
export UMANS_API_KEY=sk-...
```

Do not put secrets in shared config examples, docs, prompts, or `AGENTS.md`.
