//! Session recap generation
//!
//! Creates human-readable summaries of session activity.

use crate::Error;
use crate::error::Result;
use crate::memory::gardener::config::RecapConfig;
use crate::memory::gardener::extraction::ExtractedEntities;
use crate::session::LoggedEvent;

use chrono::{Datelike, Utc};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;

/// Statistics about a session recap
#[derive(Debug, Clone)]
pub struct RecapStats {
    pub event_count: usize,
    pub duration_minutes: u64,
    pub files_modified: usize,
    pub commands_run: usize,
    pub entities_extracted: usize,
}

/// Result of recap generation
#[derive(Debug, Clone)]
pub struct RecapResult {
    /// Path to the generated recap
    pub path: PathBuf,
    /// Document ID
    pub doc_id: String,
    /// Summary statistics
    pub stats: RecapStats,
}

/// Template for session recaps
#[derive(Debug, Clone)]
pub struct RecapTemplate {
    /// Include file changes in recap
    pub include_file_changes: bool,
    /// Maximum files to list
    pub max_files_listed: usize,
}

impl From<RecapConfig> for RecapTemplate {
    fn from(config: RecapConfig) -> Self {
        Self { include_file_changes: config.include_file_changes, max_files_listed: config.max_files_listed }
    }
}

/// Generates session recap documents
#[derive(Debug, Clone)]
pub struct RecapGenerator {
    // TODO: Use template to customize recap format and content
    #[allow(dead_code)]
    template: RecapTemplate,
}

impl RecapGenerator {
    /// Create a new recap generator
    pub fn new(config: RecapConfig) -> Self {
        Self { template: config.into() }
    }

    /// Generate a session recap from events and extractions
    pub fn generate(
        &self, session_id: &str, events: &[LoggedEvent], entities: &ExtractedEntities, patches: &[PathBuf],
    ) -> Result<RecapResult> {
        let now = Utc::now();
        let month_dir = format!("{:04}-{:02}", now.year(), now.month());

        let content = self.render_recap(session_id, events, entities, patches)?;

        let filename = format!("{}.md", session_id.replace(':', "-"));
        let path = PathBuf::from("/tmp").join("recap").join(&month_dir);
        fs::create_dir_all(&path)?;

        let full_path = path.join(&filename);

        let file = File::create(&full_path).map_err(Error::Io)?;
        let mut writer = BufWriter::new(file);
        std::io::Write::write_all(&mut writer, content.as_bytes()).map_err(Error::Io)?;

        let stats = self.calculate_stats(events, entities);

        Ok(RecapResult { path: full_path, doc_id: format!("recap.{}", session_id), stats })
    }

    /// Render the recap markdown
    fn render_recap(
        &self, session_id: &str, events: &[LoggedEvent], entities: &ExtractedEntities, _patches: &[PathBuf],
    ) -> Result<String> {
        let mut md = String::new();

        md.push_str(&format!("# Session Recap: {}\n\n", session_id));

        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        md.push_str(&format!("**Generated:** {}\n\n", now));

        md.push_str("## Summary\n\n");
        md.push_str(&format!("- **Events:** {}\n", events.len()));
        md.push_str(&format!("- **Commands:** {}\n", entities.commands.len()));
        md.push_str(&format!("- **Gotchas:** {}\n", entities.gotchas.len()));
        md.push_str(&format!("- **Decisions:** {}\n", entities.decisions.len()));
        md.push_str(&format!("- **Workflows:** {}\n\n", entities.workflows.len()));

        if !entities.commands.is_empty() {
            md.push_str("## Commands Executed\n\n");
            for cmd in &entities.commands {
                let status = match cmd.outcome {
                    crate::memory::gardener::entities::CommandOutcome::Success => "✓",
                    crate::memory::gardener::entities::CommandOutcome::Failure => "✗",
                    crate::memory::gardener::entities::CommandOutcome::Partial => "~",
                };
                md.push_str(&format!("- {} `{}`\n", status, cmd.command));
            }
            md.push('\n');
        }

        if !entities.gotchas.is_empty() {
            md.push_str("## Issues and Resolutions\n\n");
            for gotcha in &entities.gotchas {
                md.push_str(&format!("### {}\n\n", gotcha.category));
                md.push_str(&format!("**Issue:** {}\n\n", gotcha.issue));
                md.push_str(&format!("**Resolution:** {}\n\n", gotcha.resolution));
            }
        }

        if !entities.decisions.is_empty() {
            md.push_str("## Decisions\n\n");
            for decision in &entities.decisions {
                md.push_str(&format!("### {}\n\n", decision.decision));
                md.push_str(&format!("**Context:** {}\n\n", decision.context));
                md.push_str(&format!("**Rationale:** {}\n\n", decision.rationale));
            }
        }

        if !entities.workflows.is_empty() {
            md.push_str("## Workflows Identified\n\n");
            for workflow in &entities.workflows {
                md.push_str(&format!("### {}\n\n", workflow.title));
                for (i, step) in workflow.steps.iter().enumerate() {
                    md.push_str(&format!("{}. {}\n", i + 1, step.description));
                    if let Some(action) = &step.action {
                        md.push_str(&format!("   - `{}`\n", action));
                    }
                }
                md.push('\n');
            }
        }

        Ok(md)
    }

    /// Calculate recap statistics
    fn calculate_stats(&self, events: &[LoggedEvent], entities: &ExtractedEntities) -> RecapStats {
        let event_count = events.len();

        let duration_minutes = if events.len() >= 2 {
            if let (Some(first), Some(last)) = (
                events.first().and_then(|e| parse_timestamp(&e.timestamp)),
                events.last().and_then(|e| parse_timestamp(&e.timestamp)),
            ) {
                let duration = last.signed_duration_since(first);
                duration.num_minutes().max(0) as u64
            } else {
                0
            }
        } else {
            0
        };

        let files_modified = events
            .iter()
            .filter(|e| matches!(e.event, crate::session::Event::Patch { .. }))
            .count();

        let commands_run = entities.commands.len();
        let entities_extracted =
            entities.commands.len() + entities.gotchas.len() + entities.decisions.len() + entities.workflows.len();

        RecapStats { event_count, duration_minutes, files_modified, commands_run, entities_extracted }
    }
}

/// Parse a timestamp string into a DateTime
fn parse_timestamp(s: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Event, LoggedEvent, Seq};

    fn create_test_event(seq: Seq, session_id: &str, event: Event) -> LoggedEvent {
        LoggedEvent { seq, session_id: session_id.to_string(), timestamp: "2026-01-22T10:00:00Z".to_string(), event }
    }

    #[test]
    fn test_recap_generator_new() {
        let config = RecapConfig::default();
        let generator = RecapGenerator::new(config);
        assert_eq!(generator.template.max_files_listed, 20);
    }

    #[test]
    fn test_calculate_stats() {
        let config = RecapConfig::default();
        let generator = RecapGenerator::new(config);

        let events = vec![
            create_test_event(0, "test", Event::UserMessage { content: "Hello".to_string() }),
            create_test_event(
                1,
                "test",
                Event::ModelMessage { content: "Hi".to_string(), tokens_used: None },
            ),
        ];

        let entities = ExtractedEntities { commands: vec![], gotchas: vec![], decisions: vec![], workflows: vec![] };

        let stats = generator.calculate_stats(&events, &entities);
        assert_eq!(stats.event_count, 2);
        assert_eq!(stats.entities_extracted, 0);
    }

    #[test]
    fn test_render_recap() {
        let config = RecapConfig::default();
        let generator = RecapGenerator::new(config);

        let events = vec![create_test_event(
            0,
            "test-session",
            Event::UserMessage { content: "Test message".to_string() },
        )];

        let entities = ExtractedEntities { commands: vec![], gotchas: vec![], decisions: vec![], workflows: vec![] };

        let recap = generator.render_recap("test-session", &events, &entities, &[]);
        assert!(recap.is_ok());
        let md = recap.unwrap();
        assert!(md.contains("# Session Recap"));
        assert!(md.contains("test-session"));
    }
}
