//! Append-only JSONL session persistence.
//!
//! Records prompt metadata for audit and resume without storing full raw
//! provider payloads by default. Raw payloads can contain prompt text,
//! repository content, and secrets — only structured metadata is persisted.
//!
//! ## Metadata shape
//!
//! [`PromptMetadata`] captures everything needed to audit or replay a turn:
//! model, search mode, context sources (path, scope, hash, truncation state),
//! tool catalog size, and whether history reuse was active.
//!
//! It does **not** include the prompt text, AGENTS.md content, transcript text,
//! or provider request/response bodies.

use serde::{Deserialize, Serialize};

use crate::context::ContextSource;
use crate::prompt::{EnvironmentMetadata, HistoryReuse, PromptBundle};

/// Metadata for a single prompt turn, suitable for append-only JSONL storage.
///
/// This is the audit record: enough to reconstruct *what* was sent without
/// storing *the content itself*. Full raw provider payloads are deliberately
/// excluded because they can contain prompt text, repo content, and secrets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptMetadata {
    /// Selected model name.
    pub model: String,
    /// Web search mode label.
    pub search_mode: String,
    /// Rounded date (YYYY-MM-DD) used for cache stability.
    pub date: String,
    /// Workspace root path.
    pub cwd: String,
    /// Metadata for each loaded AGENTS.md source (no content).
    pub context_sources: Vec<ContextSourceMeta>,
    /// Number of tools in the catalog sent this turn.
    pub tool_catalog_size: usize,
    /// Whether history reuse was active for this turn.
    pub history_reuse: bool,
    /// Content hash of the root AGENTS.md from the previous turn, if any.
    pub prev_context_hash: Option<u64>,
    /// Number of transcript entries included in the projected tail.
    pub transcript_tail_size: usize,
    /// Whether the user turn was non-empty.
    pub has_user_turn: bool,
}

/// Metadata for a loaded context source, without the content itself.
///
/// Records the path, scope, content hash, and truncation state so the
/// session can audit which AGENTS.md was loaded and whether it was capped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextSourceMeta {
    /// Absolute path to the source file.
    pub path: String,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// Stable hash of the full original content (before truncation).
    pub content_hash: u64,
    /// Whether the content was truncated to fit the size cap.
    pub truncated: bool,
    /// Original byte count of the file (before truncation).
    pub byte_count: usize,
}

impl PromptMetadata {
    /// Extract prompt metadata from a [`PromptBundle`] for session storage.
    ///
    /// This captures the structural metadata of the turn — model, search mode,
    /// context sources (hashes and truncation, not content), tool count, and
    /// transcript tail size. It does not store prompt text, AGENTS.md content,
    /// or provider request/response bodies.
    pub fn from_bundle(bundle: &PromptBundle) -> Self {
        let environment: &EnvironmentMetadata = &bundle.environment;
        PromptMetadata {
            model: environment.model.clone(),
            search_mode: environment.search_mode.clone(),
            date: environment.date.clone(),
            cwd: environment.cwd.clone(),
            context_sources: bundle
                .project_context
                .iter()
                .map(ContextSourceMeta::from_source)
                .collect(),
            tool_catalog_size: bundle.tool_catalog.len(),
            history_reuse: bundle.history_reuse == HistoryReuse::Available,
            prev_context_hash: bundle.prev_context_hash,
            transcript_tail_size: bundle.transcript_tail.len(),
            has_user_turn: !bundle.user_turn.is_empty(),
        }
    }

    /// Serialize to a JSON string for JSONL append.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from a JSON string (for resume/replay).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl ContextSourceMeta {
    /// Extract metadata from a [`ContextSource`], omitting the content.
    pub fn from_source(source: &ContextSource) -> Self {
        ContextSourceMeta {
            path: source.path.display().to_string(),
            scope: source.scope.clone(),
            content_hash: source.content_hash,
            truncated: source.truncated,
            byte_count: source.byte_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::WebSearchMode;
    use crate::prompt::PromptBundle;
    use std::path::Path;
    use std::path::PathBuf;

    fn bundle_with_context() -> PromptBundle {
        let source = ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: ".".to_string(),
            content: "# Project\nBuild with cargo.".to_string(),
            content_hash: 12345,
            truncated: false,
            byte_count: 25,
        };
        PromptBundle::new(
            Path::new("/repo"),
            "umans-coder",
            WebSearchMode::Native,
            &[source],
            &[],
            "explain this repo",
        )
    }

    #[test]
    fn from_bundle_captures_model_and_search_mode() {
        let bundle = bundle_with_context();
        let meta = PromptMetadata::from_bundle(&bundle);
        assert_eq!(meta.model, "umans-coder");
        assert_eq!(meta.search_mode, "native");
    }

    #[test]
    fn from_bundle_captures_context_source_metadata_without_content() {
        let bundle = bundle_with_context();
        let meta = PromptMetadata::from_bundle(&bundle);
        assert_eq!(meta.context_sources.len(), 1);

        let ctx = &meta.context_sources[0];
        assert_eq!(ctx.path, "/repo/AGENTS.md");
        assert_eq!(ctx.scope, ".");
        assert_eq!(ctx.content_hash, 12345);
        assert!(!ctx.truncated);
        assert_eq!(ctx.byte_count, 25);
    }

    #[test]
    fn from_bundle_captures_tool_catalog_size() {
        let bundle = bundle_with_context();
        let meta = PromptMetadata::from_bundle(&bundle);
        assert!(meta.tool_catalog_size > 0, "should record non-zero tool count");
        assert_eq!(meta.tool_catalog_size, bundle.tool_catalog.len());
    }

    #[test]
    fn from_bundle_captures_history_reuse_state() {
        let mut bundle = bundle_with_context();
        bundle.history_reuse = HistoryReuse::Available;
        bundle.prev_context_hash = Some(99999);

        let meta = PromptMetadata::from_bundle(&bundle);
        assert!(meta.history_reuse);
        assert_eq!(meta.prev_context_hash, Some(99999));
    }

    #[test]
    fn from_bundle_defaults_history_reuse_false() {
        let bundle = bundle_with_context();
        let meta = PromptMetadata::from_bundle(&bundle);
        assert!(!meta.history_reuse, "default should be unavailable");
        assert_eq!(meta.prev_context_hash, None);
    }

    #[test]
    fn from_bundle_captures_transcript_tail_size_and_user_turn() {
        use crate::app::Entry;
        let transcript = vec![
            Entry::User { text: "hello".to_string() },
            Entry::Assistant { text: "hi".to_string(), streaming: false },
        ];
        let bundle = PromptBundle::new(
            Path::new("/repo"),
            "umans-coder",
            WebSearchMode::Native,
            &[],
            &transcript,
            "next question",
        );
        let meta = PromptMetadata::from_bundle(&bundle);
        assert_eq!(meta.transcript_tail_size, 2);
        assert!(meta.has_user_turn);
    }

    #[test]
    fn from_bundle_empty_user_turn_records_false() {
        let bundle = PromptBundle::new(Path::new("/repo"), "umans-coder", WebSearchMode::Native, &[], &[], "");
        let meta = PromptMetadata::from_bundle(&bundle);
        assert!(!meta.has_user_turn);
    }

    #[test]
    fn json_round_trip_preserves_all_fields() {
        let bundle = bundle_with_context();
        let meta = PromptMetadata::from_bundle(&bundle);
        let json = meta.to_json().expect("serialize");
        let restored = PromptMetadata::from_json(&json).expect("deserialize");
        assert_eq!(meta, restored);
    }

    #[test]
    fn json_round_trip_with_history_reuse() {
        let mut bundle = bundle_with_context();
        bundle.history_reuse = HistoryReuse::Available;
        bundle.prev_context_hash = Some(77777);

        let meta = PromptMetadata::from_bundle(&bundle);
        let json = meta.to_json().expect("serialize");
        let restored = PromptMetadata::from_json(&json).expect("deserialize");
        assert_eq!(meta, restored);
        assert!(restored.history_reuse);
        assert_eq!(restored.prev_context_hash, Some(77777));
    }

    #[test]
    fn json_round_trip_truncated_context() {
        let source = ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: ".".to_string(),
            content: "x".repeat(100),
            content_hash: 88888,
            truncated: true,
            byte_count: 40_000,
        };
        let bundle = PromptBundle::new(
            Path::new("/repo"),
            "umans-glm-5.2",
            WebSearchMode::Exa,
            &[source],
            &[],
            "explain",
        );
        let meta = PromptMetadata::from_bundle(&bundle);
        let json = meta.to_json().expect("serialize");
        let restored = PromptMetadata::from_json(&json).expect("deserialize");
        assert_eq!(meta, restored);
        assert!(restored.context_sources[0].truncated);
        assert_eq!(restored.context_sources[0].byte_count, 40_000);
    }

    #[test]
    fn from_json_rejects_malformed_input() {
        let result = PromptMetadata::from_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn metadata_does_not_contain_prompt_text() {
        let bundle = bundle_with_context();
        let meta = PromptMetadata::from_bundle(&bundle);
        let json = meta.to_json().expect("serialize");
        assert!(
            !json.contains("explain this repo"),
            "metadata must not contain user prompt text"
        );
        assert!(
            !json.contains("Build with cargo"),
            "metadata must not contain AGENTS.md content"
        );
        assert!(
            !json.contains("thndrs"),
            "metadata must not contain base/policy prompt text"
        );
    }

    #[test]
    fn context_source_meta_from_source_omits_content() {
        let source = ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: ".".to_string(),
            content: "secret project content".to_string(),
            content_hash: 42,
            truncated: true,
            byte_count: 1000,
        };
        let meta = ContextSourceMeta::from_source(&source);
        assert_eq!(meta.path, "/repo/AGENTS.md");
        assert_eq!(meta.content_hash, 42);
        assert!(meta.truncated);
        assert_eq!(meta.byte_count, 1000);
    }
}
