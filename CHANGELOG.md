# CHANGELOG

## [Unreleased]

### Added

#### [2026-01-26]

- Finalized provider implementations for GLM-4.7 and Gemini 3 with thinking modes, resilience retries, and structured error handling.

- Test harness (`thunderus test`) including mock providers and scenario replay for deterministic QA and debugging.

#### [2026-01-24]

- Mixed-Initiative Collaboration with drift detection to automatically pause the agent when external file changes are detected.

- "User Right-of-Way" protocol and "Reconcile Ritual" to safely handle concurrent edits and interrupt agent generation.

#### [2026-01-23]

- Memory Gardener to consolidate session history into stable, deduplicated artifacts (FACTS, ADRs) and prevent memory rot with hygiene rules.

- TUI-based Inspector to trace agent beliefs back to source events, enforcing strict provenance links between memory and session logs.

#### [2026-01-22]

- SQLite-backed lexical search engine with FTS5 for retrieval of memory documents and session recaps.

- Repo-native hierarchical memory system categorized into core, semantic, procedural, and episodic tiers with patch-driven updates.

- Automated retrieval policy for relevant context chunks with full provenance citations during task execution.

#### [2026-01-21]

- Event-driven record of agent actions using JSONL with materialized Markdown views for project memory and plans.

- Dedicated context pane and slash commands for real-time search and plan management from the TUI

#### [2026-01-20]

- Overhauled the TUI with full background fills, styled message bubbles, and refined component layouts.

- Dual-theme architecture supporting Iceberg and Oxocarbon variants, persistent theme selection, and global exit keybinds and animated streaming indicators.

#### [2026-01-19]

- "PR-stack" style patch management system using the imara-diff Histogram algorithm for semantic code diffs and conflict-aware editing.

- Hunk labeling to categorize changes by intent and conflict messaging to provide clear resolution strategies when git applications fail.

- Action cards now have automated task context tracking and scope extraction to provide clearer rationale for agent operations during approval prompts.

#### [2026-01-18]

- Multi-tiered approval and sandboxing system with read-only, auto, and full-access modes to safely gate agent actions and shell commands.

- Workspace boundary enforcement and a command risk classifier that provides pedagogical context and mandatory backups for destructive operations.

#### [2026-01-16]

- Text processing tools including ripgrep-powered search, gitignore-aware file discovery, and atomic find-replace primitives.

- Tools with a session-aware dispatcher that enforces "read-before-edit" protocols and maintains a full audit trail of file modifications.

#### [2026-01-15]

- Tool execution display with risk classification reasoning, improved tool descriptions, output truncation in brief mode, and execution time tracking.

- Implemented a unified event multiplexer that handles real-time model streaming, tool execution cards, and interactive approval prompts within a single non-blocking loop.

- Session persistence and recovery using a JSONL event model, allowing transcripts to be reconstructed from append-only logs and ensuring context is preserved across restarts.

- Structured error handling for network and provider timeouts, automatic git branch tracking in the TUI header, and shell completion generation for the CLI.

#### [2026-01-13]

- Keyboard-driven interface with fuzzy file discovery (@), message history navigation, and external editor integration (Ctrl+Shift+G) for complex prompts.

- Expanded toolset with direct shell command execution (!cmd), sidebar ui, and multi-level progress disclosure for action cards.

- Core slash commands (/model, /approvals, /status, /plan) to control agent behavior, permission policies, and project context directly from the composer.

- Provider-agnostic agent architecture with streaming support and dedicated adapters for GLM-4.7 and Gemini backends.

- Responsive TUI transcript featuring width-aware rendering, inline tool execution cards with risk classification reasoning, and an interactive approval protocol for gated operations.

#### [2026-01-12]

- Core repository structure and session management system using a Rust workspace and JSONL-based event logging for full auditability.

- Flexible, profile-based configuration engine for managing model providers, approval modes, and environment-specific settings.
