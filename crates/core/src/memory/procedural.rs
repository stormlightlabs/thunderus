//! Procedural memory management for Playbooks
//!
//! Procedural memory stores reusable workflows: step-by-step playbooks
//! with verification and rollback procedures.

use crate::error::{Error, Result};
use crate::memory::document::MemoryDoc;
use crate::memory::kinds::{MemoryKind, VerificationStatus};
use crate::memory::paths::MemoryPaths;

use std::fs;
use std::path::PathBuf;

/// Manager for procedural memory documents (Playbooks)
///
/// Provides methods for creating, loading, updating, and validating
/// playbook documents.
#[derive(Debug, Clone)]
pub struct ProceduralMemory {
    /// Memory directory paths
    paths: MemoryPaths,
}

impl ProceduralMemory {
    /// Create a new procedural memory manager
    pub fn new(paths: MemoryPaths) -> Self {
        Self { paths }
    }

    /// List all playbook documents
    pub fn list_playbooks(&self) -> Result<Vec<PlaybookDoc>> {
        let mut playbooks = Vec::new();

        let playbooks_dir = &self.paths.playbooks;
        if !playbooks_dir.exists() {
            return Ok(playbooks);
        }

        let entries = fs::read_dir(playbooks_dir).map_err(Error::Io)?;

        for entry in entries {
            let entry = entry.map_err(Error::Io)?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let content = fs::read_to_string(&path).map_err(Error::Io)?;

                let doc = MemoryDoc::parse(&content)
                    .map_err(|e| Error::Parse(format!("Failed to parse playbook document: {}", e)))?;

                if doc.frontmatter.kind == MemoryKind::Playbook {
                    playbooks.push(PlaybookDoc { path, doc });
                }
            }
        }

        Ok(playbooks)
    }

    /// Load a playbook document by ID
    pub fn load_playbook(&self, id: &str) -> Result<PlaybookDoc> {
        let playbooks = self.list_playbooks()?;
        playbooks
            .into_iter()
            .find(|p| p.doc.frontmatter.id == id)
            .ok_or_else(|| Error::Other(format!("Playbook not found: {}", id)))
    }

    /// Create a new playbook document
    pub fn create_playbook(&self, playbook: NewPlaybook) -> Result<PlaybookDoc> {
        let doc = MemoryDoc::new(
            playbook.id,
            playbook.title,
            MemoryKind::Playbook,
            playbook.tags,
            playbook.body,
        );

        let errors = doc.validate();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "Invalid playbook document: {}",
                errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            )));
        }

        let filename = format!("{}.md", doc.frontmatter.id.replace('.', "_"));
        let path = self.paths.playbooks.join(filename);

        let content = doc.to_string();
        fs::write(&path, content).map_err(|e| Error::Other(format!("Failed to write playbook document: {}", e)))?;

        Ok(PlaybookDoc { path, doc })
    }

    /// Update an existing playbook document
    pub fn update_playbook(&self, id: &str, update: PlaybookUpdate) -> Result<PlaybookDoc> {
        let mut playbook = self.load_playbook(id)?;

        if let Some(body) = update.body {
            playbook.doc.update_body(body);
        }

        if let Some(tags) = update.tags {
            playbook.doc.frontmatter.tags = tags;
        }

        if let Some(event_id) = update.provenance_event {
            playbook.doc.add_provenance_event(event_id);
        }

        if let Some(status) = update.status {
            playbook.doc.frontmatter.verification.status = status;
        }

        let errors = playbook.doc.validate();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "Invalid playbook document: {}",
                errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            )));
        }

        let content = playbook.doc.to_string();
        fs::write(&playbook.path, content)
            .map_err(|e| Error::Other(format!("Failed to write playbook document: {}", e)))?;

        Ok(playbook)
    }

    /// Delete a playbook document
    pub fn delete_playbook(&self, id: &str) -> Result<()> {
        let playbook = self.load_playbook(id)?;
        fs::remove_file(&playbook.path)
            .map_err(|e| Error::Other(format!("Failed to delete playbook document: {}", e)))?;
        Ok(())
    }

    /// Search playbooks by tag
    pub fn find_playbooks_by_tag(&self, tag: &str) -> Result<Vec<PlaybookDoc>> {
        let playbooks = self.list_playbooks()?;
        Ok(playbooks
            .into_iter()
            .filter(|p| p.doc.frontmatter.tags.iter().any(|t| t == tag))
            .collect())
    }

    /// Parse playbook sections from body content
    ///
    /// Extracts preconditions, steps, verification, and rollback sections.
    pub fn parse_playbook_sections(&self, playbook: &PlaybookDoc) -> Result<PlaybookSections> {
        let body = &playbook.doc.body;

        let preconditions = Self::extract_section(body, "## Preconditions");
        let steps = Self::extract_section(body, "## Steps");
        let verification = Self::extract_section(body, "## Verification");
        let rollback = Self::extract_section(body, "## Rollback");

        Ok(PlaybookSections { preconditions, steps, verification, rollback })
    }

    /// Extract a section from markdown body
    fn extract_section(body: &str, section_name: &str) -> String {
        let section_start = body.find(section_name);

        if let Some(start) = section_start {
            let after_start = &body[start + section_name.len()..];
            let section_end = after_start
                .find("## ")
                .map(|pos| start + section_name.len() + pos)
                .or(Some(body.len()));

            if let Some(end) = section_end {
                return after_start[..end - start - section_name.len()].trim().to_string();
            }
        }

        String::new()
    }

    /// Validate playbook structure beyond basic document validation
    ///
    /// Checks that the playbook has meaningful content in each required section.
    pub fn validate_playbook_content(&self, playbook: &PlaybookDoc) -> Result<Vec<PlaybookIssue>> {
        let mut issues = Vec::new();
        let sections = self.parse_playbook_sections(playbook)?;

        if sections.preconditions.trim().is_empty() {
            issues.push(PlaybookIssue {
                severity: IssueSeverity::Warning,
                message: "Preconditions section is empty".to_string(),
                section: "Preconditions".to_string(),
            });
        }

        if sections.steps.trim().is_empty() {
            issues.push(PlaybookIssue {
                severity: IssueSeverity::Error,
                message: "Steps section is empty".to_string(),
                section: "Steps".to_string(),
            });
        }

        if sections.verification.trim().is_empty() {
            issues.push(PlaybookIssue {
                severity: IssueSeverity::Warning,
                message: "Verification section is empty".to_string(),
                section: "Verification".to_string(),
            });
        }

        if sections.rollback.trim().is_empty() {
            issues.push(PlaybookIssue {
                severity: IssueSeverity::Info,
                message: "Rollback section is empty (optional but recommended)".to_string(),
                section: "Rollback".to_string(),
            });
        }

        Ok(issues)
    }
}

/// A playbook document with its path
#[derive(Debug, Clone)]
pub struct PlaybookDoc {
    /// Path to the playbook file
    pub path: PathBuf,
    /// The parsed playbook document
    pub doc: MemoryDoc,
}

/// Parsed sections from a playbook document
#[derive(Debug, Clone)]
pub struct PlaybookSections {
    /// Preconditions that must be met before running the playbook
    pub preconditions: String,
    /// Step-by-step instructions
    pub steps: String,
    /// Verification steps to confirm success
    pub verification: String,
    /// Rollback procedure if something goes wrong
    pub rollback: String,
}

/// An issue found during playbook content validation
#[derive(Debug, Clone)]
pub struct PlaybookIssue {
    /// Severity of the issue
    pub severity: IssueSeverity,
    /// Issue message
    pub message: String,
    /// Section where the issue was found
    pub section: String,
}

/// Severity level for playbook validation issues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Informational (suggestions)
    Info,
    /// Warning (should fix)
    Warning,
    /// Error (must fix)
    Error,
}

/// Data for creating a new playbook
pub struct NewPlaybook {
    /// Unique identifier (e.g., "playbook.release")
    pub id: String,
    /// Title of the playbook
    pub title: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Body content (markdown with required sections)
    pub body: String,
}

/// Data for updating a playbook
pub struct PlaybookUpdate {
    /// New body content
    pub body: Option<String>,
    /// New tags
    pub tags: Option<Vec<String>>,
    /// Provenance event to add
    pub provenance_event: Option<String>,
    /// Verification status
    pub status: Option<VerificationStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_procedural_memory() -> (TempDir, ProceduralMemory) {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();
        let procedural = ProceduralMemory::new(paths);
        (temp, procedural)
    }

    fn valid_playbook_body() -> String {
        r#"## Preconditions

- [ ] All tests passing on main branch
- [ ] CHANGELOG.md updated

## Steps

1. Create release branch
   ```bash
   git checkout -b release/v0.2.0
   ```

2. Run full test suite
   ```bash
   cargo test --all-features
   ```

## Verification

- [ ] CI passed on release tag
- [ ] Binaries published to releases page

## Rollback

1. Delete tag: `git tag -d v0.2.0`
2. Revert release commit"#
            .to_string()
    }

    #[test]
    fn test_procedural_memory_list_empty() {
        let (_temp, procedural) = create_test_procedural_memory();

        let playbooks = procedural.list_playbooks().unwrap();

        assert!(playbooks.is_empty());
    }

    #[test]
    fn test_create_playbook() {
        let (_temp, procedural) = create_test_procedural_memory();

        let playbook = procedural
            .create_playbook(NewPlaybook {
                id: "playbook.release".to_string(),
                title: "Release Process".to_string(),
                tags: vec!["release".to_string(), "ci".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        assert_eq!(playbook.doc.frontmatter.id, "playbook.release");
        assert_eq!(playbook.doc.frontmatter.title, "Release Process");
        assert!(playbook.path.exists());
    }

    #[test]
    fn test_load_playbook() {
        let (_temp, procedural) = create_test_procedural_memory();

        procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.example".to_string(),
                title: "Test Playbook".to_string(),
                tags: vec!["test".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        let loaded = procedural.load_playbook("playbook.test.example").unwrap();
        assert_eq!(loaded.doc.frontmatter.id, "playbook.test.example");
        assert_eq!(loaded.doc.frontmatter.title, "Test Playbook");
    }

    #[test]
    fn test_update_playbook() {
        let (_temp, procedural) = create_test_procedural_memory();

        procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.updatable".to_string(),
                title: "Original Title".to_string(),
                tags: vec!["test".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        let updated = procedural
            .update_playbook(
                "playbook.test.updatable",
                PlaybookUpdate {
                    body: Some(valid_playbook_body()),
                    tags: Some(vec!["test".to_string(), "updated".to_string()]),
                    provenance_event: Some("evt_001".to_string()),
                    status: Some(VerificationStatus::Verified),
                },
            )
            .unwrap();

        assert_eq!(updated.doc.frontmatter.tags.len(), 2);
        assert!(
            updated
                .doc
                .frontmatter
                .provenance
                .events
                .contains(&"evt_001".to_string())
        );
        assert_eq!(
            updated.doc.frontmatter.verification.status,
            VerificationStatus::Verified
        );
    }

    #[test]
    fn test_delete_playbook() {
        let (_temp, procedural) = create_test_procedural_memory();

        procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.deletable".to_string(),
                title: "Delete Me".to_string(),
                tags: vec!["test".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        procedural.delete_playbook("playbook.test.deletable").unwrap();

        let result = procedural.load_playbook("playbook.test.deletable");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_playbooks_by_tag() {
        let (_temp, procedural) = create_test_procedural_memory();

        procedural
            .create_playbook(NewPlaybook {
                id: "playbook.deploy.one".to_string(),
                title: "One".to_string(),
                tags: vec!["deploy".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        procedural
            .create_playbook(NewPlaybook {
                id: "playbook.deploy.two".to_string(),
                title: "Two".to_string(),
                tags: vec!["deploy".to_string(), "production".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        procedural
            .create_playbook(NewPlaybook {
                id: "playbook.build.three".to_string(),
                title: "Three".to_string(),
                tags: vec!["build".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        let deploy_playbooks = procedural.find_playbooks_by_tag("deploy").unwrap();
        assert_eq!(deploy_playbooks.len(), 2);
    }

    #[test]
    fn test_parse_playbook_sections() {
        let (_temp, procedural) = create_test_procedural_memory();

        let playbook = procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.sections".to_string(),
                title: "Test Sections".to_string(),
                tags: vec!["test".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        let sections = procedural.parse_playbook_sections(&playbook).unwrap();

        assert!(!sections.preconditions.is_empty());
        assert!(!sections.steps.is_empty());
        assert!(!sections.verification.is_empty());
        assert!(!sections.rollback.is_empty());

        assert!(sections.preconditions.contains("CHANGELOG.md"));
        assert!(sections.steps.contains("git checkout"));
        assert!(sections.verification.contains("CI passed"));
        assert!(sections.rollback.contains("Delete tag"));
    }

    #[test]
    fn test_validate_playbook_content_valid() {
        let (_temp, procedural) = create_test_procedural_memory();

        let playbook = procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.valid".to_string(),
                title: "Valid Playbook".to_string(),
                tags: vec!["test".to_string()],
                body: valid_playbook_body(),
            })
            .unwrap();

        let issues = procedural.validate_playbook_content(&playbook).unwrap();

        assert!(issues.is_empty());
    }

    #[test]
    fn test_validate_playbook_content_empty_steps() {
        let (_temp, procedural) = create_test_procedural_memory();

        let body = r#"## Preconditions

- Some precondition

## Steps

## Verification

- Some check

## Rollback

Some rollback"#
            .to_string();

        let playbook = procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.empty_steps".to_string(),
                title: "Empty Steps".to_string(),
                tags: vec!["test".to_string()],
                body,
            })
            .unwrap();

        let issues = procedural.validate_playbook_content(&playbook).unwrap();

        assert!(!issues.is_empty());
        assert!(
            issues
                .iter()
                .any(|i| i.section == "Steps" && i.severity == IssueSeverity::Error)
        );
    }

    #[test]
    fn test_validate_playbook_content_missing_optional_sections() {
        let (_temp, procedural) = create_test_procedural_memory();

        let body = r#"## Preconditions

## Steps

1. Do something

## Verification

- Check something

## Rollback

"#
        .to_string();

        let playbook = procedural
            .create_playbook(NewPlaybook {
                id: "playbook.test.minimal".to_string(),
                title: "Minimal Playbook".to_string(),
                tags: vec!["test".to_string()],
                body,
            })
            .unwrap();

        let issues = procedural.validate_playbook_content(&playbook).unwrap();

        assert!(!issues.is_empty());
        assert!(
            issues
                .iter()
                .any(|i| i.section == "Preconditions" && i.severity == IssueSeverity::Warning)
        );
        assert!(
            issues
                .iter()
                .any(|i| i.section == "Rollback" && i.severity == IssueSeverity::Info)
        );
    }

    #[test]
    fn test_playbook_validation_empty_id() {
        let (_temp, procedural) = create_test_procedural_memory();

        let result = procedural.create_playbook(NewPlaybook {
            id: "".to_string(),
            title: "Test".to_string(),
            tags: vec!["test".to_string()],
            body: valid_playbook_body(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_playbook_validation_missing_sections() {
        let (_temp, procedural) = create_test_procedural_memory();

        let result = procedural.create_playbook(NewPlaybook {
            id: "playbook.test.invalid".to_string(),
            title: "Invalid Playbook".to_string(),
            tags: vec!["test".to_string()],
            body: "Missing required sections".to_string(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_section() {
        let body = r#"## First Section

Content of first section

## Second Section

Content of second section

## Third Section

Content of third section"#;

        let first = ProceduralMemory::extract_section(body, "## First Section");
        let second = ProceduralMemory::extract_section(body, "## Second Section");
        let third = ProceduralMemory::extract_section(body, "## Third Section");

        assert!(first.contains("Content of first section"));
        assert!(second.contains("Content of second section"));
        assert!(third.contains("Content of third section"));

        assert!(!first.contains("Second Section"));
        assert!(!second.contains("Third Section"));
    }
}
