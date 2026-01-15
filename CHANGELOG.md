# CHANGELOG

## [Unreleased]

### Added

#### [2026-01-15]

- Tool execution display with risk classification reasoning, improved tool descriptions,
output truncation in brief mode, and execution time tracking.

- Implemented a unified event multiplexer that handles real-time model streaming, tool
execution cards, and interactive approval prompts within a single non-blocking loop.

- Session persistence and recovery using a JSONL event model, allowing transcripts to be
reconstructed from append-only logs and ensuring context is preserved across restarts.

- Structured error handling for network and provider timeouts, automatic git branch
tracking in the TUI header, and shell completion generation for the CLI.

#### [2026-01-13]

- Keyboard-driven interface with fuzzy file discovery (@), message history navigation,
and external editor integration (Ctrl+Shift+G) for complex prompts.

- Expanded toolset with direct shell command execution (!cmd), sidebar ui, and
multi-level progress disclosure for action cards.

- Added core slash commands (/model, /approvals, /status, /plan) to control agent
behavior, permission policies, and project context directly from the composer.

- Developed a provider-agnostic agent architecture with streaming support and dedicated
adapters for GLM-4.7 and Gemini backends.

- Implemented a responsive TUI transcript featuring width-aware rendering, inline tool
execution cards with risk classification reasoning, and an interactive approval protocol
for gated operations.

#### [2026-01-12]

- Established the core repository structure and session management system using a Rust
workspace and JSONL-based event logging for full auditability.

- Implemented a flexible, profile-based configuration engine for managing model
providers, approval modes, and environment-specific settings.
