# MCP Tasks

## EXT-1: Apply project trust and permissions to MCP

- [ ] Keep project MCP configuration inactive until explicitly trusted for
      MCP.
- [ ] Make trust decisions scoped, durable, inspectable, and revocable.
- [ ] Route startup, tool, and resource authority through the shared permission
      flow.
- [ ] Record server, capability, requested authority, decision, result, and
      observed effects where available.
- [ ] Make configuration precedence visible and state when a server runs
      outside an enforcing sandbox.

## EXT-2: Add bounded MCP resources

**Blocked by:** EXT-1.

- [ ] List compact namespaced metadata only when a server advertises resources.
- [ ] Fetch contents explicitly with URI, item, byte, timeout, and
      serialization limits.
- [ ] Preserve media type and distinguish text from opaque binary data.
- [ ] Apply trust, permission, cancellation, redaction, and auditing.
- [ ] Isolate resource failures from unrelated servers and tools.

## EXT-3: Make MCP lifecycle and failures diagnosable

**Blocked by:** EXT-1.

- [ ] Use disabled, blocked by trust, starting, ready, degraded, failed, and
      stopped consistently across commands and status surfaces.
- [ ] Identify configuration scope and failure phase.
- [ ] Bound and redact stderr and protocol diagnostics.
- [ ] Isolate failed servers and settle their processes during cancellation and
      shutdown.
- [ ] Recommend only actions supported by the current CLI.
