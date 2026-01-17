//! Backup management for risky file operations
//!
//! This module provides backup functionality for risky file operations,
//! ensuring that files can be restored if something goes wrong.
//!
//! Backups are created:
//! - Before file edits (Edit, MultiEdit tools)
//! - Before risky shell commands that modify files
//! - In full-access mode when explicitly requested

use std::path::{Path, PathBuf};
use thunderus_core::Result;

use thunderus_core::Error;

/// Configuration for backup behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupMode {
    /// Never create backups
    Never,
    /// Create backups only for risky operations
    RiskyOnly,
    /// Create backups for all file operations
    Always,
}

/// Backup metadata
#[derive(Debug, Clone)]
pub struct BackupMetadata {
    /// Original file path
    pub original_path: PathBuf,
    /// Backup file path
    pub backup_path: PathBuf,
    /// Timestamp when backup was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Reason for backup (e.g., "edit", "shell command")
    pub reason: String,
}

impl BackupMetadata {
    /// Create new backup metadata
    pub fn new(original_path: PathBuf, backup_path: PathBuf, reason: impl Into<String>) -> Self {
        Self { original_path, backup_path, created_at: chrono::Utc::now(), reason: reason.into() }
    }
}

/// Backup manager for creating and restoring file backups
#[derive(Debug)]
pub struct BackupManager {
    /// Directory where backups are stored
    backup_dir: PathBuf,
    /// Backup mode
    mode: BackupMode,
    /// Maximum number of backups to keep (0 = unlimited)
    max_backups: usize,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(backup_dir: PathBuf, mode: BackupMode, max_backups: usize) -> Self {
        Self { backup_dir, mode, max_backups }
    }

    /// Get the backup directory
    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    /// Set the backup mode
    pub fn set_mode(&mut self, mode: BackupMode) {
        self.mode = mode;
    }

    /// Check if backups should be created for a risky operation
    pub fn should_backup(&self, is_risky: bool) -> bool {
        match self.mode {
            BackupMode::Never => false,
            BackupMode::RiskyOnly => is_risky,
            BackupMode::Always => true,
        }
    }

    /// Create a backup of a file
    pub fn create_backup(&self, file_path: &Path, reason: impl Into<String>) -> Result<BackupMetadata> {
        if !file_path.exists() {
            return Err(Error::Validation(format!(
                "Cannot backup non-existent file: {}",
                file_path.display()
            )));
        }

        std::fs::create_dir_all(&self.backup_dir)
            .map_err(|e| Error::Other(format!("Failed to create backup directory: {}", e)))?;

        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Validation("Invalid file name".to_string()))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f");
        let backup_name = format!("{}.{}.bak", file_name, timestamp);
        let backup_path = self.backup_dir.join(&backup_name);

        std::fs::copy(file_path, &backup_path).map_err(|e| Error::Other(format!("Failed to create backup: {}", e)))?;

        Ok(BackupMetadata::new(file_path.to_path_buf(), backup_path, reason))
    }

    /// Restore a file from backup
    pub fn restore_backup(&self, backup: &BackupMetadata) -> Result<()> {
        if !backup.backup_path.exists() {
            return Err(Error::Validation(format!(
                "Backup file not found: {}",
                backup.backup_path.display()
            )));
        }

        std::fs::copy(&backup.backup_path, &backup.original_path)
            .map_err(|e| Error::Other(format!("Failed to restore backup: {}", e)))?;

        Ok(())
    }

    /// Delete a backup file
    pub fn delete_backup(&self, backup: &BackupMetadata) -> Result<()> {
        if !backup.backup_path.exists() {
            return Ok(());
        }

        std::fs::remove_file(&backup.backup_path)
            .map_err(|e| Error::Other(format!("Failed to delete backup: {}", e)))?;

        Ok(())
    }

    /// Clean up old backups for a specific file
    ///
    /// Keeps only the most recent `max_backups` backups for each original file.
    pub fn cleanup_old_backups(&self) -> Result<()> {
        if self.max_backups == 0 {
            return Ok(());
        }

        if !self.backup_dir.exists() {
            return Ok(());
        }

        let mut backups_by_file: std::collections::HashMap<String, Vec<PathBuf>> = std::collections::HashMap::new();

        let entries = std::fs::read_dir(&self.backup_dir)
            .map_err(|e| Error::Other(format!("Failed to read backup directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| Error::Other(format!("Failed to read backup entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("bak") {
                continue;
            }

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let original_name = file_name.rsplit('.').nth(2).unwrap_or(file_name);
            backups_by_file.entry(original_name.to_string()).or_default().push(path);
        }

        for backups in backups_by_file.values() {
            if backups.len() <= self.max_backups {
                continue;
            }

            let mut sorted_backups = backups.clone();
            sorted_backups.sort_by_key(|p| {
                std::fs::metadata(p)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });
            sorted_backups.reverse();

            for old_backup in sorted_backups.iter().skip(self.max_backups) {
                let _ = std::fs::remove_file(old_backup);
            }
        }

        Ok(())
    }

    /// Get all backup metadata files
    pub fn list_backups(&self) -> Result<Vec<BackupMetadata>> {
        if !self.backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();

        let entries = std::fs::read_dir(&self.backup_dir)
            .map_err(|e| Error::Other(format!("Failed to read backup directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| Error::Other(format!("Failed to read backup entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("bak") {
                continue;
            }

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let metadata = std::fs::metadata(&path).ok();
            let created_at = metadata
                .and_then(|m| m.created().ok())
                .map(chrono::DateTime::<chrono::Utc>::from)
                .unwrap_or_else(chrono::Utc::now);

            let original_name = file_name.rsplit('.').nth(2).unwrap_or(file_name);

            backups.push(BackupMetadata {
                original_path: PathBuf::from(original_name),
                backup_path: path,
                created_at,
                reason: "manual".to_string(),
            });
        }

        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(backups)
    }
}

/// Helper to determine if a shell command requires a backup
pub fn command_requires_backup(command: &str) -> bool {
    let command_lower = command.to_lowercase();
    let first_word = command_lower.split_whitespace().next().unwrap_or("");

    const ALWAYS_RISKY_COMMANDS: &[&str] = &["mv", "cp", "chmod", "chown", "rm", "rmdir", "shred", "truncate"];

    if ALWAYS_RISKY_COMMANDS.contains(&first_word) {
        return true;
    }

    if command_lower.contains("sed -i") || command_lower.contains("sed --in-place") {
        return true;
    }

    if first_word == "awk" && (command_lower.contains(">") || command_lower.contains(">>")) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (TempDir, BackupManager) {
        let temp = TempDir::new().unwrap();
        let backup_dir = temp.path().join(".backups");
        let manager = BackupManager::new(backup_dir, BackupMode::Always, 5);
        (temp, manager)
    }

    #[test]
    fn test_backup_metadata_creation() {
        let original = PathBuf::from("/path/to/file.txt");
        let backup = PathBuf::from("/backup/file.txt.20230101_120000_000.bak");
        let metadata = BackupMetadata::new(original.clone(), backup.clone(), "test");

        assert_eq!(metadata.original_path, original);
        assert_eq!(metadata.backup_path, backup);
        assert_eq!(metadata.reason, "test");
    }

    #[test]
    fn test_backup_mode_should_backup() {
        let manager_risky = BackupManager::new(PathBuf::from("/backup"), BackupMode::RiskyOnly, 10);

        assert!(!manager_risky.should_backup(false));
        assert!(manager_risky.should_backup(true));

        let manager_always = BackupManager::new(PathBuf::from("/backup"), BackupMode::Always, 10);

        assert!(manager_always.should_backup(false));
        assert!(manager_always.should_backup(true));

        let manager_never = BackupManager::new(PathBuf::from("/backup"), BackupMode::Never, 10);

        assert!(!manager_never.should_backup(false));
        assert!(!manager_never.should_backup(true));
    }

    #[test]
    fn test_create_and_restore_backup() {
        let (temp, manager) = create_test_manager();

        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "original content").unwrap();

        let backup = manager.create_backup(&test_file, "test edit").unwrap();
        assert!(backup.backup_path.exists());

        std::fs::write(&test_file, "modified content").unwrap();
        assert_eq!(std::fs::read_to_string(&test_file).unwrap(), "modified content");

        manager.restore_backup(&backup).unwrap();
        assert_eq!(std::fs::read_to_string(&test_file).unwrap(), "original content");
    }

    #[test]
    fn test_create_backup_nonexistent_file() {
        let (temp, manager) = create_test_manager();

        let nonexistent = temp.path().join("nonexistent.txt");
        let result = manager.create_backup(&nonexistent, "test");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-existent"));
    }

    #[test]
    fn test_delete_backup() {
        let (temp, manager) = create_test_manager();

        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "content").unwrap();

        let backup = manager.create_backup(&test_file, "test").unwrap();
        assert!(backup.backup_path.exists());

        manager.delete_backup(&backup).unwrap();
        assert!(!backup.backup_path.exists());
    }

    #[test]
    fn test_list_backups() {
        let (temp, manager) = create_test_manager();

        let test_file1 = temp.path().join("test1.txt");
        let test_file2 = temp.path().join("test2.txt");

        std::fs::write(&test_file1, "content1").unwrap();
        std::fs::write(&test_file2, "content2").unwrap();

        manager.create_backup(&test_file1, "backup1").unwrap();
        manager.create_backup(&test_file2, "backup2").unwrap();

        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 2);
    }

    #[test]
    fn test_cleanup_old_backups() {
        let (temp, manager) = create_test_manager();

        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "content").unwrap();

        for i in 0..7 {
            manager.create_backup(&test_file, format!("backup{}", i)).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        manager.cleanup_old_backups().unwrap();

        let backups = manager.list_backups().unwrap();

        assert!(backups.len() <= 5);
    }

    #[test]
    fn test_command_requires_backup() {
        assert!(command_requires_backup("sed -i 's/old/new/g' file.txt"));
        assert!(command_requires_backup("sed --in-place 's/old/new/g' file.txt"));
        assert!(command_requires_backup("awk '{print $1}' file > output.txt"));
        assert!(command_requires_backup("mv file.txt new.txt"));
        assert!(command_requires_backup("cp src dst"));
        assert!(command_requires_backup("rm file.txt"));

        assert!(!command_requires_backup("cat file.txt"));
        assert!(!command_requires_backup("grep pattern file.txt"));
        assert!(!command_requires_backup("ls -la"));
        assert!(!command_requires_backup("sed 's/old/new/g' file.txt"));
        assert!(!command_requires_backup("awk '{print $1}' file.txt"));
    }

    #[test]
    fn test_set_mode() {
        let mut manager = BackupManager::new(PathBuf::from("/backup"), BackupMode::RiskyOnly, 10);

        assert!(manager.should_backup(true));
        assert!(!manager.should_backup(false));

        manager.set_mode(BackupMode::Never);
        assert!(!manager.should_backup(true));
        assert!(!manager.should_backup(false));
    }

    #[test]
    fn test_restore_nonexistent_backup() {
        let (temp, manager) = create_test_manager();
        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "content").unwrap();

        let backup = BackupMetadata::new(test_file.clone(), temp.path().join("nonexistent.bak"), "test");

        let result = manager.restore_backup(&backup);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_list_backups_empty_directory() {
        let (_temp, manager) = create_test_manager();
        let backups = manager.list_backups().unwrap();
        assert!(backups.is_empty());
    }

    #[test]
    fn test_max_backups_zero_unlimited() {
        let temp = TempDir::new().unwrap();
        let backup_dir = temp.path().join(".backups");
        let manager = BackupManager::new(backup_dir, BackupMode::Always, 0);
        let test_file = temp.path().join("test.txt");

        std::fs::write(&test_file, "content").unwrap();

        for _ in 0..10 {
            manager.create_backup(&test_file, "backup").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        manager.cleanup_old_backups().unwrap();

        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 10);
    }
}
