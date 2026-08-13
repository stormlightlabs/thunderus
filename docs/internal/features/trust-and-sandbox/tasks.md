# Trust and Sandbox Tasks

## SAFETY-1: Gate project runtime configuration on trust

- [ ] Scope trust for project ACP, prompt templates, commands, hooks, skills,
      and MCP.
- [ ] Ignore untrusted project runtime configuration and show what was ignored.
- [ ] Make decisions durable, inspectable, scoped, and revocable.
- [ ] Prevent project resources from rewriting harness identity, direct
      instructions, tool schemas, provider boundaries, or safety policy.

## SAFETY-2: Define the sandbox execution boundary

**Blocked by:** SAFETY-1.

- [ ] Distinguish read-only, workspace-write, and external isolation.
- [ ] Treat filesystem and network authority separately.
- [ ] Report the boundary used by built-in shell, ACP terminal, and MCP server
      processes.
- [ ] Claim no isolation when no enforcing backend exists.

## SAFETY-3: Implement the first OS sandbox backend

**Blocked by:** SAFETY-2.

- [ ] Enforce workspace reads and writes and network policy.
- [ ] Fail closed outside allowed roots and for disallowed network access.
- [ ] Protect repository-control and credential paths.
- [ ] Prevent descendants from outliving cancellation or shutdown.

## SAFETY-4: Ask for approval at authority boundaries

**Blocked by:** SAFETY-2; enforced cases also require SAFETY-3.

- [ ] Describe the command, resource, effects, and requested authority.
- [ ] Audit allow, reject, cancel, timeout, and unavailable-interaction results.
- [ ] Never weaken the sandbox silently or overstate enforcement.
- [ ] Apply skill and MCP constraints through the shared policy.
