# Cargo Release Checklist

Complete the [common release gate](README.md#common-release-gate) before starting this checklist.

The library must reach crates.io before Cargo can verify the application's
registry dependency. Treat the two publishes as separate irreversible actions
with separate approval gates.

## 1. Review the `thndrs-agent` archive

```sh
cargo publish -p thndrs-agent --dry-run --locked
cargo package -p thndrs-agent --locked --list
tar -tzf target/package/thndrs-agent-0.1.0.crate
```

- [ ] The archive contains `Cargo.toml`, `README.md`, `LICENSE`, intended Rust
      sources, the context replay benchmark, and its fixture.
- [ ] It contains no credentials, `.thndrs` state, logs, editor files, build
      output, application sources, internal plans, or unrelated files.
- [ ] The packaged README example and API documentation describe a
      provider-neutral library boundary.
- [ ] Public APIs expose no application session, terminal, CLI, provider client,
      or provider wire types.
- [ ] The dry run builds the crate from the packaged contents.

Record:

- Archive path and size:
- Archive reviewer:
- Included sources and fixtures:
- Exclusion result:
- Dry-run result:

## 2. Publish `thndrs-agent`

Confirm that the release owner has a narrowly scoped crates.io token available
to Cargo. Do not paste the token into this file or captured terminal output.

```sh
cargo publish -p thndrs-agent --locked
```

- [ ] The upload succeeds.
- [ ] crates.io serves version `0.1.0` and its metadata is correct.
- [ ] The version is available through the registry index:

  ```sh
  cargo info --registry crates-io thndrs-agent@0.1.0
  ```

- [ ] docs.rs builds the public API documentation without warnings.

Record:

- Approval:
- Publish result:
- crates.io page:
- Registry availability:
- docs.rs page:

If Cargo times out while waiting for the index, do not upload again. Check the
crate page and repeat `cargo info` until the published version appears or the
registry reports that the upload failed.

## 3. Review the final `thndrs` archive

Run this only after `thndrs-agent 0.1.0` is available from the registry:

```sh
cargo publish -p thndrs --dry-run --locked
cargo package -p thndrs --locked --list
tar -tzf target/package/thndrs-0.1.0.crate
```

- [ ] Cargo resolves `thndrs-agent = "0.1.0"` from crates.io while verifying
      the package.
- [ ] The archive contains the Apache-2.0 license, package metadata,
      application README, sources, tests, snapshots, prompt fragments, and the
      intended provider and ACP fixtures.
- [ ] It contains no credentials, `.thndrs` state, logs, editor files, build
      output, internal plans, or unrelated files.
- [ ] The packaged README's links and installation commands work without a
      source checkout.
- [ ] The dry run builds the application from the packaged contents.

Record:

- Archive path and size:
- Archive reviewer:
- Included tests and fixtures:
- Exclusion result:
- Dry-run result:

## 4. Publish `thndrs`

```sh
cargo publish -p thndrs --locked
```

- [ ] The upload succeeds.
- [ ] crates.io serves version `0.1.0` with the correct metadata and README.
- [ ] The version is available through the registry index:

  ```sh
  cargo info --registry crates-io thndrs@0.1.0
  ```

Record:

- Approval:
- Publish result:
- crates.io page:
- Registry availability:

## 5. Prove a clean registry install

Use a disposable Cargo root so the smoke cannot use a workspace build or an
existing `thndrs` binary:

```sh
release_root="$(mktemp -d)"
mkdir -p "$release_root/home" "$release_root/cargo"
HOME="$release_root/home" CARGO_HOME="$release_root/cargo" \
  cargo install --locked --version 0.1.0 thndrs
HOME="$release_root/home" \
  "$release_root/cargo/bin/thndrs" --version
```

- [ ] Cargo downloads and installs `thndrs 0.1.0` from crates.io.
- [ ] The installed binary reports `thndrs 0.1.0`.
- [ ] Complete the installed application acceptance checks with that binary.
- [ ] Remove the disposable directory after recording redacted evidence.

Record:

- Install command and result:
- Installed version:
- Setup and diagnostics result:
- Provider and terminal smoke result:

## 6. Finish v0.1

- [ ] Review the final crates.io pages, docs.rs output, public docs, and
      repository links.
- [ ] Confirm this checklist contains no secrets or account data.
- [ ] Tag the exact published and tested revision as `v0.1.0` only after both
      crate publishes and the clean-install smoke succeed.
- [ ] Publish release notes from the matching changelog section.
- [ ] Record any failure or workaround before closing the release.

Record:

- Final review:
- Tag approval and result:
- Release notes:

## Existing v0.1 Package Evidence

These preliminary reviews were completed on 2026-07-16. They are mechanical
evidence, not publication approval, and do not replace the clean release
commands above.

### `thndrs-agent 0.1.0`

- Archive: `target/package/thndrs-agent-0.1.0.crate`
- Contents: 20 files, 176.3 KiB before compression, 43.4 KiB compressed
- Included material: Apache-2.0 license, package metadata, library README, 13
  Rust source files, the context replay benchmark, and its JSON fixture
- Exclusion review: no credentials, `.thndrs` state, logs, editor files, build
  output, application sources, internal plans, or unrelated files
- Boundary review: the public modules expose agent contracts, context policy,
  accounting, replay evaluation, hooks, cancellation, and run control. They do
  not expose application session, terminal, CLI, provider client, or provider
  wire types. Path values are inert context metadata; the crate performs no
  filesystem I/O.
- Verification: a clean copy without repository metadata passed
  `cargo package -p thndrs-agent --locked`; Cargo packaged and compiled the
  archive successfully.

### `thndrs 0.1.0`

Cargo assembled this preliminary archive with a command-local crates.io patch
pointing to the workspace copy of `thndrs-agent`; the package manifest still
names the registry dependency. Repeat the normal verification after
`thndrs-agent 0.1.0` is available from crates.io.

- Archive: `target/package/thndrs-0.1.0.crate`
- Contents: 226 files, 2.8 MiB before compression, 542.4 KiB compressed
- Included material: Apache-2.0 license, package metadata, application README,
  Rust sources, snapshots, tests, prompt fragments, three provider fixtures,
  and two ACP smoke-test fixtures
- Exclusion review: no credentials, `.thndrs` state, logs, editor files, build
  output, internal plans, or unrelated files

## References

- [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [`cargo publish` reference](https://doc.rust-lang.org/cargo/commands/cargo-publish.html)
