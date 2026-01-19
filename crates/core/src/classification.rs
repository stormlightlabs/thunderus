use std::fmt::Display;

/// Risk level of a tool or command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ToolRisk {
    /// Safe operations: tests, formatters, linters, read-only operations
    #[default]
    Safe,
    /// Risky operations: package install, file deletion, network tooling
    Risky,
    /// Blocked operations: always denied regardless of approval mode (e.g., sudo, rm -rf /)
    Blocked,
}

impl Display for ToolRisk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ToolRisk {
    /// Returns true if this is a safe operation
    pub fn is_safe(&self) -> bool {
        matches!(self, Self::Safe)
    }

    /// Returns true if this is a risky operation
    pub fn is_risky(&self) -> bool {
        matches!(self, Self::Risky)
    }

    /// Returns true if this is a blocked operation
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked)
    }

    /// Returns the string representation of the risk level
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Risky => "risky",
            Self::Blocked => "blocked",
        }
    }
}

/// Classification result with reasoning
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Classification {
    pub risk: ToolRisk,
    pub reasoning: String,
    /// Suggested safer alternative (if applicable)
    pub suggestion: Option<String>,
}

impl Classification {
    pub fn new(risk: ToolRisk, reasoning: impl Into<String>) -> Self {
        Self { risk, reasoning: reasoning.into(), suggestion: None }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn is_safe(&self) -> bool {
        self.risk.is_safe()
    }

    pub fn is_risky(&self) -> bool {
        self.risk.is_risky()
    }

    pub fn is_blocked(&self) -> bool {
        self.risk.is_blocked()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_risk_variants() {
        assert!(ToolRisk::Safe.is_safe());
        assert!(!ToolRisk::Safe.is_risky());
        assert!(!ToolRisk::Safe.is_blocked());

        assert!(!ToolRisk::Risky.is_safe());
        assert!(ToolRisk::Risky.is_risky());
        assert!(!ToolRisk::Risky.is_blocked());

        assert!(!ToolRisk::Blocked.is_safe());
        assert!(!ToolRisk::Blocked.is_risky());
        assert!(ToolRisk::Blocked.is_blocked());
    }

    #[test]
    fn test_tool_risk_as_str() {
        assert_eq!(ToolRisk::Safe.as_str(), "safe");
        assert_eq!(ToolRisk::Risky.as_str(), "risky");
        assert_eq!(ToolRisk::Blocked.as_str(), "blocked");
    }

    #[test]
    fn test_tool_risk_default() {
        assert_eq!(ToolRisk::default(), ToolRisk::Safe);
    }

    #[test]
    fn test_classification_helpers() {
        let safe_classification = Classification::new(ToolRisk::Safe, "This is safe".to_string());
        assert!(safe_classification.is_safe());
        assert!(!safe_classification.is_risky());
        assert!(!safe_classification.is_blocked());

        let risky_classification = Classification::new(ToolRisk::Risky, "This is risky".to_string());
        assert!(!risky_classification.is_safe());
        assert!(risky_classification.is_risky());
        assert!(!risky_classification.is_blocked());

        let blocked_classification = Classification::new(ToolRisk::Blocked, "This is blocked".to_string());
        assert!(!blocked_classification.is_safe());
        assert!(!blocked_classification.is_risky());
        assert!(blocked_classification.is_blocked());
    }

    #[test]
    fn test_classification_serialization() {
        let classification = Classification::new(ToolRisk::Safe, "Test reasoning".to_string());

        let json = serde_json::to_string(&classification).unwrap();
        assert!(json.contains("Safe"));
        assert!(json.contains("Test reasoning"));

        let deserialized: Classification = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.risk, ToolRisk::Safe);
        assert_eq!(deserialized.reasoning, "Test reasoning");
        assert!(deserialized.suggestion.is_none());
    }

    #[test]
    fn test_classification_with_suggestion() {
        let classification = Classification::new(ToolRisk::Risky, "Using sed -i is risky".to_string())
            .with_suggestion("Use the Edit tool instead for safer find-replace operations");

        assert_eq!(classification.risk, ToolRisk::Risky);
        assert!(classification.reasoning.contains("sed -i"));
        assert!(classification.suggestion.is_some());
        assert!(classification.suggestion.as_ref().unwrap().contains("Edit tool"));
    }

    #[test]
    fn test_classification_serialization_with_suggestion() {
        let classification =
            Classification::new(ToolRisk::Risky, "Test reasoning".to_string()).with_suggestion("Use safer alternative");

        let json = serde_json::to_string(&classification).unwrap();
        assert!(json.contains("Risky"));
        assert!(json.contains("Test reasoning"));
        assert!(json.contains("safer alternative"));

        let deserialized: Classification = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.risk, ToolRisk::Risky);
        assert_eq!(deserialized.reasoning, "Test reasoning");
        assert_eq!(deserialized.suggestion, Some("Use safer alternative".to_string()));
    }
}
