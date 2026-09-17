#!/usr/bin/env bash
# Warm the dependency caches a cloud session starts without.
#
# A web container begins with no ~/.cargo/registry, no target/, and no
# docs/node_modules, so the check gate in CLAUDE.md would otherwise download
# every dependency before the first command a reviewer asks for. Local
# checkouts already have these, so this exits early outside the cloud.
set -uo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-.}"

# The image ships whatever stable was current when it was built, while CI
# installs the current stable on every run. Checks that pass here then fail
# there on lints the older compiler does not have yet. The MSRV job at 1.88 is
# what guards compatibility, so tracking stable here costs a few seconds and
# removes the gap.
# Updating stable is not selecting it: an image whose default is a
# version-named toolchain would download a compiler nothing runs. --no-self-update
# keeps rustup itself out of the critical path.
rustup update --no-self-update stable >&2 || echo "updating the stable toolchain failed" >&2
rustup default stable >&2 || echo "selecting the stable toolchain failed" >&2
# Say what the session actually holds, so a half-finished update is visible in
# the log rather than inferred from a later compile error.
rustup show active-toolchain >&2 || true

# Neither cache depends on the other, so a registry failure must not also leave
# the docs cold. Each step reports and carries on.
#
# Both write to stderr: a SessionStart hook's stdout lands in the session's
# context, and no one needs a package list there.

# --locked so the hook never edits Cargo.lock; CI runs --locked too.
cargo fetch --locked >&2 || echo "warming the cargo registry failed" >&2

# --frozen-lockfile matches the docs job in .github/workflows/ci.yml.
pnpm --dir docs install --frozen-lockfile >&2 || echo "installing docs dependencies failed" >&2

# freeze renders a committed .ansi capture to a picture for whoever wants to
# look at one. No image is committed, so a missing renderer costs a picture and
# never a capture run: this reports and carries on like the steps above.
#
# v0.2.2 drops \e[3m italic, \e[2m dim, \e[7m reverse, and basic backgrounds,
# and renders bold, underline, every foreground, 256-color backgrounds, and
# truecolor. ratatui_style sets ITALIC and DIM
# (crates/thndrs/src/cli/renderer/ratatui.rs:77-93), so a regression in either
# leaves the image unchanged and shows only in the ANSI diff. A frame correct in
# the capture and wrong in the image is a freeze defect, not an application one.
freeze_version=v0.2.2

# go install writes to $(go env GOPATH)/bin, which is not on this image's PATH,
# so an install that succeeds there still leaves a capture run with no renderer.
# GOBIN puts the binary somewhere the shell looks.
freeze_bin="${HOME:-/root}/.local/bin"
mkdir -p "$freeze_bin" || echo "creating $freeze_bin failed" >&2
GOBIN="$freeze_bin" go install "github.com/charmbracelet/freeze@${freeze_version}" >&2 ||
  echo "installing freeze ${freeze_version} failed" >&2

# An install that landed off PATH is indistinguishable from no install at all
# for whoever renders a capture, and only the binary being reachable says which
# happened.
command -v freeze >&2 || echo "freeze is not on PATH; captures render as ANSI text alone" >&2
