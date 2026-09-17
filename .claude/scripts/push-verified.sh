#!/usr/bin/env bash
# Push the current branch and prove the remote took it.
#
#   .claude/scripts/push-verified.sh [remote]
#
# `git push` exits 0 for a push that carried nothing: it prints "Everything
# up-to-date" and succeeds. Every word of that is true and any claim of
# "pushed" built on it is false.
#
# A pre-push hook does run on such a push, with empty stdin, so a hook could
# refuse there (verified on git 2.43.0). This script compares the end state
# instead, which also covers a detached HEAD: under the default
# push.default=simple, git exits 128 rather than pushing.
set -euo pipefail

remote="${1:-origin}"

if ! branch=$(git symbolic-ref -q --short HEAD); then
  echo "HEAD is detached at $(git rev-parse --short HEAD)." >&2
  echo "Commits made here belong to no branch and no push will carry them." >&2
  echo "Attach them first:" >&2
  echo "  git branch -f <branch> HEAD && git checkout <branch>" >&2
  exit 1
fi

git push -u "$remote" "$branch"
git fetch -q "$remote" "$branch"

local_sha=$(git rev-parse HEAD)
remote_sha=$(git rev-parse "refs/remotes/$remote/$branch")

if [ "$local_sha" != "$remote_sha" ]; then
  echo "push did not land: $branch is ${remote_sha:0:8} on $remote, ${local_sha:0:8} here" >&2
  exit 1
fi

echo "$branch is ${local_sha:0:8} on $remote"
