//! Consolidation pipeline for episodic to semantic promotion
//!
//! Transforms raw session history into durable knowledge artifacts.

use crate::error::{Error, Result};
use crate::memory::gardener::config::GardenerConfig;
use crate::memory::gardener::entities::{AdrStatus, AdrUpdate, CommandEntity};
use crate::memory::gardener::extraction::{EntityExtractor, ExtractedEntities};
use crate::memory::gardener::recap::{RecapGenerator, RecapResult};
use crate::memory::kinds::MemoryKind;
use crate::memory::paths::MemoryPaths;
use crate::memory::{CommandOutcome, GotchaCategory, MemoryManifest};
use crate::patch::MemoryPatchParams;
use crate::{LoggedEvent, SessionId};

use serde_json::json;
use std::fs::{self};
use std::path::{Path, PathBuf};

/// Represents an update to a FACT document
#[derive(Debug, Clone)]
pub enum FactUpdate {
    /// Create a new fact document
    Create {
        doc_id: String,
        title: String,
        tags: Vec<String>,
        content: String,
        provenance: Vec<String>,
    },
    /// Append to an existing fact document
    Append {
        doc_id: String,
        section: String,
        content: String,
        provenance: Vec<String>,
    },
    /// No change needed (already present)
    NoOp { doc_id: String, reason: String },
}

/// Result of a consolidation run
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    /// Facts extracted or updated
    pub facts: Vec<FactUpdate>,
    /// ADRs created or updated
    pub adrs: Vec<AdrUpdate>,
    /// Playbooks created or updated (placeholder for future)
    pub playbooks: Vec<String>,
    /// Session recap written
    pub recap: Option<RecapResult>,
    /// Memory patches generated (for user approval)
    pub patches: Vec<MemoryPatchParams>,
    /// Warnings encountered
    pub warnings: Vec<String>,
}

/// Orchestrates the episodic to semantic/procedural promotion
#[derive(Debug, Clone)]
pub struct ConsolidationJob {
    session_id: String,
    events_file: PathBuf,
    config: GardenerConfig,
}

impl ConsolidationJob {
    /// Create a new consolidation job for a completed session
    pub fn new(session_id: &str, events_file: &Path, config: GardenerConfig) -> Result<Self> {
        if !events_file.exists() {
            return Err(Error::Other(format!("Events file not found: {:?}", events_file)));
        }

        Ok(Self { session_id: session_id.to_string(), events_file: events_file.to_path_buf(), config })
    }

    /// Execute the consolidation pipeline
    ///
    /// Outputs are queued as memory patches for user approval.
    pub async fn run(&self, mem_paths: &MemoryPaths) -> Result<ConsolidationResult> {
        let events = self.load_events()?;
        let extractor = EntityExtractor::with_config(self.config.extraction.clone());
        let entities = extractor.extract(&events);
        let manifest = self.load_manifest(mem_paths)?;
        let facts = self.generate_fact_updates(&entities, &manifest);
        let adrs = self.generate_adr_updates(&entities, &manifest);

        let mut patches = Vec::new();
        self.generate_fact_patches(&facts, &mut patches, mem_paths)?;
        self.generate_adr_patches(&adrs, &mut patches, mem_paths)?;
        let recap = self.generate_recap(&events, &entities, mem_paths).await.ok();
        let warnings = self.collect_warnings(&facts, &adrs);

        Ok(ConsolidationResult { facts, adrs, playbooks: Vec::new(), recap, patches, warnings })
    }

    /// Load events from the session file
    fn load_events(&self) -> Result<Vec<LoggedEvent>> {
        use crate::layout::{AgentDir, SessionId};
        use crate::session::Session;

        let agent_dir = AgentDir::new(self.events_file.parent().unwrap().parent().unwrap());
        let session_id = SessionId::from_timestamp(&self.session_id)
            .map_err(|e| Error::Other(format!("Invalid session ID: {}", e)))?;

        let session = Session::load(agent_dir, session_id)?;
        session.read_events()
    }

    /// Load the memory manifest
    fn load_manifest(&self, paths: &MemoryPaths) -> Result<MemoryManifest> {
        if !paths.manifest_file().exists() {
            return Ok(MemoryManifest::default());
        }

        let content = fs::read_to_string(paths.manifest_file())
            .map_err(|e| Error::Other(format!("Failed to read manifest: {}", e)))?;
        let manifest: MemoryManifest =
            serde_json::from_str(&content).map_err(|e| Error::Parse(format!("Failed to parse manifest: {}", e)))?;
        Ok(manifest)
    }

    /// Generate fact updates from extracted entities
    fn generate_fact_updates(&self, entities: &ExtractedEntities, _manifest: &MemoryManifest) -> Vec<FactUpdate> {
        // TODO: Use manifest to match existing facts and avoid duplicates
        let mut updates = Vec::new();

        let mut build_commands: Vec<&CommandEntity> = Vec::new();
        let mut test_commands: Vec<&CommandEntity> = Vec::new();
        let mut other_commands: Vec<&CommandEntity> = Vec::new();

        for cmd in &entities.commands {
            if cmd.outcome == CommandOutcome::Success {
                if cmd.command.contains("cargo") || cmd.command.contains("build") || cmd.command.contains("compile") {
                    build_commands.push(cmd);
                } else if cmd.command.contains("test") {
                    test_commands.push(cmd);
                } else {
                    other_commands.push(cmd);
                }
            }
        }

        if !build_commands.is_empty() {
            let content = self.format_commands_as_markdown(&build_commands);
            updates.push(FactUpdate::Append {
                doc_id: "fact.commands.build".to_string(),
                section: "Build Commands".to_string(),
                content,
                provenance: build_commands.iter().flat_map(|c| c.event_ids.clone()).collect(),
            });
        }

        if !test_commands.is_empty() {
            let content = self.format_commands_as_markdown(&test_commands);
            updates.push(FactUpdate::Append {
                doc_id: "fact.commands.test".to_string(),
                section: "Test Commands".to_string(),
                content,
                provenance: test_commands.iter().flat_map(|c| c.event_ids.clone()).collect(),
            });
        }

        for gotcha in &entities.gotchas {
            let doc_id = match gotcha.category {
                GotchaCategory::Build => "fact.gotchas.build".to_string(),
                GotchaCategory::Test => "fact.gotchas.test".to_string(),
                GotchaCategory::Runtime => "fact.gotchas.runtime".to_string(),
                _ => "fact.gotchas.other".to_string(),
            };

            let content = format!(
                "- **Issue**: {}\n- **Resolution**: {}\n",
                gotcha.issue, gotcha.resolution
            );
            updates.push(FactUpdate::Append {
                doc_id,
                section: "Gotchas".to_string(),
                content,
                provenance: gotcha.event_ids.clone(),
            });
        }

        updates
    }

    /// Format commands as markdown
    fn format_commands_as_markdown(&self, commands: &[&CommandEntity]) -> String {
        let mut md = String::new();
        for cmd in commands {
            md.push_str(&format!(
                "- `{}{}`\n",
                cmd.command,
                if cmd.outcome == crate::memory::gardener::entities::CommandOutcome::Success { " ✓" } else { "" }
            ));
        }
        md
    }

    /// Generate ADR updates from extracted entities
    fn generate_adr_updates(&self, entities: &ExtractedEntities, manifest: &MemoryManifest) -> Vec<AdrUpdate> {
        let mut updates = Vec::new();

        for decision in &entities.decisions {
            let number = AdrUpdate::next_number(manifest) + updates.len() as u32;

            updates.push(AdrUpdate {
                number,
                title: decision.decision.clone(),
                status: AdrStatus::Accepted,
                context: decision.context.clone(),
                decision: decision.decision.clone(),
                consequences: decision.rationale.clone(),
                supersedes: None,
                event_ids: decision.event_ids.clone(),
            });
        }

        updates
    }

    /// Generate memory patches from fact updates
    fn generate_fact_patches(
        &self, facts: &[FactUpdate], patches: &mut Vec<MemoryPatchParams>, paths: &MemoryPaths,
    ) -> Result<()> {
        let session_id = SessionId::from_timestamp(&self.session_id)
            .map_err(|e| Error::Other(format!("Invalid session ID: {}", e)))?;

        for (idx, fact) in facts.iter().enumerate() {
            match fact {
                FactUpdate::Create { doc_id, title, tags, content, provenance } => {
                    let path = paths.facts.join(format!("{}.md", doc_id.replace('.', "_")));
                    patches.push(MemoryPatchParams {
                        path,
                        doc_id: doc_id.clone(),
                        kind: MemoryKind::Fact,
                        description: format!("Create fact: {}", title),
                        diff: self.create_fact_diff(doc_id, title, tags, content),
                        source_events: provenance.clone(),
                        session_id: session_id.clone(),
                        seq: idx as u64,
                    });
                }
                FactUpdate::Append { doc_id, section, content, provenance } => {
                    let path = paths.facts.join(format!("{}.md", doc_id.replace('.', "_")));
                    patches.push(MemoryPatchParams {
                        path,
                        doc_id: doc_id.clone(),
                        kind: MemoryKind::Fact,
                        description: format!("Append to fact: {}", doc_id),
                        diff: self.append_fact_diff(section, content),
                        source_events: provenance.clone(),
                        session_id: session_id.clone(),
                        seq: idx as u64,
                    });
                }
                FactUpdate::NoOp { .. } => {}
            }
        }

        Ok(())
    }

    /// Generate memory patches from ADR updates
    fn generate_adr_patches(
        &self, adrs: &[AdrUpdate], patches: &mut Vec<MemoryPatchParams>, paths: &MemoryPaths,
    ) -> Result<()> {
        let session_id = SessionId::from_timestamp(&self.session_id)
            .map_err(|e| Error::Other(format!("Invalid session ID: {}", e)))?;

        for (idx, adr) in adrs.iter().enumerate() {
            let filename = format!("ADR-{:04}.md", adr.number);
            let path = paths.decisions.join(filename);

            patches.push(MemoryPatchParams {
                path,
                doc_id: format!("adr.{:04}", adr.number),
                kind: MemoryKind::Adr,
                description: format!("Create ADR-{:04}: {}", adr.number, adr.title),
                diff: self.create_adr_diff(adr),
                source_events: adr.event_ids.clone(),
                session_id: session_id.clone(),
                seq: idx as u64,
            });
        }

        Ok(())
    }

    /// Create a diff for a new fact document
    fn create_fact_diff(&self, id: &str, title: &str, tags: &[String], content: &str) -> String {
        let frontmatter = json!({
            "id": id,
            "title": title,
            "kind": "fact",
            "tags": tags,
            "created_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        });

        format!("---\n{}\n---\n\n{}", frontmatter, content)
    }

    /// Create a diff for appending to a fact
    fn append_fact_diff(&self, section: &str, content: &str) -> String {
        format!("### {}\n\n{}", section, content)
    }

    /// Create a diff for a new ADR
    fn create_adr_diff(&self, adr: &AdrUpdate) -> String {
        let frontmatter = json!({
            "id": format!("adr.{:04}", adr.number),
            "title": adr.title,
            "kind": "adr",
            "tags": [],
            "created_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "verification": {
                "status": "verified"
            }
        });

        format!("---\n{}\n---\n\n{}", frontmatter, adr.to_markdown())
    }

    /// Generate a session recap
    async fn generate_recap(
        &self, events: &[crate::session::LoggedEvent], entities: &ExtractedEntities, _paths: &MemoryPaths,
    ) -> Result<RecapResult> {
        // TODO: Use paths to determine recap output location
        let generator = RecapGenerator::new(self.config.recap.clone());
        generator.generate(&self.session_id, events, entities, &[])
    }

    /// Collect warnings from the consolidation
    fn collect_warnings(&self, facts: &[FactUpdate], adrs: &[AdrUpdate]) -> Vec<String> {
        let mut warnings = Vec::new();

        let noop_count = facts.iter().filter(|f| matches!(f, FactUpdate::NoOp { .. })).count();
        if noop_count > 0 {
            warnings.push(format!("{} facts already existed and were skipped", noop_count));
        }

        if facts.is_empty() && adrs.is_empty() {
            warnings.push("No entities extracted from session".to_string());
        }

        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::gardener::config::ExtractionConfig;
    use crate::session::{Event, LoggedEvent, Seq};
    use serde_json::json;
    use std::fs::File;
    use tempfile::TempDir;

    fn create_test_event(seq: Seq, session_id: &str, event: Event) -> LoggedEvent {
        LoggedEvent { seq, session_id: session_id.to_string(), timestamp: "2026-01-22T10:00:00Z".to_string(), event }
    }

    #[test]
    fn test_consolidation_job_new() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let session_dir = paths.root.join(".agent/sessions/test-session");
        fs::create_dir_all(&session_dir).unwrap();
        let events_file = session_dir.join("events.jsonl");
        File::create(&events_file).unwrap();

        let config = GardenerConfig::default();
        let job = ConsolidationJob::new("test-session", &events_file, config);
        assert!(job.is_ok());
    }

    #[test]
    fn test_fact_update_create() {
        let update = FactUpdate::Create {
            doc_id: "fact.test.example".to_string(),
            title: "Test Fact".to_string(),
            tags: vec!["test".to_string()],
            content: "Test content".to_string(),
            provenance: vec!["evt_001".to_string()],
        };

        match update {
            FactUpdate::Create { doc_id, title, .. } => {
                assert_eq!(doc_id, "fact.test.example");
                assert_eq!(title, "Test Fact");
            }
            _ => panic!("Expected Create variant"),
        }
    }

    #[test]
    fn test_consolidation_golden_commands() {
        let events = vec![
            create_test_event(
                0,
                "test-session",
                Event::ToolCall { tool: "shell".to_string(), arguments: json!({"cmd": "cargo build"}) },
            ),
            create_test_event(
                1,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo build", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(
                2,
                "test-session",
                Event::ToolCall { tool: "shell".to_string(), arguments: json!({"cmd": "cargo test"}) },
            ),
            create_test_event(
                3,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo test", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
        ];

        let extractor = EntityExtractor::new();
        let entities = extractor.extract(&events);

        assert_eq!(entities.commands.len(), 2);
        assert_eq!(entities.commands[0].command, "cargo build");
        assert_eq!(entities.commands[1].command, "cargo test");
        assert_eq!(entities.commands[0].outcome, CommandOutcome::Success);
        assert_eq!(entities.commands[1].outcome, CommandOutcome::Success);
    }

    #[test]
    fn test_consolidation_golden_gotchas() {
        let events = vec![
            create_test_event(
                0,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo build", "exit_code": 1}),
                    success: false,
                    error: Some("error: feature not found".to_string()),
                },
            ),
            create_test_event(
                1,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo build --features foo", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
        ];

        let extractor = EntityExtractor::new();
        let entities = extractor.extract(&events);

        assert_eq!(entities.gotchas.len(), 1);
        assert!(entities.gotchas[0].issue.contains("cargo build"));
        assert!(entities.gotchas[0].resolution.contains("--features foo"));
        assert_eq!(entities.gotchas[0].category, GotchaCategory::Build);
    }

    #[test]
    fn test_consolidation_golden_adr() {
        let events = vec![create_test_event(
            0,
            "test-session",
            Event::ModelMessage {
                content: "I decided to use tokio-rusqlite for the database layer because we need async access."
                    .to_string(),
                tokens_used: None,
            },
        )];

        let extractor = EntityExtractor::new();
        let entities = extractor.extract(&events);

        assert_eq!(entities.decisions.len(), 1);
        assert!(
            entities.decisions[0].decision.to_lowercase().contains("tokio")
                || entities.decisions[0].decision.to_lowercase().contains("database")
        );
    }

    #[test]
    fn test_consolidation_generates_fact_updates() {
        let events = vec![
            create_test_event(
                0,
                "test-session",
                Event::ToolCall { tool: "shell".to_string(), arguments: json!({"cmd": "cargo build"}) },
            ),
            create_test_event(
                1,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo build", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
        ];

        let extractor = EntityExtractor::new();
        let entities = extractor.extract(&events);
        let manifest = MemoryManifest::default();

        let config = GardenerConfig::default();
        let job = ConsolidationJob {
            session_id: "test-session".to_string(),
            events_file: std::path::PathBuf::from("/tmp/test"),
            config,
        };

        let facts = job.generate_fact_updates(&entities, &manifest);

        assert_eq!(facts.len(), 1);
        match &facts[0] {
            FactUpdate::Append { doc_id, section, .. } => {
                assert!(doc_id.contains("build"));
                assert_eq!(section, "Build Commands");
            }
            _ => panic!("Expected Append for build commands"),
        }
    }

    #[test]
    fn test_consolidation_workflow_with_custom_config() {
        let events = vec![
            create_test_event(
                0,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo fmt", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(
                1,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo clippy", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(
                2,
                "test-session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo test", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(3, "test-session", Event::UserMessage { content: "Done".to_string() }),
        ];

        let config = ExtractionConfig { min_workflow_steps: 2, ..Default::default() };
        let extractor = EntityExtractor::with_config(config);
        let entities = extractor.extract(&events);

        assert_eq!(entities.commands.len(), 3);
        assert_eq!(entities.workflows.len(), 1);
        assert!(entities.workflows[0].title.contains("Rust") || entities.workflows[0].title.contains("step"));
    }
}
