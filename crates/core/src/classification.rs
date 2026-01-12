/// Risk level of a tool or command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ToolRisk {
    /// Safe operations: tests, formatters, linters, read-only operations
    #[default]
    Safe,
    /// Risky operations: package install, file deletion, network tooling
    Risky,
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
}

/// Classification result with reasoning
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Classification {
    pub risk: ToolRisk,
    pub reasoning: String,
}

impl Classification {
    pub fn new(risk: ToolRisk, reasoning: impl Into<String>) -> Self {
        Self { risk, reasoning: reasoning.into() }
    }

    pub fn is_safe(&self) -> bool {
        self.risk.is_safe()
    }

    pub fn is_risky(&self) -> bool {
        self.risk.is_risky()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_risk_variants() {
        assert!(ToolRisk::Safe.is_safe());
        assert!(!ToolRisk::Safe.is_risky());

        assert!(!ToolRisk::Risky.is_safe());
        assert!(ToolRisk::Risky.is_risky());
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

        let risky_classification = Classification::new(ToolRisk::Risky, "This is risky".to_string());
        assert!(!risky_classification.is_safe());
        assert!(risky_classification.is_risky());
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
    }
}
