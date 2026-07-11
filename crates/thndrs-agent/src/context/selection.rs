//! Context selection policy: build candidate ledger items and select the
//! working set for a turn.
//!
//! This is a pure policy layer. It consumes already-loaded candidate sources
//! (harness fragments, instruction selections, pins, compaction
//! summaries, transcript entries, skills) and produces a [`ContextLedger`]
//! whose items carry a [`ContextVisibility`] and a human-readable `reason`
//! for every inclusion or omission decision. Side effects belong to the caller.
//!
//! ## Selection order
//!
//! 1. Always-loaded harness context (visible, never evicted by budget).
//! 2. Current user turn (visible, outside ordinary budget eviction).
//! 3. Active pins (pinned, before ordinary transcript items).
//! 4. Applicable closest `AGENTS.md` before broader guidance (visible).
//! 6. Active skill instructions then discovered skill metadata (visible).
//! 7. Latest compaction summary (visible) when older transcript turns are
//!    omitted; older summaries stay archived.
//! 8. Recent transcript entries (visible) within budget, oldest evicted
//!    first under pressure.
//!
//! UI-only and live-only transcript entries are never selected. Items whose
//! token estimate alone would exceed the available input budget are marked
//! [`ContextVisibility::Blocked`] instead of being silently truncated.

use std::path::PathBuf;

use super::support::{ratio_of, scope_depth};
use crate::context::{
    ContextBudget, ContextDiagnostic, ContextItem, ContextItemKind, ContextLedger, ContextVisibility,
    DiagnosticSeverity, ModelContextLimits, estimate_tokens, item_id_for_path, item_id_for_session_range,
};

/// A transcript entry considered for selection.
///
/// Carries the stable sequence number used for ids and eviction ordering, the
/// kind label for inclusion/omission rules, and an estimated byte size so the
/// policy stays free of the rendering layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptCandidate {
    /// Monotonic sequence number within the session (0-based).
    pub seq: u64,
    /// Session id scoping this entry's item id.
    pub session_id: String,
    /// Short label (e.g. `"user"`, `"assistant"`, `"tool:read_file"`).
    pub label: String,
    /// Estimated UTF-8 byte size of the rendered entry content.
    pub bytes: usize,
    /// Whether the entry is UI-only (status/error rows) or live-only (streaming).
    pub ui_only: bool,
    /// Whether the entry is still streaming and not yet settled.
    pub streaming: bool,
}

impl TranscriptCandidate {
    /// Build a settled transcript candidate.
    pub fn new(session_id: impl Into<String>, seq: u64, label: impl Into<String>, bytes: usize) -> Self {
        TranscriptCandidate {
            seq,
            session_id: session_id.into(),
            label: label.into(),
            bytes,
            ui_only: false,
            streaming: false,
        }
    }

    /// Mark this candidate as UI-only (status/error rows).
    pub fn ui_only(mut self) -> Self {
        self.ui_only = true;
        self
    }

    /// Mark this candidate as still streaming and not yet settled.
    pub fn streaming(mut self) -> Self {
        self.streaming = true;
        self
    }
}

/// A pinned working-set item considered for selection.
///
/// Pins are task-local evidence kept visible across turns until dropped or
/// expired. They are not durable context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedCandidate {
    /// Stable id from [`item_id_for_path`] or an explicit handle.
    pub id: String,
    /// Kind of pinned context.
    pub kind: ContextItemKind,
    /// Short label (e.g. file path, tool-result label).
    pub label: String,
    /// Absolute source path when file-backed.
    pub source_path: Option<PathBuf>,
    /// Scope label.
    pub scope: String,
    /// Content hash when applicable.
    pub content_hash: Option<u64>,
    /// Estimated UTF-8 byte size of the pinned content.
    pub bytes: usize,
}

impl PinnedCandidate {
    /// Build a pinned file candidate with a stable path-derived id.
    pub fn file(kind: ContextItemKind, path: PathBuf, scope: impl Into<String>, bytes: usize) -> Self {
        let id = item_id_for_path(&kind, &path);
        let label = path.display().to_string();
        PinnedCandidate { id, kind, label, source_path: Some(path), scope: scope.into(), content_hash: None, bytes }
    }
}

/// A compaction summary considered for selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionSummaryCandidate {
    /// Stable id for the summary range.
    pub id: String,
    /// Inclusive start sequence covered by the summary.
    pub start_seq: u64,
    /// Inclusive end sequence covered by the summary.
    pub end_seq: u64,
    /// Short label (e.g. `"summary 12..47"`).
    pub label: String,
    /// Renderable summary text, used for prompt projection when the summary is
    /// selected as normal context content. `None` when only metadata is
    /// needed.
    pub content: Option<String>,
    /// Estimated UTF-8 byte size of the summary text.
    pub bytes: usize,
    /// Whether this summary is the latest compaction.
    pub latest: bool,
}

impl CompactionSummaryCandidate {
    /// Build a summary candidate covering `start_seq..=end_seq` in `session_id`.
    pub fn new(session_id: &str, start_seq: u64, end_seq: u64, bytes: usize, latest: bool) -> Self {
        let id = item_id_for_session_range(&ContextItemKind::Summary, session_id, start_seq, end_seq);
        CompactionSummaryCandidate {
            id,
            start_seq,
            end_seq,
            label: format!("summary {start_seq}..{end_seq}"),
            content: None,
            bytes,
            latest,
        }
    }
}

/// A loaded skill considered for selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillCandidate {
    /// Skill name.
    pub name: String,
    /// Absolute path to the `SKILL.md`.
    pub path: PathBuf,
    /// Content hash of the loaded skill text.
    pub content_hash: u64,
    /// Estimated UTF-8 byte size of the loaded skill instructions.
    pub bytes: usize,
    /// Whether the skill instructions are fully loaded (`true`) or only
    /// discovered as metadata (`false`).
    pub loaded: bool,
}

impl SkillCandidate {
    /// Build a discovered-only skill candidate from its stable metadata.
    pub fn discovered(name: impl Into<String>, path: PathBuf, content_hash: u64, bytes: usize) -> Self {
        SkillCandidate { name: name.into(), path, content_hash, bytes, loaded: false }
    }

    /// Build a fully-loaded skill candidate.
    pub fn loaded(name: impl Into<String>, path: PathBuf, rendered_bytes: usize, rendered_hash: u64) -> Self {
        SkillCandidate { name: name.into(), path, content_hash: rendered_hash, bytes: rendered_bytes, loaded: true }
    }
}

/// An instruction source considered for selection.
///
/// Mirrors the fields the policy needs from [`crate::context::ContextSource`]
/// without forcing the caller to clone full content; selection works on
/// metadata only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionCandidate {
    /// Absolute source path.
    pub path: PathBuf,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// Content hash of the full source.
    pub content_hash: u64,
    /// Original byte count (before any truncation at load time).
    pub byte_count: usize,
    /// Renderable instruction text (size-capped), used for prompt projection
    /// when the source is selected as normal context content. `None` when only
    /// metadata is needed.
    pub content: Option<String>,
    /// Whether the source was truncated when loaded.
    pub truncated: bool,
    /// Whether the source is applicable this turn (closest-first ordering is
    /// the caller's responsibility).
    pub applicable: bool,
}

impl InstructionCandidate {
    /// Depth of the scope: `.` = 0, `src` = 1, `src/core` = 2.
    pub fn scope_depth(&self) -> usize {
        scope_depth(&self.scope)
    }
}

/// A harness context fragment considered for selection.
///
/// Harness fragments are always-loaded and never evicted by the budget policy;
/// they are recorded so the ledger is honest about what the model sees.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessCandidate {
    /// Stable id for the fragment.
    pub id: String,
    /// Short label (e.g. `"base_identity"`, `"tool_schemas"`).
    pub label: String,
    /// Estimated UTF-8 byte size of the fragment.
    pub bytes: usize,
}

impl HarnessCandidate {
    /// Build a harness fragment candidate.
    pub fn new(label: impl Into<String>, bytes: usize) -> Self {
        let label = label.into();
        let id = format!("ctx_harness_{}", slug(&label));
        HarnessCandidate { id, label, bytes }
    }
}

/// All candidate sources for a turn, ready for selection.
///
/// Built by the caller from boundary work (filesystem discovery and transcript
/// projection). The policy consumes this without further
/// I/O.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionInput {
    /// Always-loaded harness fragments.
    pub harness: Vec<HarnessCandidate>,
    /// Current user turn text, outside ordinary budget eviction.
    pub user_turn: Option<UserTurnCandidate>,
    /// Scoped project instructions, closest-first.
    pub instructions: Vec<InstructionCandidate>,
    /// Active pins.
    pub pins: Vec<PinnedCandidate>,
    /// Compaction summaries, latest last.
    pub compaction_summaries: Vec<CompactionSummaryCandidate>,
    /// Recent transcript candidates, oldest first.
    pub transcript: Vec<TranscriptCandidate>,
    /// Discovered and loaded skills.
    pub skills: Vec<SkillCandidate>,
    /// Explicitly dropped item ids (persist until source change or reset).
    pub dropped_ids: Vec<String>,
}

/// The current user turn, kept outside ordinary budget eviction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserTurnCandidate {
    /// Stable id for the turn within the session.
    pub id: String,
    /// Estimated UTF-8 byte size of the user turn text.
    pub bytes: usize,
}

impl UserTurnCandidate {
    /// Build a user turn candidate with a session-scoped id.
    pub fn new(session_id: &str, seq: u64, bytes: usize) -> Self {
        UserTurnCandidate { id: item_id_for_session_range(&ContextItemKind::Transcript, session_id, seq, seq), bytes }
    }
}

/// Select the context working set for a turn.
///
/// Builds a [`ContextLedger`] from [`SelectionInput`] and the resolved
/// [`ModelContextLimits`]. The policy is deterministic for the same inputs.
pub fn select_context(input: &SelectionInput, limits: ModelContextLimits) -> ContextLedger {
    let mut items: Vec<ContextItem> = Vec::new();
    let mut diagnostics = Vec::new();

    let available_input = limits.available_input_budget();

    for harness in &input.harness {
        items.push(ContextItem {
            id: harness.id.clone(),
            kind: ContextItemKind::Harness,
            label: harness.label.clone(),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            byte_count: harness.bytes,
            content: None,
            token_estimate: estimate_tokens(harness.bytes),
            visibility: ContextVisibility::Visible,
            reason: "always-loaded harness context".to_string(),
        });
    }

    if let Some(turn) = &input.user_turn {
        items.push(ContextItem {
            id: turn.id.clone(),
            kind: ContextItemKind::Transcript,
            label: "current user turn".to_string(),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            byte_count: turn.bytes,
            content: None,
            token_estimate: estimate_tokens(turn.bytes),
            visibility: ContextVisibility::Visible,
            reason: "current user turn, outside ordinary budget eviction".to_string(),
        });
    }

    for pin in &input.pins {
        let visibility = if input.dropped_ids.iter().any(|id| id == &pin.id) {
            ContextVisibility::Dropped
        } else if estimate_tokens(pin.bytes) as u64 > available_input {
            diagnostics.push(blocked_oversized(&pin.id, "pinned item"));
            ContextVisibility::Blocked
        } else {
            ContextVisibility::Pinned
        };
        let reason = visibility.reason("user pin");
        items.push(ContextItem {
            id: pin.id.clone(),
            kind: pin.kind.clone(),
            label: pin.label.clone(),
            source_path: pin.source_path.clone(),
            scope: pin.scope.clone(),
            content_hash: pin.content_hash,
            byte_count: pin.bytes,
            content: None,
            token_estimate: estimate_tokens(pin.bytes),
            visibility,
            reason,
        });
    }

    let mut ordered_instructions: Vec<&InstructionCandidate> = input.instructions.iter().collect();
    ordered_instructions.sort_by_key(|b| std::cmp::Reverse(b.scope_depth()));
    for instruction in ordered_instructions {
        let id = item_id_for_path(&ContextItemKind::ProjectInstruction, &instruction.path);
        let visibility = if input.dropped_ids.iter().any(|d| d == &id) {
            ContextVisibility::Dropped
        } else if !instruction.applicable {
            ContextVisibility::Candidate
        } else if estimate_tokens(instruction.byte_count) as u64 > available_input {
            diagnostics.push(blocked_oversized(&id, "project instruction"));
            ContextVisibility::Blocked
        } else {
            ContextVisibility::Visible
        };
        let reason =
            visibility.reason(if instruction.applicable { "applicable AGENTS.md" } else { "discovered AGENTS.md" });
        items.push(ContextItem {
            id,
            kind: ContextItemKind::ProjectInstruction,
            label: instruction.path.display().to_string(),
            source_path: Some(instruction.path.clone()),
            scope: instruction.scope.clone(),
            content_hash: Some(instruction.content_hash),
            byte_count: instruction.byte_count,
            content: if visibility.is_rendered() { instruction.content.clone() } else { None },
            token_estimate: estimate_tokens(instruction.byte_count),
            visibility,
            reason,
        });
    }

    let mut ordered_skills: Vec<&SkillCandidate> = input.skills.iter().collect();
    ordered_skills.sort_by_key(|s| !s.loaded);
    for skill in ordered_skills {
        let id = item_id_for_path(&ContextItemKind::Skill, &skill.path);
        let visibility = if input.dropped_ids.iter().any(|d| d == &id) {
            ContextVisibility::Dropped
        } else if estimate_tokens(skill.bytes) as u64 > available_input {
            diagnostics.push(blocked_oversized(&id, "skill"));
            ContextVisibility::Blocked
        } else if skill.loaded {
            ContextVisibility::Visible
        } else {
            ContextVisibility::Candidate
        };
        let reason = visibility.reason(if skill.loaded { "loaded skill" } else { "discovered skill metadata" });
        items.push(ContextItem {
            id,
            kind: ContextItemKind::Skill,
            label: skill.name.clone(),
            source_path: Some(skill.path.clone()),
            scope: ".".to_string(),
            content_hash: Some(skill.content_hash),
            byte_count: skill.bytes,
            content: None,
            token_estimate: estimate_tokens(skill.bytes),
            visibility,
            reason,
        });
    }

    let omitting_older = should_omit_older_transcript(input, available_input);
    for summary in &input.compaction_summaries {
        let visibility = if input.dropped_ids.iter().any(|d| d == &summary.id) {
            ContextVisibility::Dropped
        } else if !omitting_older {
            ContextVisibility::Archived
        } else if summary.latest {
            if estimate_tokens(summary.bytes) as u64 > available_input {
                diagnostics.push(blocked_oversized(&summary.id, "compaction summary"));
                ContextVisibility::Blocked
            } else {
                ContextVisibility::Visible
            }
        } else {
            ContextVisibility::Archived
        };
        let reason = visibility.reason("compaction summary");
        items.push(ContextItem {
            id: summary.id.clone(),
            kind: ContextItemKind::Summary,
            label: summary.label.clone(),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            byte_count: summary.bytes,
            content: if visibility.is_rendered() { summary.content.clone() } else { None },
            token_estimate: estimate_tokens(summary.bytes),
            visibility,
            reason,
        });
    }

    push_transcript(&mut items, input, available_input);

    let budget = ContextBudget::from_limits(limits, &items);
    ContextLedger { items, budget, diagnostics }
}

/// Push transcript candidates, omitting UI-only and live-only entries and
/// evicting oldest entries under budget pressure.
fn push_transcript(items: &mut Vec<ContextItem>, input: &SelectionInput, available_input: u64) {
    let target = ratio_of(available_input, super::control::TARGET_BUDGET_RATIO);

    let consumed: u64 = items
        .iter()
        .filter(|item| item.visibility.is_rendered())
        .map(|item| item.token_estimate as u64)
        .sum();

    let transcript: Vec<&TranscriptCandidate> = input.transcript.iter().collect();
    let mut selected: Vec<&TranscriptCandidate> = Vec::new();
    let mut running = consumed;
    for candidate in transcript.iter().rev() {
        if candidate.ui_only || candidate.streaming {
            continue;
        }
        let tokens = estimate_tokens(candidate.bytes) as u64;
        if running + tokens > target {
            break;
        }
        running += tokens;
        selected.push(candidate);
    }

    let selected_seq: std::collections::HashSet<u64> = selected.iter().map(|c| c.seq).collect();

    for candidate in &transcript {
        let id = item_id_for_session_range(
            &ContextItemKind::Transcript,
            &candidate.session_id,
            candidate.seq,
            candidate.seq,
        );
        let (visibility, reason) = if candidate.ui_only {
            (ContextVisibility::Candidate, "omitted: ui-only transcript entry")
        } else if candidate.streaming {
            (ContextVisibility::Candidate, "omitted: live-only streaming entry")
        } else if input.dropped_ids.iter().any(|d| d == &id) {
            (ContextVisibility::Dropped, "explicit drop")
        } else if selected_seq.contains(&candidate.seq) {
            (ContextVisibility::Visible, "recent transcript entry")
        } else {
            (ContextVisibility::Archived, "archived: evicted under budget pressure")
        };
        items.push(ContextItem {
            id,
            kind: ContextItemKind::Transcript,
            label: candidate.label.clone(),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            byte_count: candidate.bytes,
            content: None,
            token_estimate: estimate_tokens(candidate.bytes),
            visibility,
            reason: reason.to_string(),
        });
    }
}

/// Whether older transcript turns should be omitted (summarized) this turn.
///
/// Omits when a latest compaction summary is available and the transcript
/// entries' own token cost would exceed the target selection budget.
///
/// When omitted, the latest summary stands in for the older transcript detail.
fn should_omit_older_transcript(input: &SelectionInput, available_input: u64) -> bool {
    if input.compaction_summaries.iter().all(|s| !s.latest) {
        return false;
    }
    let target = ratio_of(available_input, super::control::TARGET_BUDGET_RATIO);
    let transcript_tokens: u64 = input
        .transcript
        .iter()
        .filter(|c| !c.ui_only && !c.streaming)
        .map(|c| estimate_tokens(c.bytes) as u64)
        .sum();
    transcript_tokens > target
}

/// Slugify a label for use in an id.
fn slug(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Build a diagnostic for an item blocked because its token estimate alone
/// exceeds the available input budget.
fn blocked_oversized(id: &str, kind: &str) -> ContextDiagnostic {
    ContextDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: "blocked_oversized".to_string(),
        message: format!("{kind} {id} exceeds available input budget; marked blocked instead of truncating"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ModelLimitConfidence, ModelLimitSource};

    fn limits(context_window: u64) -> ModelContextLimits {
        ModelContextLimits {
            provider: "test".to_string(),
            model: "test".to_string(),
            context_window,
            max_completion_tokens: 1_024,
            recommended_completion_tokens: 512,
            source: ModelLimitSource::LiveMetadata,
            confidence: ModelLimitConfidence::Exact,
        }
    }

    #[test]
    fn pin_is_selected_and_can_be_dropped() {
        let pin = PinnedCandidate::file(
            ContextItemKind::PinnedFile,
            PathBuf::from("/repo/src/lib.rs"),
            "src",
            120,
        );
        let input = SelectionInput { pins: vec![pin.clone()], ..Default::default() };
        let visible = select_context(&input, limits(200_000));
        assert!(
            visible
                .items
                .iter()
                .any(|item| item.id == pin.id && item.visibility == ContextVisibility::Pinned)
        );

        let dropped = SelectionInput { dropped_ids: vec![pin.id], ..input };
        let ledger = select_context(&dropped, limits(200_000));
        assert!(
            ledger
                .items
                .iter()
                .any(|item| item.visibility == ContextVisibility::Dropped)
        );
    }

    #[test]
    fn latest_summary_replaces_older_transcript_under_pressure() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 1_000)],
            user_turn: Some(UserTurnCandidate::new("session", 0, 100)),
            compaction_summaries: vec![CompactionSummaryCandidate::new("session", 1, 10, 300, true)],
            transcript: vec![
                TranscriptCandidate::new("session", 1, "old", 5_000),
                TranscriptCandidate::new("session", 2, "recent", 100),
            ],
            ..Default::default()
        };
        let ledger = select_context(&input, limits(4_000));
        assert!(
            ledger
                .items
                .iter()
                .any(|item| item.kind == ContextItemKind::Summary && item.visibility == ContextVisibility::Visible)
        );
        assert!(
            ledger
                .items
                .iter()
                .any(|item| item.kind == ContextItemKind::Transcript && item.visibility == ContextVisibility::Archived)
        );
    }
}
