use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use thunderus_core::Result;

/// Snapshot capture mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotMode {
    /// No snapshots
    Disabled,
    /// Capture snapshots on every state change
    EveryState,
    /// Capture snapshots only on specific trigger events
    Triggered,
}

/// Snapshot metadata
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub timestamp: SystemTime,
    pub event_type: String,
    pub description: String,
}

/// Snapshot capture for TUI states
///
/// Captures terminal state snapshots for regression testing and debugging.
/// Snapshots are saved to `.thunderus/snapshots/` directory.
pub struct SnapshotCapture {
    enabled: bool,
    mode: SnapshotMode,
    snapshot_dir: PathBuf,
    snapshot_count: usize,
}

impl SnapshotCapture {
    pub fn new(enabled: bool, snapshot_dir: PathBuf) -> Self {
        Self { enabled, mode: SnapshotMode::Disabled, snapshot_dir, snapshot_count: 0 }
    }

    pub fn with_mode(mut self, mode: SnapshotMode) -> Self {
        self.mode = mode;
        self
    }

    /// Enable snapshot capture
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable snapshot capture
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Set snapshot mode
    pub fn set_mode(&mut self, mode: SnapshotMode) {
        self.mode = mode;
    }

    /// Check if snapshots are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Capture a snapshot
    ///
    /// # Arguments
    /// * `content` - The terminal content to capture
    /// * `event_type` - Type of event triggering the snapshot
    /// * `description` - Description of the state
    pub fn capture(&mut self, content: &str, event_type: &str, description: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if let SnapshotMode::Triggered = self.mode {
            return Ok(());
        }

        self.snapshot_count += 1;
        let filename = format!("snapshot_{:04}.txt", self.snapshot_count);
        let filepath = self.snapshot_dir.join(&filename);

        fs::create_dir_all(&self.snapshot_dir).map_err(thunderus_core::Error::Io)?;

        let metadata = SnapshotMetadata {
            timestamp: SystemTime::now(),
            event_type: event_type.to_string(),
            description: description.to_string(),
        };

        let header = format!(
            "# Snapshot #{}\n# Timestamp: {:?}\n# Event: {}\n# Description: {}\n\n",
            self.snapshot_count, metadata.timestamp, metadata.event_type, metadata.description
        );

        let full_content = format!("{}{}", header, content);

        fs::write(&filepath, full_content).map_err(thunderus_core::Error::Io)?;

        Ok(())
    }

    /// Capture a triggered snapshot (only works in Triggered mode)
    pub fn capture_triggered(&mut self, content: &str, event_type: &str, description: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if !matches!(self.mode, SnapshotMode::Triggered) {
            return Ok(());
        }

        self.snapshot_count += 1;
        let filename = format!("triggered_snapshot_{:04}.txt", self.snapshot_count);
        let filepath = self.snapshot_dir.join(&filename);

        fs::create_dir_all(&self.snapshot_dir).map_err(thunderus_core::Error::Io)?;

        let metadata = SnapshotMetadata {
            timestamp: SystemTime::now(),
            event_type: event_type.to_string(),
            description: description.to_string(),
        };

        let header = format!(
            "# Triggered Snapshot #{}\n# Timestamp: {:?}\n# Event: {}\n# Description: {}\n\n",
            self.snapshot_count, metadata.timestamp, metadata.event_type, metadata.description
        );

        let full_content = format!("{}{}", header, content);

        fs::write(&filepath, full_content).map_err(thunderus_core::Error::Io)?;

        Ok(())
    }

    /// Get snapshot count
    pub fn snapshot_count(&self) -> usize {
        self.snapshot_count
    }

    /// Clear all snapshots
    pub fn clear(&self) -> Result<()> {
        if self.snapshot_dir.exists() {
            fs::remove_dir_all(&self.snapshot_dir).map_err(thunderus_core::Error::Io)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_capture_disabled() {
        let temp = TempDir::new().unwrap();
        let snapshot_dir = temp.path().join("snapshots");
        let mut capture = SnapshotCapture::new(false, snapshot_dir.clone());

        let result = capture.capture("test content", "test_event", "test description");
        assert!(result.is_ok());
        assert_eq!(capture.snapshot_count(), 0);
        assert!(!snapshot_dir.exists());
    }

    #[test]
    fn test_snapshot_capture_enabled() {
        let temp = TempDir::new().unwrap();
        let snapshot_dir = temp.path().join("snapshots");
        let mut capture = SnapshotCapture::new(true, snapshot_dir.clone()).with_mode(SnapshotMode::EveryState);

        let result = capture.capture("test content", "test_event", "test description");
        assert!(result.is_ok());
        assert_eq!(capture.snapshot_count(), 1);
        assert!(snapshot_dir.exists());

        let snapshot_file = snapshot_dir.join("snapshot_0001.txt");
        assert!(snapshot_file.exists());

        let content = fs::read_to_string(&snapshot_file).unwrap();
        assert!(content.contains("Snapshot #1"));
        assert!(content.contains("test_event"));
        assert!(content.contains("test description"));
    }

    #[test]
    fn test_snapshot_capture_triggered() {
        let temp = TempDir::new().unwrap();
        let snapshot_dir = temp.path().join("snapshots");
        let mut capture = SnapshotCapture::new(true, snapshot_dir.clone()).with_mode(SnapshotMode::Triggered);

        let result = capture.capture("test content", "test_event", "test description");
        assert!(result.is_ok());
        assert_eq!(capture.snapshot_count(), 0);

        let result = capture.capture_triggered("triggered content", "trigger_event", "triggered description");
        assert!(result.is_ok());
        assert_eq!(capture.snapshot_count(), 1);

        let snapshot_file = snapshot_dir.join("triggered_snapshot_0001.txt");
        assert!(snapshot_file.exists());
    }

    #[test]
    fn test_snapshot_clear() {
        let temp = TempDir::new().unwrap();
        let snapshot_dir = temp.path().join("snapshots");
        let mut capture = SnapshotCapture::new(true, snapshot_dir.clone()).with_mode(SnapshotMode::EveryState);

        capture
            .capture("test content", "test_event", "test description")
            .unwrap();
        assert!(snapshot_dir.exists());

        let result = capture.clear();
        assert!(result.is_ok());
        assert!(!snapshot_dir.exists());
    }
}
