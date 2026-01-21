//! Memory document parsing and validation
//!
//! Handles YAML frontmatter parsing and validation for memory documents.

use crate::error::{Error, Result};
use crate::memory::kinds::{MemoryKind, Provenance, SessionMeta, Verification};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// A memory document with frontmatter and body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDoc {
    /// Frontmatter metadata
    pub frontmatter: MemoryFrontmatter,
    /// Document body content (markdown)
    pub body: String,
}

/// Frontmatter for memory documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    /// Unique identifier for the document
    pub id: String,
    /// Document title
    pub title: String,
    /// Kind of memory document
    pub kind: MemoryKind,
    /// Tags for categorization and search
    pub tags: Vec<String>,
    /// Creation timestamp
    pub created: DateTime<Utc>,
    /// Last update timestamp
    pub updated: DateTime<Utc>,
    /// Provenance information
    #[serde(default)]
    pub provenance: Provenance,
    /// Verification status
    #[serde(default)]
    pub verification: Verification,
    /// Optional session-specific fields (for recaps)
    #[serde(default)]
    pub session: Option<SessionMeta>,
}

/// Validation error for a memory document
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Field that failed validation
    pub field: String,
    /// Error message
    pub message: String,
}

impl ValidationError {
    /// Create a new validation error
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

impl Display for MemoryDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let frontmatter_yaml = serde_yml::to_string(&self.frontmatter).expect("Frontmatter serialization failed");

        write!(f, "---\n{}---\n\n{}", frontmatter_yaml, self.body)
    }
}

impl MemoryDoc {
    /// Parse a markdown file with YAML frontmatter
    ///
    /// Expected format:
    /// ```markdown
    /// ---
    /// id: core.project
    /// title: Project Core Memory
    /// kind: core
    /// tags: [core, always-loaded]
    /// created: 2026-01-21T00:00:00Z
    /// updated: 2026-01-21T00:00:00Z
    /// ---
    ///
    /// # Body content here
    /// ```
    pub fn parse(content: &str) -> Result<Self> {
        let content = content.trim_start();

        if !content.starts_with("---") {
            return Err(Error::Parse("Missing frontmatter delimiter".to_string()));
        }

        let after_delim = &content[3..];
        let end_idx = after_delim
            .find("---")
            .ok_or_else(|| Error::Parse("Unclosed frontmatter delimiter".to_string()))?;

        let frontmatter_str = &after_delim[..end_idx];
        let body_start = end_idx + 3;
        let body = after_delim[body_start..].trim_start().to_string();

        let frontmatter: MemoryFrontmatter = serde_yml::from_str(frontmatter_str)
            .map_err(|e| Error::Parse(format!("Invalid YAML frontmatter: {}", e)))?;

        Ok(Self { frontmatter, body })
    }

    /// Validate document structure
    ///
    /// Returns a list of validation errors (empty if valid)
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.frontmatter.id.is_empty() {
            errors.push(ValidationError::new("id", "ID cannot be empty"));
        }

        if self.frontmatter.title.is_empty() {
            errors.push(ValidationError::new("title", "Title cannot be empty"));
        }

        if !self.frontmatter.id.contains('.') && self.frontmatter.kind != MemoryKind::Core {
            errors.push(ValidationError::new(
                "id",
                "ID should use dot notation (e.g., 'fact.testing.coverage')",
            ));
        }

        if self.frontmatter.tags.is_empty() {
            errors.push(ValidationError::new("tags", "At least one tag is required"));
        }

        if self.frontmatter.updated < self.frontmatter.created {
            errors.push(ValidationError::new(
                "updated",
                "Updated timestamp cannot be before created timestamp",
            ));
        }

        if self.frontmatter.kind == MemoryKind::Core {
            self.validate_core(&mut errors);
        }

        if self.frontmatter.kind == MemoryKind::Adr {
            self.validate_adr(&mut errors);
        }

        if self.frontmatter.kind == MemoryKind::Playbook {
            self.validate_playbook(&mut errors);
        }

        errors
    }

    /// Validate core memory requirements
    fn validate_core(&self, body: &mut Vec<ValidationError>) {
        let required_sections = ["## Identity", "## Commands", "## Architecture", "## Conventions"];
        for section in required_sections {
            if !self.body.contains(section) {
                body.push(ValidationError::new(
                    "body",
                    format!("Missing required section: {}", section),
                ));
            }
        }
    }

    /// Validate ADR requirements
    fn validate_adr(&self, body: &mut Vec<ValidationError>) {
        let required_sections = ["## Status", "## Context", "## Decision", "## Consequences"];
        for section in required_sections {
            if !self.body.contains(section) {
                body.push(ValidationError::new(
                    "body",
                    format!("Missing required section: {}", section),
                ));
            }
        }
    }

    /// Validate playbook requirements
    fn validate_playbook(&self, body: &mut Vec<ValidationError>) {
        let required_sections = ["## Preconditions", "## Steps", "## Verification"];
        for section in required_sections {
            if !self.body.contains(section) {
                body.push(ValidationError::new(
                    "body",
                    format!("Missing required section: {}", section),
                ));
            }
        }
    }

    /// Check if the document body is empty
    pub fn is_body_empty(&self) -> bool {
        self.body.trim().is_empty()
    }

    /// Get the approximate token count of the document
    ///
    /// Uses a simple heuristic: ~4 characters per token
    pub fn approx_token_count(&self) -> usize {
        (self.frontmatter.id.len() + self.frontmatter.title.len() + self.body.len()) / 4
    }

    /// Create a new memory document with minimal fields
    pub fn new(
        id: impl Into<String>, title: impl Into<String>, kind: MemoryKind, tags: Vec<String>, body: impl Into<String>,
    ) -> Self {
        let now = Utc::now();

        Self {
            frontmatter: MemoryFrontmatter {
                id: id.into(),
                title: title.into(),
                kind,
                tags,
                created: now,
                updated: now,
                provenance: Provenance::default(),
                verification: Verification::default(),
                session: None,
            },
            body: body.into(),
        }
    }

    /// Update the document body and set updated timestamp
    pub fn update_body(&mut self, new_body: impl Into<String>) {
        self.body = new_body.into();
        self.frontmatter.updated = Utc::now();
    }

    /// Add a tag to the document
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.frontmatter.tags.contains(&tag) {
            self.frontmatter.tags.push(tag);
        }
    }

    /// Add provenance event
    pub fn add_provenance_event(&mut self, event_id: impl Into<String>) {
        self.frontmatter.provenance.events.push(event_id.into());
        self.frontmatter.updated = Utc::now();
    }
}

impl From<MemoryDoc> for String {
    fn from(doc: MemoryDoc) -> String {
        doc.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CORE_DOC: &str = r#"---
id: core.project
title: Project Core Memory
kind: core
tags: [core, always-loaded]
created: 2026-01-21T00:00:00Z
updated: 2026-01-21T00:00:00Z
provenance:
  events: []
  patches: []
  commits: []
verification:
  last_verified_commit: null
  status: unknown
---

# Project Core Memory

## Identity
Project name, purpose, one-liner description

## Commands
Common dev commands

## Architecture
High-level structure

## Conventions
Code style patterns

## Gotchas
Common mistakes
"#;

    const VALID_ADR_DOC: &str = r#"---
id: adr.0001
title: Use imara-diff for code diffing
kind: adr
tags: [diff, dependencies, core]
created: 2026-01-19T00:00:00Z
updated: 2026-01-19T00:00:00Z
provenance:
  events: [evt_def456, evt_ghi789]
  patches: [patch_diff_upgrade]
  commits: [d4e5f6a]
verification:
  last_verified_commit: d4e5f6a
  status: verified
---

# ADR-0001: Use imara-diff for code diffing

## Status
Accepted

## Context
Need semantic-aware diffing for code edits.

## Decision
Use `imara-diff` crate.

## Consequences
- Positive: Better diffs for refactoring
- Negative: Additional dependency
"#;

    #[test]
    fn test_parse_valid_core_doc() {
        let doc = MemoryDoc::parse(VALID_CORE_DOC).unwrap();

        assert_eq!(doc.frontmatter.id, "core.project");
        assert_eq!(doc.frontmatter.title, "Project Core Memory");
        assert_eq!(doc.frontmatter.kind, MemoryKind::Core);
        assert_eq!(doc.frontmatter.tags, vec!["core", "always-loaded"]);
        assert!(doc.body.contains("## Identity"));
        assert!(doc.body.contains("## Commands"));
    }

    #[test]
    fn test_parse_valid_adr_doc() {
        let doc = MemoryDoc::parse(VALID_ADR_DOC).unwrap();

        assert_eq!(doc.frontmatter.id, "adr.0001");
        assert_eq!(doc.frontmatter.kind, MemoryKind::Adr);
        assert_eq!(doc.frontmatter.provenance.events, vec!["evt_def456", "evt_ghi789"]);
        assert_eq!(
            doc.frontmatter.verification.last_verified_commit,
            Some("d4e5f6a".to_string())
        );
        assert!(doc.body.contains("## Status"));
        assert!(doc.body.contains("## Decision"));
    }

    #[test]
    fn test_parse_missing_delimiter() {
        let content = "# No frontmatter here";
        let result = MemoryDoc::parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unclosed_delimiter() {
        let content = "---\nid: test\n# No closing delimiter";
        let result = MemoryDoc::parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let content = "---\nid: test\ntitle: [invalid\n---\n\nBody";
        let result = MemoryDoc::parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_string_roundtrip() {
        let doc = MemoryDoc::parse(VALID_CORE_DOC).unwrap();
        let serialized = doc.to_string();
        let parsed = MemoryDoc::parse(&serialized).unwrap();

        assert_eq!(parsed.frontmatter.id, doc.frontmatter.id);
        assert_eq!(parsed.frontmatter.title, doc.frontmatter.title);
        assert_eq!(parsed.body, doc.body);
    }

    #[test]
    fn test_validate_valid_doc() {
        let doc = MemoryDoc::parse(VALID_CORE_DOC).unwrap();
        let errors = doc.validate();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_empty_id() {
        let mut doc = MemoryDoc::parse(VALID_CORE_DOC).unwrap();
        doc.frontmatter.id = String::new();

        let errors = doc.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "id"));
    }

    #[test]
    fn test_validate_empty_title() {
        let mut doc = MemoryDoc::parse(VALID_CORE_DOC).unwrap();
        doc.frontmatter.title = String::new();

        let errors = doc.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "title"));
    }

    #[test]
    fn test_validate_empty_tags() {
        let mut doc = MemoryDoc::parse(VALID_CORE_DOC).unwrap();
        doc.frontmatter.tags = Vec::new();

        let errors = doc.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "tags"));
    }

    #[test]
    fn test_validate_adr_missing_sections() {
        let doc = MemoryDoc::new(
            "adr.0001",
            "Test ADR",
            MemoryKind::Adr,
            vec!["test".to_string()],
            "# Title\n\nNo required sections here",
        );

        let errors = doc.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "body"));
    }

    #[test]
    fn test_validate_playbook_missing_sections() {
        let doc = MemoryDoc::new(
            "playbook.test",
            "Test Playbook",
            MemoryKind::Playbook,
            vec!["test".to_string()],
            "# Title\n\nNo required sections here",
        );

        let errors = doc.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "body"));
    }

    #[test]
    fn test_is_body_empty() {
        let doc = MemoryDoc::new("test.id", "Test", MemoryKind::Fact, vec!["test".to_string()], "");

        assert!(doc.is_body_empty());

        let doc = MemoryDoc::new(
            "test.id",
            "Test",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Some content",
        );

        assert!(!doc.is_body_empty());
    }

    #[test]
    fn test_approx_token_count() {
        let doc = MemoryDoc::new(
            "test.id",
            "Test Document",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "This is some body content that we can use to estimate token count.",
        );

        let count = doc.approx_token_count();
        assert!(count > 0);
    }

    #[test]
    fn test_new_doc() {
        let doc = MemoryDoc::new(
            "fact.test",
            "Test Fact",
            MemoryKind::Fact,
            vec!["test".to_string(), "fact".to_string()],
            "# Test\n\nContent here",
        );

        assert_eq!(doc.frontmatter.id, "fact.test");
        assert_eq!(doc.frontmatter.title, "Test Fact");
        assert_eq!(doc.frontmatter.kind, MemoryKind::Fact);
        assert!(doc.body.contains("Content here"));
    }

    #[test]
    fn test_update_body() {
        let mut doc = MemoryDoc::new(
            "test.id",
            "Test",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Old body",
        );

        let old_updated = doc.frontmatter.updated;
        std::thread::sleep(std::time::Duration::from_millis(10));
        doc.update_body("New body");

        assert_eq!(doc.body, "New body");
        assert!(doc.frontmatter.updated > old_updated);
    }

    #[test]
    fn test_add_tag() {
        let mut doc = MemoryDoc::new("test.id", "Test", MemoryKind::Fact, vec!["test".to_string()], "Body");

        doc.add_tag("new-tag");

        assert!(doc.frontmatter.tags.contains(&"new-tag".to_string()));
        assert_eq!(doc.frontmatter.tags.len(), 2);
    }

    #[test]
    fn test_add_tag_no_duplicate() {
        let mut doc = MemoryDoc::new("test.id", "Test", MemoryKind::Fact, vec!["test".to_string()], "Body");

        doc.add_tag("test");
        assert_eq!(doc.frontmatter.tags.len(), 1);
    }

    #[test]
    fn test_add_provenance_event() {
        let mut doc = MemoryDoc::new("test.id", "Test", MemoryKind::Fact, vec!["test".to_string()], "Body");

        let old_updated = doc.frontmatter.updated;
        doc.add_provenance_event("evt_001");

        assert_eq!(doc.frontmatter.provenance.events, vec!["evt_001".to_string()]);
        assert!(doc.frontmatter.updated > old_updated);
    }

    #[test]
    fn test_memory_doc_serialization() {
        let doc = MemoryDoc::parse(VALID_ADR_DOC).unwrap();

        let json = serde_json::to_string(&doc).unwrap();
        let parsed: MemoryDoc = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.frontmatter.id, doc.frontmatter.id);
        assert_eq!(parsed.frontmatter.title, doc.frontmatter.title);
    }

    #[test]
    fn test_frontmatter_default_fields() {
        let frontmatter = MemoryFrontmatter {
            id: "test".to_string(),
            title: "Test".to_string(),
            kind: MemoryKind::Fact,
            tags: vec!["test".to_string()],
            created: Utc::now(),
            updated: Utc::now(),
            provenance: Provenance::default(),
            verification: Verification::default(),
            session: None,
        };

        assert!(frontmatter.provenance.is_empty());
        assert!(!frontmatter.verification.is_verified());
        assert!(frontmatter.session.is_none());
    }
}
