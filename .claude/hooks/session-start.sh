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

# Neither cache depends on the other, so a registry failure must not also leave
# the docs cold. Each step reports and carries on.
#
# Both write to stderr: a SessionStart hook's stdout lands in the session's
# context, and no one needs a package list there.

# --locked so the hook never edits Cargo.lock; CI runs --locked too.
cargo fetch --locked >&2 || echo "warming the cargo registry failed" >&2

# --frozen-lockfile matches the docs job in .github/workflows/ci.yml.
pnpm --dir docs install --frozen-lockfile >&2 || echo "installing docs dependencies failed" >&2
