//! Materialized Markdown views for agent sessions
//!
//! Views are deterministically regenerated from events.jsonl and
//! provide human-readable representations of session state.

use crate::error::{self, Result};
use crate::layout::{AgentDir, ViewFile};
use crate::session::{Event, LoggedEvent, Seq, Session};

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Kind of markdown view that can be materialized
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewKind {
    /// MEMORY.md - Project-level memory, always loaded
    Memory,
    /// PLAN.md - Current plan + checkpoints
    Plan,
    /// DECISIONS.md - Architecture decision records (ADR-lite)
    Decisions,
}

impl ViewKind {
    /// Get the filename for this view
    pub fn filename(&self) -> &'static str {
        match self {
            ViewKind::Memory => "MEMORY.md",
            ViewKind::Plan => "PLAN.md",
            ViewKind::Decisions => "DECISIONS.md",
        }
    }

    /// Get the display name for this view
    pub fn display_name(&self) -> &'static str {
        match self {
            ViewKind::Memory => "Memory",
            ViewKind::Plan => "Plan",
            ViewKind::Decisions => "Decisions",
        }
    }
}

/// All materialized views with their content and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedViews {
    /// MEMORY.md content
    pub memory: String,
    /// PLAN.md content
    pub plan: String,
    /// DECISIONS.md content
    pub decisions: String,
    /// Last sequence number used for materialization
    pub last_seq: u64,
}

/// View materializer that regenerates markdown views from events
#[derive(Debug, Clone)]
pub struct ViewMaterializer<'a> {
    /// Session to materialize views from
    session: &'a Session,
}

impl<'a> ViewMaterializer<'a> {
    /// Create a new view materializer for the given session
    pub fn new(session: &'a Session) -> Self {
        Self { session }
    }

    /// Regenerate all views from events.jsonl
    pub fn materialize_all(&self) -> Result<MaterializedViews> {
        let events = self.session.read_events()?;
        let last_seq = events.iter().map(|e| e.seq).max().unwrap_or(0);

        Ok(MaterializedViews {
            memory: self.materialize(ViewKind::Memory)?,
            plan: self.materialize(ViewKind::Plan)?,
            decisions: self.materialize(ViewKind::Decisions)?,
            last_seq,
        })
    }

    /// Regenerate a specific view
    pub fn materialize(&self, view: ViewKind) -> Result<String> {
        let events = self.session.read_events()?;

        let content = match view {
            ViewKind::Memory => self.materialize_memory(&events),
            ViewKind::Plan => self.materialize_plan(&events),
            ViewKind::Decisions => self.materialize_decisions(&events),
        };

        Ok(content)
    }

    /// Apply user edit to a view and log as event
    ///
    /// This method:
    /// 1. Logs a ViewEdit event to the session
    /// 2. Saves the new content to the view file on disk
    /// 3. Returns the sequence number of the logged event
    pub fn apply_edit(session: &mut Session, view: ViewKind, new_content: &str, seq_refs: Vec<Seq>) -> Result<Seq> {
        let view_name = view.filename();
        let seq = session.append_view_edit(view_name, "manual", new_content, seq_refs)?;
        Self::save_to_disk(session, view, new_content)?;
        Ok(seq)
    }

    /// Save a view's content to disk
    ///
    /// Creates the views directory if it doesn't exist and writes the view content.
    pub fn save_to_disk(session: &Session, view: ViewKind, content: &str) -> Result<()> {
        let session_dir = session.session_dir();
        let agent_root = session_dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| session_dir.parent().unwrap_or_else(|| Path::new(".")));

        let agent_dir = AgentDir::new(agent_root);
        let views_dir = agent_dir.views_dir();

        fs::create_dir_all(&views_dir)
            .map_err(|e| error::Error::Other(format!("Failed to create views directory: {}", e)))?;

        let view_file = match view {
            ViewKind::Memory => agent_dir.view_file(ViewFile::Memory),
            ViewKind::Plan => agent_dir.view_file(ViewFile::Plan),
            ViewKind::Decisions => agent_dir.view_file(ViewFile::Decisions),
        };

        fs::write(&view_file, content)
            .map_err(|e| error::Error::Other(format!("Failed to write view file {}: {}", view_file.display(), e)))?;

        Ok(())
    }

    /// Load a view's content from disk if it exists
    pub fn load_from_disk(session: &Session, view: ViewKind) -> Result<Option<String>> {
        let session_dir = session.session_dir();
        let agent_root = session_dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| session_dir.parent().unwrap_or_else(|| Path::new(".")));

        let agent_dir = AgentDir::new(agent_root);

        let view_file = match view {
            ViewKind::Memory => agent_dir.view_file(ViewFile::Memory),
            ViewKind::Plan => agent_dir.view_file(ViewFile::Plan),
            ViewKind::Decisions => agent_dir.view_file(ViewFile::Decisions),
        };

        if !view_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&view_file)
            .map_err(|e| error::Error::Other(format!("Failed to read view file {}: {}", view_file.display(), e)))?;

        Ok(Some(content))
    }

    /// Materialize and save all views to disk
    ///
    /// This is a convenience method that materializes all views and persists
    /// them to disk in one operation.
    pub fn materialize_and_save_all(&self) -> Result<MaterializedViews> {
        let views = self.materialize_all()?;

        Self::save_to_disk(self.session, ViewKind::Memory, &views.memory)?;
        Self::save_to_disk(self.session, ViewKind::Plan, &views.plan)?;
        Self::save_to_disk(self.session, ViewKind::Decisions, &views.decisions)?;

        Ok(views)
    }

    /// Materialize MEMORY.md from events
    fn materialize_memory(&self, events: &[LoggedEvent]) -> String {
        let mut commands = Vec::new();
        let mut facts = Vec::new();
        let mut architecture = Vec::new();
        let mut gotchas = Vec::new();

        for event in events {
            match &event.event {
                Event::ModelMessage { content, .. } => {
                    if let Some(cmd_section) = extract_section(content, "Commands") {
                        for line in cmd_section.lines() {
                            if let Some(cmd) = parse_command(line)
                                && !commands.contains(&cmd)
                            {
                                commands.push(cmd);
                            }
                        }
                    }
                    if let Some(arch_section) = extract_section(content, "Architecture") {
                        for line in arch_section.lines() {
                            let line = line.trim();
                            if !line.is_empty() && !line.starts_with('#') && !architecture.contains(&line.to_string()) {
                                architecture.push(line.to_string());
                            }
                        }
                    }
                    if let Some(gotcha_section) = extract_section(content, "Gotchas") {
                        for line in gotcha_section.lines() {
                            let line = line.trim();
                            if !line.is_empty() && !line.starts_with('#') && !gotchas.contains(&line.to_string()) {
                                gotchas.push(line.to_string());
                            }
                        }
                    }
                }
                Event::MemoryUpdate { kind, path, operation, .. } if kind == "core" => {
                    if operation == "create" || operation == "update" {
                        facts.push(format!("Memory file: {}", path));
                    }
                }
                _ => {}
            }
        }

        let mut output = String::from("# Project Memory\n\n");

        output.push_str("<!-- Auto-materialized from events.jsonl -->\n");
        output.push_str("<!-- Last updated: ");
        output.push_str(&chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        output.push_str(" -->\n\n");

        if !commands.is_empty() {
            output.push_str("## Commands\n\n");
            for cmd in &commands {
                output.push_str("- ");
                output.push_str(cmd);
                output.push('\n');
            }
            output.push('\n');
        }

        if !architecture.is_empty() {
            output.push_str("## Architecture\n\n");
            for arch in &architecture {
                output.push_str("- ");
                output.push_str(arch);
                output.push('\n');
            }
            output.push('\n');
        }

        if !gotchas.is_empty() {
            output.push_str("## Gotchas\n\n");
            for gotcha in &gotchas {
                output.push_str("- ");
                output.push_str(gotcha);
                output.push('\n');
            }
            output.push('\n');
        }

        if !facts.is_empty() {
            output.push_str("## Facts\n\n");
            for fact in &facts {
                output.push_str("- ");
                output.push_str(fact);
                output.push('\n');
            }
            output.push('\n');
        }

        for event in events.iter().rev() {
            if let Event::ViewEdit { view, content, .. } = &event.event
                && view == "MEMORY.md"
            {
                return content.clone();
            }
        }

        output
    }

    /// Materialize PLAN.md from events
    fn materialize_plan(&self, events: &[LoggedEvent]) -> String {
        let mut tasks: Vec<(String, bool, Option<String>)> = Vec::new();
        let mut checkpoints: Vec<(String, String, u64)> = Vec::new();
        let mut current_checkpoint: Option<(String, String, u64)> = None;

        for event in events {
            match &event.event {
                Event::PlanUpdate { action, item, reason } => {
                    let completed = action == "complete" || action == "done";
                    if action == "remove" {
                        tasks.retain(|(t, _, _)| t != item);
                    } else {
                        tasks.push((item.clone(), completed, reason.clone()));
                    }
                }
                Event::Checkpoint { label, description, .. } => {
                    if let Some((l, d, seq)) = current_checkpoint.take() {
                        checkpoints.push((l, d, seq));
                    }
                    current_checkpoint = Some((label.clone(), description.clone(), event.seq));
                }
                _ => {}
            }
        }

        if let Some(cp) = current_checkpoint {
            checkpoints.push(cp);
        }

        let mut output = String::from("# Current Plan\n\n");

        output.push_str("<!-- Session: ");
        output.push_str(self.session.id.as_str());
        output.push_str(" -->\n");

        if let Some((label, _, seq)) = checkpoints.last() {
            output.push_str("<!-- Checkpoint: \"");
            output.push_str(label);
            output.push_str("\" @ seq ");
            output.push_str(&seq.to_string());
            output.push_str(" -->\n\n");
        } else {
            output.push_str("<!-- No checkpoint set -->\n\n");
        }

        output.push_str("## Tasks\n\n");
        if tasks.is_empty() {
            output.push_str("No tasks defined yet.\n");
        } else {
            for (task, completed, reason) in &tasks {
                let checkbox = if *completed { "[x]" } else { "[ ]>" };
                output.push_str("- ");
                output.push_str(checkbox);
                output.push(' ');
                output.push_str(task);
                if let Some(reason_text) = reason {
                    output.push_str(" (");
                    output.push_str(reason_text);
                    output.push(')');
                }
                output.push('\n');
            }
        }
        output.push('\n');

        if !checkpoints.is_empty() {
            output.push_str("## Checkpoints\n\n");
            output.push_str("| Label | Seq | Description |\n");
            output.push_str("| --- | --- | --- |\n");
            for (label, description, seq) in &checkpoints {
                output.push_str("| ");
                output.push_str(label);
                output.push_str(" | ");
                output.push_str(&seq.to_string());
                output.push_str(" | ");
                output.push_str(description);
                output.push_str(" |\n");
            }
        }

        for event in events.iter().rev() {
            if let Event::ViewEdit { view, content, .. } = &event.event
                && view == "PLAN.md"
            {
                return content.clone();
            }
        }

        output
    }

    /// Materialize DECISIONS.md from events
    fn materialize_decisions(&self, events: &[LoggedEvent]) -> String {
        let mut decisions: Vec<(String, String, String, Vec<u64>)> = Vec::new();

        for event in events {
            match &event.event {
                Event::MemoryUpdate { kind, path, operation, .. } if kind == "semantic" || kind == "decisions" => {
                    let decision_id = path.split('/').next_back().unwrap_or("unknown");
                    if operation == "create" || operation == "update" {
                        let (title, rationale) = if let Ok(content) = std::fs::read_to_string(path) {
                            extract_decision_content(&content)
                        } else {
                            let title = decision_id.replace('-', " ");
                            let rationale = format!("Recorded in event {}", event.seq);
                            (title, rationale)
                        };
                        decisions.push((decision_id.to_string(), title, rationale, vec![event.seq]));
                    }
                }
                Event::ModelMessage { content, .. } => {
                    if content.contains("Decision:") || content.contains("decided to") {
                        let title = "Model decision".to_string();
                        let rationale = content.chars().take(200).collect::<String>();
                        decisions.push((format!("model-{}", event.seq), title, rationale, vec![event.seq]));
                    }
                }
                _ => {}
            }
        }

        let mut output = String::from("# Decisions Log\n\n");

        output.push_str("<!-- Auto-generated from events -->\n\n");

        if decisions.is_empty() {
            output.push_str("No decisions recorded yet.\n");
        } else {
            for (id, title, rationale, refs) in &decisions {
                output.push_str("## ");
                output.push_str(id);
                output.push_str(": ");
                output.push_str(title);
                output.push_str("\n\n");
                output.push_str("**Rationale:** ");
                output.push_str(rationale);
                output.push_str("\n\n");
                output.push_str("**References:** events ");
                for (i, seq) in refs.iter().enumerate() {
                    if i > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(&seq.to_string());
                }
                output.push_str("\n\n");
                output.push_str("---\n\n");
            }
        }

        for event in events.iter().rev() {
            if let Event::ViewEdit { view, content, .. } = &event.event
                && view == "DECISIONS.md"
            {
                return content.clone();
            }
        }

        output
    }
}

/// Extract title and rationale from decision markdown content
///
/// Looks for the first heading as the title and the first paragraph as the rationale.
/// Falls back to sensible defaults if the structure is not as expected.
fn extract_decision_content(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();

    let title = lines
        .iter()
        .find(|line| line.trim().starts_with('#'))
        .map(|line| line.trim().trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| "Untitled Decision".to_string());

    if title == "Untitled Decision" {
        return (title, "No rationale provided".to_string());
    }

    let rationale = lines
        .iter()
        .skip_while(|line| line.trim().is_empty() || line.trim().starts_with('#'))
        .take_while(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(500)
        .collect::<String>();

    let rationale = if rationale.is_empty() { "No rationale provided".to_string() } else { rationale };

    (title, rationale)
}

/// Extract a section by name from markdown content
fn extract_section(content: &str, section_name: &str) -> Option<String> {
    let section_start = format!("## {}", section_name);
    let content_lower = content.to_lowercase();
    let search_lower = section_start.to_lowercase();

    let start_idx = content_lower.find(&search_lower)?;
    let start_idx = start_idx + section_start.len();

    let remaining = &content[start_idx..];
    let end_idx = remaining.find("\n##").unwrap_or(remaining.len());

    Some(remaining[..end_idx].trim().to_string())
}

/// Parse a command from a line like "- `cargo test` - runs tests"
fn parse_command(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || !line.starts_with('-') {
        return None;
    }

    let start = line.find('`')? + 1;
    let end = line[start..].find('`')?;
    Some(line[start..start + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::AgentDir;
    use tempfile::TempDir;

    fn create_test_session() -> (TempDir, Session) {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session = Session::new(agent_dir).unwrap();
        (temp, session)
    }

    #[test]
    fn test_view_kind_filenames() {
        assert_eq!(ViewKind::Memory.filename(), "MEMORY.md");
        assert_eq!(ViewKind::Plan.filename(), "PLAN.md");
        assert_eq!(ViewKind::Decisions.filename(), "DECISIONS.md");
    }

    #[test]
    fn test_view_kind_display_names() {
        assert_eq!(ViewKind::Memory.display_name(), "Memory");
        assert_eq!(ViewKind::Plan.display_name(), "Plan");
        assert_eq!(ViewKind::Decisions.display_name(), "Decisions");
    }

    #[test]
    fn test_materialize_empty_session() {
        let (temp, session) = create_test_session();
        let materializer = ViewMaterializer::new(&session);

        let views = materializer.materialize_all().unwrap();

        assert!(!views.memory.is_empty());
        assert!(!views.plan.is_empty());
        assert!(!views.decisions.is_empty());
        assert_eq!(views.last_seq, 0);

        assert!(views.memory.contains("# Project Memory"));
        assert!(views.plan.contains("# Current Plan"));
        assert!(views.decisions.contains("# Decisions Log"));

        drop(temp);
    }

    #[test]
    fn test_materialize_plan_with_tasks() {
        let (temp, mut session) = create_test_session();
        session.append_plan_update("add", "Task 1", None).unwrap();
        session
            .append_plan_update("add", "Task 2", Some("User request".to_string()))
            .unwrap();
        session.append_plan_update("complete", "Task 1", None).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let plan = materializer.materialize(ViewKind::Plan).unwrap();
        assert!(plan.contains("[x] Task 1"));
        assert!(plan.contains("[ ]> Task 2"));
        assert!(plan.contains("User request"));

        drop(temp);
    }

    #[test]
    fn test_materialize_plan_with_checkpoint() {
        let (temp, mut session) = create_test_session();
        session.append_checkpoint("Initial", "Initial plan", None).unwrap();
        session.append_plan_update("add", "Task 1", None).unwrap();
        session.append_checkpoint("Progress", "Tasks added", None).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let plan = materializer.materialize(ViewKind::Plan).unwrap();

        assert!(plan.contains("Checkpoint: \"Progress\" @ seq"));

        drop(temp);
    }

    #[test]
    fn test_materialize_memory_from_model_messages() {
        let (temp, mut session) = create_test_session();

        let content = r#"
## Commands
- `cargo test` - run tests
- `cargo build` - build project

## Architecture
- TUI built with Ratatui
- Provider abstraction in crates/providers/

## Gotchas
- File edits require read-before-write validation
"#;
        session.append_model_message(content, None).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let memory = materializer.materialize(ViewKind::Memory).unwrap();

        assert!(memory.contains("cargo test"));
        assert!(memory.contains("cargo build"));
        assert!(memory.contains("TUI built with Ratatui"));
        assert!(memory.contains("File edits require read-before-write"));

        drop(temp);
    }

    #[test]
    fn test_materialize_decisions_from_memory_updates() {
        let (temp, mut session) = create_test_session();

        session
            .append_memory_update(
                "semantic",
                "/repo/memory/decisions/001-use-imara-diff.md",
                "create",
                "hash1",
            )
            .unwrap();

        let materializer = ViewMaterializer::new(&session);
        let decisions = materializer.materialize(ViewKind::Decisions).unwrap();

        assert!(decisions.contains("001-use-imara-diff"));
        assert!(decisions.contains("##"));
        assert!(decisions.contains("**Rationale:**"));

        drop(temp);
    }

    #[test]
    fn test_extract_decision_content() {
        let content = r#"
# Use Imara Diff

We decided to use the imara-diff library for diff generation because
it provides better performance and more accurate results than the
previous implementation.

## Implementation

The library is integrated into the patch module.
"#;

        let (title, rationale) = extract_decision_content(content);

        assert_eq!(title, "Use Imara Diff");
        assert!(rationale.contains("imara-diff library"));
        assert!(rationale.contains("better performance"));
    }

    #[test]
    fn test_extract_decision_content_fallback() {
        let content = "Just some text without proper heading structure";

        let (title, rationale) = extract_decision_content(content);

        assert_eq!(title, "Untitled Decision");
        assert_eq!(rationale, "No rationale provided");
    }

    #[test]
    fn test_extract_section() {
        let content = r#"
## Commands
- `cargo test`
- `cargo build`

## Architecture
Some text here
"#;

        let commands = extract_section(content, "Commands").unwrap();
        assert!(commands.contains("cargo test"));
        assert!(commands.contains("cargo build"));
        assert!(!commands.contains("Architecture"));

        let arch = extract_section(content, "Architecture").unwrap();
        assert!(arch.contains("Some text here"));
    }

    #[test]
    fn test_parse_command() {
        let line = "- `cargo test` - run tests";
        let cmd = parse_command(line).unwrap();
        assert_eq!(cmd, "cargo test");

        let line2 = "- `git commit -m 'message'`";
        let cmd2 = parse_command(line2).unwrap();
        assert_eq!(cmd2, "git commit -m 'message'");

        assert!(parse_command("not a command").is_none());
        assert!(parse_command("").is_none());
    }

    #[test]
    fn test_task_removal() {
        let (temp, mut session) = create_test_session();
        session.append_plan_update("add", "Task 1", None).unwrap();
        session.append_plan_update("add", "Task 2", None).unwrap();
        session.append_plan_update("remove", "Task 1", None).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let plan = materializer.materialize(ViewKind::Plan).unwrap();

        assert!(!plan.contains("Task 1"));
        assert!(plan.contains("Task 2"));

        drop(temp);
    }

    #[test]
    fn test_view_serialization() {
        let view = ViewKind::Memory;

        let json = serde_json::to_string(&view).unwrap();
        let deserialized: ViewKind = serde_json::from_str(&json).unwrap();

        assert_eq!(view, deserialized);
    }

    #[test]
    fn test_materialized_views_serialization() {
        let views = MaterializedViews {
            memory: "# Memory".to_string(),
            plan: "# Plan".to_string(),
            decisions: "# Decisions".to_string(),
            last_seq: 42,
        };

        let json = serde_json::to_string(&views).unwrap();
        let deserialized: MaterializedViews = serde_json::from_str(&json).unwrap();

        assert_eq!(views.memory, deserialized.memory);
        assert_eq!(views.plan, deserialized.plan);
        assert_eq!(views.decisions, deserialized.decisions);
        assert_eq!(views.last_seq, deserialized.last_seq);
    }

    #[test]
    fn test_apply_edit() {
        let (temp, mut session) = create_test_session();
        session.append_plan_update("add", "Task 1", None).unwrap();
        session.append_plan_update("add", "Task 2", None).unwrap();

        let new_plan = "# Manual Plan\n\n- [x] Custom task 1\n- [ ] Custom task 2\n";
        let seq = ViewMaterializer::apply_edit(&mut session, ViewKind::Plan, new_plan, vec![0, 1]).unwrap();
        assert_eq!(seq, 2);

        let events = session.read_events().unwrap();
        assert!(matches!(events[2].event, Event::ViewEdit { .. }));

        let materializer = ViewMaterializer::new(&session);
        let plan = materializer.materialize(ViewKind::Plan).unwrap();
        assert!(plan.contains("Manual Plan"));
        assert!(plan.contains("Custom task 1"));
        assert!(plan.contains("Custom task 2"));

        drop(temp);
    }

    #[test]
    fn test_view_edit_overrides_generated_content() {
        let (temp, mut session) = create_test_session();
        let content = "## Commands\n- `cargo test`\n";
        session.append_model_message(content, None).unwrap();

        let custom_memory = "# Custom Memory\n\nThis is custom content.\n";
        ViewMaterializer::apply_edit(&mut session, ViewKind::Memory, custom_memory, vec![0]).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let memory = materializer.materialize(ViewKind::Memory).unwrap();

        assert!(!memory.contains("cargo test"));
        assert!(memory.contains("Custom Memory"));
        assert!(memory.contains("custom content"));

        drop(temp);
    }

    #[test]
    fn test_latest_view_edit_wins() {
        let (temp, mut session) = create_test_session();
        let edit1 = "# First Edit\n";
        ViewMaterializer::apply_edit(&mut session, ViewKind::Decisions, edit1, vec![]).unwrap();

        let edit2 = "# Second Edit\n";
        ViewMaterializer::apply_edit(&mut session, ViewKind::Decisions, edit2, vec![]).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let decisions = materializer.materialize(ViewKind::Decisions).unwrap();

        assert!(!decisions.contains("First Edit"));
        assert!(decisions.contains("Second Edit"));

        drop(temp);
    }

    #[test]
    fn test_load_from_disk() {
        let (temp, session) = create_test_session();
        let loaded = ViewMaterializer::load_from_disk(&session, ViewKind::Plan).unwrap();
        assert!(loaded.is_none());

        let content = "# Test Plan\n\n- Task 1\n";
        ViewMaterializer::save_to_disk(&session, ViewKind::Plan, content).unwrap();

        let loaded = ViewMaterializer::load_from_disk(&session, ViewKind::Plan).unwrap();
        assert!(loaded.is_some());
        assert!(loaded.unwrap().contains("Task 1"));

        drop(temp);
    }

    #[test]
    fn test_materialize_and_save_all() {
        let (temp, mut session) = create_test_session();
        session.append_plan_update("add", "Task 1", None).unwrap();

        let content = "## Commands\n- `cargo build`\n";
        session.append_model_message(content, None).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let views = materializer.materialize_and_save_all().unwrap();

        assert!(views.memory.contains("# Project Memory"));
        assert!(views.plan.contains("# Current Plan"));
        assert!(views.decisions.contains("# Decisions Log"));

        let session_dir = session.session_dir();
        let agent_root = session_dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| session_dir.parent().unwrap_or_else(|| Path::new(".")));

        let agent_dir = AgentDir::new(agent_root);
        assert!(agent_dir.view_file(ViewFile::Memory).exists());
        assert!(agent_dir.view_file(ViewFile::Plan).exists());
        assert!(agent_dir.view_file(ViewFile::Decisions).exists());

        let memory_content = fs::read_to_string(agent_dir.view_file(ViewFile::Memory)).unwrap();
        assert!(memory_content.contains("# Project Memory"));

        drop(temp);
    }

    #[test]
    fn test_view_edit_with_multiple_seq_refs() {
        let (temp, mut session) = create_test_session();
        session.append_plan_update("add", "Task 1", None).unwrap();
        session.append_checkpoint("Initial", "Start", None).unwrap();
        session.append_plan_update("add", "Task 2", None).unwrap();

        let new_plan = "# Updated Plan\n\n- [x] Task 1\n- [ ] Task 2\n";
        ViewMaterializer::apply_edit(&mut session, ViewKind::Plan, new_plan, vec![0, 1, 2]).unwrap();

        let events = session.read_events().unwrap();
        if let Event::ViewEdit { seq_refs, .. } = &events[3].event {
            assert_eq!(seq_refs, &vec![0, 1, 2]);
        } else {
            panic!("Expected ViewEdit event");
        }

        drop(temp);
    }

    #[test]
    fn test_view_edit_does_not_affect_other_views() {
        let (temp, mut session) = create_test_session();
        session.append_plan_update("add", "Task 1", None).unwrap();

        let content = "## Commands\n- `cargo test`\n";
        session.append_model_message(content, None).unwrap();

        let new_plan = "# Edited Plan\n";
        ViewMaterializer::apply_edit(&mut session, ViewKind::Plan, new_plan, vec![]).unwrap();

        let materializer = ViewMaterializer::new(&session);
        let memory = materializer.materialize(ViewKind::Memory).unwrap();
        assert!(memory.contains("cargo test"));

        let plan = materializer.materialize(ViewKind::Plan).unwrap();
        assert!(plan.contains("Edited Plan"));

        drop(temp);
    }
}
