//! Context selection policy: build candidate ledger items and select the
//! working set for a turn.
//!
//! This is a pure policy layer. It consumes already-loaded candidate sources
//! (harness fragments, instruction selections, memory, pins, compaction
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
//! 5. Core memory then path-applicable archival memory (visible).
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

use crate::context::{
    ContextBudget, ContextDiagnostic, ContextItem, ContextItemKind, ContextLedger, ContextVisibility,
    DiagnosticSeverity, ModelContextLimits, estimate_tokens, item_id_for_path, item_id_for_session_range,
};
use crate::memory::MemoryItem;
use crate::skills::SkillMetadata;
use crate::utils::{ratio_of, scope_depth};

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
/// expired. They are not memory.
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
    /// Build a discovered-only skill candidate from metadata.
    pub fn metadata(skill: &SkillMetadata) -> Self {
        SkillCandidate {
            name: skill.name.clone(),
            path: skill.path.clone(),
            content_hash: skill.content_hash,
            bytes: skill.byte_count,
            loaded: false,
        }
    }

    /// Build a fully-loaded skill candidate.
    pub fn loaded(skill: &SkillMetadata, rendered_bytes: usize, rendered_hash: u64) -> Self {
        SkillCandidate {
            name: skill.name.clone(),
            path: skill.path.clone(),
            content_hash: rendered_hash,
            bytes: rendered_bytes,
            loaded: true,
        }
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
/// Built by the caller from boundary work (filesystem discovery, memory
/// indexing, transcript projection). The policy consumes this without further
/// I/O.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionInput {
    /// Always-loaded harness fragments.
    pub harness: Vec<HarnessCandidate>,
    /// Current user turn text, outside ordinary budget eviction.
    pub user_turn: Option<UserTurnCandidate>,
    /// Scoped project instructions, closest-first.
    pub instructions: Vec<InstructionCandidate>,
    /// Core memory items, user then project.
    pub core_memory: Vec<MemoryItem>,
    /// Path-applicable archival memory notes.
    pub archival_memory: Vec<MemoryItem>,
    /// Session-scoped memory (survives compaction and resume).
    pub session_memory: Vec<MemoryItem>,
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
            token_estimate: estimate_tokens(instruction.byte_count),
            visibility,
            reason,
        });
    }

    push_memory_items(
        &mut items,
        &input.core_memory,
        true,
        &input.dropped_ids,
        available_input,
        &mut diagnostics,
    );
    push_memory_items(
        &mut items,
        &input.archival_memory,
        false,
        &input.dropped_ids,
        available_input,
        &mut diagnostics,
    );
    push_session_memory(
        &mut items,
        &input.session_memory,
        &input.dropped_ids,
        available_input,
        &mut diagnostics,
    );

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
            token_estimate: estimate_tokens(summary.bytes),
            visibility,
            reason,
        });
    }

    push_transcript(&mut items, input, available_input);

    let budget = ContextBudget::from_limits(limits, &items);
    ContextLedger { items, budget, diagnostics }
}

/// Push core or archival memory items.
fn push_memory_items(
    items: &mut Vec<ContextItem>, memory: &[MemoryItem], core: bool, dropped_ids: &[String], available_input: u64,
    diagnostics: &mut Vec<ContextDiagnostic>,
) {
    for item in memory {
        let id = item.memory_item_id();
        let kind = item.item_kind();
        let visibility = if dropped_ids.iter().any(|d| d == &id) {
            ContextVisibility::Dropped
        } else if estimate_tokens(item.byte_count) as u64 > available_input {
            diagnostics.push(blocked_oversized(&id, "memory item"));
            ContextVisibility::Blocked
        } else {
            ContextVisibility::Visible
        };
        let reason = visibility.reason(if core { "core memory" } else { "archival memory" });
        items.push(ContextItem {
            id,
            kind,
            label: item.title.clone(),
            source_path: if item.path.as_os_str().is_empty() { None } else { Some(item.path.clone()) },
            scope: item.scope_label(),
            content_hash: Some(item.content_hash),
            byte_count: item.byte_count,
            token_estimate: estimate_tokens(item.byte_count),
            visibility,
            reason,
        });
    }
}

/// Push session-scoped memory.
///
/// Session memory stays eligible after compaction and resume, and is removed only through explicit deletion.
fn push_session_memory(
    items: &mut Vec<ContextItem>, memory: &[MemoryItem], dropped_ids: &[String], available_input: u64,
    diagnostics: &mut Vec<ContextDiagnostic>,
) {
    for item in memory {
        let id = item.memory_item_id();
        let kind = item.item_kind();
        let visibility = if dropped_ids.iter().any(|d| d == &id) {
            ContextVisibility::Dropped
        } else if estimate_tokens(item.byte_count) as u64 > available_input {
            diagnostics.push(blocked_oversized(&id, "session memory"));
            ContextVisibility::Blocked
        } else {
            ContextVisibility::Visible
        };
        let reason = visibility.reason("session-scoped memory");
        items.push(ContextItem {
            id,
            kind,
            label: item.title.clone(),
            source_path: None,
            scope: "session".to_string(),
            content_hash: Some(item.content_hash),
            byte_count: item.byte_count,
            token_estimate: estimate_tokens(item.byte_count),
            visibility,
            reason,
        });
    }
}

/// Push transcript candidates, omitting UI-only and live-only entries and
/// evicting oldest entries under budget pressure.
fn push_transcript(items: &mut Vec<ContextItem>, input: &SelectionInput, available_input: u64) {
    let target = ratio_of(available_input, crate::context::control::TARGET_BUDGET_RATIO);

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
    let target = ratio_of(available_input, crate::context::control::TARGET_BUDGET_RATIO);
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
    use crate::context::ModelLimitSource;
    use crate::memory::{MemoryKind, MemoryRootKind, MemoryScope, MemorySource};

    fn limits(context_window: u64) -> ModelContextLimits {
        ModelContextLimits {
            provider: "umans".to_string(),
            model: "umans-coder".to_string(),
            context_window,
            max_completion_tokens: 1_024,
            recommended_completion_tokens: 512,
            source: ModelLimitSource::LiveMetadata,
            confidence: crate::context::ModelLimitConfidence::Exact,
        }
    }

    fn memory_item(id: &str, title: &str, root: MemoryRootKind, bytes: usize) -> MemoryItem {
        MemoryItem {
            id: id.to_string(),
            title: title.to_string(),
            kind: MemoryKind::Fact,
            scope: root.default_scope(),
            paths: Vec::new(),
            tags: Vec::new(),
            created: "2026-07-03T00:00:00Z".to_string(),
            updated: "2026-07-03T00:00:00Z".to_string(),
            source: MemorySource::ExplicitUser,
            root,
            path: if matches!(root, MemoryRootKind::Project) {
                PathBuf::from(format!("/repo/.thndrs/memory/notes/{id}.md"))
            } else {
                PathBuf::from(format!("/home/.thndrs/memory/notes/{id}.md"))
            },
            content_hash: 1,
            byte_count: bytes,
            truncated: false,
            body: "body".to_string(),
        }
    }

    fn instruction(scope: &str, applicable: bool, bytes: usize) -> InstructionCandidate {
        InstructionCandidate {
            path: PathBuf::from(format!("/repo/{scope}/AGENTS.md")),
            scope: scope.to_string(),
            content_hash: 1,
            byte_count: bytes,
            truncated: false,
            applicable,
        }
    }

    fn transcript(seq: u64, label: &str, bytes: usize) -> TranscriptCandidate {
        TranscriptCandidate::new("sess_1", seq, label, bytes)
    }

    /// A normal budget fits harness, user turn, pins, instructions, memory,
    /// and recent transcript.
    #[test]
    fn normal_budget_selects_all_visible() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base_identity", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            instructions: vec![instruction(".", true, 200)],
            core_memory: vec![memory_item("mem_core", "Core", MemoryRootKind::User, 150)],
            pins: vec![PinnedCandidate::file(
                ContextItemKind::PinnedFile,
                PathBuf::from("/repo/src/lib.rs"),
                "src",
                300,
            )],
            transcript: vec![transcript(1, "user", 80), transcript(2, "assistant", 120)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let visible: Vec<&ContextItem> = ledger.rendered().into_iter().collect();
        assert!(visible.iter().any(|i| i.kind == ContextItemKind::Harness));
        assert!(
            visible
                .iter()
                .any(|i| i.kind == ContextItemKind::Transcript && i.label == "current user turn")
        );
        assert!(visible.iter().any(|i| i.visibility == ContextVisibility::Pinned));
        assert!(visible.iter().any(|i| i.kind == ContextItemKind::ProjectInstruction));
        assert!(visible.iter().any(|i| i.kind == ContextItemKind::UserMemory));
        assert!(ledger.diagnostics.is_empty());
    }

    /// A short budget evicts the oldest transcript entries.
    #[test]
    fn short_budget_evicts_oldest_transcript() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 1_000)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 100)),
            transcript: vec![
                transcript(1, "old user", 5_000),
                transcript(2, "old assistant", 5_000),
                transcript(3, "recent user", 100),
                transcript(4, "recent assistant", 100),
            ],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(6_000));
        let archived: Vec<&ContextItem> = ledger
            .items
            .iter()
            .filter(|i| i.visibility == ContextVisibility::Archived)
            .collect();
        assert!(!archived.is_empty(), "oldest entries must be archived under pressure");
        for item in &archived {
            assert!(
                item.reason.contains("evicted"),
                "archived reason must explain eviction: {}",
                item.reason
            );
        }
    }

    /// An overloaded budget blocks oversized items instead of truncating them.
    #[test]
    fn overloaded_budget_blocks_oversized_items() {
        let huge = 1_000_000;
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            pins: vec![PinnedCandidate::file(
                ContextItemKind::PinnedFile,
                PathBuf::from("/repo/big.rs"),
                "src",
                huge,
            )],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let blocked: Vec<&ContextItem> = ledger
            .items
            .iter()
            .filter(|i| i.visibility == ContextVisibility::Blocked)
            .collect();
        assert!(!blocked.is_empty(), "oversized pin must be blocked, not truncated");
        assert!(ledger.diagnostics.iter().any(|d| d.code == "blocked_oversized"));
        for item in &blocked {
            assert!(
                item.reason.contains("oversized"),
                "blocked reason must mention oversized: {}",
                item.reason
            );
        }
    }

    /// Pins survive across turns (they are pinned, not evicted by recency).
    #[test]
    fn pins_survive_across_turns() {
        let pin = PinnedCandidate::file(
            ContextItemKind::PinnedFile,
            PathBuf::from("/repo/src/lib.rs"),
            "src",
            100,
        );
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            pins: vec![pin.clone()],
            transcript: vec![
                transcript(1, "user", 100),
                transcript(2, "assistant", 100),
                transcript(3, "user", 100),
            ],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(4_000));
        let pinned = ledger
            .items
            .iter()
            .find(|i| i.id == pin.id)
            .expect("pin must be in ledger");
        assert_eq!(pinned.visibility, ContextVisibility::Pinned);
        assert!(pinned.reason.contains("pin"));
    }

    /// Drops remove pins from future turns until reset.
    #[test]
    fn drops_remove_pins_until_reset() {
        let pin = PinnedCandidate::file(
            ContextItemKind::PinnedFile,
            PathBuf::from("/repo/src/lib.rs"),
            "src",
            100,
        );
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            pins: vec![pin.clone()],
            dropped_ids: vec![pin.id.clone()],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let dropped = ledger.items.iter().find(|i| i.id == pin.id).expect("pin in ledger");
        assert_eq!(dropped.visibility, ContextVisibility::Dropped);
        assert!(dropped.reason.contains("dropped"));
    }

    /// Explicit dropped-item rules persist until source change or reset.
    #[test]
    fn dropped_rules_persist_across_turns() {
        let pin = PinnedCandidate::file(
            ContextItemKind::PinnedFile,
            PathBuf::from("/repo/src/lib.rs"),
            "src",
            100,
        );

        let input1 = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            pins: vec![pin.clone()],
            dropped_ids: vec![pin.id.clone()],
            ..Default::default()
        };
        let ledger1 = select_context(&input1, limits(200_000));
        assert_eq!(
            ledger1.items.iter().find(|i| i.id == pin.id).unwrap().visibility,
            ContextVisibility::Dropped
        );

        let input2 = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 1, 50)),
            pins: vec![pin.clone()],
            dropped_ids: vec![pin.id.clone()],
            ..Default::default()
        };
        let ledger2 = select_context(&input2, limits(200_000));
        assert_eq!(
            ledger2.items.iter().find(|i| i.id == pin.id).unwrap().visibility,
            ContextVisibility::Dropped
        );

        let input3 = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 2, 50)),
            pins: vec![pin],
            dropped_ids: vec![],
            ..Default::default()
        };
        let ledger3 = select_context(&input3, limits(200_000));
        assert_eq!(
            ledger3
                .items
                .iter()
                .find(|i| i.id == ledger3.items[0].id)
                .unwrap()
                .visibility,
            ContextVisibility::Visible
        );
    }

    /// The current user turn is included outside ordinary budget eviction.
    #[test]
    fn current_user_turn_outside_budget_eviction() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 1_000)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 5_000)),
            transcript: vec![transcript(1, "old", 5_000)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(8_000));
        let user_turn = ledger
            .items
            .iter()
            .find(|i| i.label == "current user turn")
            .expect("user turn in ledger");
        assert_eq!(user_turn.visibility, ContextVisibility::Visible);
        assert!(user_turn.reason.contains("outside ordinary budget eviction"));
    }

    /// Active pins are included before ordinary recent transcript items.
    #[test]
    fn pins_before_transcript() {
        let pin = PinnedCandidate::file(ContextItemKind::PinnedFile, PathBuf::from("/repo/pinned.rs"), ".", 100);
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            pins: vec![pin.clone()],
            transcript: vec![transcript(1, "user", 5_000), transcript(2, "assistant", 5_000)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(3_000));
        let pin_item = ledger.items.iter().find(|i| i.id == pin.id).expect("pin in ledger");
        assert_eq!(pin_item.visibility, ContextVisibility::Pinned);

        let transcript_items: Vec<&ContextItem> = ledger
            .items
            .iter()
            .filter(|i| i.kind == ContextItemKind::Transcript && i.label != "current user turn")
            .collect();
        assert!(
            transcript_items
                .iter()
                .any(|i| i.visibility == ContextVisibility::Archived),
            "transcript must be evicted before pin"
        );
    }

    /// Applicable closest AGENTS.md is included before broader guidance.
    #[test]
    fn closest_agents_md_before_broader() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            instructions: vec![
                instruction(".", true, 200),
                instruction("src", true, 150),
                instruction("src/core", true, 100),
                instruction("docs", false, 100),
            ],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let visible_instructions: Vec<&ContextItem> = ledger
            .items
            .iter()
            .filter(|i| i.kind == ContextItemKind::ProjectInstruction && i.visibility == ContextVisibility::Visible)
            .collect();
        let scopes: Vec<&str> = visible_instructions.iter().map(|i| i.scope.as_str()).collect();
        assert!(scopes.contains(&"src/core"), "closest applicable must be visible");
        assert!(scopes.contains(&"src"));
        assert!(scopes.contains(&"."));
        let docs = ledger
            .items
            .iter()
            .find(|i| i.scope == "docs")
            .expect("docs instruction in ledger");
        assert_eq!(docs.visibility, ContextVisibility::Candidate);
        assert!(docs.reason.contains("candidate"));
    }

    /// The latest compaction summary is included when older turns are omitted.
    #[test]
    fn latest_summary_when_older_turns_omitted() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            compaction_summaries: vec![
                CompactionSummaryCandidate::new("sess_1", 1, 10, 200, false),
                CompactionSummaryCandidate::new("sess_1", 11, 20, 200, true),
            ],
            transcript: vec![transcript(1, "user", 5_000), transcript(2, "assistant", 5_000)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(4_000));
        let summaries: Vec<&ContextItem> = ledger
            .items
            .iter()
            .filter(|i| i.kind == ContextItemKind::Summary)
            .collect();
        let latest = summaries
            .iter()
            .find(|s| s.label == "summary 11..20")
            .expect("latest summary");
        assert_eq!(latest.visibility, ContextVisibility::Visible);
        let older = summaries
            .iter()
            .find(|s| s.label == "summary 1..10")
            .expect("older summary");
        assert_eq!(older.visibility, ContextVisibility::Archived);
    }

    /// Session-scoped memory stays eligible after compaction and resume.
    #[test]
    fn session_memory_survives_compaction_and_resume() {
        let session_item = MemoryItem {
            id: "mem_session_1".to_string(),
            title: "session note".to_string(),
            kind: MemoryKind::Context,
            scope: MemoryScope::Session,
            paths: Vec::new(),
            tags: Vec::new(),
            created: "2026-07-03T00:00:00Z".to_string(),
            updated: "2026-07-03T00:00:00Z".to_string(),
            source: MemorySource::ExplicitUserSession,
            root: MemoryRootKind::User,
            path: PathBuf::new(),
            content_hash: 1,
            byte_count: 100,
            truncated: false,
            body: "keep".to_string(),
        };
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            session_memory: vec![session_item],
            compaction_summaries: vec![CompactionSummaryCandidate::new("sess_1", 1, 10, 200, true)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let session_mem = ledger
            .items
            .iter()
            .find(|i| i.kind == ContextItemKind::UserMemory && i.label == "session note")
            .expect("session memory in ledger");
        assert_eq!(session_mem.visibility, ContextVisibility::Visible);
        assert_eq!(session_mem.scope, "session");
        assert!(session_mem.reason.contains("session-scoped memory"));
    }

    /// UI-only and live-only transcript entries are omitted.
    #[test]
    fn ui_only_and_live_only_transcript_omitted() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            transcript: vec![
                transcript(1, "user", 100),
                TranscriptCandidate::new("sess_1", 2, "status", 100).ui_only(),
                TranscriptCandidate::new("sess_1", 3, "streaming assistant", 100).streaming(),
            ],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let status = ledger
            .items
            .iter()
            .find(|i| i.label == "status")
            .expect("status in ledger");
        assert_eq!(status.visibility, ContextVisibility::Candidate);
        assert!(status.reason.contains("ui-only"));
        let streaming = ledger
            .items
            .iter()
            .find(|i| i.label == "streaming assistant")
            .expect("streaming in ledger");
        assert_eq!(streaming.visibility, ContextVisibility::Candidate);
        assert!(streaming.reason.contains("live-only"));
    }

    /// Every item has a non-empty reason.
    #[test]
    fn every_item_has_a_reason() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            instructions: vec![instruction(".", true, 100), instruction("docs", false, 100)],
            core_memory: vec![memory_item("mem_core", "Core", MemoryRootKind::User, 100)],
            archival_memory: vec![memory_item("mem_arch", "Arch", MemoryRootKind::Project, 100)],
            session_memory: vec![MemoryItem {
                id: "mem_sess".to_string(),
                title: "sess".to_string(),
                kind: MemoryKind::Context,
                scope: MemoryScope::Session,
                paths: Vec::new(),
                tags: Vec::new(),
                created: "2026-07-03T00:00:00Z".to_string(),
                updated: "2026-07-03T00:00:00Z".to_string(),
                source: MemorySource::ExplicitUserSession,
                root: MemoryRootKind::User,
                path: PathBuf::new(),
                content_hash: 1,
                byte_count: 100,
                truncated: false,
                body: "b".to_string(),
            }],
            pins: vec![PinnedCandidate::file(
                ContextItemKind::PinnedFile,
                PathBuf::from("/repo/x.rs"),
                ".",
                100,
            )],
            compaction_summaries: vec![CompactionSummaryCandidate::new("sess_1", 1, 5, 100, true)],
            transcript: vec![transcript(1, "user", 100), transcript(2, "assistant", 100)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        assert!(!ledger.items.is_empty());
        for item in &ledger.items {
            assert!(!item.reason.is_empty(), "item {} must have a reason", item.id);
        }
    }

    #[test]
    fn ledger_excludes_full_content() {
        let secret_body = "api_key=supersecretvalue";
        let item = MemoryItem {
            id: "mem_secret".to_string(),
            title: "creds".to_string(),
            kind: MemoryKind::Context,
            scope: MemoryScope::User,
            paths: Vec::new(),
            tags: Vec::new(),
            created: "2026-07-03T00:00:00Z".to_string(),
            updated: "2026-07-03T00:00:00Z".to_string(),
            source: MemorySource::ExplicitUser,
            root: MemoryRootKind::User,
            path: PathBuf::from("/home/.thndrs/memory/notes/mem_secret.md"),
            content_hash: 42,
            byte_count: secret_body.len(),
            truncated: false,
            body: secret_body.to_string(),
        };
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            core_memory: vec![item],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let dashboard = crate::context::render_model_dashboard(&ledger);
        assert!(!dashboard.contains(secret_body));
        assert!(!dashboard.contains("supersecretvalue"));
        assert!(dashboard.contains("mem_secret") || dashboard.contains("creds"));
    }

    #[test]
    fn clear_context_keeps_memory_and_summaries() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            core_memory: vec![memory_item("mem_core", "Core", MemoryRootKind::User, 100)],
            compaction_summaries: vec![CompactionSummaryCandidate::new("sess_1", 1, 5, 100, true)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        assert!(ledger.items.iter().any(|i| i.kind == ContextItemKind::UserMemory));
        assert!(ledger.items.iter().any(|i| i.kind == ContextItemKind::Summary));
        assert!(!ledger.items.iter().any(|i| i.visibility == ContextVisibility::Pinned));
    }

    /// Recover reopens archived/omitted context by id
    ///
    /// an archived item becomes visible when it is re-pinned (added to pins)
    /// and removed from dropped_ids.
    #[test]
    fn recover_reopens_archived_item() {
        let input1 = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 1_000)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 100)),
            transcript: vec![transcript(1, "old", 5_000)],
            ..Default::default()
        };
        let ledger1 = select_context(&input1, limits(3_000));
        let archived = ledger1
            .items
            .iter()
            .find(|i| i.visibility == ContextVisibility::Archived)
            .expect("an archived entry");
        let archived_id = archived.id.clone();

        let pin = PinnedCandidate {
            id: archived_id.clone(),
            kind: ContextItemKind::Transcript,
            label: "recovered".to_string(),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            bytes: 5_000,
        };
        let input2 = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 5, 100)),
            pins: vec![pin],
            ..Default::default()
        };
        let ledger2 = select_context(&input2, limits(200_000));
        let recovered = ledger2
            .items
            .iter()
            .find(|i| i.id == archived_id)
            .expect("recovered item");
        assert_eq!(recovered.visibility, ContextVisibility::Pinned);
    }

    /// The compaction summary replaces older transcript entries in the
    /// projection when the budget is tight.
    #[test]
    fn compaction_summary_replaces_older_transcript() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 1_000)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 100)),
            compaction_summaries: vec![CompactionSummaryCandidate::new("sess_1", 1, 10, 300, true)],
            transcript: vec![
                transcript(1, "summarized user", 5_000),
                transcript(2, "summarized assistant", 5_000),
                transcript(3, "recent user", 100),
            ],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(4_000));
        let summary = ledger
            .items
            .iter()
            .find(|i| i.kind == ContextItemKind::Summary)
            .expect("summary in ledger");
        assert_eq!(summary.visibility, ContextVisibility::Visible);
        assert!(
            ledger
                .items
                .iter()
                .any(|i| i.kind == ContextItemKind::Transcript && i.visibility == ContextVisibility::Archived),
            "older transcript must be archived when a summary is visible"
        );
    }

    /// Core memory is included before archival memory.
    #[test]
    fn core_memory_before_archival() {
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            core_memory: vec![memory_item("mem_core", "Core", MemoryRootKind::User, 100)],
            archival_memory: vec![memory_item("mem_arch", "Arch", MemoryRootKind::Project, 100)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let positions: Vec<(usize, &ContextItem)> = ledger
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.kind, ContextItemKind::UserMemory | ContextItemKind::ProjectMemory))
            .collect();
        assert_eq!(positions.len(), 2);
        assert!(positions[0].1.kind == ContextItemKind::UserMemory);
        assert!(positions[1].1.kind == ContextItemKind::ProjectMemory);
    }

    /// A discovered skill (metadata only) is a candidate; a loaded skill is visible.
    #[test]
    fn skill_metadata_candidate_loaded_visible() {
        let skill = SkillMetadata {
            name: "test-skill".to_string(),
            description: "a skill".to_string(),
            path: PathBuf::from("/repo/.thndrs/skills/test/SKILL.md"),
            root: PathBuf::from("/repo/.thndrs/skills/test"),
            content_hash: 1,
            byte_count: 100,
            source: crate::skills::SkillSource::Project,
            allowed_tools: Vec::new(),
            license: None,
            compatibility: None,
            metadata: None,
            references: Vec::new(),
        };
        let input = SelectionInput {
            harness: vec![HarnessCandidate::new("base", 100)],
            user_turn: Some(UserTurnCandidate::new("sess_1", 0, 50)),
            skills: vec![SkillCandidate::metadata(&skill), SkillCandidate::loaded(&skill, 200, 2)],
            ..Default::default()
        };

        let ledger = select_context(&input, limits(200_000));
        let skills: Vec<&ContextItem> = ledger
            .items
            .iter()
            .filter(|i| i.kind == ContextItemKind::Skill)
            .collect();
        assert_eq!(skills.len(), 2);
        let loaded = skills
            .iter()
            .find(|s| s.visibility == ContextVisibility::Visible)
            .expect("loaded skill visible");
        assert!(loaded.reason.contains("loaded skill"));
        let discovered = skills
            .iter()
            .find(|s| s.visibility == ContextVisibility::Candidate)
            .expect("discovered skill candidate");
        assert!(discovered.reason.contains("discovered skill metadata"));
    }

    /// Empty input produces an empty (but valid) ledger with no diagnostics.
    #[test]
    fn empty_input_produces_valid_ledger() {
        let input = SelectionInput::default();
        let ledger = select_context(&input, limits(200_000));
        assert!(ledger.items.is_empty());
        assert!(ledger.diagnostics.is_empty());
        assert_eq!(ledger.budget.used, 0);
    }
}
