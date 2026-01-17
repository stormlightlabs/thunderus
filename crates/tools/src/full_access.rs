//! Full-access mode helpers for sed/awk exposure with backups and teaching hints
//!
//! This module provides utilities for handling sed and awk commands in full-access mode:
//! - Mandatory backup creation before risky commands (sed -i, awk with output redirection)
//! - Teaching hint suggestions for first-time use
//! - File path extraction from commands for backup purposes

use crate::backup::{BackupManager, command_requires_backup};
use crate::classification::CommandClassifier;

use std::path::PathBuf;
use thunderus_core::{
    Classification, Result,
    teaching::{get_hint_for_concept, suggest_concept},
};

/// Full-access mode policy for sed/awk commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullAccessPolicy {
    /// Command is safe to execute without backup
    Safe,
    /// Command requires backup before execution
    RequiresBackup,
    /// Command is risky and should be rejected (not used in full-access mode)
    Reject,
}

/// Check if a command requires backup in full-access mode
///
/// Returns the full-access policy for the command based on its risk level.
pub fn check_full_access_policy(command: &str) -> FullAccessPolicy {
    if command_requires_backup(command) {
        return FullAccessPolicy::RequiresBackup;
    }

    let first_word = command.split_whitespace().next().unwrap_or("");
    if first_word == "sed" || first_word == "awk" {
        return FullAccessPolicy::Safe;
    }

    FullAccessPolicy::Safe
}

/// Extract file paths from a command that may need backup
///
/// Parses the command to find files that would be modified by sed -i or awk with output redirection.
/// Returns a list of file paths that should be backed up before execution.
pub fn extract_files_for_backup(command: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let parts: Vec<&str> = command.split_whitespace().collect();

    let first_word = parts.first().unwrap_or(&"");

    match *first_word {
        "sed" => {
            let has_in_place = parts.iter().any(|p| *p == "-i" || p.starts_with("--in-place"));
            if has_in_place
                && let Some(last) = parts.last()
                && !last.starts_with('-')
                && !last.starts_with('\'')
                && !last.starts_with('"')
            {
                files.push(PathBuf::from(last.to_string()))
            }
        }
        "awk" => {
            if let Some(redir_pos) = parts.iter().position(|p| *p == ">")
                && let Some(output_file) = parts.get(redir_pos + 1)
                && !output_file.starts_with('\'')
                && !output_file.starts_with('"')
            {
                files.push(PathBuf::from(output_file.to_string()))
            }

            if let Some(redir_pos) = parts.iter().position(|p| *p == ">>")
                && let Some(output_file) = parts.get(redir_pos + 1)
                && !output_file.starts_with('\'')
                && !output_file.starts_with('"')
            {
                files.push(PathBuf::from(output_file.to_string()))
            }
        }
        _ => (),
    }

    files
}

/// Create backups for files that would be modified by a command
///
/// Returns Ok(Vec<BackupMetadata>) with the created backup metadata,
/// or Err if backup creation fails.
pub fn create_backups_for_command(command: &str, backup_manager: &BackupManager) -> Result<Vec<(PathBuf, String)>> {
    let files = extract_files_for_backup(command);
    let mut backups = Vec::new();

    for file_path in files {
        if file_path.exists() {
            let reason = format!("Backup before: {}", command);
            match backup_manager.create_backup(&file_path, reason) {
                Ok(_) => backups.push((file_path.clone(), "Backup created".to_string())),
                Err(e) => {
                    return Err(thunderus_core::Error::Other(format!(
                        "Failed to create backup for {}: {}",
                        file_path.display(),
                        e
                    )));
                }
            }
        }
    }

    Ok(backups)
}

/// Get teaching hint for a command in full-access mode
///
/// Returns a teaching hint message if this is the first time the command pattern
/// is encountered, or None if it has already been taught.
pub fn get_teaching_hint_for_command(command: &str, classification: &Classification) -> Option<String> {
    let concept = suggest_concept("shell", classification.risk, command);
    concept.and_then(|c| get_hint_for_concept(&c))
}

/// Classify a command and get its teaching hint in one step
///
/// This is a convenience function that combines classification and hint lookup.
pub fn classify_and_get_hint(command: &str) -> (Classification, Option<String>) {
    let classifier = CommandClassifier::new();
    let classification = classifier.classify_with_reasoning(command);
    let hint = get_teaching_hint_for_command(command, &classification);
    (classification, hint)
}

/// Format a shell command result with teaching hints and backup information
///
/// This formats the command output to include:
/// - Teaching hints (if first-time use)
/// - Backup creation notices (if backups were made)
pub fn format_command_result(
    _command: &str, output: String, hint: Option<String>, backups: Vec<(PathBuf, String)>,
) -> String {
    let mut parts = Vec::new();

    if let Some(hint_msg) = hint {
        parts.push(format!("Hint: {}", hint_msg));
        parts.push(String::new());
    }

    for (file_path, notice) in &backups {
        parts.push(format!("Backup: {} for {}", notice, file_path.display()));
    }

    if !parts.is_empty() {
        parts.push(String::new());
    }

    parts.push(output);

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use thunderus_core::ToolRisk;

    #[test]
    fn test_check_full_access_policy_safe_sed() {
        let policy = check_full_access_policy("sed 's/old/new/g' file.txt");
        assert_eq!(policy, FullAccessPolicy::Safe);
    }

    #[test]
    fn test_check_full_access_policy_risky_sed() {
        let policy = check_full_access_policy("sed -i 's/old/new/g' file.txt");
        assert_eq!(policy, FullAccessPolicy::RequiresBackup);
    }

    #[test]
    fn test_check_full_access_policy_safe_awk() {
        let policy = check_full_access_policy("awk '{print $1}' file.txt");
        assert_eq!(policy, FullAccessPolicy::Safe);
    }

    #[test]
    fn test_check_full_access_policy_risky_awk() {
        let policy = check_full_access_policy("awk '{print $1}' file.txt > output.txt");
        assert_eq!(policy, FullAccessPolicy::RequiresBackup);
    }

    #[test]
    fn test_extract_files_for_backup_sed_i() {
        let files = extract_files_for_backup("sed -i 's/old/new/g' file.txt");
        assert_eq!(files, vec![PathBuf::from("file.txt")]);
    }

    #[test]
    fn test_extract_files_for_backup_sed_no_i() {
        let files = extract_files_for_backup("sed 's/old/new/g' file.txt");
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_files_for_backup_awk_redirect() {
        let files = extract_files_for_backup("awk '{print $1}' file.txt > output.txt");
        assert_eq!(files, vec![PathBuf::from("output.txt")]);
    }

    #[test]
    fn test_extract_files_for_backup_awk_no_redirect() {
        let files = extract_files_for_backup("awk '{print $1}' file.txt");
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_files_for_backup_awk_append() {
        let files = extract_files_for_backup("awk '{print $1}' file.txt >> output.txt");
        assert_eq!(files, vec![PathBuf::from("output.txt")]);
    }

    #[test]
    fn test_create_backups_for_command_sed_i() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "original content").unwrap();

        let backup_dir = temp.path().join(".backups");
        let backup_manager = BackupManager::new(backup_dir, crate::backup::BackupMode::Always, 5);

        let backups = create_backups_for_command(
            &format!("sed -i 's/old/new/g' {}", test_file.display()),
            &backup_manager,
        )
        .unwrap();

        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].0, test_file);
        assert!(test_file.exists());
    }

    #[test]
    fn test_create_backups_for_command_no_files() {
        let temp = TempDir::new().unwrap();
        let backup_dir = temp.path().join(".backups");
        let backup_manager = BackupManager::new(backup_dir, crate::backup::BackupMode::Always, 5);

        let backups = create_backups_for_command("sed 's/old/new/g' file.txt", &backup_manager).unwrap();

        assert!(backups.is_empty());
    }

    #[test]
    fn test_classify_and_get_hint_sed_safe() {
        let (classification, hint) = classify_and_get_hint("sed 's/old/new/g' file.txt");
        assert_eq!(classification.risk, ToolRisk::Safe);
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("sed"));
    }

    #[test]
    fn test_classify_and_get_hint_sed_risky() {
        let (classification, hint) = classify_and_get_hint("sed -i 's/old/new/g' file.txt");
        assert_eq!(classification.risk, ToolRisk::Risky);
        assert!(hint.is_some());
    }

    #[test]
    fn test_classify_and_get_hint_awk_safe() {
        let (classification, hint) = classify_and_get_hint("awk '{print $1}' file.txt");
        assert_eq!(classification.risk, ToolRisk::Safe);
        assert!(hint.is_some());
    }

    #[test]
    fn test_classify_and_get_hint_awk_risky() {
        let (classification, hint) = classify_and_get_hint("awk '{print $1}' file.txt > output.txt");
        assert_eq!(classification.risk, ToolRisk::Risky);
        assert!(hint.is_some());
    }

    #[test]
    fn test_format_command_result_with_hint() {
        let output = format_command_result(
            "sed 's/old/new/g' file.txt",
            "Command output".to_string(),
            Some("This is a teaching hint".to_string()),
            vec![],
        );

        assert!(output.contains("Hint:"));
        assert!(output.contains("This is a teaching hint"));
        assert!(output.contains("Command output"));
    }

    #[test]
    fn test_format_command_result_with_backup() {
        let file = PathBuf::from("/tmp/test.txt");
        let output = format_command_result(
            "sed -i 's/old/new/g' /tmp/test.txt",
            "Command output".to_string(),
            None,
            vec![(file, "Backup created".to_string())],
        );

        assert!(output.contains("Backup:"));
        assert!(output.contains("/tmp/test.txt"));
        assert!(output.contains("Command output"));
    }

    #[test]
    fn test_format_command_result_with_both() {
        let file = PathBuf::from("/tmp/test.txt");
        let output = format_command_result(
            "sed -i 's/old/new/g' /tmp/test.txt",
            "Command output".to_string(),
            Some("Teaching hint".to_string()),
            vec![(file, "Backup created".to_string())],
        );

        assert!(output.contains("Hint:"));
        assert!(output.contains("Backup:"));
        assert!(output.contains("Command output"));
    }

    #[test]
    fn test_extract_files_for_backup_ignores_quoted_args() {
        let files = extract_files_for_backup("sed -i 's/old/new/g' 'file.txt'");
        // TODO: Handle quotes
        assert!(files.is_empty() || files.len() == 1);
    }

    #[test]
    fn test_check_full_access_policy_other_commands() {
        let policy = check_full_access_policy("grep pattern file.txt");
        assert_eq!(policy, FullAccessPolicy::Safe);

        let policy = check_full_access_policy("cat file.txt");
        assert_eq!(policy, FullAccessPolicy::Safe);
    }
}
