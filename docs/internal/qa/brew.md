# Homebrew release checklist

Complete the [common release gate](README.md#common-release-gate) and the
applicable [Cargo release](cargo.md) before starting this checklist.

The v0.2 Homebrew cask installs prebuilt binaries. It does not build the Cargo
package. Complete the [Cargo release checklist](cargo.md) for each crate that
changed, using its v0.2 version, before publishing the binary channel.

The application and library version independently. If only the application
changed, publish only `thndrs 0.2.0`. If the library changed, publish it first,
wait for registry availability, update the application's dependency, and then
publish the application.

## 1. Produce release archives

- [ ] Tag the exact Cargo-published and clean-install-tested application
      revision as `v0.2.0`.
- [ ] Build `thndrs` from that tag for:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-unknown-linux-gnu`
- [ ] Each archive contains one executable named `thndrs` and any explicitly
      supported completions or man pages.
- [ ] Archive names are stable and encode version, operating system, and
      architecture.
- [ ] Generate a SHA-256 checksum manifest for every archive.
- [ ] Publish the archives and checksum manifest on the `v0.2.0` GitHub release.
- [ ] Download every published archive, verify its checksum, and confirm
      `thndrs --version` reports `0.2.0` on each supported target.

Record:

- GitHub release:
- Build workflow and revision:
- Archive names and sizes:
- Checksum manifest:
- Target smoke results:

## 2. Add the Homebrew cask

Use the existing public tap at
`https://github.com/stormlightlabs/homebrew-tap`. Its canonical cask directory
is `Casks/`; do not add a second lowercase `casks/` entry.

- [ ] Add `Casks/thndrs.rb` with cask token `thndrs`.
- [ ] Set the version to `0.2.0`.
- [ ] Point each macOS and Linux architecture block at the immutable GitHub
      release archive for that target.
- [ ] Copy SHA-256 values from the verified release checksum manifest.
- [ ] Set the homepage to the project repository and the description to the
      package description.
- [ ] Install the `thndrs` artifact with `binary "thndrs"`.
- [ ] Add only artifacts that are present in every matching archive.
- [ ] Do not merge the tap change until all referenced release URLs are public
      and immutable.

From a checkout of the tap, run:

```sh
brew style Casks/thndrs.rb
brew audit --new --cask thndrs
brew install --cask stormlightlabs/tap/thndrs
thndrs --version
brew uninstall --cask thndrs
```

- [ ] Style and audit pass.
- [ ] A fresh install succeeds on supported macOS architectures.
- [ ] Linuxbrew installs are tested for the Linux targets claimed by the cask.
- [ ] The installed binary reports `thndrs 0.2.0`.
- [ ] Uninstall removes the installed binary.
- [ ] `brew upgrade` is tested from the previous Homebrew release once one
      exists.

Record:

- Tap change and reviewed revision:
- Style and audit results:
- macOS install and uninstall results:
- Linux install and uninstall results:
- Merge approval and result:

## 3. Finish v0.2

- [ ] Verify the one-command install from a machine that did not already have
      the tap:

  ```sh
  brew install --cask stormlightlabs/tap/thndrs
  ```

- [ ] Update public installation docs only after the command succeeds.
- [ ] Keep `cargo install --locked thndrs` documented as the source-build
      alternative.
- [ ] Confirm the cask, release notes, checksum manifest, and public docs all
      name the same application version.

## References

- [Homebrew tap guide](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Homebrew Cask Cookbook](https://docs.brew.sh/Cask-Cookbook)
