//! Prompt assembly and context contract.
//!
//! Builds a structured [`PromptBundle`] from app state, then lowers it into
//! Umans Anthropic-compatible messages. The bundle is data, not ad hoc string
//! concatenation, so it can be inspected, tested, and serialized.
//!
//! ## Ordering
//!
//! 1. Base identity — short `thndrs`-specific behavior.
//! 2. Harness policy — tool boundary, workspace containment, no raw shell.
//! 3. Environment metadata — cwd, model, search mode, rounded date.
//! 4. Project context — loaded `AGENTS.md` text (below policy and user
//!    instructions).
//! 5. Tool catalog — provider-native schemas for local tools.
//! 6. Transcript tail — projected model-visible entries.
//! 7. User turn — current prompt text.

use std::path::Path;

use crate::app::Entry;
use crate::cli::WebSearchMode;
use crate::context::ContextSource;
use crate::providers::umans::Message;
use crate::tools::ToolDefinition;

/// The structured prompt bundle before provider-specific lowering.
///
/// Each field is a separate piece of model context. The [`Display`] impl
/// renders the full system prompt text; [`PromptBundle::lower_to_umans`]
/// converts it to Anthropic-compatible messages.
#[derive(Clone, Debug)]
pub struct PromptBundle {
    /// Base identity and behavior.
    pub base: String,
    /// Harness policy: tool boundary, safety limits.
    pub policy: String,
    /// Environment: cwd, model, search mode, rounded date.
    pub environment: EnvironmentMetadata,
    /// Loaded AGENTS.md context sources.
    pub project_context: Vec<ContextSource>,
    /// Tool catalog as provider-native schemas.
    pub tool_catalog: Vec<ToolDefinition>,
    /// Projected model-visible transcript tail.
    pub transcript_tail: Vec<Entry>,
    /// Current user prompt text.
    pub user_turn: String,
}

impl PromptBundle {
    /// Build a [`PromptBundle`] from app state and context.
    pub fn new(
        cwd: &Path, model: &str, mode: WebSearchMode, context_sources: &[ContextSource], transcript: &[Entry],
        user_turn: &str,
    ) -> PromptBundle {
        let tool_catalog = crate::tools::tool_definitions();
        let transcript_tail = project_transcript_tail(transcript);
        PromptBundle {
            base: base_prompt(),
            policy: policy_prompt(),
            environment: EnvironmentMetadata::new(cwd, model, mode),
            project_context: context_sources.to_vec(),
            tool_catalog,
            transcript_tail,
            user_turn: user_turn.to_string(),
        }
    }
}

/// Environment metadata included in the prompt for cache stability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentMetadata {
    /// Workspace root path.
    pub cwd: String,
    /// Selected model name.
    pub model: String,
    /// Web search mode label.
    pub search_mode: String,
    /// Rounded current date (YYYY-MM-DD) for cache stability.
    /// The exact timestamp stays in session JSONL when needed for audit.
    pub date: String,
}

impl EnvironmentMetadata {
    /// Build environment metadata from app state, rounding the date to the
    /// day for prompt-cache stability.
    pub fn new(cwd: &Path, model: &str, search_mode: WebSearchMode) -> Self {
        let search_mode_label = match search_mode {
            WebSearchMode::Native => "native",
            WebSearchMode::Exa => "exa",
            WebSearchMode::None => "none",
        };

        EnvironmentMetadata {
            cwd: cwd.display().to_string(),
            model: model.to_string(),
            search_mode: search_mode_label.to_string(),
            date: rounded_date(),
        }
    }
}

/// The short base identity prompt for `thndrs`.
///
/// Kept concise and specific to this harness — not copied from a larger agent
/// prompt wholesale. Covers identity, output style, and work approach.
pub fn base_prompt() -> String {
    include_str!("base.txt").to_string()
}

/// The harness policy prompt with tool boundaries, safety limits, and editing rules.
pub fn policy_prompt() -> String {
    include_str!("policy.md").to_string()
}

/// Project the model-visible transcript tail from the full UI transcript.
///
/// Excludes UI-only status rows, live-only stream deltas, sidebar state, and
/// renderer artifacts. Keeps `User`, `Assistant`, `Reasoning`, and `Tool`
/// entries.
fn project_transcript_tail(transcript: &[Entry]) -> Vec<Entry> {
    transcript
        .iter()
        .rev()
        .take(20)
        .filter(|e| {
            matches!(
                e,
                Entry::User { .. } | Entry::Assistant { .. } | Entry::Reasoning { .. } | Entry::Tool { .. }
            )
        })
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Render the system prompt text from the bundle.
///
/// This is the concatenation of base, policy, environment, and project context
/// in the correct precedence order. Tool catalog and transcript tail are
/// rendered as separate message blocks during lowering.
///
/// 1. Base identity
/// 2. Harness policy.
/// 3. Environment metadata.
/// 4. Project context (AGENTS.md) — below harness policy and user instructions.
pub fn render_system_prompt(bundle: &PromptBundle) -> String {
    let mut parts: Vec<String> = vec![
        bundle.base.clone(),
        bundle.policy.clone(),
        format!(
            "## Environment\n\
                 - workspace: {}\n\
                 - model: {}\n\
                 - search: {}\n\
                 - date: {}",
            bundle.environment.cwd, bundle.environment.model, bundle.environment.search_mode, bundle.environment.date
        ),
    ];

    if !bundle.project_context.is_empty() {
        let mut context_lines = vec!["## Project Context".to_string()];
        for source in &bundle.project_context {
            context_lines.push(format!(
                "### {} (scope: {}, hash: {}, truncated: {})",
                source.path.display(),
                source.scope,
                source.content_hash,
                source.truncated
            ));
            context_lines.push(String::new());
            context_lines.push(source.content.clone());
        }
        parts.push(context_lines.join("\n"));
    }

    parts.join("\n\n")
}

/// Lower a [`PromptBundle`] into Umans Anthropic-compatible messages.
///
/// The first message is a `user` message containing the system prompt (base +
/// policy + environment + project context). The transcript tail follows as
/// alternating user/assistant messages. The final message is the current user
/// turn.
pub fn lower_to_umans_messages(bundle: &PromptBundle) -> Vec<Message> {
    let mut messages: Vec<Message> = vec![Message::user(&render_system_prompt(bundle))];

    for entry in &bundle.transcript_tail {
        match entry {
            Entry::User { text } => messages.push(Message::user(text)),
            Entry::Assistant { text, .. } => messages.push(Message::assistant(text)),
            Entry::Reasoning { text, .. } => messages.push(Message::assistant(text)),
            Entry::Tool { name, output, .. } => messages.push(Message::user(
                &(if output.is_empty() {
                    format!("[tool: {name} — no output]")
                } else {
                    format!("[tool: {name}]\n{}", output.join("\n"))
                }),
            )),
            _ => (),
        }
    }

    if !bundle.user_turn.is_empty() {
        messages.push(Message::user(&bundle.user_turn));
    }

    messages
}

/// Render the tool catalog as a compact JSON schema block for the system prompt.
///
/// Uses Anthropic-compatible tool definition format: name, description,
/// input_schema.
pub fn render_tool_catalog(bundle: &PromptBundle) -> serde_json::Value {
    serde_json::Value::Array(
        bundle
            .tool_catalog
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect(),
    )
}

/// Get the current date rounded to the day (YYYY-MM-DD) for cache stability.
///
/// The exact timestamp stays in session JSONL metadata when needed for audit.
///
/// TODO: chrono
fn rounded_date() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    date_from_days_since_epoch(now / 86_400)
}

/// Convert days since Unix epoch (1970-01-01) to a YYYY-MM-DD string.
///
/// Uses the Howard Hinnant algorithm for date calculation.
fn date_from_days_since_epoch(days: u64) -> String {
    let z = days as i64 + 719_468; // Days from 0000-03-01 to 1970-01-01
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ToolStatus;
    use std::path::PathBuf;

    fn test_bundle() -> PromptBundle {
        PromptBundle {
            base: base_prompt(),
            policy: policy_prompt(),
            environment: EnvironmentMetadata {
                cwd: "/repo".to_string(),
                model: "umans-coder".to_string(),
                search_mode: "native".to_string(),
                date: "2026-06-29".to_string(),
            },
            project_context: Vec::new(),
            tool_catalog: crate::tools::tool_definitions(),
            transcript_tail: Vec::new(),
            user_turn: "explain this repo".to_string(),
        }
    }

    #[test]
    fn base_prompt_is_short_and_specific() {
        let base = base_prompt();
        assert!(base.contains("thndrs"), "should mention thndrs");
        assert!(
            base.len() < 600,
            "base prompt should be concise, got {} chars",
            base.len()
        );
    }

    #[test]
    fn policy_prompt_mentions_tools_and_safety() {
        let policy = policy_prompt();
        assert!(policy.contains("read-only"));
        assert!(policy.contains("workspace"));
        assert!(policy.contains("AGENTS.md"));
    }

    #[test]
    fn environment_metadata_rounds_date() {
        let env = EnvironmentMetadata::new(Path::new("/repo"), "umans-coder", WebSearchMode::Native);
        assert_eq!(env.date.len(), 10, "date should be YYYY-MM-DD");
        assert!(env.date.starts_with("20"), "date should be in the 2000s");
    }

    #[test]
    fn environment_metadata_search_mode_labels() {
        let native = EnvironmentMetadata::new(Path::new("."), "m", WebSearchMode::Native);
        assert_eq!(native.search_mode, "native");

        let exa = EnvironmentMetadata::new(Path::new("."), "m", WebSearchMode::Exa);
        assert_eq!(exa.search_mode, "exa");

        let none = EnvironmentMetadata::new(Path::new("."), "m", WebSearchMode::None);
        assert_eq!(none.search_mode, "none");
    }

    #[test]
    fn system_prompt_orders_base_before_policy_before_env() {
        let bundle = test_bundle();
        let prompt = render_system_prompt(&bundle);
        let base_pos = prompt.find("thndrs").unwrap();
        let policy_pos = prompt.find("Harness Policy").unwrap();
        let env_pos = prompt.find("Environment").unwrap();
        assert!(base_pos < policy_pos, "base should come before policy");
        assert!(policy_pos < env_pos, "policy should come before environment");
    }

    #[test]
    fn system_prompt_includes_agents_md_below_policy() {
        let mut bundle = test_bundle();
        bundle.project_context = vec![ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: ".".to_string(),
            content: "# Project\nBuild with cargo.".to_string(),
            content_hash: 12345,
            truncated: false,
            byte_count: 25,
        }];

        let prompt = render_system_prompt(&bundle);
        let policy_pos = prompt.find("Harness Policy").unwrap();
        let context_pos = prompt.find("Project Context").unwrap();
        assert!(policy_pos < context_pos, "AGENTS.md should be below harness policy");
        assert!(prompt.contains("# Project"), "should include AGENTS.md content");
        assert!(prompt.contains("12345"), "should include content hash");
    }

    #[test]
    fn lower_to_umans_produces_messages() {
        let bundle = test_bundle();
        let messages = lower_to_umans_messages(&bundle);
        assert!(!messages.is_empty());
        assert_eq!(messages[0].role, "user");
        assert!(messages[0].content.contains("thndrs"));

        let last = messages.last().unwrap();
        assert_eq!(last.role, "user");
        assert!(last.content.contains("explain this repo"));
    }

    #[test]
    fn lower_to_umans_includes_transcript_tail() {
        let mut bundle = test_bundle();
        bundle.transcript_tail = vec![
            Entry::User { text: "what is this?".to_string() },
            Entry::Assistant { text: "a repo".to_string(), streaming: false },
        ];

        let messages = lower_to_umans_messages(&bundle);
        assert!(messages.len() >= 3);
    }

    #[test]
    fn lower_to_umans_excludes_status_entries() {
        let mut bundle = test_bundle();
        bundle.transcript_tail = vec![
            Entry::Status { text: "loaded context".to_string() },
            Entry::User { text: "hi".to_string() },
            Entry::Error { text: "boom".to_string() },
        ];

        let messages = lower_to_umans_messages(&bundle);
        let all_content: String = messages.iter().map(|m| m.content.as_str()).collect();
        assert!(
            !all_content.contains("loaded context"),
            "status entries should be excluded"
        );
        assert!(!all_content.contains("boom"), "error entries should be excluded");
    }

    #[test]
    fn project_transcript_tail_excludes_status_and_errors() {
        let transcript = vec![
            Entry::User { text: "hello".to_string() },
            Entry::Status { text: "loaded".to_string() },
            Entry::Assistant { text: "hi".to_string(), streaming: false },
            Entry::Error { text: "fail".to_string() },
            Entry::Tool {
                name: "find_files#0".to_string(),
                arguments: "{}".to_string(),
                status: ToolStatus::Ok,
                output: vec!["src/main.rs".to_string()],
            },
        ];

        let tail = project_transcript_tail(&transcript);
        assert!(
            tail.iter()
                .all(|e| !matches!(e, Entry::Status { .. } | Entry::Error { .. }))
        );
        assert_eq!(tail.len(), 3);
    }

    #[test]
    fn render_tool_catalog_produces_json() {
        let bundle = test_bundle();
        let catalog = render_tool_catalog(&bundle);
        let arr = catalog.as_array().unwrap();
        assert!(!arr.is_empty(), "tool catalog should not be empty");
        assert!(
            arr.iter()
                .all(|t| t.get("name").is_some() && t.get("input_schema").is_some())
        );
    }

    #[test]
    fn build_prompt_bundle_assembles_all_parts() {
        let bundle = PromptBundle::new(
            Path::new("/repo"),
            "umans-coder",
            WebSearchMode::Native,
            &[],
            &[Entry::User { text: "test".to_string() }],
            "hello",
        );

        assert!(bundle.base.contains("thndrs"));
        assert!(bundle.policy.contains("Harness Policy"));
        assert_eq!(bundle.environment.model, "umans-coder");
        assert!(!bundle.tool_catalog.is_empty());
        assert_eq!(bundle.user_turn, "hello");
    }

    #[test]
    fn date_from_days_since_epoch_known_values() {
        assert_eq!(date_from_days_since_epoch(0), "1970-01-01");
        let d = date_from_days_since_epoch(20_745);
        assert!(d.starts_with("2026"), "day 20745 should be in 2026, got {d}");
    }
}
