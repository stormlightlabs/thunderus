---
title: v0.1 Release Candidate And Workbench UI
status: Ready
captured: 2026-07-11
---

## Objective

Ship usable `0.1.0` releases of the `thndrs` application and the
`thndrs-agent` primitives library to crates.io. A user must be able to install
the application, complete required in-UI setup and authentication, and use it
to make and verify a real code change. `thndrs` must itself support this
workflow for its own development.

The release makes ChatGPT Codex and Umans first-class, thoroughly tested
providers. It replaces the fixed default model with required UI setup, makes
ChatGPT authentication browser-first rather than device-code-first, then
polishes the transcript-first TUI into a quiet workbench. Before that visual
work, it splits the oversized application module without changing observable
behavior.

## Settled Decisions

- Release both packages as `0.1.0`, using normal SemVer rather than an
  `-alpha.N` prerelease train. Compatible fixes use `0.1.z`; public API
  additions or breaking changes use `0.y.0`; `1.0.0` waits for validated
  external use.
- The packages version independently after their initial releases. A CLI
  change does not force an unrelated library release, and a library change
  communicates its own compatibility boundary.
- `thndrs` is a usable but experimental coding-agent product. It must not
  claim a fixed default model or silently choose an authenticated provider.
- `thndrs-agent` is a developer-facing, experimental primitives library on a
  deliberate path to stability. Its modules remain public: experimental does
  not mean private, unsupported, or disposable. Every public item needs
  accurate documentation and a SemVer-aware change record.
- A fresh `thndrs` installation enters a keyboard-first UI setup gateway before
  a coding workspace is usable. The user chooses a provider and either confirms
  an existing valid credential or authenticates it; no model is selected until
  setup completes. CLI setup remains an equivalent route, not a bypass around
  authentication.
- ChatGPT Codex and Umans are first-class v0.1 providers. Each is a release
  gate: authenticated setup, a real repository coding task, local tool use,
  verification, and session recovery must be demonstrated with real accounts.
  Existing OpenCode and ACP paths remain available but are not presented as the
  first-run default or used to lower these gates.
- ChatGPT Codex uses browser-first OAuth with PKCE. The application launches or
  displays a copyable authorization URL, completes through a short-lived
  loopback callback, and can accept a pasted full redirect URL when that
  callback cannot arrive. Device-code OAuth is a deliberately chosen
  headless/remote path, never a silent or required first attempt. Both paths
  produce the same safely stored, refreshable credential; neither asks for or
  stores a ChatGPT API key.
- Preserve the implemented model-specific `reasoning_effort` and
  `reasoning_summary` controls through the refactor and new setup flow. Their
  existing config, TUI, ACP, provider-lowering, session, redaction, and
  renderer contracts are not a second feature plan; ChatGPT Codex and Umans
  release evidence must exercise their supported paths.
- Preserve behavior while splitting `app.rs`, then update the UI architecture.
  `update(&mut App, Msg) -> Option<Msg>` remains the sole application-state
  mutation path throughout the split.
- The renderer has two explicit lanes. Direct rows own committed transcript
  scrollback, prompt editing, cursor placement, resize replay, and terminal
  I/O. The one centralized iocraft adapter renders only declarative bounded
  surfaces: setup/authentication, pickers, permission decisions, help, and
  inspect/detail views. App modules expose semantic state and events, never
  rows, styles, or iocraft elements.
- The visual tone is a restrained workbench: Unicode frames identify an active
  task or decision, while committed history remains open and scannable.

## Users And Release Promise

### Coding-agent users

- A developer or vibe coder can install `thndrs` from crates.io, follow a
  clear first-run UI path, authenticate with ChatGPT Codex or Umans, and start
  a session in a repository.
- They can see what the agent is doing, inspect tool/diff detail, decide
  sensitive permissions, retain a draft through setup or recoverable failure,
  and resume the resulting session.
- They are told plainly that v0.1 behavior can evolve, that local tools are not
  a sandbox, and which providers are first-class versus advanced.

### Agent-library authors

- An application author can discover `thndrs-agent`, understand its
  provider-neutral contracts and pure context primitives, and compile the
  documented example.
- They can depend on the public experimental API with a clear v0 SemVer policy
  and changelog/migration notes. The library never absorbs provider wire types,
  filesystem policy, terminal I/O, or session persistence.

## Success Criteria

### Packaging And Documentation

- Both packages have complete crates.io metadata, an Apache-2.0 license, a
  useful README, repository/homepage/documentation links, keywords, categories,
  a declared MSRV, and intentionally reviewed package contents.
- `thndrs-agent` can be packaged and verified before publication. After its
  `0.1.0` publication is visible in the registry, `thndrs` packages and
  verifies against the registry version rather than only its path dependency.
- The application README and public docs contain no user-facing placeholders,
  stale “Coming Soon” claims for delivered context controls, or source-checkout
  commands presented as installed-user instructions.
- Installation, first-run setup, provider limitations, safety boundary,
  sessions, and troubleshooting are navigable without reading internal notes.
- The `thndrs-agent` README/API docs identify the stable-direction primitives,
  the experimental contract, and the intended application-owned boundaries.

### First-Run And Provider Readiness

- A fresh HOME and workspace have no implied default model. Launching the app
  shows the setup gateway before a prompt can submit.
- The gateway has an explicit provider choice, provider-specific authentication
  language, hidden secret entry where appropriate, cancellation, failure
  recovery, and clear next action. It never writes secrets to TOML, session
  records, logs, prompt inspection, or rendered views.
- ChatGPT Codex setup recognizes an existing valid credential or completes
  browser-first OAuth through the supported provider route. Its browser path
  exposes a copyable URL and state-validated redirect recovery; device code is
  available only as an explicit headless/remote choice. Umans setup completes
  through its supported credential route. Both persist only their own credential
  material at the existing safe storage boundary.
- Existing model-specific reasoning effort/summary configuration remains
  available after provider-led setup. Unsupported models do not show a
  misleading control, and raw hidden reasoning remains absent from persisted
  or rendered surfaces.
- For both providers, a human records a release smoke in a disposable real
  repository: authenticate, ask for a bounded code change, approve required
  local tool actions, run verification, inspect output, and resume the session.
- Automated tests deterministically cover setup stage transitions, cancellation,
  redaction, provider/model selection, failed authentication, draft retention,
  browser callback and explicit device-code OAuth boundaries, and the
  provider-specific request boundary. Real credentials remain absent from CI and
  fixtures.

### Application And Renderer Architecture

- The current `app.rs` responsibilities are extracted into cohesive child
  modules such as onboarding, input/pickers, commands, context/compaction, and
  agent lifecycle. The root module is a small, documented composition boundary
  for shared state, messages, and `update` routing.
- Every extraction preserves current command, input, permission, persistence,
  session, and renderer behavior before the UI architecture changes.
- The application layer no longer constructs renderer view cells or imports
  presentation-only types for domain decisions. Semantic projection lives in
  the renderer/view boundary.
- `renderer/adapter.rs` remains the only source module that imports or calls
  iocraft. It returns existing `Row` values, performs no terminal writes, owns
  no focus/selection/scroll/app state, and never uses iocraft fullscreen or
  render-loop APIs.
- Direct rendering remains responsible for native transcript scrollback, prompt
  cursor behavior, terminal writes, and resize replay. There is no second UI
  state machine in iocraft.

### Workbench Polish

- Normal work remains one text column. At wide widths, extra room goes to
  trailing metadata and longer tool previews, never to a persistent sidebar,
  dashboard, fake terminal chrome, or a second main panel.
- Setup/authentication, permissions, pickers, help, and detail inspection use
  semantic bounded surfaces with a consistent Unicode frame, title/status row,
  keyboard affordances, explicit focus, and narrow/tiny-height fallbacks.
- Committed transcript entries are not individually carded or boxed. Role,
  spacing, text hierarchy, and minimal left rules make history scannable in
  native scrollback.
- The active prompt/work region provides a compact orientation line: selected
  provider/model, running or idle state, focused operation, and useful compact
  context/queue status without a persistent dashboard.
- Borders, color, and icons are redundant cues; labels and state remain
  understandable in monochrome, small terminals, and terminals with imperfect
  Unicode glyph support.
- Color uses semantic roles from the existing Eldritch Minimal, Iceberg Dark,
  and Catppuccin Mocha palettes rather than page-local palettes. The transcript
  signal grammar remains readable through glyphs, labels, and weight when color
  is absent.
- Unicode display-width, long unbroken paths, CJK, emoji, combining marks,
  clipping above/below, and small terminal dimensions have deterministic
  behavior coverage.

## Current State

The workspace already has two packages at a shared `0.1.0` workspace version.
`thndrs` has good application metadata, but `thndrs-agent` lacks homepage,
documentation, keywords, and categories. `thndrs-agent` packages locally;
`thndrs` correctly cannot package until that dependency version exists in the
registry.

The public `thndrs-agent` surface includes contracts, adapters, tool budgets,
cancellation, background runs, and `context`. It has useful module docs and a
small example, but needs a complete developer-release audit before publication.

The application has deterministic provider, CLI/TUI, session, renderer, and
ACP coverage, plus a thorough manual release QA checklist. Its public landing
documentation and root README still contain placeholders or stale delivery
claims. Fresh-install and real-provider release smoke evidence are missing.

Model-specific `ReasoningEffort`/`ReasoningSummary` controls already resolve
through TOML/environment/effective config, the TUI, ACP, provider request
lowering, session metadata, and status rendering. The previous readiness plan
described a pre-implementation design with different keys and provider
assumptions; its completed contract is archived rather than retained as an
active feature.

The current ChatGPT Codex path inherited an earlier device-code-first decision:
the TUI owns only device-code request/poll states, and the CLI tries device code
before its existing browser-PKCE fallback. The fallback blocks on a loopback
listener, writes its URL directly to stderr, and has no pasted-redirect recovery.
This release plan supersedes that choice; Ticket 8 makes browser OAuth the
normal path while preserving device code as an explicit headless alternative.

`crates/thndrs/src/cli/app.rs` is 4,741 lines. It mixes core state/message
definitions with key routing, focused pickers, setup/OAuth recovery,
slash-command projection, context/compaction, agent lifecycle, and session
persistence. It currently imports a renderer view-cell type for context-table
data. Its renderer is already a separate module family. iocraft is centralized
in `crates/thndrs/src/cli/renderer/adapter.rs`, but the old expansion plan does
not reflect the new release, onboarding, or architecture goals.

## Visual Concepts

The implementation must use these concepts as review artifacts, not as a
browser runtime or a pixel-perfect requirement. They demonstrate one
single-column signal rail, not a sidebar shell or a browser-window simulation:

| Concept                                                                             | Decision it demonstrates                                                                                                 |
| ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| [Setup handoff](../../../../.sandbox/concepts/01-setup-gateway.html)                | Provider choice plus browser-default ChatGPT OAuth, with an explicit headless device-code alternative; no default model. |
| [Active signal rail](../../../../.sandbox/concepts/02-active-workbench.html)        | Open single-column transcript, typed event marks, and a compact focused prompt/work region.                              |
| [In-place guarded decision](../../../../.sandbox/concepts/03-guarded-decision.html) | A high-priority decision in the transcript flow, with visible scope, honest local-process wording, and keyboard focus.   |

The concepts draw on iocraft’s declarative bounded-layout/canvas role and its
border support, Ratatui’s state/update/testing discipline, and Gridland’s
cell-budget and responsive-density lessons. They do not add a component tree or
layout dependency. Their palette variables mirror the renderer’s three existing
themes, and their distinctive character comes from an event/signal grammar—not
from arbitrary browser effects. They intentionally reject a persistent dashboard
or card-per-message layout.

## Technical Plan

### 1. Establish independent package and release contracts

Replace the workspace-inherited package version with explicit package versions.
Keep both initial package versions at `0.1.0`, make the application depend on
the published-compatible `thndrs-agent` line, and document the v0 SemVer policy
in each public README and changelog. Do not publish, tag, or change an initial
version number without the release owner’s final approval.

Finish `thndrs-agent` discovery metadata and audit every public module/type
against its stated provider-neutral boundary. Add documentation tests or a
compile-tested example for the intended client use. Update public package docs
and README copy so installed users receive a complete setup-and-workflow path.

### 2. Split the application coordinator without behavior changes

Keep the existing Elm-style update function as the only state mutation route.
Extract cohesive behavior in compilable, reviewable moves, retaining private
helpers beside the feature they serve. Begin with state that has strong tests
and clean dependencies—onboarding/authentication, input and accessories,
commands, context/compaction, and agent lifecycle/persistence—rather than
introducing traits or a second effect system.

Move semantic presentation projection from the application into renderer view
types. Extraction commits must preserve current snapshots and focused behavior;
no UI redesign is allowed until this boundary is green.

### 3. Make onboarding provider-led instead of default-model-led

Represent setup as typed semantic state independent of terminal rendering.
Remove the app’s fixed default model path. On a fresh installation, route
startup through provider choice, existing-credential recognition or
authentication, model discovery/selection where needed, and an explicit ready
state. Retain equivalent CLI entry points and clear non-interactive failure
messages.

For ChatGPT Codex, represent the authentication method and lifecycle as private
application state. Browser PKCE is preselected: start a short-lived loopback
listener, open or display a copyable authorization URL, validate the callback
state, and offer pasted full-redirect recovery when the callback cannot reach
the application. Device code is a separately selected headless/remote path with
its own start, polling, slow-down, expiry, cancellation, and failure states.
Never silently switch between the two methods. The application adapter owns
browser/URL presentation, loopback transport, and credential persistence;
`thndrs-agent` stays provider-neutral and no generic OAuth framework or new
dependency is introduced without approval.

Strengthen ChatGPT Codex and Umans independently: typed provider setup state,
redaction, credential ownership, request/stream failure behavior, model
availability, and realistic fake coverage. Preserve advanced provider and ACP
configuration; do not conflate their readiness with first-class onboarding.
Preserve the existing model-specific reasoning controls, including their
provider-owned lowering and their no-raw-reasoning persistence boundary.

### 4. Rebuild bounded renderer surfaces around semantic workbench state

Define renderer-owned semantic view data for the active orientation line,
onboarding stages, permissions, picker rows, detail headers, clipping, and
keyboard hints. Keep visual style lookup and Unicode/frame composition inside
renderer modules.

Compose normal work as a single signal rail: terse typed event marks establish
the transcript rhythm, while only the active prompt or a focused decision gains
a bounded surface. Treat narrow, normal, and wide terminals as density changes
within that column, not as invitations to add a side panel.

Evolve the centralized adapter into a small set of pure, snapshot-tested
bounded-surface components. It receives semantic view data, a theme-role set,
and a row budget; it produces `Vec<Row>`. It must preserve priority of setup
and permissions, redaction of secrets, available text when clipped, and direct
renderer ownership of transcript/prompt behavior.

### 5. Verify the real release path

Run the existing deterministic test/check suite, package archives, clean
installation smoke, and the two manual provider coding workflows. Inspect
archives and registry propagation in publication order. The final human release
review confirms docs/screenshots, permissions/safety wording, and real-terminal
legibility before authorizing publication.

## Testing Plan

**Test boundary:** pure `thndrs-agent` contracts/context policy through library
unit and documentation tests; `thndrs` behavior through its existing app
update, CLI, provider fake, renderer snapshot/region, session, and binary
integration boundaries. Use a real terminal and two real provider accounts only
for the final explicit human smoke.

- Add compile/doc tests for the `thndrs-agent` intended consumer flow and
  public-API documentation checks where practical.
- Retain an update/app regression test for every extracted `app` behavior;
  moving a function without a stable behavior check is not a valid extraction.
- Snapshot focused surfaces at normal, narrow, wide, and tiny height. Cover the
  three renderer palettes, monochrome-equivalent labels, secret masking, CJK,
  emoji, combining marks, long paths, long tool output, clipping states, and
  selected/focused rows.
- Add deterministic setup tests for fresh HOME, no default model, provider
  choice, existing ChatGPT credentials, browser OAuth URL/callback/state
  mismatch/pasted-redirect/cancel/failure/success projection, explicitly chosen
  ChatGPT device-code start/poll/slow-down/expiry/cancel/failure/success,
  Umans secret entry/cancel/failure/success projection, model-specific reasoning
  choices, non-interactive instructions, draft retention, and redaction.
- Add provider fake coverage for ChatGPT Codex and Umans model/request/stream
  failure boundaries, while retaining real smoke tests as explicitly ignored or
  human-run tests with no credentials in the repository.
- Test binary-facing `--help`, `--version`, setup/doctor exit behavior, package
  install behavior, and an installed `thndrs` first-run path from a clean HOME.

## Commands

For every Rust implementation slice:

```text
cargo fmt
cargo clippy --workspace --fix --allow-dirty --allow-staged
cargo clippy --workspace
cargo test --workspace
```

For public documentation changes:

```text
pnpm --dir docs build
```

For final package release validation, after the dependency publication order is
available:

```text
cargo package -p thndrs-agent
cargo package -p thndrs
cargo install --locked thndrs
thndrs --version
thndrs setup
```

## Boundaries

Always:

- preserve the application/library ownership split and the single update path;
- keep public `thndrs-agent` modules experimental, documented, and
  provider-neutral;
- keep renderer state semantic and side-effect free where practical;
- centralize iocraft in the adapter and keep terminal I/O direct-renderer owned;
- add behavior/snapshot coverage before changing a visible surface;
- record a changelog entry and migration note for every public library break.

Ask first:

- adding a dependency, workspace crate, provider capability, or default model;
- changing public `thndrs-agent` API, session format, permission semantics, or
  tool policy beyond this documented release contract;
- publishing, tagging, adding owners, or changing registry credentials;
- using real credentials or an external provider account for the final smoke.

Never:

- publish, tag, or alter registry credentials without direct human approval;
- put credentials in TOML, logs, tests, snapshots, sessions, prompt inspection,
  package archives, or visual concepts;
- make ChatGPT device code the default or a prerequisite for browser-capable
  users, silently cross over between OAuth methods, or persist/render an
  authorization code, callback query, PKCE verifier, device-auth identifier, or
  token;
- let iocraft own `App` state, terminal I/O, cursor placement, native
  scrollback, or a render loop;
- make committed transcript entries persistent cards or hide a permission/setup
  decision behind optional UI, add a persistent sidebar, or imply that an
  approval surface is an operating-system sandbox;
- add a new default model implicitly after removing the current one.

## Deferred Milestones

- Evolve individual `thndrs-agent` primitives toward `1.0.0` only after
  external consumer feedback establishes the public API worth stabilizing.
- Bring additional providers to the ChatGPT Codex/Umans first-class bar through
  their own setup, fake-coverage, and real-smoke evidence.
- Add release automation and binary distribution only after manual crates.io
  publication and clean-install evidence are repeatable.
- Broaden iocraft surface migration only when a bounded surface has a semantic
  view model, row-budget/clipping coverage, and a demonstrated reduction in
  layout complexity.

## Risks And Review Points

- Crates.io publication is irreversible and dependency-index propagation can
  delay the application package check. Treat the registry order as a human
  release gate, not an implementation test shortcut.
- The first-run redesign changes a central user journey. A default-model
  removal without clear recovery or non-interactive instructions would make the
  app less usable, not more honest.
- ChatGPT account/OAuth policy and remote provider behavior can change outside
  the repository. Keep the final account smoke explicit and document its date,
  model, and limited scope without storing account data.
- `app.rs` extraction can silently change event ordering, draft retention,
  persistence, or permission priority. Preserve tests at each move and do not
  mix extraction with visual changes.
- Unicode frames vary across terminal fonts. Layout must degrade to text labels
  and remain width-safe instead of assuming every box-drawing glyph is perfect.
