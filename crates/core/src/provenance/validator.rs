use crate::MemoryDoc;
use crate::error::{Error, Result};
use crate::layout::SessionId;

use serde::{Deserialize, Serialize};

/// Mode for provenance validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationMode {
    /// Reject documents with missing provenance
    Strict,
    /// Auto-link to the current session ID if missing
    Loose,
}

/// Validator for memory document provenance
pub struct ProvenanceValidator {
    mode: ValidationMode,
}

impl ProvenanceValidator {
    pub fn new(mode: ValidationMode) -> Self {
        Self { mode }
    }

    /// Validate a memory document's provenance
    pub fn validate(&self, doc: &mut MemoryDoc, session_id: &SessionId) -> Result<()> {
        self.validate_provenance(&mut doc.frontmatter.provenance.events, &doc.frontmatter.id, session_id)
    }

    /// Validate a provenance vector directly
    pub fn validate_provenance(&self, events: &mut Vec<String>, doc_id: &str, session_id: &SessionId) -> Result<()> {
        if events.is_empty() {
            match self.mode {
                ValidationMode::Strict => {
                    return Err(Error::Other(format!(
                        "Provenance validation failed: Document '{}' is missing provenance backlinks in Strict mode.",
                        doc_id
                    )));
                }
                ValidationMode::Loose => {
                    events.push(format!("session_{}", session_id.as_str()));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryKind;

    #[test]
    fn test_strict_validation_fails_on_empty() {
        let mut doc = MemoryDoc::new("fact.test", "Test", MemoryKind::Fact, vec!["test".to_string()], "Body");
        let session_id = SessionId::new();
        let validator = ProvenanceValidator::new(ValidationMode::Strict);

        let result = validator.validate(&mut doc, &session_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_loose_validation_adds_session() {
        let mut doc = MemoryDoc::new("fact.test", "Test", MemoryKind::Fact, vec!["test".to_string()], "Body");
        let session_id = SessionId::from_timestamp("2026-01-23T22-30-00Z").unwrap();
        let validator = ProvenanceValidator::new(ValidationMode::Loose);

        validator.validate(&mut doc, &session_id).unwrap();
        assert!(!doc.frontmatter.provenance.is_empty());
        assert_eq!(doc.frontmatter.provenance.events[0], "session_2026-01-23T22-30-00Z");
    }

    #[test]
    fn test_validation_passes_on_non_empty() {
        let mut doc = MemoryDoc::new("fact.test", "Test", MemoryKind::Fact, vec!["test".to_string()], "Body");
        doc.frontmatter.provenance.events.push("evt_1".to_string());
        let session_id = SessionId::new();
        let validator = ProvenanceValidator::new(ValidationMode::Strict);

        validator.validate(&mut doc, &session_id).unwrap();
        assert_eq!(doc.frontmatter.provenance.events.len(), 1);
    }
}
