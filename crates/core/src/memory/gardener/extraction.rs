//! Entity extraction from session events
//!
//! Extracts durable knowledge entities (commands, gotchas, decisions, workflows)
//! from raw session event logs.

use crate::memory::gardener::config::ExtractionConfig;
use crate::memory::gardener::entities::{
    CommandEntity, CommandOutcome, DecisionEntity, GotchaCategory, GotchaEntity, WorkflowEntity, WorkflowStep,
};
use crate::session::{Event, LoggedEvent};

use std::collections::HashMap;

/// Extracted entities from a session
#[derive(Debug, Clone)]
pub struct ExtractedEntities {
    pub commands: Vec<CommandEntity>,
    pub gotchas: Vec<GotchaEntity>,
    pub decisions: Vec<DecisionEntity>,
    pub workflows: Vec<WorkflowEntity>,
}

/// Extracts durable knowledge entities from session events
#[derive(Debug, Clone)]
pub struct EntityExtractor {
    config: ExtractionConfig,
}

impl EntityExtractor {
    /// Create a new entity extractor with default config
    pub fn new() -> Self {
        Self { config: ExtractionConfig::default() }
    }

    /// Create a new entity extractor with custom config
    pub fn with_config(config: ExtractionConfig) -> Self {
        Self { config }
    }

    /// Extract entities from session events
    pub fn extract(&self, events: &[LoggedEvent]) -> ExtractedEntities {
        let mut commands = Vec::new();
        let mut gotchas = Vec::new();
        let mut decisions = Vec::new();
        let mut pending_tool_calls: HashMap<String, (String, serde_json::Value)> = HashMap::new();
        let mut command_sequences: Vec<Vec<CommandEntity>> = Vec::new();
        let mut current_sequence: Vec<CommandEntity> = Vec::new();

        let mut last_failed_command: Option<CommandEntity> = None;

        for (idx, logged_event) in events.iter().enumerate() {
            match &logged_event.event {
                Event::ToolCall { tool, arguments } => {
                    pending_tool_calls.insert(format!("{}_{}", idx, tool), (tool.clone(), arguments.clone()));
                }

                Event::ToolResult { tool, result, success, error } => {
                    if tool == "shell"
                        && let Some(cmd_entity) = self.extract_shell_command(
                            result,
                            *success,
                            error,
                            &logged_event.session_id,
                            logged_event.seq,
                        )
                    {
                        if !*success {
                            last_failed_command = Some(cmd_entity.clone());
                        } else {
                            if let Some(failed_cmd) = &last_failed_command
                                && self.is_resolution_attempt(&failed_cmd.command, &cmd_entity.command)
                            {
                                if let Some(gotcha) =
                                    self.extract_gotcha(failed_cmd, &cmd_entity, &logged_event.session_id)
                                {
                                    gotchas.push(gotcha);
                                }
                                last_failed_command = None;
                            }

                            current_sequence.push(cmd_entity.clone());
                            commands.push(cmd_entity);
                        }
                    }
                    pending_tool_calls.retain(|k, _| !k.starts_with(&format!("{}_", idx)));
                }

                Event::ModelMessage { content, .. } => {
                    if let Some(decision) = self.extract_decision(content, logged_event) {
                        decisions.push(decision);
                    }
                }

                Event::UserMessage { content: _ } => {
                    // TODO: Use content to detect user intent and task boundaries
                    if !current_sequence.is_empty() && current_sequence.len() >= self.config.min_workflow_steps {
                        command_sequences.push(current_sequence.clone());
                    }
                    current_sequence.clear();
                }

                _ => {}
            }
        }

        let workflows = self.extract_workflows(&command_sequences, events);

        ExtractedEntities { commands, gotchas, decisions, workflows }
    }

    /// Extract a shell command from tool result
    fn extract_shell_command(
        &self, result: &serde_json::Value, success: bool, _error: &Option<String>, session_id: &str, seq: u64,
    ) -> Option<CommandEntity> {
        // TODO: Use error for gotcha extraction when available in result
        let cmd = result.get("cmd")?.as_str()?;
        let cwd = result.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
        let exit_code = result.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);

        let outcome = if success || exit_code == 0 { CommandOutcome::Success } else { CommandOutcome::Failure };

        Some(CommandEntity {
            command: cmd.to_string(),
            cwd,
            args: vec![],
            outcome,
            event_ids: vec![format!("{}_{}", session_id, seq)],
        })
    }

    /// Check if a command is an attempt to resolve a previous failure
    fn is_resolution_attempt(&self, failed_cmd: &str, resolution_cmd: &str) -> bool {
        let failed_base = failed_cmd.split_whitespace().next().unwrap_or("");
        let resolution_base = resolution_cmd.split_whitespace().next().unwrap_or("");
        failed_base == resolution_base || resolution_cmd.contains(failed_base)
    }

    /// Extract a gotcha from a failed command and its resolution
    fn extract_gotcha(
        &self, failed: &CommandEntity, resolution: &CommandEntity, _session_id: &str,
    ) -> Option<GotchaEntity> {
        // TODO: Use session_id for provenance tracking
        let category = self.classify_gotcha(&failed.command);

        let issue = format!("Command failed: {}", failed.command);
        let resolution_text = format!("Fixed with: {}", resolution.command);

        let mut event_ids = failed.event_ids.clone();
        event_ids.extend(resolution.event_ids.clone());

        Some(GotchaEntity { issue, resolution: resolution_text, category, event_ids })
    }

    /// Classify a gotcha into a category
    fn classify_gotcha(&self, command: &str) -> GotchaCategory {
        let cmd_lower = command.to_lowercase();

        if cmd_lower.contains("cargo")
            || cmd_lower.contains("build")
            || cmd_lower.contains("compile")
            || cmd_lower.contains("make")
            || cmd_lower.contains("cmake")
        {
            GotchaCategory::Build
        } else if cmd_lower.contains("test") || cmd_lower.contains("pytest") || cmd_lower.contains("jest") {
            GotchaCategory::Test
        } else if cmd_lower.contains("config")
            || cmd_lower.contains("settings")
            || cmd_lower.contains(".toml")
            || cmd_lower.contains(".yaml")
        {
            GotchaCategory::Config
        } else if cmd_lower.contains("panic") || cmd_lower.contains("error") || cmd_lower.contains("exception") {
            GotchaCategory::Runtime
        } else {
            GotchaCategory::Other
        }
    }

    /// Extract a decision from model message content
    fn extract_decision(&self, content: &str, logged_event: &LoggedEvent) -> Option<DecisionEntity> {
        let content_lower = content.to_lowercase();

        let keyword_found = self
            .config
            .decision_keywords
            .iter()
            .any(|kw| content_lower.contains(&format!(" {}", kw)) || content_lower.contains(&format!("{}.", kw)));

        if !keyword_found {
            return None;
        }

        let lines: Vec<&str> = content.lines().collect();

        let mut decision = String::new();
        let mut context = String::new();
        let mut rationale = String::new();
        let mut in_decision = false;
        let mut in_rationale = false;

        for line in lines {
            let line_lower = line.to_lowercase();

            if self.config.decision_keywords.iter().any(|kw| line_lower.contains(kw)) {
                in_decision = true;
                decision.push_str(line.trim());
                decision.push(' ');
                continue;
            }

            if in_decision {
                if line_lower.contains("because")
                    || line_lower.contains("since")
                    || line_lower.contains("due to")
                    || line_lower.contains("reason")
                {
                    in_decision = false;
                    in_rationale = true;
                    rationale.push_str(line.trim());
                    rationale.push(' ');
                    continue;
                }

                if !line.trim().is_empty() {
                    decision.push_str(line.trim());
                    decision.push(' ');
                } else {
                    in_decision = false;
                }
            } else if in_rationale {
                if !line.trim().is_empty() {
                    rationale.push_str(line.trim());
                    rationale.push(' ');
                } else {
                    in_rationale = false;
                }
            } else if !line.trim().is_empty() && context.len() < 500 {
                context.push_str(line.trim());
                context.push(' ');
            }
        }

        if decision.len() > 10 {
            Some(DecisionEntity {
                decision: decision.trim().to_string(),
                context: context.trim().to_string(),
                rationale: rationale.trim().to_string(),
                event_ids: vec![format!("{}_{}", logged_event.session_id, logged_event.seq)],
            })
        } else {
            None
        }
    }

    /// Extract workflows from command sequences
    fn extract_workflows(&self, sequences: &[Vec<CommandEntity>], _events: &[LoggedEvent]) -> Vec<WorkflowEntity> {
        // TODO: Use events to provide context for workflow titles/descriptions
        let mut workflows = Vec::new();

        for sequence in sequences {
            if sequence.len() < self.config.min_workflow_steps {
                continue;
            }

            // TODO: Check if this is a repeatable pattern
            let title = self.generate_workflow_title(sequence);

            let steps: Vec<WorkflowStep> = sequence
                .iter()
                .map(|cmd| WorkflowStep {
                    description: format!("Run: {}", cmd.command),
                    action: Some(cmd.command.clone()),
                    outcome: match cmd.outcome {
                        CommandOutcome::Success => "Command succeeds".to_string(),
                        CommandOutcome::Failure => "Command fails".to_string(),
                        CommandOutcome::Partial => "Partial success".to_string(),
                    },
                })
                .collect();

            workflows.push(WorkflowEntity {
                title,
                steps,
                event_ids: sequence.iter().flat_map(|c| c.event_ids.clone()).collect(),
            });
        }

        workflows
    }

    /// Generate a workflow title from a command sequence
    fn generate_workflow_title(&self, sequence: &[CommandEntity]) -> String {
        let commands: Vec<&str> = sequence.iter().map(|c| c.command.as_str()).collect();

        if commands.iter().all(|c| c.contains("cargo")) {
            let first = commands[0];
            if first.contains("fmt") {
                return "Prepare code for commit (Rust)".to_string();
            } else if first.contains("test") {
                return "Run Rust tests".to_string();
            } else if first.contains("build") {
                return "Build Rust project".to_string();
            }
        } else if commands.iter().all(|c| c.contains("git")) {
            return "Git workflow".to_string();
        }

        format!("{}-step workflow", sequence.len())
    }
}

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Event, LoggedEvent, Seq};
    use serde_json::json;

    fn create_test_event(seq: Seq, session_id: &str, event: Event) -> LoggedEvent {
        LoggedEvent { seq, session_id: session_id.to_string(), timestamp: "2026-01-22T10:00:00Z".to_string(), event }
    }

    #[test]
    fn test_extract_shell_command_success() {
        let extractor = EntityExtractor::new();

        let result = json!({
            "cmd": "cargo test",
            "cwd": "/workspace",
            "exit_code": 0
        });

        let cmd = extractor
            .extract_shell_command(&result, true, &None, "test_session", 0)
            .unwrap();

        assert_eq!(cmd.command, "cargo test");
        assert_eq!(cmd.cwd, Some("/workspace".to_string()));
        assert_eq!(cmd.outcome, CommandOutcome::Success);
    }

    #[test]
    fn test_extract_shell_command_failure() {
        let extractor = EntityExtractor::new();

        let result = json!({
            "cmd": "cargo build",
            "exit_code": 1
        });

        let cmd = extractor
            .extract_shell_command(&result, false, &Some("error".to_string()), "test_session", 0)
            .unwrap();

        assert_eq!(cmd.command, "cargo build");
        assert_eq!(cmd.outcome, CommandOutcome::Failure);
    }

    #[test]
    fn test_classify_gotcha() {
        let extractor = EntityExtractor::new();

        assert_eq!(extractor.classify_gotcha("cargo build"), GotchaCategory::Build);
        assert_eq!(extractor.classify_gotcha("cargo test"), GotchaCategory::Build);
        assert_eq!(extractor.classify_gotcha("pytest"), GotchaCategory::Test);
        assert_eq!(extractor.classify_gotcha("config.toml"), GotchaCategory::Config);
    }

    #[test]
    fn test_extract_decision() {
        let extractor = EntityExtractor::new();

        let content = "I decided to use tokio-rusqlite for the database layer. This choice was made because we need async SQLite access without blocking.";

        let logged_event = create_test_event(
            0,
            "test_session",
            Event::ModelMessage { content: content.to_string(), tokens_used: None },
        );

        let decision = extractor.extract_decision(content, &logged_event);

        assert!(decision.is_some());
        let decision = decision.unwrap();
        assert!(decision.decision.contains("tokio-rusqlite") || decision.decision.contains("database layer"));
    }

    #[test]
    fn test_extract_from_events() {
        let extractor = EntityExtractor::new();

        let events = vec![
            create_test_event(
                0,
                "test_session",
                Event::ToolCall { tool: "shell".to_string(), arguments: json!({"cmd": "cargo test"}) },
            ),
            create_test_event(
                1,
                "test_session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo test", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(
                2,
                "test_session",
                Event::ModelMessage {
                    content: "We chose to use Rust for this project.".to_string(),
                    tokens_used: None,
                },
            ),
        ];

        let entities = extractor.extract(&events);

        assert_eq!(entities.commands.len(), 1);
        assert_eq!(entities.commands[0].command, "cargo test");
        assert_eq!(entities.decisions.len(), 1);
        assert!(entities.decisions[0].decision.contains("Rust"));
    }

    #[test]
    fn test_extract_gotcha_from_failure_resolution() {
        let extractor = EntityExtractor::new();

        let events = vec![
            create_test_event(
                0,
                "test_session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo build", "exit_code": 1}),
                    success: false,
                    error: Some("error: feature not found".to_string()),
                },
            ),
            create_test_event(
                1,
                "test_session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo build --features foo", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
        ];

        let entities = extractor.extract(&events);

        assert_eq!(entities.gotchas.len(), 1);
        assert!(entities.gotchas[0].issue.contains("cargo build"));
        assert!(entities.gotchas[0].resolution.contains("--features foo"));
        assert_eq!(entities.gotchas[0].category, GotchaCategory::Build);
    }

    #[test]
    fn test_extract_workflow_from_sequence() {
        let extractor = EntityExtractor::with_config(ExtractionConfig { min_workflow_steps: 2, ..Default::default() });

        let events = vec![
            create_test_event(
                0,
                "test_session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo fmt", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(
                1,
                "test_session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo clippy", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(
                2,
                "test_session",
                Event::ToolResult {
                    tool: "shell".to_string(),
                    result: json!({"cmd": "cargo test", "exit_code": 0}),
                    success: true,
                    error: None,
                },
            ),
            create_test_event(3, "test_session", Event::UserMessage { content: "Done".to_string() }),
        ];

        let entities = extractor.extract(&events);

        assert_eq!(entities.workflows.len(), 1);
        assert_eq!(entities.workflows[0].steps.len(), 3);
        assert!(!entities.workflows[0].title.is_empty());
    }
}
