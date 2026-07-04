---
title: "Reading A New Codebase"
sources:
  - https://piechowski.io/post/git-commands-before-reading-code/
  - https://aider.chat/docs/repomap.html
Author: Ally Piechowski; Aider maintainers
date: 2026-04-08
Tags: [codebase-analysis, git-history, repo-map, onboarding, code-intelligence]
---

## Summary

A good first pass on a new codebase combines two maps: Git history shows where
people have been working, struggling, and firefighting, while a structural code
map shows which files and symbols currently define the system.

## Key Ideas

- **Start with project behavior before source text:** Piechowski's article
  argues that commit history gives a fast diagnostic picture before opening
  files: churn, ownership, bug clusters, project activity, and crisis patterns.
- **Churn and bugs are stronger together:** A frequently changed file may just
  be active, and a bug-fix-heavy file may reflect good cleanup. A file that is
  both high-churn and bug-heavy is the sharper risk signal.
- **Ownership is part of code comprehension:** `git shortlog` can reveal whether
  the current maintainers are the same people who built the system, and whether
  knowledge is concentrated in one former contributor.
- **Aider's repo map is structural context under a token budget:** Aider extracts
  definitions and references, ranks files and symbols with a graph algorithm,
  and renders only the most relevant lines that fit the active map token budget.
- **Mentioned files and identifiers should bias discovery:** Aider personalizes
  ranking when the current request mentions filenames, path components, or
  identifiers, so the map becomes query-sensitive instead of static.
- **The active files and the map serve different jobs:** Aider excludes files
  already in chat from the repo map. Full files provide edit context; the map
  fills in surrounding structure.
- **Refresh behavior is a product contract:** Aider exposes refresh modes and a
  force-refresh path because maps can be expensive and stale context is a real
  risk.

## Claims & Evidence

### Git history can tell you where to read first.

Piechowski proposes five commands before reading code: most-changed files,
contributors by commit count, bug-keyword hotspots, commits by month, and recent
reverts/hotfixes. The article frames these as a first-hour diagnostic, not a
complete audit.

Caveat/confidence: High for usefulness as triage. The signals depend on commit
message quality, merge strategy, and filtering out generated files or lockfiles.

### High churn is not automatically bad, but high churn plus bug density is risky.

The article recommends cross-referencing the most-changed files with files
appearing often in bug-related commits. It cites churn research and Adam
Tornhill's code-forensics work as support for treating churn as a useful defect
risk signal.

Caveat/confidence: Medium-high. This note did not independently review the cited
research; the article's concrete command is still a practical heuristic.

### Contributor history can reveal knowledge concentration and team change.

`git shortlog -sn --no-merges` shows contributor concentration. Comparing the
all-time list with a recent time window can reveal whether the people who built
the system are still maintaining it.

Caveat/confidence: Medium. Squash-merge workflows can make the result reflect
mergers rather than authors.

### Aider's repo map extracts definitions and references from syntax trees.

`RepoMap.get_tags_raw` uses language detection, tree-sitter parsers, and
language-specific tag queries to collect definition and reference tags. When a
language query exposes definitions but not references, Aider can backfill
identifier references with Pygments tokens.

Caveat/confidence: High. The mechanism is visible in `aider/repomap.py`; quality
depends on language support and tag-query coverage.

### Aider ranks structural relevance with a graph.

`RepoMap.get_ranked_tags` builds a graph where files are nodes and identifier
references create weighted edges from referencers to definers. It uses PageRank,
then distributes rank back to definitions so important files and symbols rise to
the top.

Caveat/confidence: High. The exact ranking is heuristic: identifiers mentioned
by the user, longer compound names, public names, and references from active
chat files receive different weights.

### Aider renders maps as compact code context, not just file lists.

The repo-map docs describe a concise whole-repository map containing important
classes, functions, types, call signatures, and critical definition lines.
`RepoMap.to_tree` groups selected lines by file and uses `TreeContext` to render
nearby structural context while truncating long lines.

Caveat/confidence: High.

### Token budget is enforced by selection, not by blind truncation.

`RepoMap.get_ranked_tags_map_uncached` binary-searches over the ranked tag list
to find the largest rendered tree that fits the requested token budget within a
tolerance. When no files are in chat, Aider can expand the map using
`map_mul_no_files` to provide a broader first view.

Caveat/confidence: High. Token counting uses an estimate for longer text, so the
budget is approximate.

### Tests define the map's behavioral contract.

Aider's tests cover empty and unsupported files appearing in the map, chat files
being excluded, refresh modes preserving or updating stale results, language
coverage across many fixtures, and comparison against an expected sample
codebase map.

Caveat/confidence: High.

## Important Terms

| Term              | Meaning                                                                                                        |
| ----------------- | -------------------------------------------------------------------------------------------------------------- |
| Churn hotspot     | A file that changes frequently over a chosen time window.                                                      |
| Bug hotspot       | A file that appears often in commits whose messages indicate fixes or breakage.                                |
| Bus factor        | How concentrated project knowledge or authorship appears to be.                                                |
| Crisis pattern    | Repeated reverts, rollbacks, hotfixes, or emergency commits.                                                   |
| Repo map          | Compact structural overview of a repository, usually file names plus important symbols and definition context. |
| Tag               | A symbol record such as a definition or reference, with file and line metadata.                                |
| Personalization   | Ranking bias from current chat files, mentioned filenames, path components, or identifiers.                    |
| Lines of interest | Source lines selected for rendering because they define or surround important symbols.                         |
| Structural map    | Current-code view of relationships between files, symbols, definitions, and references.                        |
| Historical map    | Git-derived view of change frequency, authorship, bug clustering, and delivery rhythm.                         |

## Questions for Review

- What are the five Git history questions Piechowski asks before reading source?
- Why is high churn alone a weaker signal than high churn combined with bug
  density?
- How does squash merging distort contributor analysis?
- How does Aider turn definitions and references into a ranked repo map?
- Why should files already in active context be excluded from the structural map?
- What failure modes come from stale maps, unsupported languages, or weak commit
  messages?

## Connections

- Related ideas: code forensics, context budgeting, symbol indexing,
  project onboarding, change-risk analysis.
- Related sources: Aider repo-map docs, Aider `RepoMap`, Piechowski's Git
  commands article, Adam Tornhill's code-maat/code-forensics work.
- Contradictions or tensions: Git history can overemphasize past pain, while a
  structural map can overemphasize present topology and miss social or
  operational risk.
- Useful applications: first-hour codebase triage, context selection, review
  planning, deciding which files to inspect before changing behavior.

## Open Questions

- Should historical signals and structural repo-map signals be merged into one
  score, or shown as separate lenses?
- Which generated files, vendored paths, lockfiles, and build outputs should be
  filtered before churn analysis?
- How should map refresh policy balance cost, latency, and stale-context risk?
- Can a structural repo map remain useful for dynamic languages or codebases
  with weak static references?
- What is the right output format for a codebase map so both humans and models
  can inspect it?

## Notable Quotes

> "They won't tell you everything."

## Takeaways

- Read a new codebase through both history and structure.
- Use Git to find social and risk hotspots before opening source files.
- Use a ranked repo map to turn large-codebase context into a bounded,
  query-sensitive working set.
