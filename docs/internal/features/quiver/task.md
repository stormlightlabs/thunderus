# Tickets: Quiver v1

These draft tickets implement the plan in [plan.md](plan.md). They deliberately
prove a host-installed, read-only mccabre arrow before adding repository maps,
memory, a daemon, sandboxing, ocaat, or a marketplace.

Work the frontier: any ticket whose blockers are complete can start. The
proposed granularity and dependency edges should be reviewed before coding.

## Ticket 1: Resolve Arrow Manifests Without Running Them

**What to build:** Let thndrs discover and validate global and project arrow
manifests, resolve project-over-global shadowing, and expose an inspectable
resolved registry without executing any external command.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Valid manifests are discovered from the documented global and project
      locations.
- [ ] An arrow may use a TOML manifest with a sibling overlay.toml, or a JSON
      manifest with a sibling overlay.json; both formats deserialize to the
      same versioned contract.
- [ ] An arrow directory containing both manifest formats or a mismatched
      manifest/overlay format is rejected with an actionable diagnostic.
- [ ] A project arrow fully shadows a global arrow with the same name, and the
      selected source is visible.
- [ ] Invalid manifests produce actionable diagnostics without breaking normal
      thndrs startup.
- [ ] Discovery alone neither enables an arrow nor runs its entrypoint.
- [ ] The parsed contract distinguishes trusted manifest fields from the
      agent-writable learned overlay.

**Verification:**

- Focused deterministic tests cover discovery, validation, shadowing, and
  missing/unreadable files.
- Run the workspace Rust checks in the plan.

## Ticket 2: Add Arrow Lifecycle, Health, and Explicit Enablement

**What to build:** Make arrows visible as discovered, enabled, healthy, and
runnable capabilities. Give humans and agents a controlled way to enable an
arrow, including an enabled-but-unhealthy state that presents setup guidance.

**Blocked by:** Ticket 1: Resolve Arrow Manifests Without Running Them

**Acceptance criteria:**

- [ ] A human can inspect arrow status and explicitly enable or disable an
      arrow.
- [ ] An agent can request enablement through the normal application
      permission/audit path.
- [ ] Health checks use manifest-declared explicit argv and never shell text.
- [ ] An enabled arrow missing its command or runtime remains visible with
      setup guidance and cannot run operations.
- [ ] Project-local entrypoints are visibly classified and require explicit
      first-run trust.

**Verification:**

- A temporary executable fixture proves unhealthy-to-healthy transition without
  an installed third-party CLI.
- Run focused lifecycle tests, then the workspace Rust checks.

## Ticket 3: Project Arrow Knowledge Into Context Safely

**What to build:** Give every enabled arrow a compact, bounded catalogue card,
on-demand documentation inspection, and a schema-checked learned overlay that
the agent can improve without changing authority.

**Blocked by:** Ticket 2: Add Arrow Lifecycle, Health, and Explicit Enablement

**Acceptance criteria:**

- [ ] The default agent context receives only compact arrow identity, state,
      trust, effects, and one-line guidance.
- [ ] Full documentation and learned notes load only through explicit
      inspection/selection.
- [ ] Agent-written learning records retain provenance, confidence, and review
      or expiry data.
- [ ] Learned state is stored in the optional sibling overlay file using the
      manifest's format, so users can inspect, ignore, or commit it as they
      choose.
- [ ] An overlay cannot modify entrypoints, operations, effects,
      documentation sources, or permission policy.
- [ ] Users can inspect, reset, and deliberately promote learned changes.

**Verification:**

- Focused tests prove context bounds, on-demand loading, rejected authority
  mutations, and reset/promotion behavior.
- Run the workspace Rust checks.

## Ticket 4: Invoke Declared Arrow Operations as Tools

**What to build:** Connect healthy arrows to thndrs' tool execution path:
generic Quiver inspection/status/enablement tools plus selected direct,
typed arrow operations with structured output and durable audit.

**Blocked by:** Ticket 2: Add Arrow Lifecycle, Health, and Explicit Enablement

**Acceptance criteria:**

- [ ] Quiver provides generic inspection, status, and enablement operations.
- [ ] A selected healthy arrow can contribute a direct named operation without
      expanding every model tool catalogue.
- [ ] Invocation uses explicit argv, workspace containment, timeouts, bounded
      output, redaction, and structured audit records.
- [ ] Declared effects participate in permission policy before execution.
- [ ] The normal shell tool remains an explicit escape hatch rather than the
      primary integration contract.

**Verification:**

- A headless fixture verifies manifest operation input, received argv, known
  JSON output, denied permission, and recorded audit information.
- Run focused operation tests, then the workspace Rust checks.

## Ticket 5: Ship mccabre as the First-Class Read-Only Arrow

**What to build:** Bundle the mccabre arrow definition and setup guidance, then
expose its supported read-only JSON analysis through Quiver.

**Blocked by:** Ticket 3: Project Arrow Knowledge Into Context Safely; Ticket
4: Invoke Declared Arrow Operations as Tools

**Acceptance criteria:**

- [ ] thndrs presents mccabre as a bundled but independently installed arrow.
- [ ] The setup path identifies an absent or incompatible host executable
      without attempting installation.
- [ ] Supported mccabre analysis runs from the chosen workspace and returns
      structured evidence.
- [ ] Artifact-writing/report operations and remote authority are not exposed.
- [ ] mccabre threshold output is not misrepresented as an enforced quality
      gate.
- [ ] A real local mccabre smoke test is documented, while automated tests use
      the deterministic shell fixture.

**Verification:**

- Run the headless Quiver fixture suite without mccabre installed.
- When available, manually run the documented smoke path with a real mccabre.
- Run the workspace Rust checks.

## Ticket 6: Document and Review the Proven Vertical Slice

**What to build:** Document how developers install, enable, inspect, learn, and
use an arrow; update product comparison language only to claims proven by the
mccabre vertical slice.

**Blocked by:** Ticket 5: Ship mccabre as the First-Class Read-Only Arrow

**Acceptance criteria:**

- [ ] User-facing setup documentation distinguishes arrow installation records
      from external executable installation.
- [ ] Documentation explains global/project scope, project-local script trust,
      health state, enablement, and learned-overlay boundaries.
- [ ] The comparison language presents Quiver as an implemented, transparent
      toolchain extension capability without claiming deferred features.
- [ ] Public documentation builds successfully.

**Verification:**

- Review the setup flow on a host without mccabre, then with a real installed
  mccabre.
- Run the documentation build and the workspace Rust checks.

## Frontier

Ticket 1 can start immediately. Tickets 2 and 3 establish the safe
user-visible lifecycle; Ticket 4 enables real operations; Ticket 5 proves the
first end-to-end arrow; Ticket 6 communicates only the capability that exists.
