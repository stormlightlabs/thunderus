---
title: Quiver and Arrows
status: Draft
captured: 2026-07-14
---

## Objective

Build Quiver as thndrs' extension subsystem. A Quiver arrow is an
independently useful, host-installed CLI integration with a user-owned
manifest and documentation. Quiver lets a developer teach thndrs about their
own toolchain without expanding the core into a package manager or a
collection of domain-specific features.

The first vertical slice is a read-only first-class arrow for mccabre. It must
prove discovery, enablement, health checking, documentation, controlled
invocation, and durable audit without requiring mccabre to be installed in
tests.

This document uses "arrow" as the working public term. The product may choose
"bolt" instead before implementation, but the two must not become distinct
extension types without a demonstrated need.

## Product Model

### Terms

- **Quiver:** the thndrs subsystem that discovers arrows, resolves their
  configuration, projects them into agent context/tools, controls their
  authority, and records their use.
- **Arrow:** a plugin-like integration record for a standalone CLI, its
  documentation, declared operations, health checks, and learned operating
  knowledge.
- **Operation:** one agent-callable action supplied by an arrow. An operation
  has typed input, a bounded output contract, and declared effects.
- **Learned overlay:** agent-maintained, evidence-bearing notes that make an
  arrow easier to use without altering its authority.

An arrow is tool-like, but is not necessarily one tool. A small utility arrow
such as ripgrep may primarily improve documentation and shell use. A richer
arrow such as mccabre can provide several structured operations.

### User Value

- A developer can register the tools they already use, including commands such
  as rg, fd, bat, jq, curl, mccabre, ocaat, and project scripts.
- thndrs can identify a usable tool, explain how to set it up, and improve its
  use over time without silently gaining new authority.
- Tool authors can ship a CLI and a small integration record rather than a
  thndrs dependency or a bespoke plugin runtime.
- First-class arrows feel native because thndrs ships their integration
  definition and setup guidance, while the user still installs and updates the
  executable.

### Relationship to Existing Concepts

- Skills teach procedures and project conventions.
- Slash commands are human-invoked workflows.
- Tools perform individual actions.
- Arrows package an external capability, its knowledge, and its operations.

Quiver complements MCP rather than replacing it. An arrow may later use an MCP
transport, but the v1 contract must also support an ordinary local CLI.

## Current State

thndrs already has the application-layer seams Quiver needs:

- The built-in tool registry has stable definitions, schemas, executors, and
  structured side-effect results.
- The MCP manager already namespaces third-party tools and caps/redacts their
  output, but an MCP server is not required for an arrow.
- The application owns permission and execution hooks; the reusable
  thndrs-agent crate intentionally does not own filesystem, process, or
  provider wire policy.
- Sessions preserve tool-related audit data and thndrs has explicit context
  selection/compaction controls.

The current application does not have an arrow registry, a manifest contract,
durable tool-learning records, a semantic repository map, cross-session
memory, a durable job daemon, or an operating-system sandbox.

## Arrow Storage and Resolution

Quiver discovers integration records at these roots:

```text
~/.thndrs/arrows/<arrow-name>/manifest.toml
~/.thndrs/arrows/<arrow-name>/overlay.toml
<workspace>/.thndrs/arrows/<arrow-name>/manifest.toml
<workspace>/.thndrs/arrows/<arrow-name>/overlay.toml
```

- JSON is an equal alternate format:
  manifest.json with an optional sibling overlay.json.
- An arrow directory contains exactly one manifest format. Its optional overlay
  must use the same format. Finding both TOML and JSON, or a mismatched pair,
  is an actionable configuration error rather than an implicit precedence
  choice.
- TOML is the documented default because people will commonly author and
  review these files; JSON serves generated or tool-owned integrations. Both
  deserialize into the same versioned Rust contract through serde.
- Global and project arrows are user-owned files.
- Whether a project arrow is committed to version control is entirely the
  user's decision; Quiver makes no VCS decision.
- A project arrow with the same name fully shadows a global arrow. Quiver must
  display the selected source and never silently merge authority settings.
- Discovery does not enable an arrow. An arrow can be enabled before its
  executable exists on PATH.

The resolved lifecycle is:

```text
discovered -> enabled -> healthy -> runnable
```

- **Discovered:** a valid manifest was found but it is inactive.
- **Enabled:** its compact catalogue card and setup guidance are available.
- **Healthy:** its entrypoint and declared runtime/version requirements are
  satisfied.
- **Runnable:** a healthy operation is permitted by the active authority
  policy.

An unavailable executable is a visible "enabled, setup required" state, not an
error that removes documentation or makes the agent believe the tool works.

## Manifest and Learning Contract

The trusted manifest is human- or tool-author-owned. At minimum it declares:

- schema version, name, concise description, and source scope;
- documentation paths or references;
- an explicit entrypoint argv array and required runtime/version probe;
- declared operations with input schema, output format, and effects;
- entrypoint origin: host-installed or project-local.

For example, an entrypoint may be a host command such as ["mccabre"] or an
explicit script argv such as ["/path/to/fake-mccabre.sh"]. Python, Ruby, Lua,
and shell scripts are supported through explicit argv arrays; Quiver must not
build opaque shell strings such as "sh -c ...".

Each arrow may have an agent-writable sibling overlay, named overlay.toml for a
TOML manifest or overlay.json for a JSON manifest. The overlay may record
verified examples, limitations, project conventions, observed command behavior,
provenance, confidence, and review/expiry data. It must not alter the
manifest's entrypoint, operations, effects, documentation source, trust class,
or enabled authority.

All learned changes must be schema-checked, diffable, attributable to a session
or user action, and easy to inspect, reset, or promote deliberately into a
shared manifest. Project and global overlays follow the same user-controlled
storage model as their manifests.

## Trust, Permission, and Invocation

- Humans and agents can request arrow enablement. The default policy must make
  an agent-initiated enablement visible and auditable; a user may configure a
  more permissive policy.
- A project-local script is a stricter trust class than a host-installed CLI.
  Its first runnable operation requires explicit approval.
- Documentation and learned notes are reference material, never authority to
  alter permissions or execute arbitrary instructions embedded in tool output.
- Quiver executes declared operations with argv arrays, an explicit workspace
  context, bounded output, timeouts, redaction, and session audit data.
- A normal shell path remains an escape hatch under the existing shell
  permission policy. It is not a substitute for declared operations.
- Quiver neither installs nor updates external executables. First-class setup
  guidance may explain installation, then health checks confirm readiness.

Sandbox selection is controlled by the user and policy, never by an arrow or
the model. A future sandbox arrow/backend must be explicit about whether it
isolates spawned commands only or the complete agent workspace.

## Agent Projection

Every enabled arrow contributes a compact catalogue card to the agent's
available context:

- name and purpose;
- source scope and trust class;
- enabled/healthy/runnable state;
- declared effects;
- one-line guidance for inspecting or using it.

Full documentation and learned notes load only on demand. This preserves
thndrs' explicit context-control model and prevents a large local toolchain
from consuming every prompt.

Quiver exposes generic agent operations for inspection, status, and enablement.
Healthy, selected arrows may additionally project direct named operations when
their typed contract improves reliability. The generic surface is the fallback;
the full set of every arrow operation must not be injected into every model
tool catalogue.

## First-Class mccabre Arrow

mccabre is the v1 reference arrow because it is an existing local CLI with
structured JSON output and read-only analysis use cases.

The bundled integration definition must:

- provide setup and version guidance but no bundled mccabre binary;
- resolve an explicitly configured or PATH-installed mccabre command;
- expose only read-only JSON analysis operations in v1;
- keep execution rooted at the selected workspace;
- treat output as analysis evidence, not a command-quality gate;
- exclude report/coverage operations that write artifacts until their effects
  receive a separate design and permission path.

The first-class catalogue will later include ocaat and the to-be-named
repomapper. They are sequenced after mccabre proves the host-CLI contract.

## Testing and Verification Plan

### Test Boundary

The primary v1 acceptance boundary is a deterministic, headless integration
test. A temporary global/project arrow layout and a simple executable shell
fixture stand in for mccabre. The fixture supports a version probe and emits
known JSON while recording received argv.

The test verifies discovery, project-over-global resolution, explicit
enablement, an enabled-but-unhealthy state, health transition, compact
catalogue projection, safe argv invocation, output handling, and session/audit
records. It must not depend on an installed mccabre binary, a model provider,
or TUI automation.

A manual smoke test with a real locally installed mccabre remains useful after
the deterministic suite passes.

### Required Checks for Rust Changes

```sh
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
```

Run the narrowest Quiver test first. Public documentation changes also require:

```sh
pnpm --dir docs build
```

## Boundaries

Always:

- preserve the separation between thndrs-agent's provider-neutral contracts
  and application-owned process/policy code;
- use explicit argv, bounded output, deterministic fakes, and structured audit
  records;
- keep manifests and learned overlays inspectable and reversible;
- treat project-provided data as untrusted until an explicit policy grants
  authority.

Ask first:

- adding a package manager, marketplace, automatic installer, or runtime
  dependency on an external arrow;
- adding remote documentation fetching, persistent approvals, an MCP transport
  requirement, or a public extension API;
- changing permission semantics, sandboxing behavior, session format, or
  thndrs-agent public contracts.

Never:

- auto-enable a discovered project arrow;
- let an agent-written overlay expand executable authority;
- execute an arbitrary shell string derived from an arrow manifest;
- expose default mounts, credentials, or network authority through a future
  sandbox integration;
- claim mccabre threshold output is an enforced quality gate when the CLI does
  not enforce it.

## Deferred Milestones

- A repository-map arrow with bounded, targeted context output and its own
  cache/index lifecycle.
- A memory arrow with explicit facts, provenance, retention/forgetting, and
  user-controlled writes.
- A sandbox execution backend with clear command-only versus whole-workspace
  isolation semantics.
- Scriptable jobs, watch triggers, and an on-demand local daemon only after a
  concrete durable-workload need.
- First-class ocaat integration, with read and remote-write operations
  separately permissioned.
- Public documentation and comparison-copy updates after the mccabre vertical
  slice is real and reviewable.

## Risks and Open Questions

- Choose one public term, "arrow" or "bolt", before CLI/config names become
  durable.
- Decide the exact user experience for an agent-initiated enablement prompt and
  any user-configurable auto-enable policy.
- Define the initial manifest schema narrowly enough to support mccabre without
  prematurely standardizing every future transport.
- Confirm the proposed headless integration-test boundary before implementation.

The detailed implementation frontier is in [task.md](task.md).
