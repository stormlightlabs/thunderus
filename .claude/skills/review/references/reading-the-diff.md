# Reading the diff

Why the review skill pins a commit, fetches before reading, and keeps its own
files outside the repository.

## Pin the commit

A branch moves while a review reads it. Take the head commit once and review
that, so every finding describes one state of the tree:

```sh
git fetch origin <branch>
commit=$(git rev-parse origin/<branch>)
git show "$commit":<path>
git diff origin/edge..."$commit"
```

The fetch is not optional. A cloud container clones a few branches at a shallow
depth, so the branch under review is usually absent and `git show` fails on a
commit the repository has never had.

## Do not read the working tree

`Read`, `Grep`, and `Glob` read the working tree, which holds whatever branch
the session sits on and is rarely the one under review. Where reading a tree is
worth the setup, take a worktree that cannot move:

```sh
git worktree add --detach ../thndrs-worktrees/review-<n> "$commit"
```

Git allows that beside the worktree that holds the branch. Remove it when the
pass ends.

## Write nothing inside the repository

A reviewer with no worktree of its own shares a checkout with whoever is
writing there, and a findings file left in that checkout is untracked work
belonging to nobody. Keep the comment body in the session's scratch directory.

## Transport

The `github-board` skill's Transport section decides whether a run uses `gh` or
the GitHub MCP tools. Use one transport for the whole pass.

```sh
gh pr view <n> --json title,body,headRefName,baseRefName,files
gh pr diff <n>
git log --oneline origin/edge..<branch>
```

Through MCP: `pull_request_read` methods `get`, `get_files`, and `get_diff`.
The `git log` line is local either way.
