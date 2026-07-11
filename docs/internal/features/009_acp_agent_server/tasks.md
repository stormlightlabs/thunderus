# Tickets: ACP Validation And Expansion

The baseline exposes the proven server through `thndrs acp serve`. These
tickets start after the baseline release gate.

## Ticket 1: Validate A Real Editor Client

**What to build:** Run a documented ACP editor/client path against the packaged
server and fix only demonstrated compatibility gaps.

**Blocked by:** Baseline release gate

**Acceptance criteria:**

- [ ] One real client/version/date is recorded.
- [ ] Initialization, prompt streaming, cancellation, permissions, and session
      behavior work without stdout pollution.
- [ ] Any compatibility change has a matching fixture regression test.

## Ticket 2: Prepare Registry Packaging

**What to build:** Add package/discovery metadata only after the real-client
contract is proven.

**Blocked by:** Ticket 1: Validate A Real Editor Client

**Acceptance criteria:**

- [ ] Documentation names the `thndrs acp serve` command and supported capabilities.
- [ ] Version reporting matches package metadata.
- [ ] Registry smoke checks run without publishing.

## Ticket 3: Evaluate A Non-stdio Transport Only On Demand

**What to build:** Add a remote/custom transport only for a concrete target
that cannot use the local stdio executable.

**Blocked by:** Ticket 2: Prepare Registry Packaging; explicit user demand

**Acceptance criteria:**

- [ ] The target and why stdio is insufficient are documented.
- [ ] Capability checks, cancellation, redaction, cleanup, and audit behavior
      match stdio through fixtures.
- [ ] No transport is enabled by default.

## Frontier

Ticket 1 starts after the baseline release gate.
