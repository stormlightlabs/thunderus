---
name: enmrs
last_updated: 2026-09-19
id: 01M2VZKCYG893B14P5N7A5JA3V
---

# Enmrs, a remote surface for thunderus

Enmrs, for enamorous, is a remote control for this repository's own harness.

## Problem

The remote surface a run depends on belongs to one vendor and one harness.
`remote-operation.md` (`01M2VZFXVCF6XSDF3PWMQYTWHT`) settles that runs happen on
the operator's machine and that Claude Code Remote Control is how the operator
reaches them. That path covers Claude Code sessions, requires a subscription,
and rejects API keys.

`thndrs` has no remote surface of its own, so remote work depends on a harness
this repository did not write. Codex and Pi staying local is a settled choice
rather than a gap, and dogfooding `thndrs` in the roles it can take does not
close this one: a `thndrs` session away from the machine cannot be reached at
all.

## Decisions

### Enmrs is a client of the lndrs control protocol

It does not define a protocol. `../features/lndrs/plan.md`
(`01M2S3GAM20S0VNJ4A10MBBWR0`) already specifies one: newline-delimited JSON
over a Unix socket or named pipe, addressed by dotted method names, carrying the
four verbs ACP cannot express, which are listing runs, reading a run's units and
their stages, cancelling a run, and subscribing to run events.

That specification already contains the property a remote surface needs. Its
events carry a monotonic sequence number per run, and a subscriber names the
sequence it has seen, so a client that attaches to a run in progress can
reconstruct what happened. The plan states that requirement as a correction to
Herdr, which does not replay what a late subscriber missed.

### What enmrs adds is a transport and an identity

A socket on one machine is not reachable from another, and that is the entire
gap. Enmrs is the same protocol over a transport that crosses machines, plus an
answer to who is allowed to speak it.

### Over a tailnet it is only a client

Tailscale already supplies reachability and identity, so enmrs bound to a
tailnet address needs no relay, no accounts, and no public endpoint. A relay is
what Claude Code Remote Control provides and it is also what turns this from a
tool into a service to operate. The tailnet version is the one to build, and it
is small enough to be worth building.

## Open

Which client comes first. A TUI attaching from another machine reuses the
interface track lndrs already has and costs almost nothing beyond the transport.
A phone needs a web client, which is a different program with a different review
problem.

Whether enmrs precedes or follows the lndrs interface. The lndrs plan defers its
interface until the capture loop in `../features/tui-verification/plan.md` can
show a reviewer a frame, and that argument applies unchanged here: a remote
client shipped before that lands is an interface nobody reviews.

Whether enmrs needs lndrs at all. A single `thndrs` session has no run to
attach to, but it does have a session: `core/session/writer.rs` appends JSONL,
and `thndrs acp serve` already exposes the harness over stdio. Whether those two
are already a remote surface, reached by attaching to a session rather than a
run, is unmeasured. Settled by trying it against a session on another machine
before writing any new protocol code.

What authentication looks like if it ever leaves the tailnet. Not decided,
because nothing yet requires it.

## Sources

- `../features/lndrs/plan.md` (`01M2S3GAM20S0VNJ4A10MBBWR0`), for the control
  protocol, the four verbs, and the sequence-numbered event stream.
- [Remote Control](https://code.claude.com/docs/en/remote-control), read
  2026-09-18, for what a relayed remote surface provides and its subscription
  requirement.
- `@earendil-works/pi-coding-agent` 0.85.1, `docs/development.md`, for the
  experimental remote harness being excluded from published builds.
