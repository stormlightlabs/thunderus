# Provider Tasks

## PROVIDER-1: Support the OpenAI Platform API

- [ ] Keep Platform and ChatGPT credentials and routes distinct.
- [ ] Normalize text, images, reasoning, tools, usage, retries, and errors.
- [ ] Identify the route accurately in setup, doctor, model selection, status,
      and sessions.

## PROVIDER-2: Support the Anthropic API

- [ ] Add setup, doctor, model selection, recovery, and session identity.
- [ ] Normalize text, images, reasoning, tools, usage, retries, and errors.
- [ ] Reject unsupported controls through explicit capabilities.

## PROVIDER-3: Configure compatible endpoints

**Blocked by:** Two native adapters with stable capability contracts.

- [ ] Declare protocol, base URL, credential source, model, and trust scope.
- [ ] Describe tools, images, reasoning, context, and output limits.
- [ ] Reject incomplete capability declarations before a request.

## PROV-1: Expose supported account capacity without scraping

**Blocked by:** An evidenced supported account API for each route.

- [ ] Show returned allowance windows with used or remaining values, reset,
      observation time, and stale state.
- [ ] Keep refresh and detail in `/usage`; use compact redacted summaries
      elsewhere.
- [ ] Represent missing, unsupported, and stale data accurately.
- [ ] Never persist or render raw responses, email, token, account ID, or
      authorization URL.
