//! Builds the context ledger from prompt fragments, workspace instructions,
//! skills, pins, compaction summaries, and transcript candidates.
//!
//! Pin, drop, and recovery actions append redacted metadata to the session.
//! Compaction keeps the transcript append-only and applies a provider summary
//! to the model-facing projection only after its audit record is written.
//! Failed or rejected compactions preserve the projection and pending user turn.

use crate::context::export::{ContextExport, ContextExportFormat, ExportArtifact, artifact_from_recovery};
use crate::session::CompactionRisk;

use super::*;

pub const CONTEXT_INSPECTION_MAX_ITEMS: usize = 64;

const CONTEXT_DISPLAY_MAX_BYTES: usize = 160;

/// Pending compaction request with enough information to atomically update the
/// model-facing projection after a successful configured-model response.
///
/// Carries both the manual (`/compact`) and automatic (preflight pressure)
/// paths. For automatic compaction, `original_user_turn` holds the user turn
/// to restart after the summary is applied; for manual compaction it is
/// `None` because `/compact` is a command, not a submitted turn.
#[derive(Clone, Debug)]
pub struct PendingManualCompaction {
    original_transcript: TranscriptBlocks,
    source_transcript: Vec<Entry>,
    covered_start_seq: u64,
    covered_end_seq: u64,
    recovery_handle: String,
    /// Manual or automatic initiation, written to the audit record.
    trigger: session::CompactionTrigger,
    /// The user turn to restart after a successful automatic compaction.
    /// `None` for manual compaction.
    original_user_turn: Option<String>,
    /// Typed source contract that the configured model response must satisfy.
    request: agent_context::RangeCompressionRequest,
    /// Earlier summaries covered by this summary, retained as provenance.
    source_summary_ids: Vec<String>,
}

/// A provider-generated summary waiting for the user to approve or reject its
/// replacement of the active model-facing range.
#[derive(Clone, Debug)]
pub struct PendingCompactionReview {
    pending: PendingManualCompaction,
    summary: agent_context::RangeSummary,
}

impl App {
    /// Recover bounded redacted evidence for a context item or artifact handle.
    ///
    /// The recovery action is appended even when the body is missing or
    /// expired, so the item's audit metadata remains useful across resume.
    pub fn recover_context_evidence(&mut self, reference: &str) -> Result<crate::artifacts::ArtifactRecovery, String> {
        self.ensure_context_ledger();
        let item = self
            .transcript
            .context_ledger
            .as_ref()
            .and_then(|ledger| {
                ledger.items.iter().find(|item| {
                    item.artifact_handle.as_deref() == Some(reference)
                        || item.id == reference
                        || item.id.starts_with(reference)
                })
            })
            .ok_or_else(|| format!("unknown context artifact `{}`", redact_context_display(reference)))?
            .clone();
        let handle = item.artifact_handle.as_deref().ok_or_else(|| {
            format!(
                "context item `{}` has no recoverable artifact",
                redact_context_display(&item.id)
            )
        })?;
        self.recover_artifact_for_item(&item, handle, "user requested bounded artifact recovery")
    }

    /// Propose a verification relation from protected evidence to a candidate
    /// result. The proposal is durable and changes neither lifecycle nor
    /// protection until the user reviews it.
    pub fn propose_context_verification(
        &mut self, evidence_reference: &str, candidate_reference: &str,
    ) -> Result<String, String> {
        self.ensure_context_ledger();
        let evidence = self
            .context_item(evidence_reference)?
            .ok_or_else(|| {
                format!(
                    "unknown protected context item `{}`",
                    redact_context_display(evidence_reference)
                )
            })?
            .clone();
        let candidate = self
            .context_item(candidate_reference)?
            .ok_or_else(|| {
                format!(
                    "unknown verification candidate `{}`",
                    redact_context_display(candidate_reference)
                )
            })?
            .clone();
        if evidence.id == candidate.id {
            return Err("verification evidence and candidate must be different context items".to_string());
        }
        if !evidence.lifecycle.is_protected() {
            return Err(format!(
                "context item `{}` has no active protection to verify",
                redact_context_display(&evidence.id)
            ));
        }
        let relation_id = agent_context::relation_id_for(
            agent_context::ContextRelationKind::VerifiedBy,
            &evidence.id,
            &candidate.id,
        );
        let relation = agent_context::ContextRelation::proposed_verification(
            relation_id.clone(),
            evidence.id.clone(),
            candidate.id,
        );
        let next_lifecycle = evidence
            .lifecycle
            .apply(agent_context::ContextLifecycleAction::ProposeVerification { relation: relation.clone() })
            .map_err(|error| error.to_string())?;
        self.append_lifecycle_transition(
            &evidence,
            next_lifecycle,
            agent_context::ContextLifecycleAction::ProposeVerification { relation },
            "agent proposed verification relation",
        )?;
        Ok(relation_id)
    }

    /// Approve a proposed verification relation without releasing protection.
    pub fn approve_context_verification(&mut self, reference: &str) -> Result<(), String> {
        self.apply_verification_review(reference, false)
    }

    /// Reject a proposed verification relation while retaining protection.
    pub fn reject_context_verification(&mut self, reference: &str) -> Result<(), String> {
        self.apply_verification_review(reference, true)
    }

    /// Release protected evidence through an approved verification relation.
    pub fn release_context_verification(&mut self, reference: &str) -> Result<(), String> {
        let Some((item, relation)) = self.context_relation_owner(reference)? else {
            return Err(format!(
                "unknown verification relation `{}`",
                redact_context_display(reference)
            ));
        };
        if relation.status != agent_context::ContextRelationStatus::Approved {
            return Err(format!(
                "verification relation `{}` must be approved before release",
                redact_context_display(&relation.id)
            ));
        }
        let action = agent_context::ContextLifecycleAction::Release { relation_id: Some(relation.id) };
        let lifecycle = item
            .lifecycle
            .apply(action.clone())
            .map_err(|error| error.to_string())?;
        self.append_lifecycle_transition(&item, lifecycle, action, "user released verified context evidence")
    }

    /// Explicitly release protection for an item without claiming that any
    /// command or assistant statement verified it.
    pub fn release_context_item(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let item = self
            .context_item(reference)?
            .ok_or_else(|| format!("unknown context item `{}`", redact_context_display(reference)))?
            .clone();
        let action = agent_context::ContextLifecycleAction::Release { relation_id: None };
        let lifecycle = item
            .lifecycle
            .apply(action.clone())
            .map_err(|error| error.to_string())?;
        self.append_lifecycle_transition(&item, lifecycle, action, "user explicitly released context protection")
    }

    /// Rebuild the deterministic context ledger for a turn boundary.
    ///
    /// The caller owns discovery, transcript projection, and persistence. The
    /// agent library receives only typed candidates and returns the policy
    /// result. This method also stores the latest ledger for bounded inspection.
    pub fn refresh_context_ledger(&mut self, user_turn: Option<&str>) -> agent_context::ContextLedger {
        let pinned_paths = self
            .transcript
            .context_pins
            .iter()
            .filter_map(|pin| pin.source_path.clone())
            .collect::<Vec<_>>();
        let instruction_selection =
            crate::context::select_instructions(&self.transcript.context_sources, &[], &pinned_paths);
        let applicable_paths = instruction_selection
            .applicable
            .iter()
            .map(|source| source.path.clone())
            .collect::<std::collections::HashSet<_>>();

        let mut harness = prompt::default_fragments()
            .into_iter()
            .map(|fragment| {
                let candidate = HarnessCandidate::new(fragment.name, fragment.content.len());
                if fragment.name == "action_safety" {
                    candidate.with_protection(agent_context::ContextProtection::from_reason(
                        agent_context::ContextProtectionReason::SafetyState,
                    ))
                } else {
                    candidate
                }
            })
            .collect::<Vec<_>>();
        harness.push(HarnessCandidate::new(
            "tool_catalog",
            tools::tool_definitions()
                .into_iter()
                .map(|tool| tool.name.len() + tool.description.len())
                .sum(),
        ));

        let instructions = self
            .transcript
            .context_sources
            .iter()
            .map(|source| InstructionCandidate {
                path: source.path.clone(),
                scope: source.scope.clone(),
                content_hash: source.content_hash,
                byte_count: source.byte_count,
                content: Some(source.content.clone()),
                truncated: source.truncated,
                applicable: applicable_paths.contains(&source.path),
            })
            .collect();
        let skills = self
            .transcript
            .skills
            .iter()
            .map(|skill| {
                SkillCandidate::discovered(&skill.name, skill.path.clone(), skill.content_hash, skill.byte_count)
            })
            .collect();
        let transcript = self
            .transcript
            .entries
            .iter()
            .filter(|entry| is_model_context_entry(entry))
            .enumerate()
            .map(|(index, entry)| TranscriptCandidate {
                seq: index as u64 + 1,
                session_id: self.session.context_id_namespace.clone(),
                label: transcript_candidate_label(entry),
                bytes: transcript_candidate_bytes(entry),
                artifact_handle: match entry {
                    Entry::Tool { name, .. } => name
                        .rsplit_once('#')
                        .and_then(|(_, id)| self.transcript.tool_artifacts.get(id))
                        .cloned(),
                    _ => None,
                },
                ui_only: false,
                streaming: matches!(
                    entry,
                    Entry::Agent { streaming: true, .. } | Entry::Reasoning { streaming: true, .. }
                ),
                protection: transcript_protection(entry),
            })
            .collect();
        let pending_permissions = self
            .overlay
            .permission()
            .as_ref()
            .map(|permission| {
                PendingPermissionCandidate::new(
                    format!("ctx_permission_{}", permission.tool_call_id),
                    format!("pending permission: {}", redact_context_display(&permission.title)),
                    permission.title.len()
                        + permission
                            .options
                            .iter()
                            .map(|option| option.id.len() + option.name.len())
                            .sum::<usize>(),
                )
            })
            .into_iter()
            .collect();
        let selection_input = SelectionInput {
            harness,
            user_turn: user_turn.map(|text| {
                UserTurnCandidate::new(
                    &self.session.context_id_namespace,
                    context_entry_count(&self.transcript.entries) + 1,
                    text.len(),
                )
            }),
            pending_permissions,
            instructions,
            pins: self.transcript.context_pins.clone(),
            compaction_summaries: self.transcript.compaction_summaries.clone(),
            transcript,
            skills,
            dropped_ids: self.transcript.context_dropped_ids.clone(),
        };

        let provider = provider_label(&self.runtime.model);
        let (limits, mut diagnostics) =
            agent_context::ModelContextLimits::resolve(provider, &self.runtime.model, None, None);
        let mut ledger = agent_context::select_context(&selection_input, limits);
        if let Some(accounting) = self.session.last_request_accounting.as_ref()
            && request_context_is_retained(&ledger, accounting)
            && let Some(used) = super::agent_lifecycle::observed_context_usage(accounting)
        {
            ledger.budget.used = ledger.budget.used.max(used);
        }
        ledger.budget.auto_compaction_threshold = self
            .effective_compaction_policy()
            .auto_compaction_threshold(ledger.budget.available_input);
        for item in &mut ledger.items {
            if let Some(mut lifecycle) = self.transcript.context_lifecycles.get(&item.id).cloned() {
                lifecycle.merge_derived_protection(&item.lifecycle.protection);
                item.lifecycle = lifecycle.clone();
                self.transcript.context_lifecycles.insert(item.id.clone(), lifecycle);
            } else {
                self.transcript
                    .context_lifecycles
                    .insert(item.id.clone(), item.lifecycle.clone());
            }
        }
        self.apply_tool_projection_relations(&mut ledger);
        diagnostics.extend(self.transcript.context_diagnostics.iter().map(|diagnostic| {
            agent_context::ContextDiagnostic {
                severity: match diagnostic.severity {
                    crate::context::InstructionSeverity::Info => agent_context::DiagnosticSeverity::Info,
                    crate::context::InstructionSeverity::Warning => agent_context::DiagnosticSeverity::Warning,
                    crate::context::InstructionSeverity::Error => agent_context::DiagnosticSeverity::Error,
                },
                code: "instruction_discovery".to_string(),
                message: diagnostic.summary(),
            }
        }));
        ledger.diagnostics.extend(diagnostics);
        self.transcript.context_ledger = Some(ledger.clone());
        ledger
    }

    /// Open the bounded context inspection surface.
    pub fn open_context_surface(&mut self) {
        self.refresh_context_ledger(None);
        self.overlay.show_context();
        self.composer.input.clear();
    }

    /// Build the current bounded context export.
    pub fn build_context_export(&mut self, include_artifacts: bool) -> ContextExport {
        self.ensure_context_ledger();
        let ledger = self
            .transcript
            .context_ledger
            .clone()
            .unwrap_or_else(|| self.refresh_context_ledger(None));
        let content_permitted = self.session.context_capture_policy.permits_content();
        let include_artifacts = include_artifacts && content_permitted;
        let mut artifacts = Vec::new();
        let store = self.artifact_store();
        let mut handles = std::collections::BTreeSet::new();
        for item in &ledger.items {
            let Some(handle) = item.artifact_handle.as_deref() else { continue };
            if !handles.insert(handle.to_string()) {
                continue;
            }
            let artifact = match store.as_ref().and_then(|store| store.recover(handle).ok()) {
                Some(recovery) => artifact_from_recovery(recovery, include_artifacts),
                None => ExportArtifact {
                    handle: handle.to_string(),
                    metadata: None,
                    body: None,
                    diagnostic: Some("artifact metadata is unavailable".to_string()),
                },
            };
            artifacts.push(artifact);
        }
        let diagnostics = ledger
            .diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect();
        let mut accounting = self.session.last_request_accounting.clone();
        if !content_permitted && let Some(accounting) = accounting.as_mut() {
            accounting.model_projection.clear();
        }
        ContextExport::from_parts(self.session.id.clone(), &ledger, accounting, artifacts, diagnostics)
    }

    /// Render and atomically write a bounded context export.
    pub fn write_context_export(
        &mut self, path: &Path, format: ContextExportFormat, include_artifacts: bool,
    ) -> Result<(), String> {
        if include_artifacts && !self.session.context_capture_policy.permits_content() {
            return Err("artifact bodies require --capture-context-content when the session starts".to_string());
        }
        let export = self.build_context_export(include_artifacts);
        let content = match format {
            ContextExportFormat::Json => export
                .to_json()
                .map_err(|error| format!("failed to encode export: {error}"))?,
            ContextExportFormat::Markdown => export.to_markdown(),
        };
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("context-export");
        let temporary = parent.join(format!(".{file_name}.thndrs-export-{}", std::process::id()));
        if let Err(error) = std::fs::write(&temporary, content.as_bytes()) {
            return Err(format!("cannot write context export: {error}"));
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("cannot finalize context export: {error}"));
        }
        Ok(())
    }

    fn pin_context_reference(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let (candidate, item) = if let Some(item) = self.context_item(reference)? {
            if item.kind == ContextItemKind::Harness {
                return Err("harness context is always loaded and cannot be pinned".to_string());
            }
            (
                PinnedCandidate {
                    id: item.id.clone(),
                    kind: item.kind.clone(),
                    label: item.label.clone(),
                    source_path: item.source_path.clone(),
                    scope: item.scope.clone(),
                    content_hash: item.content_hash,
                    artifact_handle: item.artifact_handle.clone(),
                    bytes: item.byte_count,
                },
                item.clone(),
            )
        } else {
            let path = self.resolve_context_path(reference)?;
            let candidate = PinnedCandidate::file(ContextItemKind::PinnedFile, path.clone(), ".", file_size(&path));
            let item = agent_context::ContextItem {
                id: candidate.id.clone(),
                kind: candidate.kind.clone(),
                label: candidate.label.clone(),
                source_path: candidate.source_path.clone(),
                scope: candidate.scope.clone(),
                content_hash: candidate.content_hash,
                artifact_handle: candidate.artifact_handle.clone(),
                byte_count: candidate.bytes,
                content: None,
                token_estimate: agent_context::estimate_tokens(candidate.bytes),
                visibility: ContextVisibility::Pinned,
                reason_code: "user_pin".to_string(),
                reason: "user pin".to_string(),
                lifecycle: agent_context::ContextLifecycle::new(agent_context::ContextProtection::from_reason(
                    agent_context::ContextProtectionReason::UserPin,
                )),
            };
            (candidate, item)
        };
        if self.transcript.context_pins.iter().any(|pin| pin.id == candidate.id) {
            return Err(format!(
                "context item `{}` is already pinned",
                redact_context_display(&candidate.id)
            ));
        }
        if let Some(writer) = self.session.writer.as_mut() {
            writer
                .append_context_pin(&item, "user pinned context item")
                .map_err(|error| format!("failed to record context pin: {error}"))?;
        }
        self.transcript.context_pins.push(candidate);
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn drop_context_reference(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let item = self
            .context_item(reference)?
            .ok_or_else(|| format!("unknown context item `{}`", redact_context_display(reference)))?
            .clone();
        if item.kind == ContextItemKind::Harness {
            return Err("harness context cannot be dropped".to_string());
        }
        if self.transcript.context_dropped_ids.iter().any(|id| id == &item.id) {
            return Err(format!(
                "context item `{}` is already dropped",
                redact_context_display(&item.id)
            ));
        }
        if let Some(writer) = self.session.writer.as_mut() {
            writer
                .append_context_drop(&item, "user dropped context item")
                .map_err(|error| format!("failed to record context drop: {error}"))?;
        }
        self.transcript.context_dropped_ids.push(item.id);
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn recover_context_reference(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let item = self
            .context_item(reference)?
            .ok_or_else(|| format!("unknown context item `{}`", redact_context_display(reference)))?
            .clone();
        if item.kind == ContextItemKind::Harness {
            return Err("harness context is always available and needs no recovery".to_string());
        }
        let was_dropped = self.transcript.context_dropped_ids.iter().any(|id| id == &item.id);
        let needs_pin = !item.visibility.is_rendered();
        let artifact_recovered = if let Some(handle) = item.artifact_handle.as_deref() {
            let recovery = self.recover_artifact_for_item(&item, handle, "user requested bounded artifact recovery")?;
            if let Some(diagnostic) = recovery.diagnostic {
                return Err(diagnostic.message);
            }
            true
        } else {
            false
        };
        if !was_dropped && !needs_pin && !artifact_recovered {
            return Err(format!(
                "context item `{}` is already active",
                redact_context_display(&item.id)
            ));
        }
        if !artifact_recovered {
            let recovery_target = format!("session:{}", item.id);
            let relation = agent_context::ContextRelation::applied(
                agent_context::relation_id_for(
                    agent_context::ContextRelationKind::RecoveredFrom,
                    &item.id,
                    &recovery_target,
                ),
                agent_context::ContextRelationKind::RecoveredFrom,
                item.id.clone(),
                recovery_target,
            );
            let lifecycle = item
                .lifecycle
                .apply(agent_context::ContextLifecycleAction::Recover { relation })
                .map_err(|error| error.to_string())?;
            let mut next_item = item.clone();
            next_item.lifecycle = lifecycle.clone();
            if let Some(writer) = self.session.writer.as_mut() {
                writer
                    .append_context_recovery(&next_item, "user recovered context item")
                    .map_err(|error| format!("failed to record context recovery: {error}"))?;
            }
            self.record_live_context_recovery(&next_item, "user recovered context item");
            self.transcript.context_lifecycles.insert(item.id.clone(), lifecycle);
        }
        self.transcript.context_dropped_ids.retain(|id| id != &item.id);
        if needs_pin && !self.transcript.context_pins.iter().any(|pin| pin.id == item.id) {
            self.transcript.context_pins.push(PinnedCandidate {
                id: item.id.clone(),
                kind: item.kind.clone(),
                label: item.label.clone(),
                source_path: item.source_path.clone(),
                scope: item.scope.clone(),
                content_hash: item.content_hash,
                artifact_handle: item.artifact_handle.clone(),
                bytes: item.byte_count,
            });
        }
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn recover_artifact_for_item(
        &mut self, item: &agent_context::ContextItem, handle: &str, reason: &str,
    ) -> Result<crate::artifacts::ArtifactRecovery, String> {
        let recovery = self
            .artifact_store()
            .ok_or_else(|| String::from("ephemeral runs do not retain recoverable context artifacts"))?
            .recover(handle)
            .map_err(|_| format!("artifact `{}` metadata is unavailable", redact_context_display(handle)))?;
        let relation_id =
            agent_context::relation_id_for(agent_context::ContextRelationKind::RecoveredFrom, &item.id, handle);
        let lifecycle = item
            .lifecycle
            .apply(agent_context::ContextLifecycleAction::Recover {
                relation: agent_context::ContextRelation::applied(
                    relation_id,
                    agent_context::ContextRelationKind::RecoveredFrom,
                    item.id.clone(),
                    handle,
                ),
            })
            .map_err(|error| error.to_string())?;
        let mut next_item = item.clone();
        next_item.lifecycle = lifecycle.clone();
        if let Some(writer) = self.session.writer.as_mut() {
            writer
                .append_context_recovery(&next_item, reason)
                .map_err(|error| format!("failed to record context recovery: {error}"))?;
        }
        self.record_live_context_recovery(&next_item, reason);
        self.transcript.context_lifecycles.insert(item.id.clone(), lifecycle);
        Ok(recovery)
    }

    fn record_live_context_recovery(&mut self, item: &agent_context::ContextItem, reason: &str) {
        let item = session::ContextItemMeta::from(item);
        let action = self.transcript.entries.len();
        self.transcript.entries.push_context_event(
            format!("context:recovery:live:{}:{action}", item.id),
            session::ContextHistory::live_recovery_event(&item, reason),
        );
        self.transcript.context_history.record_recovery(item, reason);
    }

    fn reset_context_drops(&mut self) -> Result<(), String> {
        if self.transcript.context_dropped_ids.is_empty() {
            return Err("no dropped context items to reset".to_string());
        }
        self.transcript.context_dropped_ids.clear();
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn ensure_context_ledger(&mut self) {
        if self.transcript.context_ledger.is_none() {
            self.refresh_context_ledger(None);
        }
    }

    fn apply_tool_projection_relations(&mut self, ledger: &mut agent_context::ContextLedger) {
        let tool_items = self
            .transcript
            .entries
            .iter()
            .filter(|entry| is_model_context_entry(entry))
            .enumerate()
            .filter_map(|(index, entry)| match entry {
                Entry::Tool { name, .. } => name.rsplit_once('#').map(|(_, call_id)| {
                    (
                        format!("tool:{call_id}"),
                        (
                            index,
                            agent_context::item_id_for_session_range(
                                &ContextItemKind::Transcript,
                                &self.session.context_id_namespace,
                                index as u64 + 1,
                                index as u64 + 1,
                            ),
                        ),
                    )
                }),
                _ => None,
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut decisions = self
            .transcript
            .tool_projection_decisions
            .iter()
            .map(|(call_id, decision)| (call_id.clone(), decision.clone()))
            .collect::<Vec<_>>();
        decisions.sort_by_key(|(call_id, _)| {
            tool_items
                .get(&format!("tool:{call_id}"))
                .map(|(index, _)| *index)
                .unwrap_or(usize::MAX)
        });

        for (call_id, decision) in decisions {
            let current = format!("tool:{call_id}");
            let relation = match decision {
                agent_context::StateProjectionDecision::Retained => continue,
                agent_context::StateProjectionDecision::DuplicateOf { canonical_id } => {
                    (current, canonical_id, agent_context::ContextRelationKind::DuplicateOf)
                }
                agent_context::StateProjectionDecision::Supersedes { previous_id } => {
                    (previous_id, current, agent_context::ContextRelationKind::SupersededBy)
                }
            };
            let (source_call, target_call, kind) = relation;
            let (Some((_, source_id)), Some((_, target_id))) =
                (tool_items.get(&source_call), tool_items.get(&target_call))
            else {
                continue;
            };
            self.apply_tool_projection_relation(ledger, source_id, target_id, kind);
        }
    }

    fn apply_tool_projection_relation(
        &mut self, ledger: &mut agent_context::ContextLedger, source_id: &str, target_id: &str,
        kind: agent_context::ContextRelationKind,
    ) {
        let Some(index) = ledger.items.iter().position(|item| item.id == source_id) else {
            return;
        };
        let item = ledger.items[index].clone();
        if item
            .lifecycle
            .relations
            .iter()
            .any(|relation| relation.kind == kind && relation.target_id == target_id)
        {
            return;
        }
        let relation = agent_context::ContextRelation::applied(
            agent_context::relation_id_for(kind, source_id, target_id),
            kind,
            source_id,
            target_id,
        );
        let action = match kind {
            agent_context::ContextRelationKind::DuplicateOf => {
                agent_context::ContextLifecycleAction::Duplicate { relation }
            }
            agent_context::ContextRelationKind::SupersededBy => {
                agent_context::ContextLifecycleAction::Supersede { relation }
            }
            _ => return,
        };
        let Ok(lifecycle) = item.lifecycle.apply(action.clone()) else {
            ledger.diagnostics.push(agent_context::ContextDiagnostic {
                severity: agent_context::DiagnosticSeverity::Warning,
                code: "state_identity_lifecycle_rejected".to_string(),
                message: format!(
                    "state-aware {} relation could not be applied to {source_id}",
                    kind.label()
                ),
            });
            return;
        };
        let mut next_item = item;
        next_item.lifecycle = lifecycle.clone();
        if let Some(writer) = self.session.writer.as_mut()
            && writer
                .append_context_lifecycle(&next_item, action, "state-aware model projection relation")
                .is_err()
        {
            ledger.diagnostics.push(agent_context::ContextDiagnostic {
                severity: agent_context::DiagnosticSeverity::Warning,
                code: "state_identity_lifecycle_not_persisted".to_string(),
                message: format!(
                    "state-aware {} relation was not persisted for {source_id}",
                    kind.label()
                ),
            });
            return;
        }
        self.transcript
            .context_lifecycles
            .insert(source_id.to_string(), lifecycle.clone());
        ledger.items[index].lifecycle = lifecycle;
    }

    pub fn restore_context_state(&mut self, records: &[session::SessionRecord]) {
        self.transcript.context_pins.clear();
        self.transcript.context_dropped_ids.clear();
        self.transcript.compaction_summaries.clear();
        self.transcript.tool_artifacts.clear();
        self.transcript.tool_projection_decisions.clear();
        self.transcript.context_lifecycles.clear();
        self.transcript.context_history = session::ContextHistory::from_records(records);
        self.transcript.last_compaction_review = None;
        for record in records {
            match record {
                session::SessionRecord::ContextLedger { ledger, .. } => {
                    for item in &ledger.items {
                        if item.lifecycle != agent_context::ContextLifecycle::default() {
                            self.transcript
                                .context_lifecycles
                                .insert(item.id.clone(), item.lifecycle.clone());
                        }
                    }
                }
                session::SessionRecord::ContextSnapshot { snapshot, .. } => {
                    for item in &snapshot.ledger.items {
                        if item.lifecycle != agent_context::ContextLifecycle::default() {
                            self.transcript
                                .context_lifecycles
                                .insert(item.id.clone(), item.lifecycle.clone());
                        }
                    }
                }
                session::SessionRecord::ContextPin { item, .. } => {
                    if item.lifecycle != agent_context::ContextLifecycle::default() {
                        self.transcript
                            .context_lifecycles
                            .insert(item.id.clone(), item.lifecycle.clone());
                    }
                    if item.kind != ContextItemKind::Harness
                        && !self.transcript.context_pins.iter().any(|pin| pin.id == item.id)
                    {
                        self.transcript.context_pins.push(pinned_candidate_from_meta(item));
                    }
                }
                session::SessionRecord::ContextDrop { item, .. } => {
                    if item.lifecycle != agent_context::ContextLifecycle::default() {
                        self.transcript
                            .context_lifecycles
                            .insert(item.id.clone(), item.lifecycle.clone());
                    }
                    if !self.transcript.context_dropped_ids.iter().any(|id| id == &item.id) {
                        self.transcript.context_dropped_ids.push(item.id.clone());
                    }
                }
                session::SessionRecord::ContextRecovery { item, .. } => {
                    if item.lifecycle != agent_context::ContextLifecycle::default() {
                        self.transcript
                            .context_lifecycles
                            .insert(item.id.clone(), item.lifecycle.clone());
                    }
                    self.transcript.context_dropped_ids.retain(|id| id != &item.id);
                    if item.kind != ContextItemKind::Harness
                        && !item.visibility.is_rendered()
                        && !self.transcript.context_pins.iter().any(|pin| pin.id == item.id)
                    {
                        self.transcript.context_pins.push(pinned_candidate_from_meta(item));
                    }
                }
                session::SessionRecord::ContextLifecycle { audit, .. } => {
                    self.transcript
                        .context_lifecycles
                        .insert(audit.item.id.clone(), audit.item.lifecycle.clone());
                }
                session::SessionRecord::ToolFinished { call_id, artifact: Some(artifact), .. } => {
                    self.transcript
                        .tool_artifacts
                        .insert(call_id.clone(), artifact.handle.clone());
                }
                session::SessionRecord::Compaction { audit, .. } => {
                    for candidate in &mut self.transcript.compaction_summaries {
                        candidate.latest = false;
                    }
                    let mut candidate = CompactionSummaryCandidate::new(
                        &self.session.context_id_namespace,
                        audit.covered_start_seq,
                        audit.covered_end_seq,
                        audit.summary.len(),
                        true,
                    );
                    candidate.content = Some(audit.summary.clone());
                    self.transcript.compaction_summaries.push(candidate);
                    self.transcript.last_compaction_review = audit.review;
                }
                session::SessionRecord::CompactionReview { review, .. } => {
                    self.transcript.last_compaction_review = Some(*review);
                }
                _ => {}
            }
        }
        self.transcript.context_ledger = None;
    }

    fn context_item(&self, reference: &str) -> Result<Option<&agent_context::ContextItem>, String> {
        let Some(ledger) = &self.transcript.context_ledger else {
            return Ok(None);
        };
        let matches = ledger
            .items
            .iter()
            .filter(|item| item.id == reference || item.id.starts_with(reference))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Ok(None),
            [item] => Ok(Some(item)),
            _ => Err(format!("context reference `{reference}` is ambiguous")),
        }
    }

    fn apply_verification_review(&mut self, reference: &str, reject: bool) -> Result<(), String> {
        let Some((item, relation)) = self.context_relation_owner(reference)? else {
            return Err(format!(
                "unknown verification relation `{}`",
                redact_context_display(reference)
            ));
        };
        let action = if reject {
            agent_context::ContextLifecycleAction::RejectVerification { relation_id: relation.id }
        } else {
            agent_context::ContextLifecycleAction::ApproveVerification { relation_id: relation.id }
        };
        let lifecycle = item
            .lifecycle
            .apply(action.clone())
            .map_err(|error| error.to_string())?;
        let reason = if reject { "user rejected verification relation" } else { "user approved verification relation" };
        self.append_lifecycle_transition(&item, lifecycle, action, reason)
    }

    fn context_relation_owner(
        &mut self, reference: &str,
    ) -> Result<Option<(agent_context::ContextItem, agent_context::ContextRelation)>, String> {
        self.ensure_context_ledger();
        let Some(ledger) = &self.transcript.context_ledger else { return Ok(None) };
        let mut matches = Vec::new();
        for item in &ledger.items {
            for relation in item.lifecycle.verification_relations() {
                if relation.id == reference || relation.id.starts_with(reference) {
                    matches.push((item.clone(), relation.clone()));
                }
            }
        }
        match matches.as_slice() {
            [] => Ok(None),
            [match_] => Ok(Some(match_.clone())),
            _ => Err(format!("verification relation `{reference}` is ambiguous")),
        }
    }

    fn append_lifecycle_transition(
        &mut self, item: &agent_context::ContextItem, lifecycle: agent_context::ContextLifecycle,
        action: agent_context::ContextLifecycleAction, reason: &str,
    ) -> Result<(), String> {
        let mut next_item = item.clone();
        next_item.lifecycle = lifecycle.clone();
        if let Some(writer) = self.session.writer.as_mut() {
            writer
                .append_context_lifecycle(&next_item, action, reason)
                .map_err(|error| format!("failed to record context lifecycle action: {error}"))?;
        }
        self.transcript.context_lifecycles.insert(item.id.clone(), lifecycle);
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn resolve_context_path(&self, value: &str) -> Result<PathBuf, String> {
        let path = Path::new(value);
        let path = if path.is_absolute() { path.to_path_buf() } else { self.runtime.cwd.join(path) };
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("cannot pin `{}`: {error}", redact_context_display(value)))?;
        if !canonical.starts_with(&self.runtime.cwd) {
            return Err("context pins must stay inside the workspace".to_string());
        }
        if !canonical.is_file() {
            return Err(format!("context pin is not a file: {}", redact_context_display(value)));
        }
        Ok(canonical)
    }

    pub fn compaction_mode_label(&self) -> &'static str {
        self.effective_compaction_policy().mode.label()
    }
}

/// Start an automatic compaction triggered by preflight context pressure.
///
/// `original_user_turn` is the user turn that was about to be sent to the
/// provider; it is restarted after the summary is applied. The user turn is
/// already in the transcript, so the covered range starts from sequence 1.
///
/// Returns `None` (without spawning) when compaction cannot start, leaving
/// the submitted turn recoverable.
pub fn start_auto_compaction(app: &mut App, original_user_turn: String) -> Option<Msg> {
    start_compaction(app, session::CompactionTrigger::Automatic, Some(original_user_turn))
}

/// Shared core for manual and automatic compaction.
///
/// Saves the active transcript, builds a configured-model summary request,
/// starts it as an internal turn, and records enough state to atomically update
/// the model-facing projection on success or preserve it on failure.
pub fn start_compaction(
    app: &mut App, trigger: session::CompactionTrigger, original_user_turn: Option<String>,
) -> Option<Msg> {
    let original_transcript = app.transcript.entries.clone();
    let policy = app.effective_compaction_policy();
    let latest_summary = app
        .transcript
        .compaction_summaries
        .iter()
        .find(|summary| summary.latest);
    let previous_end_seq = latest_summary.map_or(0, |summary| summary.end_seq);
    let previous_end = raw_index_after_context_entries(&original_transcript, previous_end_seq);
    let maximum_end = original_user_turn
        .as_deref()
        .and_then(|turn| {
            original_transcript
                .iter()
                .rposition(|entry| matches!(entry, Entry::User { text } if text == turn))
        })
        .unwrap_or(original_transcript.len());
    let covered_end = compaction_cut(&original_transcript, maximum_end, policy.keep_recent_tokens);
    if covered_end <= previous_end {
        app.transcript
            .entries
            .push(Entry::Status { text: "there is no closed history old enough to compact".to_string() });
        if trigger == session::CompactionTrigger::Automatic
            && let Some(turn) = original_user_turn
        {
            app.composer.last_input = Some(turn);
        }
        return None;
    }
    let source_transcript = original_transcript[previous_end..covered_end]
        .iter()
        .filter(|entry| is_model_context_entry(entry))
        .cloned()
        .collect::<Vec<_>>();
    let mut source_parts = Vec::new();
    if let Some(previous) = latest_summary.and_then(|summary| summary.content.as_deref()) {
        source_parts.push(format!("previous anchored summary:\n{previous}"));
    }
    source_parts.push(render_compaction_source(&source_transcript));
    let source = source_parts.join("\n\n");
    let covered_start_seq = latest_summary.map_or(1, |summary| summary.start_seq);
    let covered_end_seq = context_entry_count(&original_transcript[..covered_end]);
    let source_start_seq = previous_end_seq + 1;
    let recovery_handle = format!("session:{}:{covered_start_seq}..{covered_end_seq}", app.session.id);
    let (sources, protected_facts) = range_sources(
        &app.session.context_id_namespace,
        &app.session.id,
        source_start_seq,
        &source_transcript,
    );
    let source_summary_ids = source_summary_ids(&app.transcript.compaction_summaries);
    let request = match agent_context::prepare_range_compression(
        policy,
        &app.runtime.model,
        agent_context::RangeCompressionInput {
            start_seq: source_start_seq,
            end_seq: covered_end_seq,
            focus: "preserve the task objective, decisions, failures, verification, and blockers for continuation"
                .to_string(),
            sources,
            protected_facts,
            source_summary_ids: source_summary_ids.clone(),
            source_text: source,
        },
    ) {
        Ok(request) => request,
        Err(message) => {
            app.transcript.entries.push(Entry::Error { text: message });
            if trigger == session::CompactionTrigger::Automatic
                && let Some(turn) = original_user_turn
            {
                app.composer.last_input = Some(turn);
            }
            return None;
        }
    };

    let started = match super::input::submit_internal_turn(app, request.prompt.clone()) {
        Some(msg) => msg,
        None => {
            app.transcript.entries = original_transcript;
            if trigger == session::CompactionTrigger::Automatic
                && let Some(turn) = original_user_turn
            {
                app.composer.last_input = Some(turn);
            }
            return None;
        }
    };
    app.transcript.pending_manual_compaction = Some(PendingManualCompaction {
        original_transcript,
        source_transcript,
        covered_start_seq,
        covered_end_seq,
        recovery_handle,
        trigger,
        original_user_turn,
        request,
        source_summary_ids,
    });
    Some(started)
}

// FIXME: wrapping an option in an option is awful
pub fn finish_manual_compaction(app: &mut App) -> Option<Option<Msg>> {
    let pending = app.transcript.pending_manual_compaction.take()?;
    let summary = app.transcript.entries.iter().rev().find_map(|entry| match entry {
        Entry::Agent { text, .. } if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    });
    let Some(summary) = summary else {
        restore_failed_compaction(app, pending);
        app.transcript
            .entries
            .push(Entry::Error { text: "compaction model returned no summary".to_string() });
        return Some(None);
    };
    let summary = match agent_context::validate_range_summary(&pending.request, &summary) {
        Ok(summary) => summary,
        Err(error) => {
            restore_failed_compaction(app, pending);
            app.transcript.entries.push(Entry::Error { text: error.to_string() });
            return Some(None);
        }
    };
    let risk = classify_compaction_risk(&pending.source_transcript);
    let review = if app.effective_compaction_policy().requires_review(match risk {
        session::CompactionRisk::Low => agent_context::CompactionRisk::Low,
        session::CompactionRisk::High => agent_context::CompactionRisk::High,
    }) {
        session::CompactionReviewResult::Pending
    } else {
        session::CompactionReviewResult::NotRequired
    };
    match review {
        session::CompactionReviewResult::Pending => {
            let recovery_handle = pending.recovery_handle.clone();
            let saved_pending = pending.clone();
            restore_failed_compaction(app, saved_pending);
            app.transcript.pending_compaction_review = Some(PendingCompactionReview { pending, summary });
            app.transcript.last_compaction_review = Some(review);
            app.transcript
                .entries
                .push(Entry::Status { text: format!("compaction review pending  {recovery_handle}") });
            Some(None)
        }
        _ => {
            app.transcript.last_compaction_review = Some(review);
            apply_compaction(app, pending, &summary, review)
        }
    }
}

/// Apply an approved or review-free summary to the model-facing working set.
/// FIXME: wrapping an option in an option is awful
pub fn apply_compaction(
    app: &mut App, pending: PendingManualCompaction, summary: &agent_context::RangeSummary,
    review: session::CompactionReviewResult,
) -> Option<Option<Msg>> {
    let is_automatic = pending.trigger == session::CompactionTrigger::Automatic;
    let original_user_turn = pending.original_user_turn.clone();
    let rendered_summary = summary.render_model_text();
    let audit = compaction_audit(
        &app.session.context_id_namespace,
        &pending,
        summary,
        &rendered_summary,
        review,
    );
    if let Some(writer) = app.session.writer.as_mut()
        && let Err(error) = writer.append_compaction(&audit)
    {
        restore_failed_compaction(app, pending);
        app.transcript
            .entries
            .push(Entry::Error { text: format!("failed to record approved compaction audit: {error}") });
        return Some(None);
    }
    app.transcript.context_history.record_compaction(audit.clone());
    for candidate in &mut app.transcript.compaction_summaries {
        candidate.latest = false;
    }
    let mut summary_candidate = CompactionSummaryCandidate::new(
        &app.session.context_id_namespace,
        pending.covered_start_seq,
        pending.covered_end_seq,
        rendered_summary.len(),
        true,
    );
    summary_candidate.content = Some(rendered_summary);
    app.transcript.compaction_summaries.push(summary_candidate);

    app.transcript.entries = pending.original_transcript;
    app.transcript.entries.push_context_event(
        format!(
            "context:compaction:live:{}-{}",
            audit.covered_start_seq, audit.covered_end_seq
        ),
        session::ContextHistory::live_compaction_event(&audit),
    );
    app.transcript.entries.push(Entry::Status {
        text: format!(
            "{}compacted  {}",
            if is_automatic { "auto-" } else { "" },
            pending.recovery_handle
        ),
    });
    app.refresh_context_ledger(None);
    if is_automatic {
        if let Some(turn) = original_user_turn {
            return Some(super::input::submit_internal_turn(app, turn));
        }
    }
    Some(None)
}

/// Restore active context when a compaction request fails or cannot complete.
///
/// For automatic compaction, the submitted user turn is preserved by restoring
/// `last_input` so the user can resubmit or edit it. For manual compaction,
/// only the transcript is restored.
pub fn restore_failed_manual_compaction(app: &mut App) -> bool {
    match app.transcript.pending_manual_compaction.take() {
        Some(pending) => {
            restore_failed_compaction(app, pending);
            true
        }
        None => false,
    }
}

/// Restore the saved transcript and, for automatic compaction, the submitted
/// user turn.
pub fn restore_failed_compaction(app: &mut App, pending: PendingManualCompaction) {
    app.transcript.entries = pending.original_transcript;
    if pending.trigger == session::CompactionTrigger::Automatic
        && let Some(turn) = pending.original_user_turn
    {
        app.composer.last_input = Some(turn);
    }
}

pub fn run_doctor_slash(app: &mut App) {
    app.refresh_context_ledger(None);
    let Some(ledger) = app.transcript.context_ledger.as_ref() else {
        app.transcript
            .entries
            .push(Entry::Error { text: "context health is unavailable".to_string() });
        return;
    };
    let counts = ledger.counts();
    let review = app
        .transcript
        .last_compaction_review
        .map(|a| a.label())
        .unwrap_or("none");
    app.transcript.entries.push(Entry::Status {
        text: format!(
            "thndrs doctor (context health)\nsources: {} ({} discovery diagnostics)\npins: {} dropped: {}\nbudget: {} / {} used, {} available, {} auto threshold\nlimits: {} ({})\ncompaction: {} review {}",
            app.transcript.context_sources.len(),
            app.transcript.context_diagnostics.len(),
            counts.pinned,
            counts.dropped,
            ledger.budget.used,
            ledger.budget.target,
            ledger.budget.available_input,
            ledger.budget.auto_compaction_threshold,
            ledger.budget.limits.source.label(),
            ledger.budget.limits.confidence.label(),
            app.compaction_mode_label(),
            review,
        ),
    });
}

pub fn handle_context_command(app: &mut App, command: &str) -> Option<Msg> {
    let Some((action, reference)) = command.split_once(' ') else {
        return match command {
            "show" | "all" => {
                app.open_context_surface();
                None
            }
            "drop --reset" => {
                match app.reset_context_drops() {
                    Ok(()) => app.composer.input.clear(),
                    Err(error) => app.transcript.entries.push(Entry::Error { text: error }),
                }
                None
            }
            "review" => {
                let state = app
                    .transcript
                    .last_compaction_review
                    .map(|a| a.label())
                    .unwrap_or("none");
                app.transcript
                    .entries
                    .push(Entry::Status { text: format!("compaction review: {state}") });
                app.composer.input.clear();
                None
            }
            "changes" => return show_context_changes(app, ""),
            "request" => return show_context_request(app, None),
            "export" => {
                app.transcript.entries.push(Entry::Error {
                    text: "usage: /context export <path> [json|markdown] [--artifacts]".to_string(),
                });
                None
            }
            "verify" | "verification" => {
                app.transcript.entries.push(Entry::Error {
                    text: "usage: /context verify <propose|approve|reject|release> <id> [candidate-id]".to_string(),
                });
                None
            }
            "release" => {
                app.transcript
                    .entries
                    .push(Entry::Error { text: "usage: /context release <id>".to_string() });
                None
            }
            "pin" | "drop" | "recover" => {
                app.transcript
                    .entries
                    .push(Entry::Error { text: format!("usage: /context {command} <id-or-path>") });
                None
            }
            _ => {
                app.transcript.entries.push(Entry::Error {
                    text:
                        "usage: /context [show|all|request|changes|item|pin|drop|recover|verify|release|review|export]"
                            .to_string(),
                });
                None
            }
        };
    };

    let result = match action {
        "changes" => return show_context_changes(app, reference.trim()),
        "request" => return show_context_request(app, Some(reference.trim())),
        "item" => return show_context_item(app, reference.trim()),
        "pin" => app.pin_context_reference(reference.trim()),
        "drop" if reference.trim() == "--reset" => app.reset_context_drops(),
        "drop" => app.drop_context_reference(reference.trim()),
        "recover" => app.recover_context_reference(reference.trim()),
        "release" => app.release_context_item(reference.trim()),
        "review" => return handle_context_review(app, reference.trim()),
        "verify" | "verification" => return handle_context_verification(app, reference.trim()),
        "export" => return handle_context_export(app, reference.trim()),
        _ => Err(
            "usage: /context [show|all|request|changes|item|pin|drop|recover|verify|release|review|export]".to_string(),
        ),
    };
    match result {
        Ok(()) => {
            app.composer.input.clear();
            if matches!(action, "pin" | "drop" | "recover") {
                app.overlay.show_context();
            }
        }
        Err(error) => app.transcript.entries.push(Entry::Error { text: error }),
    }
    None
}

fn show_context_changes(app: &mut App, input: &str) -> Option<Msg> {
    let request_ids = input.split_whitespace().collect::<Vec<_>>();
    match app.transcript.context_history.render_changes(&request_ids) {
        Ok(text) => app.transcript.entries.push(Entry::Status { text }),
        Err(error) => app.transcript.entries.push(Entry::Error { text: error.to_string() }),
    }
    app.composer.input.clear();
    None
}

fn show_context_request(app: &mut App, selector: Option<&str>) -> Option<Msg> {
    match app.transcript.context_history.render_request(selector) {
        Ok(text) => app.transcript.entries.push(Entry::Status { text }),
        Err(error) => app.transcript.entries.push(Entry::Error { text: error.to_string() }),
    }
    app.composer.input.clear();
    None
}

fn show_context_item(app: &mut App, id: &str) -> Option<Msg> {
    let ledger = app.refresh_context_ledger(None);
    let Some(item) = ledger.find(id) else {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("unknown context item `{id}`") });
        return None;
    };
    let details = crate::context::export::export_item(item);
    let origin = item
        .source_path
        .as_ref()
        .map_or_else(|| item.scope.clone(), |path| path.display().to_string());
    app.transcript.entries.push(Entry::Status {
        text: format!(
            "context item {}: origin {} ({}) · lifecycle {} · visibility {} ({}) · estimate {} tokens ({}) · artifact {} · protected {} · recovery {}",
            item.id,
            item.kind.label(),
            origin,
            details.lifecycle.label(),
            item.visibility.label(),
            item.reason,
            item.token_estimate,
            "conservative utf8 bytes / 3 + item overhead",
            item.artifact_handle.as_deref().unwrap_or("none"),
            if details.protected { "yes" } else { "no" },
            if details.recovery_available { "available" } else { "unavailable" },
        ),
    });
    app.composer.input.clear();
    None
}

/// Render active transcript material for the configured compaction model.
fn render_compaction_source(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::User { text } => format!("user: {text}"),
            Entry::Agent { text, .. } => format!("assistant: {text}"),
            Entry::Skill { content, .. } => format!("assistant: {content}"),
            Entry::Reasoning { text, .. } => format!("reasoning: {text}"),
            Entry::Tool { name, arguments, output, .. } => {
                format!("tool {name} {arguments}: {}", output.join("\n"))
            }
            Entry::Status { text } => format!("status: {text}"),
            Entry::Error { text } => format!("error: {text}"),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Select a complete prefix while retaining a recent, user-turn-aligned tail.
///
/// `maximum_end` excludes an in-flight user turn during automatic compaction.
/// Small histories still honor an explicit `/compact`; the recent-token target
/// becomes relevant once the transcript is larger than that target.
fn compaction_cut(entries: &[Entry], maximum_end: usize, keep_recent_tokens: u64) -> usize {
    let maximum_end = maximum_end.min(entries.len());
    if maximum_end == 0 || keep_recent_tokens == 0 {
        return maximum_end;
    }
    let total_tokens = agent_context::estimate_tokens(render_compaction_source(entries).len()) as u64;
    if total_tokens <= keep_recent_tokens {
        return maximum_end;
    }

    let mut tail_start = entries[..maximum_end]
        .iter()
        .rposition(|entry| matches!(entry, Entry::User { .. }))
        .unwrap_or(maximum_end);
    loop {
        let tail_tokens = agent_context::estimate_tokens(render_compaction_source(&entries[tail_start..]).len()) as u64;
        if tail_tokens >= keep_recent_tokens {
            return tail_start;
        }
        let Some(previous_start) = entries[..tail_start]
            .iter()
            .rposition(|entry| matches!(entry, Entry::User { .. }))
        else {
            return 0;
        };
        tail_start = previous_start;
    }
}

fn is_model_context_entry(entry: &Entry) -> bool {
    match entry {
        Entry::Status { .. } | Entry::Error { .. } => false,
        Entry::Skill { content, .. } => !content.is_empty(),
        _ => true,
    }
}

fn context_entry_count(entries: &[Entry]) -> u64 {
    entries.iter().filter(|entry| is_model_context_entry(entry)).count() as u64
}

fn raw_index_after_context_entries(entries: &[Entry], count: u64) -> usize {
    if count == 0 {
        return 0;
    }
    let mut seen = 0_u64;
    for (index, entry) in entries.iter().enumerate() {
        if is_model_context_entry(entry) {
            seen = seen.saturating_add(1);
            if seen == count {
                return index + 1;
            }
        }
    }
    entries.len()
}

fn handle_context_verification(app: &mut App, input: &str) -> Option<Msg> {
    let mut parts = input.split_whitespace();
    let Some(action) = parts.next() else {
        app.transcript.entries.push(Entry::Error {
            text: "usage: /context verify <propose|approve|reject|release> <id> [candidate-id]".to_string(),
        });
        return None;
    };
    let Some(reference) = parts.next() else {
        app.transcript.entries.push(Entry::Error {
            text: "usage: /context verify <propose|approve|reject|release> <id> [candidate-id]".to_string(),
        });
        return None;
    };
    let result = match action {
        "propose" => {
            let Some(candidate) = parts.next() else {
                app.transcript.entries.push(Entry::Error {
                    text: "usage: /context verify propose <protected-id> <candidate-id>".to_string(),
                });
                return None;
            };
            if parts.next().is_some() {
                Err("usage: /context verify propose <protected-id> <candidate-id>".to_string())
            } else {
                app.propose_context_verification(reference, candidate)
                    .map(|relation_id| format!("verification proposed: {relation_id}"))
            }
        }
        "approve" => {
            if parts.next().is_some() {
                Err("verification action accepts exactly one relation id".to_string())
            } else {
                app.approve_context_verification(reference).map(|()| String::new())
            }
        }
        "reject" => {
            if parts.next().is_some() {
                Err("verification action accepts exactly one relation id".to_string())
            } else {
                app.reject_context_verification(reference).map(|()| String::new())
            }
        }
        "release" => {
            if parts.next().is_some() {
                Err("verification action accepts exactly one relation id".to_string())
            } else {
                app.release_context_verification(reference).map(|()| String::new())
            }
        }
        _ => Err("usage: /context verify <propose|approve|reject|release> <id> [candidate-id]".to_string()),
    };
    match result {
        Ok(message) => app.transcript.entries.push(Entry::Status {
            text: if message.is_empty() { format!("verification {action}: recorded") } else { message },
        }),
        Err(error) => app.transcript.entries.push(Entry::Error { text: error }),
    }
    app.composer.input.clear();
    None
}

fn handle_context_export(app: &mut App, input: &str) -> Option<Msg> {
    let mut path = None;
    let mut format = None;
    let mut include_artifacts = false;
    for part in input.split_whitespace() {
        if matches!(part, "--artifacts" | "--include-artifacts") {
            include_artifacts = true;
        } else if let Some(parsed) = ContextExportFormat::parse(part) {
            if format.replace(parsed).is_some() {
                app.transcript
                    .entries
                    .push(Entry::Error { text: "context export format was specified more than once".to_string() });
                return None;
            }
        } else if path.replace(part).is_some() {
            app.transcript
                .entries
                .push(Entry::Error { text: "usage: /context export <path> [json|markdown] [--artifacts]".to_string() });
            return None;
        }
    }
    let Some(path) = path else {
        app.transcript
            .entries
            .push(Entry::Error { text: "usage: /context export <path> [json|markdown] [--artifacts]".to_string() });
        return None;
    };
    let path = Path::new(path);
    let path = if path.is_absolute() { path.to_path_buf() } else { app.runtime.cwd.join(path) };
    let format = format.unwrap_or_else(|| match path.extension().and_then(|extension| extension.to_str()) {
        Some("md" | "markdown") => ContextExportFormat::Markdown,
        _ => ContextExportFormat::Json,
    });
    match app.write_context_export(&path, format, include_artifacts) {
        Ok(()) => app.transcript.entries.push(Entry::Status {
            text: format!(
                "context exported: {} ({})",
                redact_context_display(&path.display().to_string()),
                format.label()
            ),
        }),
        Err(error) => app.transcript.entries.push(Entry::Error { text: error }),
    }
    app.composer.input.clear();
    None
}

fn handle_context_review(app: &mut App, action: &str) -> Option<Msg> {
    let Some(recovery_handle) = app
        .transcript
        .pending_compaction_review
        .as_ref()
        .map(|pending| pending.pending.recovery_handle.clone())
    else {
        app.transcript
            .entries
            .push(Entry::Error { text: "no compaction summary is awaiting review".to_string() });
        return None;
    };
    let review = match action {
        "approve" => session::CompactionReviewResult::Approved,
        "reject" => session::CompactionReviewResult::Rejected,
        _ => {
            app.transcript
                .entries
                .push(Entry::Error { text: "usage: /context review <approve|reject>".to_string() });
            return None;
        }
    };
    if review == session::CompactionReviewResult::Rejected {
        if let Some(writer) = app.session.writer.as_mut()
            && let Err(error) = writer.append_compaction_review(&recovery_handle, review)
        {
            app.transcript
                .entries
                .push(Entry::Error { text: format!("failed to record compaction review: {error}") });
            return None;
        }
        let pending = app
            .transcript
            .pending_compaction_review
            .take()
            .expect("review state checked above");
        app.transcript.last_compaction_review = Some(review);
        app.composer.input.clear();
        let original_user_turn = pending.pending.original_user_turn.clone();
        restore_failed_compaction(app, pending.pending);
        if let Some(turn) = original_user_turn {
            app.composer.input.set_text(&turn);
        }
        app.transcript
            .entries
            .push(Entry::Status { text: format!("compaction rejected  {recovery_handle}") });
        return None;
    }
    let pending = app
        .transcript
        .pending_compaction_review
        .take()
        .expect("review state checked above");
    app.transcript.last_compaction_review = Some(review);
    app.composer.input.clear();
    apply_compaction(
        app,
        pending.pending,
        &pending.summary,
        session::CompactionReviewResult::Approved,
    )
    .flatten()
}

/// Build addressable source metadata and exact protected facts for a closed
/// transcript range. The source body itself remains process-local.
fn range_sources(
    id_namespace: &str, recovery_session_id: &str, start_seq: u64, entries: &[Entry],
) -> (Vec<agent_context::RangeSource>, Vec<agent_context::ProtectedFact>) {
    let mut sources = Vec::with_capacity(entries.len());
    let mut protected_facts = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let sequence = start_seq + index as u64;
        let id =
            agent_context::item_id_for_session_range(&ContextItemKind::Transcript, id_namespace, sequence, sequence);
        let text = render_compaction_entry(entry);
        sources.push(agent_context::RangeSource {
            sequence,
            id: id.clone(),
            content_hash: tools::hash_content(&text),
            recovery_handle: format!("session:{recovery_session_id}:{sequence}"),
        });
        if transcript_protection(entry).is_protected() {
            protected_facts.push(agent_context::ProtectedFact { source_id: id, text });
        }
    }
    (sources, protected_facts)
}

fn source_summary_ids(summaries: &[CompactionSummaryCandidate]) -> Vec<String> {
    let mut ids = summaries
        .iter()
        .filter(|summary| summary.latest)
        .map(|summary| summary.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

/// Build the append-only record only after a summary is valid and approved.
fn compaction_audit(
    session_id: &str, pending: &PendingManualCompaction, summary: &agent_context::RangeSummary, rendered_summary: &str,
    review: session::CompactionReviewResult,
) -> session::CompactionAudit {
    let source_hashes = pending
        .request
        .sources
        .iter()
        .map(|source| session::CompactionSourceHash { id: source.id.clone(), content_hash: Some(source.content_hash) })
        .collect();
    let before_bytes = render_compaction_source(&pending.source_transcript).len();
    session::CompactionAudit {
        summary: rendered_summary.to_string(),
        typed_summary: Some(summary.clone()),
        summary_id: Some(
            CompactionSummaryCandidate::new(
                session_id,
                pending.covered_start_seq,
                pending.covered_end_seq,
                rendered_summary.len(),
                true,
            )
            .id,
        ),
        covered_start_seq: pending.covered_start_seq,
        covered_end_seq: pending.covered_end_seq,
        source_hashes,
        source_summary_ids: pending.source_summary_ids.clone(),
        trigger: pending.trigger,
        risk: classify_compaction_risk(&pending.source_transcript),
        review: Some(review),
        recovery_handles: pending
            .request
            .sources
            .iter()
            .map(|source| source.recovery_handle.clone())
            .collect(),
        model: pending.request.model.clone(),
        usage: None,
        local_receipt: Some(session::CompactionLocalReceipt {
            before_bytes,
            after_bytes: rendered_summary.len(),
            before_token_estimate: agent_context::estimate_tokens(before_bytes) as u64,
            after_token_estimate: agent_context::estimate_tokens(rendered_summary.len()) as u64,
        }),
        native_context_edit: Some(session::ProviderContextEdit::Unavailable {
            diagnostic: "provider adapter does not report native context editing capability".to_string(),
        }),
    }
}

/// Whether the current selection still contains every item sent with a prior
/// provider request.
///
/// A provider's input-token measurement includes serialization and provider
/// framing that the local ledger cannot reconstruct. It remains useful for the
/// next request only while the prior working set is intact. Dropping or
/// compacting any rendered item invalidates that measurement.
fn request_context_is_retained(
    ledger: &agent_context::ContextLedger, accounting: &thndrs_agent::ProviderRequestAccounting,
) -> bool {
    let retained_ids = ledger
        .items
        .iter()
        .filter(|item| item.visibility.is_rendered())
        .map(|item| item.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut has_rendered_context = false;
    for item in &accounting.context {
        if item.state.is_rendered() {
            has_rendered_context = true;
            if !retained_ids.contains(item.id.as_str()) {
                return false;
            }
        }
    }
    has_rendered_context
}

fn transcript_candidate_label(entry: &Entry) -> String {
    match entry {
        Entry::User { .. } => "user".to_string(),
        Entry::Agent { .. } => "assistant".to_string(),
        Entry::Skill { name, .. } => format!("skill:{name}"),
        Entry::Reasoning { .. } => "reasoning".to_string(),
        Entry::Tool { name, .. } => format!("tool:{name}"),
        Entry::Status { .. } => "status".to_string(),
        Entry::Error { .. } => "error".to_string(),
    }
}

fn render_compaction_entry(entry: &Entry) -> String {
    match entry {
        Entry::User { text } => format!("user: {text}"),
        Entry::Agent { text, .. } => format!("assistant: {text}"),
        Entry::Skill { content, .. } => format!("assistant: {content}"),
        Entry::Reasoning { text, .. } => format!("reasoning: {text}"),
        Entry::Tool { name, arguments, output, .. } => format!("tool {name} {arguments}: {}", output.join("\n")),
        Entry::Status { text } => format!("status: {text}"),
        Entry::Error { text } => format!("error: {text}"),
    }
}

fn transcript_protection(entry: &Entry) -> agent_context::ContextProtection {
    use agent_context::ContextProtectionReason as Reason;

    let mut reasons = Vec::new();
    match entry {
        Entry::User { .. } => reasons.push(Reason::ExplicitConstraint),
        Entry::Skill { content, .. } if !content.is_empty() => {
            reasons.push(Reason::ExplicitConstraint);
        }
        Entry::Error { .. } => reasons.push(Reason::FailureEvidence),
        Entry::Tool { name, status, .. } => {
            if matches!(status, ToolStatus::Failed | ToolStatus::Cancelled) {
                reasons.push(Reason::FailureEvidence);
            }
            if is_write_tool(name) {
                reasons.push(Reason::UnverifiedWriteEdit);
            }
        }
        Entry::Agent { .. } | Entry::Skill { .. } | Entry::Reasoning { .. } | Entry::Status { .. } => {}
    }
    agent_context::ContextProtection::from_reasons(reasons)
}

fn is_write_tool(name: &str) -> bool {
    matches!(
        name.split_once('#').map_or(name, |(name, _)| name),
        "create_file" | "replace_range" | "write_patch" | "acp.fs.write_text_file"
    )
}

fn pinned_candidate_from_meta(item: &session::ContextItemMeta) -> PinnedCandidate {
    PinnedCandidate {
        id: item.id.clone(),
        kind: item.kind.clone(),
        label: item.source_path.clone().unwrap_or_else(|| item.id.clone()),
        source_path: item.source_path.clone().map(PathBuf::from),
        scope: item.scope.clone().unwrap_or_else(|| ".".to_string()),
        content_hash: item.content_hash,
        artifact_handle: item.artifact_handle.clone(),
        bytes: item.byte_count,
    }
}

fn transcript_candidate_bytes(entry: &Entry) -> usize {
    match entry {
        Entry::User { text }
        | Entry::Agent { text, .. }
        | Entry::Reasoning { text, .. }
        | Entry::Status { text }
        | Entry::Error { text } => text.len(),
        Entry::Skill { content, .. } => content.len(),
        Entry::Tool { name, arguments, output, .. } => {
            name.len() + arguments.len() + output.iter().map(String::len).sum::<usize>()
        }
    }
}

fn file_size(path: &Path) -> usize {
    std::fs::metadata(path)
        .map(|metadata| metadata.len().min(usize::MAX as u64) as usize)
        .unwrap_or(0)
}

fn redact_context_display(value: &str) -> String {
    let redacted = tools::shell::redact_secrets(value);
    utils::truncate_ellipsis(&redacted, CONTEXT_DISPLAY_MAX_BYTES)
}

/// Map transcript signals to the durable compaction-risk classification.
fn classify_compaction_risk(entries: &[Entry]) -> CompactionRisk {
    let signals = agent_context::CompactionRiskSignals {
        has_tool_output_or_diff: entries.iter().any(|entry| matches!(entry, Entry::Tool { .. })),
        has_failure_or_permission: entries.iter().any(|entry| matches!(entry, Entry::Error { .. })),
        has_correction_or_unresolved_work: entries.iter().any(
            |entry| matches!(entry, Entry::Status { text } if text.contains("permission") || text.contains("failed")),
        ),
    };
    match signals.classify() {
        agent_context::CompactionRisk::Low => session::CompactionRisk::Low,
        agent_context::CompactionRisk::High => session::CompactionRisk::High,
    }
}

#[cfg(test)]
pub fn range_summary_response(app: &App, objective: &str) -> String {
    let request = &app
        .transcript
        .pending_manual_compaction
        .as_ref()
        .expect("compaction request is pending")
        .request;
    serde_json::to_string(&agent_context::RangeSummary {
        schema_version: agent_context::RANGE_SUMMARY_SCHEMA_VERSION,
        objective: objective.to_string(),
        findings: Vec::new(),
        decisions: Vec::new(),
        paths: Vec::new(),
        failures: Vec::new(),
        verification: Vec::new(),
        blockers: Vec::new(),
        protected_facts: request.protected_facts.clone(),
        sources: request.sources.clone(),
        source_summary_ids: request.source_summary_ids.clone(),
    })
    .expect("serialize typed test summary")
}

#[cfg(test)]
mod compaction_cut_tests {
    use super::*;

    fn user(text: impl Into<String>) -> Entry {
        Entry::User { text: text.into() }
    }

    fn agent(text: impl Into<String>) -> Entry {
        Entry::Agent { text: text.into(), streaming: false }
    }

    #[test]
    fn explicit_compaction_covers_a_small_closed_history() {
        let entries = vec![user("question"), agent("answer")];

        assert_eq!(compaction_cut(&entries, entries.len(), 20_000), entries.len());
    }

    #[test]
    fn large_history_retains_a_complete_recent_turn() {
        let large = "x".repeat(48_000);
        let entries = vec![
            user("old question"),
            agent(large.clone()),
            user("recent question"),
            agent(large),
        ];

        assert_eq!(compaction_cut(&entries, entries.len(), 10_000), 2);
    }

    #[test]
    fn automatic_compaction_never_covers_the_in_flight_user_turn() {
        let entries = vec![user("old question"), agent("old answer"), user("current question")];

        assert_eq!(compaction_cut(&entries, 2, 0), 2);
    }

    #[test]
    fn transient_rows_do_not_change_context_sequence_numbers() {
        let entries = vec![
            user("question"),
            Entry::Status { text: "resumed session".to_string() },
            agent("answer"),
            Entry::Error { text: "display-only error".to_string() },
        ];

        assert_eq!(context_entry_count(&entries), 2);
        assert_eq!(raw_index_after_context_entries(&entries, 1), 1);
        assert_eq!(raw_index_after_context_entries(&entries, 2), 3);
    }

    #[test]
    fn activated_skill_instructions_are_protected_but_read_notices_are_not() {
        let skill = |content: &str| Entry::Skill {
            name: "review".to_string(),
            path: "/skills/review/SKILL.md".to_string(),
            content: content.to_string(),
            token_estimate: 42,
            context_percent: Some(1),
        };

        assert!(
            transcript_protection(&skill("review carefully"))
                .contains(agent_context::ContextProtectionReason::ExplicitConstraint)
        );
        assert!(!transcript_protection(&skill("")).is_protected());
    }
}
