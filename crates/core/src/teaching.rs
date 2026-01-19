//! Pedagogical hints system for teaching users about safe tool usage
//!
//! This module provides a system for tracking "taught concepts" per session
//! and showing brief, one-time hints when risky tools are used for the first time.
//!
//! The teaching system is designed to be non-intrusive:
//! - Hints are shown only once per concept per session
//! - Hints are brief and educational, not nagging
//! - Teaching state is tracked in session metadata

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::ToolRisk;

/// Concept IDs that have been taught in this session
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TeachingState {
    /// Set of concept IDs that have been taught
    taught: HashSet<String>,
}

impl TeachingState {
    /// Create a new teaching state
    pub fn new() -> Self {
        Self { taught: HashSet::new() }
    }

    /// Check if a concept has been taught
    pub fn has_taught(&self, concept: &str) -> bool {
        self.taught.contains(concept)
    }

    /// Mark a concept as taught
    pub fn mark_taught(&mut self, concept: impl Into<String>) {
        self.taught.insert(concept.into());
    }

    /// Get a hint for a concept if it hasn't been taught yet
    ///
    /// Returns `Some(hint)` if this is the first time the concept is encountered, or `None` if it has already been taught.
    pub fn get_hint(&mut self, concept: &str) -> Option<String> {
        if self.has_taught(concept) {
            None
        } else {
            self.mark_taught(concept);
            get_hint_for_concept(concept)
        }
    }

    /// Get all taught concepts
    pub fn taught_concepts(&self) -> impl Iterator<Item = &str> {
        self.taught.iter().map(|s| s.as_str())
    }

    /// Clear all taught concepts (for testing)
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.taught.clear();
    }
}

/// Get the educational hint for a concept
///
/// Public API for getting hints without needing a TeachingState instance.
/// Useful for one-off hint display without tracking teaching state.
pub fn get_hint_for_concept(concept: &str) -> Option<String> {
    match concept {
        "risky_command_explained" => Some(
            "Risky commands (like rm, sed -i, package installs) require approval because they can \
                modify files or system state. Safe commands like grep, cat, and tests run automatically."
                .to_string(),
        ),
        "network_command_explained" => Some(
            "Network commands (curl, wget, ssh) require approval because they transfer data \
                with external systems. Enable network access in config if you need this regularly."
                .to_string(),
        ),
        "sed_risky_explained" => Some(
            "Using 'sed -i' directly is risky because it modifies files in-place without backups. \
                Consider using the Edit tool instead for safer find-replace operations."
                .to_string(),
        ),
        "edit_tool_benefits" => Some(
            "The Edit tool provides safer file modifications with validation, atomic writes, \
                and automatic rollback on failure. It's safer than sed -i for most operations."
                .to_string(),
        ),
        "read_before_edit" => Some(
            "Files must be Read before editing to ensure you're working with current content. \
                This prevents accidental overwrites of changes made outside the session."
                .to_string(),
        ),
        "approval_modes_explained" => Some(
            "Approval modes: read-only (no edits), auto (safe ops auto-approve, risky ops gate), \
                full-access (all logged, no gates). Default is 'auto' for balanced safety."
                .to_string(),
        ),
        "workspace_boundary" => Some(
            "Files outside your workspace roots require explicit approval. This prevents \
                accidental modifications to system files or other projects."
                .to_string(),
        ),
        "backup_on_risky" => Some(
            "Backups are automatically created before risky operations. You can restore from \
                backups if an operation doesn't go as expected."
                .to_string(),
        ),
        "file_destruction" => Some(
            "File deletion operations (rm, shred, rmdir) are permanent and cannot be undone. \
                Consider backing up important files before deletion."
                .to_string(),
        ),
        "package_install" => Some(
            "Package installation commands modify your project dependencies and may break \
                builds if versions conflict. Review changes carefully before approving."
                .to_string(),
        ),
        "git_write_operations" => Some(
            "Git write operations (commit, push, rebase) modify repository history. These \
                changes can be difficult to undo once pushed to remote repositories."
                .to_string(),
        ),
        "shell_permissions" => Some(
            "Shell commands in full-access mode run without approval gates. All commands are \
                still logged to the session for review and debugging."
                .to_string(),
        ),
        "sed_full_access" => Some(
            "Direct sed exposure in full-access mode with mandatory backups. The Edit tool \
                is still safer and more reliable for most find-replace operations."
                .to_string(),
        ),
        "awk_full_access" => Some(
            "Direct awk exposure in full-access mode. Use read-only patterns (without output \
                redirection) for safety. The Read and Edit tools are safer for file manipulation."
                .to_string(),
        ),
        "sed_backup_created" => Some(
            "A backup was created before running sed -i. If the result is unexpected, you can \
                restore from the backup. Use the Edit tool for safer operations with automatic rollback."
                .to_string(),
        ),
        "awk_read_only_recommended" => Some(
            "Awk works best for read-only data transformation. For file modifications, use \
                Read + Edit tools for better safety and validation."
                .to_string(),
        ),
        _ => None,
    }
}

/// Suggest a concept to teach based on context
///
/// This analyzes the context (action type, risk level, tool/command name) and returns an appropriate concept ID to teach.
pub fn suggest_concept(action_type: &str, risk_level: ToolRisk, context: &str) -> Option<String> {
    match (action_type, risk_level.is_risky()) {
        ("shell", true) => match context {
            c if c.contains("rm") || c.contains("shred") || c.contains("rmdir") => Some("file_destruction".to_string()),
            c if c.contains("sed -i") || c.contains("sed --in-place") => Some("sed_risky_explained".to_string()),
            c if c.contains("sed") && !c.contains("-i") => Some("sed_full_access".to_string()),
            c if c.contains("awk") && c.contains(">") => Some("awk_full_access".to_string()),
            c if c.contains("awk") => Some("awk_read_only_recommended".to_string()),
            c if c.contains("install") => Some("package_install".to_string()),
            c if c.contains("git push") || c.contains("git commit") || c.contains("git rebase") => {
                Some("git_write_operations".to_string())
            }
            c if c.contains("curl") || c.contains("wget") || c.contains("ssh") => {
                Some("network_command_explained".to_string())
            }
            _ => Some("risky_command_explained".to_string()),
        },
        ("shell", false) => match context {
            c if c.contains("sed") => Some("sed_full_access".to_string()),
            c if c.contains("awk") => Some("awk_read_only_recommended".to_string()),
            _ => None,
        },
        ("tool", true) => match context {
            c if c.contains("edit") || c.contains("multiedit") => Some("edit_tool_benefits".to_string()),
            _ => Some("backup_on_risky".to_string()),
        },
        ("file_write", true) => Some("backup_on_risky".to_string()),
        ("file_delete", _) => Some("file_destruction".to_string()),
        ("network", _) => Some("network_command_explained".to_string()),
        ("patch", true) => Some("edit_tool_benefits".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::classification::ToolRisk;
    use super::*;

    #[test]
    fn test_teaching_state_new() {
        let state = TeachingState::new();
        assert!(!state.has_taught("test_concept"));
        assert!(state.taught_concepts().count() == 0);
    }

    #[test]
    fn test_teaching_state_mark_and_check() {
        let mut state = TeachingState::new();
        assert!(!state.has_taught("test_concept"));
        state.mark_taught("test_concept");
        assert!(state.has_taught("test_concept"));
    }

    #[test]
    fn test_teaching_state_get_hint_first_time() {
        let mut state = TeachingState::new();
        let hint = state.get_hint("sed_risky_explained");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("sed -i"));
    }

    #[test]
    fn test_teaching_state_get_hint_second_time() {
        let mut state = TeachingState::new();
        let hint1 = state.get_hint("sed_risky_explained");
        assert!(hint1.is_some());

        let hint2 = state.get_hint("sed_risky_explained");
        assert!(hint2.is_none());
    }

    #[test]
    fn test_teaching_state_multiple_concepts() {
        let mut state = TeachingState::new();
        let hint1 = state.get_hint("sed_risky_explained");
        assert!(hint1.is_some());

        let hint2 = state.get_hint("network_command_explained");
        assert!(hint2.is_some());

        assert!(state.has_taught("sed_risky_explained"));
        assert!(state.has_taught("network_command_explained"));
    }

    #[test]
    fn test_get_hint_for_known_concepts() {
        assert!(get_hint_for_concept("sed_risky_explained").is_some());
        assert!(get_hint_for_concept("network_command_explained").is_some());
        assert!(get_hint_for_concept("file_destruction").is_some());
        assert!(get_hint_for_concept("edit_tool_benefits").is_some());
    }

    #[test]
    fn test_get_hint_for_unknown_concept() {
        assert!(get_hint_for_concept("unknown_concept_xyz").is_none());
    }

    #[test]
    fn test_suggest_concept_sed_risky() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "sed -i 's/old/new/g' file.txt");
        assert_eq!(concept, Some("sed_risky_explained".to_string()));
    }

    #[test]
    fn test_suggest_concept_file_deletion() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "rm -rf /tmp/test");
        assert_eq!(concept, Some("file_destruction".to_string()));
    }

    #[test]
    fn test_suggest_concept_network() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "curl https://api.example.com");
        assert_eq!(concept, Some("network_command_explained".to_string()));
    }

    #[test]
    fn test_suggest_concept_package_install() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "npm install lodash");
        assert_eq!(concept, Some("package_install".to_string()));
    }

    #[test]
    fn test_suggest_concept_git_write() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "git push origin main");
        assert_eq!(concept, Some("git_write_operations".to_string()));
    }

    #[test]
    fn test_suggest_concept_risky_command_generic() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "chmod +x script.sh");
        assert_eq!(concept, Some("risky_command_explained".to_string()));
    }

    #[test]
    fn test_suggest_concept_edit_tool() {
        let concept = suggest_concept("tool", ToolRisk::Risky, "edit");
        assert_eq!(concept, Some("edit_tool_benefits".to_string()));
    }

    #[test]
    fn test_suggest_concept_file_write() {
        let concept = suggest_concept("file_write", ToolRisk::Risky, "");
        assert_eq!(concept, Some("backup_on_risky".to_string()));
    }

    #[test]
    fn test_suggest_concept_file_delete() {
        let concept = suggest_concept("file_delete", ToolRisk::Safe, "");
        assert_eq!(concept, Some("file_destruction".to_string()));
    }

    #[test]
    fn test_suggest_concept_network_action() {
        let concept = suggest_concept("network", ToolRisk::Risky, "");
        assert_eq!(concept, Some("network_command_explained".to_string()));
    }

    #[test]
    fn test_suggest_concept_none_for_safe_operations() {
        let concept = suggest_concept("tool", ToolRisk::Safe, "grep");
        assert!(concept.is_none());
    }

    #[test]
    fn test_suggest_concept_sed_safe_full_access() {
        let concept = suggest_concept("shell", ToolRisk::Safe, "sed 's/old/new/g' file.txt");
        assert_eq!(concept, Some("sed_full_access".to_string()));
    }

    #[test]
    fn test_suggest_concept_awk_safe_read_only() {
        let concept = suggest_concept("shell", ToolRisk::Safe, "awk '{print $1}' file.txt");
        assert_eq!(concept, Some("awk_read_only_recommended".to_string()));
    }

    #[test]
    fn test_suggest_concept_awk_risky_with_redirection() {
        let concept = suggest_concept("shell", ToolRisk::Risky, "awk '{print $1}' file.txt > output.txt");
        assert_eq!(concept, Some("awk_full_access".to_string()));
    }

    #[test]
    fn test_get_hint_for_sed_full_access() {
        let hint = get_hint_for_concept("sed_full_access");
        assert!(hint.is_some());
        let hint_content = hint.unwrap();
        assert!(hint_content.contains("backup") || hint_content.contains("Edit tool"));
    }

    #[test]
    fn test_get_hint_for_awk_full_access() {
        let hint = get_hint_for_concept("awk_full_access");
        assert!(hint.is_some());
        let hint_content = hint.unwrap();
        assert!(hint_content.contains("read-only") || hint_content.contains("redirection"));
    }

    #[test]
    fn test_get_hint_for_sed_backup_created() {
        let hint = get_hint_for_concept("sed_backup_created");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("backup"));
    }

    #[test]
    fn test_get_hint_for_awk_read_only_recommended() {
        let hint = get_hint_for_concept("awk_read_only_recommended");
        assert!(hint.is_some());
        let hint_content = hint.unwrap();
        assert!(hint_content.contains("Read") || hint_content.contains("Edit"));
    }

    #[test]
    fn test_teaching_state_clear() {
        let mut state = TeachingState::new();

        state.mark_taught("test_concept");
        assert!(state.has_taught("test_concept"));

        state.clear();
        assert!(!state.has_taught("test_concept"));
    }

    #[test]
    fn test_hint_messages_are_educational() {
        let hint = get_hint_for_concept("sed_risky_explained").unwrap();
        assert!(hint.contains("safer") || hint.contains("Edit tool"));

        let hint = get_hint_for_concept("network_command_explained").unwrap();
        assert!(hint.contains("network") || hint.contains("external"));

        let hint = get_hint_for_concept("file_destruction").unwrap();
        assert!(hint.contains("permanent") || hint.contains("undo"));
    }

    #[test]
    fn test_hint_messages_are_brief() {
        let hints = vec![
            "sed_risky_explained",
            "network_command_explained",
            "file_destruction",
            "edit_tool_benefits",
            "read_before_edit",
        ];

        for hint_key in hints {
            let hint = get_hint_for_concept(hint_key).unwrap();
            assert!(
                hint.len() < 300,
                "Hint for {} should be under 300 chars, got: {}",
                hint_key,
                hint.len()
            );
        }
    }

    #[test]
    fn test_teaching_state_serialization() {
        let mut state = TeachingState::new();
        state.mark_taught("concept1");
        state.mark_taught("concept2");

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: TeachingState = serde_json::from_str(&json).unwrap();

        assert!(deserialized.has_taught("concept1"));
        assert!(deserialized.has_taught("concept2"));
        assert!(!deserialized.has_taught("concept3"));
    }
}
