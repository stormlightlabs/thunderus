//! Memory lint rules and diagnostics
//!
//! Enforces quality and consistency rules for memory documents.

use crate::memory::{self, document::MemoryDoc, kinds::MemoryKind};

use std::path::{Path, PathBuf};

/// Severity level for lint diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintSeverity {
    /// Informational only
    Info,
    /// Warning (should fix)
    Warning,
    /// Error (must fix)
    Error,
}

/// A lint diagnostic for a memory document
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    /// Rule ID (e.g., "mem001")
    pub rule: String,
    /// Severity level
    pub severity: LintSeverity,
    /// Warning message
    pub message: String,
    /// Path to the file
    pub path: PathBuf,
    /// Line number (if applicable)
    pub line: Option<usize>,
    /// Suggested fix
    pub fix_hint: Option<String>,
}

/// Memory linter with configurable rules
pub struct MemoryLinter {
    rules: Vec<Box<dyn LintRule>>,
}

impl MemoryLinter {
    /// Create a new memory linter with default rules
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(RequiredFieldsRule),
                Box::new(CoreMemorySizeRule),
                Box::new(CoreMemoryHardLimitRule),
                Box::new(ProvenanceLinksRule),
                Box::new(StaleDocumentRule),
                Box::new(EmptyBodyRule),
                Box::new(AdrSectionsRule),
                Box::new(PlaybookSectionsRule),
            ],
        }
    }

    /// Add a custom lint rule
    pub fn add_rule(&mut self, rule: Box<dyn LintRule>) {
        self.rules.push(rule);
    }

    /// Run all lint rules on a document
    pub fn lint(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for rule in &self.rules {
            let mut rule_diagnostics = rule.check(doc, path);
            diagnostics.append(&mut rule_diagnostics);
        }

        diagnostics
    }

    /// Lint all documents in memory directory
    pub fn lint_all(&self, paths: &memory::MemoryPaths) -> Vec<LintDiagnostic> {
        let mut all_diagnostics = Vec::new();

        for path in &[paths.core_memory_file(), paths.core_local_memory_file()] {
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(path)
                && let Ok(doc) = MemoryDoc::parse(&content)
            {
                all_diagnostics.extend(self.lint(&doc, path));
            }
        }

        for dir in &[&paths.facts, &paths.decisions] {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("md")
                        && let Ok(content) = std::fs::read_to_string(&path)
                        && let Ok(doc) = MemoryDoc::parse(&content)
                    {
                        all_diagnostics.extend(self.lint(&doc, &path));
                    }
                }
            }
        }

        if let Ok(entries) = std::fs::read_dir(&paths.playbooks) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md")
                    && let Ok(content) = std::fs::read_to_string(&path)
                    && let Ok(doc) = MemoryDoc::parse(&content)
                {
                    all_diagnostics.extend(self.lint(&doc, &path));
                }
            }
        }

        all_diagnostics
    }

    /// Get errors only (excluding warnings and info)
    pub fn errors(self, diagnostics: &[LintDiagnostic]) -> Vec<&LintDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.severity == LintSeverity::Error)
            .collect()
    }

    /// Get warnings only (excluding errors and info)
    pub fn warnings(self, diagnostics: &[LintDiagnostic]) -> Vec<&LintDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.severity == LintSeverity::Warning)
            .collect()
    }
}

impl Default for MemoryLinter {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for lint rules
pub trait LintRule {
    /// Get the rule ID
    fn id(&self) -> &str;

    /// Check a document and return diagnostics
    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic>;
}

/// Rule: mem001 - Missing required frontmatter field
#[derive(Debug)]
struct RequiredFieldsRule;

impl LintRule for RequiredFieldsRule {
    fn id(&self) -> &str {
        "mem001"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        let errors = doc.validate();
        for error in errors {
            if error.field == "id" || error.field == "title" || error.field == "tags" {
                diagnostics.push(LintDiagnostic {
                    rule: self.id().to_string(),
                    severity: LintSeverity::Error,
                    message: format!("Missing required field: {}", error.field),
                    path: path.to_path_buf(),
                    line: None,
                    fix_hint: Some(format!("Add {} to the frontmatter", error.field)),
                });
            }
        }

        diagnostics
    }
}

/// Rule: mem003 - Core memory exceeds soft token limit
#[derive(Debug)]
struct CoreMemorySizeRule;

impl LintRule for CoreMemorySizeRule {
    fn id(&self) -> &str {
        "mem003"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if doc.frontmatter.kind == MemoryKind::Core {
            let token_count = doc.approx_token_count();
            let soft_limit = memory::CORE_MEMORY_SOFT_LIMIT;

            if token_count > soft_limit {
                diagnostics.push(LintDiagnostic {
                    rule: self.id().to_string(),
                    severity: LintSeverity::Warning,
                    message: format!(
                        "Core memory exceeds soft limit: {} tokens (limit: {})",
                        token_count, soft_limit
                    ),
                    path: path.to_path_buf(),
                    line: None,
                    fix_hint: Some(
                        "Consider splitting core memory into smaller documents or moving content to semantic memory"
                            .to_string(),
                    ),
                });
            }
        }

        diagnostics
    }
}

/// Rule: mem004 - Core memory exceeds hard token limit
#[derive(Debug)]
struct CoreMemoryHardLimitRule;

impl LintRule for CoreMemoryHardLimitRule {
    fn id(&self) -> &str {
        "mem004"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if doc.frontmatter.kind == MemoryKind::Core {
            let token_count = doc.approx_token_count();
            let hard_limit = memory::CORE_MEMORY_HARD_LIMIT;

            if token_count > hard_limit {
                diagnostics.push(LintDiagnostic {
                    rule: self.id().to_string(),
                    severity: LintSeverity::Error,
                    message: format!(
                        "Core memory exceeds hard limit: {} tokens (limit: {})",
                        token_count, hard_limit
                    ),
                    path: path.to_path_buf(),
                    line: None,
                    fix_hint: Some(
                        "Split core memory into smaller documents or move content to semantic memory".to_string(),
                    ),
                });
            }
        }

        diagnostics
    }
}

/// Rule: mem005 - Missing provenance links
#[derive(Debug)]
struct ProvenanceLinksRule;

impl LintRule for ProvenanceLinksRule {
    fn id(&self) -> &str {
        "mem005"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if doc.frontmatter.kind == MemoryKind::Core {
            return diagnostics;
        }

        if doc.frontmatter.provenance.events.is_empty()
            && doc.frontmatter.provenance.patches.is_empty()
            && doc.frontmatter.provenance.commits.is_empty()
        {
            diagnostics.push(LintDiagnostic {
                rule: self.id().to_string(),
                severity: LintSeverity::Warning,
                message: "Missing provenance links".to_string(),
                path: path.to_path_buf(),
                line: None,
                fix_hint: Some("Add related events, patches, or commits to the provenance field".to_string()),
            });
        }

        diagnostics
    }
}

/// Rule: mem006 - Document marked as stale
#[derive(Debug)]
struct StaleDocumentRule;

impl LintRule for StaleDocumentRule {
    fn id(&self) -> &str {
        "mem006"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if let memory::VerificationStatus::Stale = doc.frontmatter.verification.status {
            diagnostics.push(LintDiagnostic {
                rule: self.id().to_string(),
                severity: LintSeverity::Warning,
                message: "Document marked as stale (repository changed since last verification)".to_string(),
                path: path.to_path_buf(),
                line: None,
                fix_hint: Some("Review and verify document content".to_string()),
            });
        }

        diagnostics
    }
}

/// Rule: mem007 - Document has empty body
#[derive(Debug)]
struct EmptyBodyRule;

impl LintRule for EmptyBodyRule {
    fn id(&self) -> &str {
        "mem007"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if doc.is_body_empty() {
            diagnostics.push(LintDiagnostic {
                rule: self.id().to_string(),
                severity: LintSeverity::Warning,
                message: "Document has empty body".to_string(),
                path: path.to_path_buf(),
                line: None,
                fix_hint: Some("Add content to the document body".to_string()),
            });
        }

        diagnostics
    }
}

/// Rule: mem008 - ADR missing required sections
#[derive(Debug)]
struct AdrSectionsRule;

impl LintRule for AdrSectionsRule {
    fn id(&self) -> &str {
        "mem008"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if doc.frontmatter.kind != MemoryKind::Adr {
            return diagnostics;
        }

        let errors = doc.validate();
        for error in errors {
            if error.field == "body" && error.message.contains("Missing required section") {
                diagnostics.push(LintDiagnostic {
                    rule: self.id().to_string(),
                    severity: LintSeverity::Warning,
                    message: error.message,
                    path: path.to_path_buf(),
                    line: None,
                    fix_hint: Some(
                        "Add the required ADR sections: Status, Context, Decision, Consequences".to_string(),
                    ),
                });
            }
        }

        diagnostics
    }
}

/// Rule: mem009 - Playbook missing preconditions/verification
#[derive(Debug)]
struct PlaybookSectionsRule;

impl LintRule for PlaybookSectionsRule {
    fn id(&self) -> &str {
        "mem009"
    }

    fn check(&self, doc: &MemoryDoc, path: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        if doc.frontmatter.kind != MemoryKind::Playbook {
            return diagnostics;
        }

        let errors = doc.validate();
        for error in errors {
            if error.field == "body" && error.message.contains("Missing required section") {
                diagnostics.push(LintDiagnostic {
                    rule: self.id().to_string(),
                    severity: LintSeverity::Warning,
                    message: error.message,
                    path: path.to_path_buf(),
                    line: None,
                    fix_hint: Some(
                        "Add the required playbook sections: Preconditions, Steps, Verification".to_string(),
                    ),
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_fields_rule() {
        let rule = RequiredFieldsRule;
        let valid_doc = MemoryDoc::new(
            "fact.test",
            "Test",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Content",
        );

        let diagnostics = rule.check(&valid_doc, Path::new("test.md"));
        assert!(diagnostics.is_empty());

        let invalid_doc = MemoryDoc::new("fact.test", "", MemoryKind::Fact, vec!["test".to_string()], "Content");
        let diagnostics = rule.check(&invalid_doc, Path::new("test.md"));
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].rule, "mem001");
        assert_eq!(diagnostics[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_empty_body_rule() {
        let rule = EmptyBodyRule;
        let empty_doc = MemoryDoc::new("fact.test", "Test", MemoryKind::Fact, vec!["test".to_string()], "");
        let diagnostics = rule.check(&empty_doc, Path::new("test.md"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "mem007");
        assert_eq!(diagnostics[0].severity, LintSeverity::Warning);
    }

    #[test]
    fn test_provenance_links_rule() {
        let rule = ProvenanceLinksRule;

        let doc = MemoryDoc::new(
            "fact.test",
            "Test",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Content",
        );

        let diagnostics = rule.check(&doc, Path::new("test.md"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "mem005");
    }

    #[test]
    fn test_stale_document_rule() {
        let rule = StaleDocumentRule;

        let mut doc = MemoryDoc::new(
            "fact.test",
            "Test",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Content",
        );
        doc.frontmatter.verification.status = memory::VerificationStatus::Stale;

        let diagnostics = rule.check(&doc, Path::new("test.md"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "mem006");
    }

    #[test]
    fn test_memory_linter_multiple_rules() {
        let linter = MemoryLinter::new();
        let mut doc = MemoryDoc::new("fact.test", "", MemoryKind::Fact, vec![], "");
        doc.frontmatter.verification.status = memory::VerificationStatus::Stale;

        let diagnostics = linter.lint(&doc, Path::new("test.md"));
        assert!(diagnostics.len() >= 3);
    }

    #[test]
    fn test_linter_errors_only() {
        let linter = MemoryLinter::new();

        let doc = MemoryDoc::new(
            "core.test",
            "Test",
            MemoryKind::Core,
            vec!["test".to_string()],
            "x".repeat(40000),
        );

        let diagnostics = linter.lint(&doc, Path::new("test.md"));
        let errors = linter.errors(&diagnostics);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.rule == "mem004"));
    }

    #[test]
    fn test_linter_warnings_only() {
        let linter = MemoryLinter::new();

        let doc = MemoryDoc::new(
            "fact.test",
            "Test",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Content",
        );

        let diagnostics = linter.lint(&doc, Path::new("test.md"));
        let warnings = linter.warnings(&diagnostics);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.rule == "mem005"));
    }
}
