//! Prompt assembly and context contract.
//!
//! Builds a structured [`PromptBundle`] from app state, then lowers it into
//! Umans Anthropic-compatible messages. The bundle is data, not ad hoc string
//! concatenation, so it can be inspected, tested, and serialized.
//!
//! ## Fragment Ordering
//!
//! The system prompt is assembled from named fragments in a fixed order:
//!
//! 1. **base_identity** — short `thndrs`-specific identity.
//! 2. **communication_style** — how to talk to the user.
//! 3. **action_model** — when to act, explore, or ask.
//! 4. **edit_guidance** — exact-edit and write-tool behavior.
//! 5. **action_safety** — tool boundaries, workspace containment, no shell.
//! 6. **self_knowledge** — how to answer questions about `thndrs`.
//! 7. **web_source_guidance** — when and how to use web tools.
//! 8. Environment metadata — cwd, model, search mode, rounded date.
//! 9. Self-knowledge snapshot — docs map and compact runtime state, plus a
//!    compact context dashboard when a context ledger is attached.
//! 10. Project context — loaded `AGENTS.md` text (below policy and user
//!     instructions), **or** the selected context projection
//!     (`<selected_context>`) from the attached [`ContextLedger`] when one is
//!     present. The ledger path renders selected instructions,
//!     compaction summaries, and pinned handles.
//! 11. Tool catalog — provider-native schemas for local tools.
//! 12. Transcript tail — projected model-visible entries (selected by the
//!     context policy when a ledger is attached).
//! 13. User turn — current prompt text.

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::app::Entry;
use crate::app::ToolStatus;
use crate::cli::WebSearchMode;
use crate::context::ContextSource;
use crate::internals;
use crate::providers::ProviderMessage;
use crate::skills;
use crate::skills::SkillMetadata;
use crate::tools;
use crate::tools::ToolDefinition;
use crate::utils;
use crate::utils::datetime;
use thndrs_agent::context::{ContextItem, ContextLedger};

/// Whether the provider supports reusable history / prompt caching for
/// AGENTS.md content.
///
/// Umans does not currently expose explicit reusable-history or prompt-cache
/// behavior, so the default is [`HistoryReuse::Unavailable`], which always
/// includes the active size-capped AGENTS.md content.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum HistoryReuse {
    /// Provider supports reusable history. Full AGENTS.md text is included
    /// only when its content hash changes; otherwise only metadata is sent.
    Available,
    /// Provider does not support reusable history (default). The active
    /// size-capped AGENTS.md content is always included.
    #[default]
    Unavailable,
}

/// A named prompt fragment — one focused piece of model-visible context.
///
/// Fragments are assembled in order into the system prompt. Each fragment owns
/// a specific concern (identity, communication style, action safety, etc.) so
/// that prompt assembly stays modular and testable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptFragment {
    /// Short label for debugging and testing (e.g. `"base_identity"`).
    pub name: &'static str,
    /// The fragment text, included verbatim in the system prompt.
    pub content: String,
}

impl PromptFragment {
    /// Create a fragment from a static name and content string.
    pub fn new(name: &'static str, content: impl Into<String>) -> Self {
        PromptFragment { name, content: content.into() }
    }
}

/// The structured prompt bundle before provider-specific lowering.
///
/// Each field is a separate piece of model context. The [`Display`] impl
/// renders the full system prompt text; [`PromptBundle::lower_to_umans`]
/// converts it to Anthropic-compatible messages.
#[derive(Clone, Debug)]
pub struct PromptBundle {
    /// Ordered prompt fragments: base identity, communication style, action
    /// model, edit guidance, action safety, self-knowledge, web/source guidance.
    pub fragments: Vec<PromptFragment>,
    /// Environment: cwd, model, search mode, rounded date.
    pub environment: EnvironmentMetadata,
    /// Loaded AGENTS.md context sources.
    pub project_context: Vec<ContextSource>,
    /// Tool catalog as provider-native schemas.
    pub tool_catalog: Vec<ToolDefinition>,
    /// Available Agent Skills metadata. Full skill instructions are read only
    /// after activation.
    pub available_skills: Vec<SkillMetadata>,
    /// Projected model-visible transcript tail.
    pub transcript_tail: Vec<Entry>,
    /// Current user prompt text.
    pub user_turn: String,
    /// Whether the provider supports reusable history / prompt caching for
    /// AGENTS.md content. When [`HistoryReuse::Available`], full text is
    /// included only when the content hash changes.
    pub history_reuse: HistoryReuse,
    /// Content hash of the root AGENTS.md from the previous turn, if any.
    ///
    /// Used with [`HistoryReuse::Available`] to skip re-sending unchanged
    /// AGENTS.md text. `None` on the first turn or when no context is loaded.
    pub prev_context_hash: Option<u64>,
    /// Selected context projection from the context selection policy (P7/P8).
    ///
    /// When `Some`, the system prompt renders selected project instructions,
    /// compaction summaries, and pinned handles from the ledger items,
    /// the transcript tail is taken from ledger-selected transcript context,
    /// and a compact context dashboard is added to `<thndrs_self_knowledge>`.
    ///
    /// When `None`, the legacy rendering path is used (`project_context` plus
    /// the fixed transcript tail projection), preserving existing behavior.
    pub context_ledger: Option<ContextLedger>,
}

impl PromptBundle {
    /// Build a [`PromptBundle`] from app state and context.
    pub fn new(
        cwd: &Path, model: &str, mode: WebSearchMode, context_sources: &[ContextSource], transcript: &[Entry],
        user_turn: &str,
    ) -> PromptBundle {
        PromptBundle::new_with_skills(cwd, model, mode, context_sources, &[], transcript, user_turn)
    }

    /// Build a [`PromptBundle`] including discovered Agent Skills metadata.
    pub fn new_with_skills(
        cwd: &Path, model: &str, mode: WebSearchMode, context_sources: &[ContextSource],
        available_skills: &[SkillMetadata], transcript: &[Entry], user_turn: &str,
    ) -> PromptBundle {
        let tool_catalog = tools::tool_definitions();
        let transcript_tail = project_transcript_tail(transcript);
        PromptBundle {
            fragments: default_fragments(),
            environment: EnvironmentMetadata::new(cwd, model, mode),
            project_context: context_sources.to_vec(),
            tool_catalog,
            available_skills: available_skills.to_vec(),
            transcript_tail,
            user_turn: user_turn.to_string(),
            history_reuse: HistoryReuse::default(),
            prev_context_hash: None,
            context_ledger: None,
        }
    }

    /// Replace the bundle's tool catalog with a runtime-specific catalog.
    pub fn with_tool_catalog(mut self, tool_catalog: Vec<ToolDefinition>) -> Self {
        self.tool_catalog = tool_catalog;
        self
    }

    /// Attach a selected context projection (P7 ledger) so the system prompt
    /// renders selected instructions, summaries, pins, and transcript
    /// context from the ledger, and adds a context dashboard to
    /// `<thndrs_self_knowledge>`.
    pub fn with_context_ledger(mut self, ledger: ContextLedger) -> Self {
        self.context_ledger = Some(ledger);
        self
    }
}

/// Environment metadata included in the prompt for cache stability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentMetadata {
    /// Workspace root path.
    pub cwd: String,
    /// Selected model name.
    pub model: String,
    /// Web search mode selected for this turn.
    pub search_mode: WebSearchMode,
    /// Rounded current date (YYYY-MM-DD) for cache stability.
    /// The exact timestamp stays in session JSONL when needed for audit.
    pub date: String,
}

impl EnvironmentMetadata {
    /// Build environment metadata from app state, rounding the date to the
    /// day for prompt-cache stability.
    pub fn new(cwd: &Path, model: &str, search_mode: WebSearchMode) -> Self {
        EnvironmentMetadata {
            cwd: cwd.display().to_string(),
            model: model.to_string(),
            search_mode,
            date: datetime::rounded_date(),
        }
    }
}

/// Build the default ordered set of prompt fragments.
///
/// Each fragment is a separate concern:
/// 1. **base_identity** — who thndrs is.
/// 2. **communication_style** — how to talk to the user.
/// 3. **action_model** — when to act, explore, or ask.
/// 4. **edit_guidance** — exact-edit and write-tool behavior.
/// 5. **action_safety** — tool boundaries, workspace containment, no shell.
/// 6. **self_knowledge** — how to answer questions about `thndrs`.
/// 7. **web_source_guidance** — when and how to use web tools.
pub fn default_fragments() -> Vec<PromptFragment> {
    vec![
        PromptFragment::new("base_identity", include_str!("fragments/base_identity.xml")),
        PromptFragment::new("communication_style", include_str!("fragments/communication_style.xml")),
        PromptFragment::new("action_model", include_str!("fragments/action_model.xml")),
        PromptFragment::new("edit_guidance", include_str!("fragments/edit_guidance.xml")),
        PromptFragment::new("action_safety", include_str!("fragments/action_safety.xml")),
        PromptFragment::new("self_knowledge", include_str!("fragments/self_knowledge.xml")),
        PromptFragment::new("web_source_guidance", include_str!("fragments/web_source_guidance.xml")),
    ]
}

/// Render the system prompt text from the bundle.
///
/// Assembles the ordered fragments, environment metadata, and project context
/// in the correct precedence order. Tool catalog and transcript tail are
/// rendered as separate message blocks during lowering.
///
/// 1. Base identity
/// 2. Communication style
/// 3. Action model
/// 4. Edit guidance
/// 5. Action safety
/// 6. Self-knowledge
/// 7. Web/source guidance
/// 8. Environment metadata.
/// 9. Self-knowledge snapshot.
/// 10. Project context (AGENTS.md) — below harness policy and user instructions.
///
/// ## AGENTS.md inclusion
///
/// When [`HistoryReuse::Available`] and the current content hash matches
/// `prev_context_hash`, the full AGENTS.md text is omitted — only a metadata
/// stub (path, hash) is included, since the provider already has the cached
/// content. When the hash differs or history reuse is unavailable, the active
/// size-capped content is always included.
pub fn render_system_prompt(bundle: &PromptBundle) -> String {
    let mut parts: Vec<String> = bundle.fragments.iter().map(|f| f.content.clone()).collect();

    parts.push(format!(
        r#"<environment>
  <workspace><![CDATA[{}]]></workspace>
  <model><![CDATA[{}]]></model>
  <search>{}</search>
  <date>{}</date>
</environment>"#,
        cdata(&bundle.environment.cwd),
        cdata(&bundle.environment.model),
        bundle.environment.search_mode.label(),
        bundle.environment.date
    ));

    let snapshot: internals::SelfKnowledgeSnapshot = bundle.into();
    parts.push(snapshot.render_model_visible());

    if let Some(ledger) = &bundle.context_ledger {
        let projection = render_context_projection(ledger);
        if !projection.is_empty() {
            parts.push(projection);
        }
    } else if !bundle.project_context.is_empty() {
        parts.push(render_legacy_project_context(bundle));
    }

    let available_skills = skills::format_available_skills(&bundle.available_skills);
    if !available_skills.is_empty() {
        parts.push(available_skills);
    }

    parts.join("\n\n")
}

/// Render the legacy `<project_context>` block from `bundle.project_context`.
///
/// Used when no context ledger is attached. Preserves the existing AGENTS.md
/// rendering including history-reuse text omission.
fn render_legacy_project_context(bundle: &PromptBundle) -> String {
    let mut context_lines = vec!["<project_context>".to_string()];
    for source in &bundle.project_context {
        let text_unchanged =
            bundle.history_reuse == HistoryReuse::Available && bundle.prev_context_hash == Some(source.content_hash);

        context_lines.push("  <source>".to_string());
        context_lines.push(format!(
            "    <path><![CDATA[{}]]></path>",
            cdata(&source.path.display().to_string())
        ));
        context_lines.push(format!("    <scope><![CDATA[{}]]></scope>", cdata(&source.scope)));
        context_lines.push(format!("    <hash>{}</hash>", source.content_hash));
        context_lines.push(format!("    <truncated>{}</truncated>", source.truncated));

        if text_unchanged {
            context_lines.push("    <status>unchanged, text omitted</status>".to_string());
        } else {
            context_lines.push(format!("    <content><![CDATA[{}]]></content>", cdata(&source.content)));
        }
        context_lines.push("  </source>".to_string());
    }
    context_lines.push("</project_context>".to_string());
    context_lines.join("\n")
}

/// Render the selected context projection from a [`ContextLedger`] into a
/// single `<selected_context>` block.
fn render_context_projection(ledger: &ContextLedger) -> String {
    use thndrs_agent::context::ContextItemKind as Kind;

    let rendered: Vec<&ContextItem> = ledger
        .items
        .iter()
        .filter(|item| {
            item.visibility.is_rendered()
                && matches!(
                    item.kind,
                    Kind::ProjectInstruction | Kind::Summary | Kind::PinnedFile | Kind::Skill
                )
        })
        .collect();
    if rendered.is_empty() {
        return String::new();
    }

    let mut out = String::from("<selected_context>\n");

    for item in &rendered {
        match item.kind {
            Kind::ProjectInstruction => {
                out.push_str("  <instruction>\n");
                push_item_meta(&mut out, 4, item);
                if let Some(content) = item.content.as_deref() {
                    out.push_str(&format!("    <content><![CDATA[{}]]></content>\n", cdata(content)));
                }
                out.push_str("  </instruction>\n");
            }
            Kind::Summary => {
                out.push_str("  <compaction_summary>\n");
                push_item_meta(&mut out, 4, item);
                if let Some(content) = item.content.as_deref() {
                    out.push_str(&format!("    <content><![CDATA[{}]]></content>\n", cdata(content)));
                }
                out.push_str("  </compaction_summary>\n");
            }
            Kind::PinnedFile => {
                out.push_str("  <pinned_handle>\n");
                push_item_meta(&mut out, 4, item);
                out.push_str("  </pinned_handle>\n");
            }
            Kind::Skill => {
                out.push_str("  <skill>\n");
                push_item_meta(&mut out, 4, item);
                out.push_str("  </skill>\n");
            }
            _ => {}
        }
    }

    out.push_str("</selected_context>");
    out
}

/// Append metadata (id, scope, path, hash) for a rendered context item.
fn push_item_meta(out: &mut String, indent: usize, item: &ContextItem) {
    let pad = " ".repeat(indent);
    out.push_str(&format!("{pad}<id>{}</id>\n", utils::escape_xml(&item.id)));
    out.push_str(&format!("{pad}<label>{}</label>\n", utils::escape_xml(&item.label)));
    out.push_str(&format!("{pad}<scope>{}</scope>\n", utils::escape_xml(&item.scope)));
    if let Some(path) = item.source_path.as_ref() {
        let path_str = path.display().to_string();
        out.push_str(&format!("{pad}<path>{}</path>\n", utils::escape_xml(&path_str)));
    }
    if let Some(hash) = item.content_hash {
        out.push_str(&format!("{pad}<hash>{}</hash>\n", hash));
    }
}

/// Lower a [`PromptBundle`] into Umans Anthropic-compatible messages.
///
/// The first message is a `user` message containing the system prompt (base +
/// policy + environment + project context). The transcript tail follows as
/// alternating user/assistant messages.
///
/// The final message is the current user turn.
pub fn lower_to_umans_messages(bundle: &PromptBundle) -> Vec<ProviderMessage> {
    let mut messages = vec![ProviderMessage::user(&render_system_prompt(bundle))];

    for entry in &bundle.transcript_tail {
        match entry {
            Entry::User { text } => messages.push(ProviderMessage::user(text)),
            Entry::Agent { text, streaming: false, .. } => messages.push(ProviderMessage::assistant(text)),
            Entry::Reasoning { text, streaming: false, .. } => messages.push(ProviderMessage::assistant(text)),
            Entry::Tool { name, output, status, .. } if *status != ToolStatus::Running => {
                // Session transcript entries currently retain their bounded
                // display lines for compatibility. Lower them through the
                // provider-neutral model projection so this boundary is
                // ready for independently persisted projections.
                let model_output = tools::ToolOutput::ok(name, output.clone()).model.lines;
                messages.push(ProviderMessage::user(
                    &(match model_output.is_empty() {
                        true => format!("[tool: {name} — no output]"),
                        false => format!("[tool: {name}]\n{}", model_output.join("\n")),
                    }),
                ))
            }
            _ => (),
        }
    }

    if !bundle.user_turn.is_empty() {
        messages.push(ProviderMessage::user(&bundle.user_turn));
    }

    messages
}

/// Render the tool catalog as a compact JSON schema block for the system prompt.
///
/// Uses Anthropic-compatible tool definition format: name, description,
/// input_schema. Delegates to the shared [`tools::tool_catalog_schemas`]
/// so the prompt-bundle view and the provider request body stay in sync.
pub fn render_tool_catalog(bundle: &PromptBundle) -> serde_json::Value {
    tools::tool_catalog_schemas(&bundle.tool_catalog)
}

/// Project the model-visible transcript tail from the full UI transcript.
///
/// Only finalized `User`, `Assistant`, `Reasoning`, and `Tool` entries reach the model.
fn project_transcript_tail(transcript: &[Entry]) -> Vec<Entry> {
    transcript
        .iter()
        .rev()
        .take(20)
        .filter(|e| match e {
            Entry::User { .. } => true,
            Entry::Agent { streaming: false, .. } => true,
            Entry::Reasoning { streaming: false, .. } => true,
            Entry::Tool { status, .. } => *status != ToolStatus::Running,
            _ => false,
        })
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn cdata(text: &str) -> String {
    text.replace("]]>", "]]]]><![CDATA[>")
}
