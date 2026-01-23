use crate::config::ApprovalMode;
use crate::teaching::TeachingState;
use serde::{Deserialize, Serialize};

/// Session metadata persisted to metadata.json
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMetadata {
    /// Approval mode for this session
    pub approval_mode: ApprovalMode,
    /// Whether network access is allowed
    pub allow_network: bool,
    /// Session title (optional)
    pub title: Option<String>,
    /// Session tags
    pub tags: Vec<String>,
    /// Teaching state for tracking taught concepts
    #[serde(default)]
    pub teaching_state: TeachingState,
    /// When the session was created
    pub created_at: String,
    /// When the session was last updated
    pub updated_at: String,
}

impl SessionMetadata {
    /// Create new session metadata with default values
    pub fn new(approval_mode: ApprovalMode) -> Self {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Self {
            approval_mode,
            allow_network: false,
            title: None,
            tags: Vec::new(),
            teaching_state: TeachingState::default(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Update the approval mode
    pub fn with_approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.approval_mode = mode;
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Update the network permission
    pub fn with_allow_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Set the session title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Add a tag to the session
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_metadata_new() {
        let metadata = SessionMetadata::new(ApprovalMode::Auto);
        assert_eq!(metadata.approval_mode, ApprovalMode::Auto);
        assert!(!metadata.allow_network);
        assert!(metadata.title.is_none());
        assert!(metadata.tags.is_empty());
        assert!(!metadata.created_at.is_empty());
        assert!(!metadata.updated_at.is_empty());
    }

    #[test]
    fn test_session_metadata_with_approval_mode() {
        let metadata = SessionMetadata::new(ApprovalMode::ReadOnly);
        let updated = metadata.with_approval_mode(ApprovalMode::FullAccess);
        assert_eq!(updated.approval_mode, ApprovalMode::FullAccess);
        assert!(!updated.updated_at.is_empty());
    }

    #[test]
    fn test_session_metadata_with_allow_network() {
        let metadata = SessionMetadata::new(ApprovalMode::Auto);
        let updated = metadata.with_allow_network(true);
        assert!(updated.allow_network);
    }

    #[test]
    fn test_session_metadata_serialization() {
        let metadata = SessionMetadata::new(ApprovalMode::Auto).with_title("Test Session");
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("\"auto\""));
        assert!(json.contains("Test Session"));

        let deserialized: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.approval_mode, ApprovalMode::Auto);
        assert_eq!(deserialized.title, Some("Test Session".to_string()));
    }
}
