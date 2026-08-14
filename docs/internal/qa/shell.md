# Shell installer release checklist

Complete the [common release gate](README.md#common-release-gate),
[Cargo release](cargo.md), and [Homebrew release](brew.md) before starting this
checklist.

The shell installer follows the first Homebrew release. It reuses the tagged
release archives and checksum manifest introduced for v0.2; it must not
maintain a second binary build path.

## 1. Define the installer contract

- [ ] Use POSIX `sh` unless a required feature has a documented portability
      reason for another shell.
- [ ] Support macOS and Linux on arm64 and x86_64, matching the published
      archive matrix.
- [ ] Reject unsupported operating systems and architectures with an actionable
      error before downloading an archive.
- [ ] Install to a user-writable directory by default, such as
      `${XDG_BIN_HOME:-$HOME/.local/bin}`, without `sudo`.
- [ ] Support an explicit version and installation directory so CI and users
      can reproduce an install.
- [ ] Download only over HTTPS with redirects, transport failures, and HTTP
      failures treated as errors.
- [ ] Download into a temporary directory, clean it with a trap, and move the
      verified binary into place atomically.
- [ ] Verify the selected archive against the published SHA-256 manifest before
      extracting or installing it.
- [ ] Use `sha256sum` or `shasum -a 256` and fail with an actionable message if
      neither is available.
- [ ] Never print credentials, environment contents, or unrelated filesystem
      paths.
- [ ] Explain how to add the installation directory to `PATH` without editing
      shell startup files automatically.
- [ ] A failed install leaves an existing `thndrs` binary untouched.

## 2. Test the script

- [ ] Unit-test OS and architecture mapping, URL construction, checksum
      selection, unsupported targets, and tool detection.
- [ ] Test the script in clean macOS and Linux environments for all claimed
      architectures.
- [ ] Test paths containing spaces, a missing installation directory, an
      unwritable destination, interrupted downloads, HTTP failures, checksum
      mismatches, and malformed archives.
- [ ] Confirm a pinned install reports the candidate version.
- [ ] Confirm rerunning the installer is idempotent.
- [ ] Confirm an upgrade replaces only the managed `thndrs` binary.
- [ ] Run a shell linter and formatter chosen by the implementation change.

Record:

- Script revision:
- Test matrix:
- Failure-path results:
- Pinned install result:
- Upgrade result:

## 3. Publish the installer

Publish in this order:

1. Complete the Cargo release and clean registry install for the candidate.
2. Publish and verify the tagged binary archives and checksum manifest.
3. Update and test the matching Homebrew cask.
4. Publish an immutable, versioned copy of the installer.
5. Run the versioned installer from its public URL in clean environments.
6. Point the stable installer URL at the tested versioned copy.
7. Update the public installation docs with a download-and-inspect command and
   an optional pipe-to-shell convenience command.

- [ ] The stable URL returns the reviewed script over HTTPS.
- [ ] The script installs only an archive whose checksum appears in the matching
      immutable release manifest.
- [ ] Public docs show how to download and inspect the script before running it.
- [ ] Public docs state the default destination, supported targets, version
      pinning syntax, and uninstall command.
- [ ] The Cargo, Homebrew, and shell paths all install the same candidate
      version and application behavior.

Record:

- Versioned installer URL:
- Stable installer URL:
- Public checksum source:
- Clean install results:
- Stable URL approval and result:
