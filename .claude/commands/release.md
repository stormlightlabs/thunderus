---
description: Cut a release from edge to main, tag it, and publish
argument-hint: [version]
---

Use the `release` skill.

Version: $ARGUMENTS (a semver version, or empty to derive it from the changelog)

Confirm with me three times: before opening the release pull request, before
tagging, and before publishing to crates.io. Each one is a step nothing
downstream can undo.

Report what the version is, what the changelog says, and what verification ran.
