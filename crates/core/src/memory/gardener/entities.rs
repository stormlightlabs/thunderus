//! Entity types extracted from session events
//!
//! These represent durable knowledge that can be promoted from episodic
//! session history into semantic memory.

use serde::{Deserialize, Serialize};

/// A shell command with its context and outcome
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandEntity {
    /// The command string that was executed
    pub command: String,
    /// Working directory context (optional)
    pub cwd: Option<String>,
    /// Arguments passed to the command
    pub args: Vec<String>,
    /// Outcome of the command
    pub outcome: CommandOutcome,
    /// Source event IDs that produced this entity
    pub event_ids: Vec<String>,
}

/// Outcome of a shell command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
pub enum CommandOutcome {
    /// Command succeeded
    Success,
    /// Command failed with an error
    Failure,
    /// Partial success (e.g., tests passed but some skipped)
    Partial,
}

/// A gotcha: error and resolution pair
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GotchaEntity {
    /// Description of the issue that was encountered
    pub issue: String,
    /// How the issue was resolved
    pub resolution: String,
    /// Category of the gotcha
    pub category: GotchaCategory,
    /// Source event IDs that produced this entity
    pub event_ids: Vec<String>,
}

/// Category of gotcha
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
pub enum GotchaCategory {
    /// Build-related issues (compilation, dependencies)
    Build,
    /// Test-related issues (failing tests, flaky tests)
    Test,
    /// Runtime issues (panics, errors in production)
    Runtime,
    /// Configuration issues (wrong settings, missing config)
    Config,
    /// Other issues
    Other,
}

impl std::fmt::Display for GotchaCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                GotchaCategory::Build => "Build",
                GotchaCategory::Test => "Test",
                GotchaCategory::Runtime => "Runtime",
                GotchaCategory::Config => "Config",
                GotchaCategory::Other => "Other",
            }
        )
    }
}

/// A decision made during the session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionEntity {
    /// What was decided
    pub decision: String,
    /// Context and alternatives considered
    pub context: String,
    /// Rationale for the decision
    pub rationale: String,
    /// Source event IDs that produced this entity
    pub event_ids: Vec<String>,
}

/// A workflow: reusable multi-step pattern
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowEntity {
    /// Workflow name/title
    pub title: String,
    /// Optional description of the workflow purpose and context
    pub description: Option<String>,
    /// Steps in sequence
    pub steps: Vec<WorkflowStep>,
    /// Source event IDs that produced this entity
    pub event_ids: Vec<String>,
}

/// A single step in a workflow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    /// Description of what to do
    pub description: String,
    /// Command or action to take (optional)
    pub action: Option<String>,
    /// Expected outcome
    pub outcome: String,
}

/// ADR (Architecture Decision Record) update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdrUpdate {
    /// New ADR number (auto-incremented)
    pub number: u32,
    /// ADR title
    pub title: String,
    /// Status: proposed, accepted, deprecated, superseded
    pub status: AdrStatus,
    /// Context section content
    pub context: String,
    /// Decision section content
    pub decision: String,
    /// Consequences section content
    pub consequences: String,
    /// If this supersedes an existing ADR
    pub supersedes: Option<String>,
    /// Source event IDs
    pub event_ids: Vec<String>,
}

impl AdrUpdate {
    /// Generate the markdown content for this ADR
    pub fn to_markdown(&self) -> String {
        let status_str = match self.status {
            AdrStatus::Proposed => "Proposed",
            AdrStatus::Accepted => "Accepted",
            AdrStatus::Deprecated => "Deprecated",
            AdrStatus::Superseded => "Superseded",
        };

        let mut md = format!(
            "## Status\n\n{}\n\n## Context\n\n{}\n\n## Decision\n\n{}\n\n## Consequences\n\n{}",
            status_str, self.context, self.decision, self.consequences
        );

        if let Some(supersedes) = &self.supersedes {
            md.push_str(&format!("\n\n## Supersedes\n\n{}", supersedes));
        }

        md
    }

    /// Compute the next ADR number from the manifest
    pub fn next_number(manifest: &crate::memory::MemoryManifest) -> u32 {
        let mut max_seq = 0;
        for entry in &manifest.docs {
            if let Some(seq_str) = entry.id.strip_prefix("adr.")
                && let Ok(seq) = seq_str.parse::<u32>()
            {
                max_seq = max_seq.max(seq);
            }
        }
        max_seq + 1
    }
}

/// ADR status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
pub enum AdrStatus {
    /// Proposed but not yet accepted
    Proposed,
    /// Accepted and active
    Accepted,
    /// Deprecated but kept for reference
    Deprecated,
    /// Superseded by a newer decision
    Superseded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_entity() {
        let cmd = CommandEntity {
            command: "cargo test".to_string(),
            cwd: Some("/workspace".to_string()),
            args: vec!["--all".to_string()],
            outcome: CommandOutcome::Success,
            event_ids: vec!["evt_001".to_string()],
        };

        assert_eq!(cmd.command, "cargo test");
        assert_eq!(cmd.outcome, CommandOutcome::Success);
    }

    #[test]
    fn test_gotcha_entity() {
        let gotcha = GotchaEntity {
            issue: "Missing feature flag".to_string(),
            resolution: "Added --features foo".to_string(),
            category: GotchaCategory::Build,
            event_ids: vec!["evt_001".to_string(), "evt_002".to_string()],
        };

        assert_eq!(gotcha.category, GotchaCategory::Build);
        assert!(gotcha.resolution.contains("features"));
    }

    #[test]
    fn test_decision_entity() {
        let decision = DecisionEntity {
            decision: "Use tokio-rusqlite for async SQLite".to_string(),
            context: "Need async database access".to_string(),
            rationale: "Better performance than synchronous SQLite".to_string(),
            event_ids: vec!["evt_003".to_string()],
        };

        assert!(decision.decision.contains("tokio-rusqlite"));
    }

    #[test]
    fn test_workflow_entity() {
        let workflow = WorkflowEntity {
            title: "Run tests before commit".to_string(),
            description: Some("Pre-commit checks to ensure code quality".to_string()),
            steps: vec![
                WorkflowStep {
                    description: "Format code".to_string(),
                    action: Some("cargo fmt".to_string()),
                    outcome: "Code is formatted".to_string(),
                },
                WorkflowStep {
                    description: "Run clippy".to_string(),
                    action: Some("cargo clippy".to_string()),
                    outcome: "No warnings".to_string(),
                },
            ],
            event_ids: vec![],
        };

        assert_eq!(workflow.steps.len(), 2);
        assert_eq!(workflow.steps[0].action, Some("cargo fmt".to_string()));
    }

    #[test]
    fn test_adr_update_to_markdown() {
        let adr = AdrUpdate {
            number: 1,
            title: "Use imara-diff for code diffing".to_string(),
            status: AdrStatus::Accepted,
            context: "Need semantic diffing".to_string(),
            decision: "Use imara-diff crate".to_string(),
            consequences: "- Positive: Better diffs\n- Negative: Additional dependency".to_string(),
            supersedes: None,
            event_ids: vec![],
        };

        let md = adr.to_markdown();
        assert!(md.contains("## Status"));
        assert!(md.contains("Accepted"));
        assert!(md.contains("imara-diff"));
    }

    #[test]
    fn test_adr_update_with_supersedes() {
        let adr = AdrUpdate {
            number: 2,
            title: "New approach".to_string(),
            status: AdrStatus::Accepted,
            context: "Context".to_string(),
            decision: "New decision".to_string(),
            consequences: "Consequences".to_string(),
            supersedes: Some("adr.0001".to_string()),
            event_ids: vec![],
        };

        let md = adr.to_markdown();
        assert!(md.contains("## Supersedes"));
        assert!(md.contains("adr.0001"));
    }
}
