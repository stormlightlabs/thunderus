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
