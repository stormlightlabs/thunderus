# Skill Tasks

## EXT-4: Improve skill compatibility diagnostics

- [ ] Show declared compatibility and missing required tools or commands in
      `skills doctor`.
- [ ] Diagnose missing or incompatible dependencies without running installers
      or project code.
- [ ] Prevent metadata from changing permissions or tool availability.
- [ ] Preserve unknown optional metadata without treating it as policy.
- [ ] Retain duplicate resolution and bounded progressive loading.

## EXT-5: Design trusted skill distribution

**Blocked by:** EXT-4 and an approved supply-chain design.

- [ ] Define metadata validation, remote-fetch safety, reference-depth limits,
      provenance, updates, removal, and revocation.
- [ ] Keep marketplace, install, and sharing behavior explicit rather than
      implying a marketplace already exists.
- [ ] Add a new extension layer only when skills, commands, CLIs, and MCP cannot
      express an evidenced capability.

## EXT-6: Keep orchestration at the skill boundary

**Blocked by:** Stable packaged runtime surfaces.

- [ ] Update `hybrid-orchestration` when packaged ACP or run interfaces can
      replace its pane-driving steps.
- [ ] Keep worker scheduling and hierarchy in skills and external clients, not
      in `thndrs`.
