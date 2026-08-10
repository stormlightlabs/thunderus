use std::time::Duration;

use super::*;
use crate::cli::WebSearchMode;
use crate::context::ContextSource;
use crate::prompt::PromptBundle;
use crate::skills::SkillActivation;
use thndrs_agent::context::{
    ContextBudget, ContextItem, ContextItemKind, ContextLedger, ContextLifecycle, ContextLifecycleAction,
    ContextProtection, ContextProtectionReason, ContextRelation, ContextVisibility, ModelContextLimits,
    ModelLimitConfidence, ModelLimitSource,
};
use thndrs_agent::{ProviderRequestAccounting, ProviderUsageComponents, ProviderUsageRule};

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
        "opencode/big-pickle",
        WebSearchMode::DuckDuckGo,
        &[source],
        &[],
        "explain this repo",
    )
}

fn context_item(content: &str) -> ContextItem {
    ContextItem {
        id: "ctx_pinned_file_1".to_string(),
        kind: ContextItemKind::PinnedFile,
        label: "repository build notes".to_string(),
        source_path: Some(PathBuf::from("/repo/docs/build.md")),
        scope: ".".to_string(),
        content_hash: Some(42),
        artifact_handle: None,
        byte_count: content.len(),
        content: Some(content.to_string()),
        token_estimate: 64,
        visibility: ContextVisibility::Archived,
        reason_code: "test_archived".to_string(),
        reason: "archived after budget selection".to_string(),
        lifecycle: thndrs_agent::context::ContextLifecycle::default(),
    }
}

fn context_ledger(content: &str) -> ContextLedger {
    let limits = ModelContextLimits {
        provider: "opencode-zen".to_string(),
        model: "opencode/big-pickle".to_string(),
        context_window: 200_000,
        max_completion_tokens: 8_192,
        recommended_completion_tokens: 4_096,
        source: ModelLimitSource::LiveMetadata,
        confidence: ModelLimitConfidence::Exact,
    };
    let items = vec![context_item(content)];
    ContextLedger { budget: ContextBudget::from_limits(limits, &items), items, diagnostics: Vec::new() }
}

fn compaction_audit(trigger: CompactionTrigger) -> CompactionAudit {
    CompactionAudit {
        summary: "Kept the build failure and the pending test fix.".to_string(),
        typed_summary: None,
        covered_start_seq: 12,
        covered_end_seq: 47,
        source_hashes: vec![
            CompactionSourceHash { id: "ctx_transcript_12".to_string(), content_hash: None },
            CompactionSourceHash { id: "ctx_tool_archive_47".to_string(), content_hash: Some(42) },
        ],
        source_summary_ids: Vec::new(),
        trigger,
        risk: CompactionRisk::High,
        review: Some(CompactionReviewResult::Approved),
        recovery_handles: vec!["session:12..47".to_string()],
        model: "opencode/big-pickle".to_string(),
        usage: Some(CompactionTokenUsage { input_tokens: 1_024, output_tokens: 256 }),
        local_receipt: None,
        native_context_edit: None,
    }
}

#[test]
fn from_bundle_captures_model_and_search_mode() {
    let bundle = bundle_with_context();
    let meta = PromptMetadata::from_bundle(&bundle);
    assert_eq!(meta.model, "opencode/big-pickle");
    assert_eq!(meta.search_mode, "duckduckgo");
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
fn from_bundle_captures_self_knowledge_inputs_without_content() {
    let bundle = bundle_with_context();
    let meta = PromptMetadata::from_bundle(&bundle);

    assert_eq!(meta.provider, "opencode-zen");
    assert_eq!(meta.renderer_mode, "direct-inline");
    assert!(meta.prompt_fragments.contains(&"base_identity".to_string()));
    assert!(
        meta.docs_map
            .contains(&"docs/src/content/docs/docs/reference/tools.md".to_string())
    );
    assert!(meta.tool_names.contains(&"read_file_range".to_string()));
    assert!(meta.skill_names.is_empty());
    assert!(meta.diagnostics.is_empty());
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
    let transcript = vec![
        Entry::User { text: "hello".to_string() },
        Entry::Agent { text: "hi".to_string(), streaming: false },
    ];
    let bundle = PromptBundle::new(
        Path::new("/repo"),
        "opencode/big-pickle",
        WebSearchMode::DuckDuckGo,
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
    let bundle = PromptBundle::new(
        Path::new("/repo"),
        "opencode/big-pickle",
        WebSearchMode::DuckDuckGo,
        &[],
        &[],
        "",
    );
    let meta = PromptMetadata::from_bundle(&bundle);
    assert!(!meta.has_user_turn);
}

#[test]
fn json_round_trip_preserves_all_fields() {
    let bundle = bundle_with_context();
    let meta = PromptMetadata::from_bundle(&bundle);
    let json = serde_json::to_string(&meta).expect("serialize");
    let restored: PromptMetadata = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(meta, restored);
}

#[test]
fn json_round_trip_with_history_reuse() {
    let mut bundle = bundle_with_context();
    bundle.history_reuse = HistoryReuse::Available;
    bundle.prev_context_hash = Some(77777);

    let meta = PromptMetadata::from_bundle(&bundle);
    let json = serde_json::to_string(&meta).expect("serialize");
    let restored: PromptMetadata = serde_json::from_str(&json).expect("deserialize");
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
        "opencode/gpt-5.6-luna",
        WebSearchMode::Searxng,
        &[source],
        &[],
        "explain",
    );
    let meta = PromptMetadata::from_bundle(&bundle);
    let json = serde_json::to_string(&meta).expect("serialize");
    let restored: PromptMetadata = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(meta, restored);
    assert!(restored.context_sources[0].truncated);
    assert_eq!(restored.context_sources[0].byte_count, 40_000);
}

#[test]
fn from_json_rejects_malformed_input() {
    let result = serde_json::from_str::<PromptMetadata>("not valid json");
    assert!(result.is_err());
}

#[test]
fn metadata_does_not_contain_prompt_text() {
    let bundle = bundle_with_context();
    let meta = PromptMetadata::from_bundle(&bundle);
    let json = serde_json::to_string(&meta).expect("serialize");
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

#[test]
fn session_record_json_round_trip_session_meta() {
    let record = SessionRecord::SessionMeta {
        schema_version: 1,
        seq: 0,
        time: "2026-06-29T12:00:00Z".to_string(),
        session_id: "test-1".to_string(),
        cwd: "/repo".to_string(),
        title: "scratch".to_string(),
        provider: "opencode-zen".to_string(),
        model: "opencode/big-pickle".to_string(),
        websearch: "duckduckgo".to_string(),
        app_version: "0.1.0".to_string(),
        config: None,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"session_meta\""));
}

#[test]
fn session_record_session_meta_persists_config_session_dir() {
    let record = SessionRecord::SessionMeta {
        schema_version: 1,
        seq: 0,
        time: "2026-06-29T12:00:00Z".to_string(),
        session_id: "test-1".to_string(),
        cwd: "/repo".to_string(),
        title: "scratch".to_string(),
        provider: "opencode-zen".to_string(),
        model: "opencode/big-pickle".to_string(),
        websearch: "duckduckgo".to_string(),
        app_version: "0.1.0".to_string(),
        config: Some(SessionConfigMeta {
            session_dir: Some("/repo/.thndrs/sessions".to_string()),
            ..SessionConfigMeta::default()
        }),
    };

    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");

    assert!(json.contains("\"session_dir\":\"/repo/.thndrs/sessions\""));
    assert_eq!(record, restored);
}

#[test]
fn session_record_json_round_trip_acp_session() {
    let record = SessionRecord::AcpSession {
        schema_version: 1,
        seq: 1,
        time: "2026-07-04T12:00:00Z".to_string(),
        local_session_id: "session-local".to_string(),
        agent_name: "local".to_string(),
        acp_session_id: "external-session".to_string(),
        command: "agent --acp".to_string(),
        protocol_version: "V1".to_string(),
        agent_info_name: Some("fake-agent".to_string()),
        agent_info_version: Some("0.0.0".to_string()),
        client_info_name: Some("fake-client".to_string()),
        client_info_version: Some("1.0.0".to_string()),
    };

    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");

    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"acp_session\""));
    assert!(json.contains("\"local_session_id\":\"session-local\""));
    assert!(json.contains("\"acp_session_id\":\"external-session\""));
    assert!(json.contains("\"client_info_name\":\"fake-client\""));
}

#[test]
fn session_record_session_meta_persists_effective_config_metadata() {
    let mut origins = std::collections::BTreeMap::new();
    origins.insert("model".to_string(), "env:THNDRS_MODEL".to_string());
    origins.insert("websearch".to_string(), "project:.thndrs/config.toml".to_string());

    let record = SessionRecord::SessionMeta {
        schema_version: 1,
        seq: 0,
        time: "2026-06-29T12:00:00Z".to_string(),
        session_id: "test-1".to_string(),
        cwd: "/repo".to_string(),
        title: "scratch".to_string(),
        provider: "opencode-zen".to_string(),
        model: "env-model".to_string(),
        websearch: "duckduckgo".to_string(),
        app_version: "0.1.0".to_string(),
        config: Some(SessionConfigMeta {
            session_dir: Some("/repo/custom-sessions".to_string()),
            files: vec![SessionConfigFile {
                path: ".thndrs/config.toml".to_string(),
                source: "project".to_string(),
                sha256: "abc123".to_string(),
            }],
            origins,
            diagnostics: Vec::new(),
            ..SessionConfigMeta::default()
        }),
    };

    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");

    assert_eq!(record, restored);
    assert!(json.contains("\"cwd\":\"/repo\""));
    assert!(json.contains("\"model\":\"env-model\""));
    assert!(json.contains("\"websearch\":\"duckduckgo\""));
    assert!(json.contains("\"session_dir\":\"/repo/custom-sessions\""));
    assert!(json.contains("\"path\":\".thndrs/config.toml\""));
    assert!(json.contains("\"model\":\"env:THNDRS_MODEL\""));
}

#[test]
fn session_record_session_meta_persists_mcp_config_metadata() {
    let record = SessionRecord::SessionMeta {
        schema_version: 1,
        seq: 0,
        time: "2026-06-29T12:00:00Z".to_string(),
        session_id: "test-1".to_string(),
        cwd: "/repo".to_string(),
        title: "scratch".to_string(),
        provider: "opencode-zen".to_string(),
        model: "opencode/big-pickle".to_string(),
        websearch: "duckduckgo".to_string(),
        app_version: "0.1.0".to_string(),
        config: Some(SessionConfigMeta {
            mcp_files: vec![SessionConfigFile {
                path: ".thndrs/mcp.toml".to_string(),
                source: "project".to_string(),
                sha256: "mcpabc123".to_string(),
            }],
            mcp_diagnostics: vec!["mcp server `docs` skipped: unresolved environment variable DOCS_TOKEN".to_string()],
            ..SessionConfigMeta::default()
        }),
    };

    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");

    assert_eq!(record, restored);
    assert!(json.contains("\"mcp_files\""));
    assert!(json.contains("\"path\":\".thndrs/mcp.toml\""));
    assert!(json.contains("\"sha256\":\"mcpabc123\""));
    assert!(json.contains("\"mcp_diagnostics\""));
}

#[test]
fn session_record_json_round_trip_mcp_config_changed() {
    let record = SessionRecord::McpConfigChanged {
        schema_version: 1,
        seq: 1,
        time: "2026-06-29T12:00:00Z".to_string(),
        turn_id: "turn_1".to_string(),
        previous_files: vec![SessionConfigFile {
            path: ".thndrs/mcp.toml".to_string(),
            source: "project".to_string(),
            sha256: "old".to_string(),
        }],
        current_files: vec![SessionConfigFile {
            path: ".thndrs/mcp.toml".to_string(),
            source: "project".to_string(),
            sha256: "new".to_string(),
        }],
        diagnostics: Vec::new(),
    };

    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");

    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"mcp_config_changed\""));
    assert!(json.contains("\"previous_files\""));
    assert!(json.contains("\"current_files\""));
}

#[test]
fn session_config_metadata_does_not_include_provider_secret_values() {
    let record = SessionRecord::SessionMeta {
        schema_version: 1,
        seq: 0,
        time: "2026-06-29T12:00:00Z".to_string(),
        session_id: "test-1".to_string(),
        cwd: "/repo".to_string(),
        title: "scratch".to_string(),
        provider: "opencode-zen".to_string(),
        model: "opencode/big-pickle".to_string(),
        websearch: "duckduckgo".to_string(),
        app_version: "0.1.0".to_string(),
        config: Some(SessionConfigMeta {
            session_dir: Some("/repo/.thndrs/sessions".to_string()),
            origins: std::collections::BTreeMap::from([("model".to_string(), "default:default".to_string())]),
            ..SessionConfigMeta::default()
        }),
    };

    let json = record.to_json().expect("serialize");

    assert!(!json.contains("UMANS_API_KEY"));
    assert!(!json.contains("OPENCODE_GO_KEY"));
    assert!(!json.contains("sk-provider-secret"));
}

#[test]
fn session_record_json_round_trip_user() {
    let record = SessionRecord::User {
        schema_version: 1,
        seq: 2,
        time: "2026-06-29T12:00:04Z".to_string(),
        turn_id: "turn_1".to_string(),
        text: "explain this repo".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"user\""));
}

#[test]
fn session_record_json_round_trip_tool_finished() {
    let record = SessionRecord::ToolFinished {
        schema_version: 1,
        seq: 5,
        time: "2026-06-29T12:00:06Z".to_string(),
        turn_id: "turn_1".to_string(),
        call_id: "call_1".to_string(),
        status: ToolStatus::Ok,
        output: vec!["src/main.rs:1:fn main()".to_string()],
        artifact: None,
        mcp: None,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"tool_finished\""));
}

#[test]
fn writer_appends_prompt_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = test_writer(dir.path(), "prompt-meta-session");
    let bundle = bundle_with_context();
    let metadata = PromptMetadata::from_bundle(&bundle);

    writer
        .append_prompt_metadata("turn_1", &metadata)
        .expect("append prompt metadata");
    let records = SessionReader::read_records(writer.path());

    assert!(matches!(
        records.get(1),
        Some(SessionRecord::PromptMetadata { turn_id, metadata: stored, .. })
            if turn_id == "turn_1" && stored == &metadata
    ));
}

#[test]
fn context_ledger_record_round_trips_without_content() {
    let secret_content = "api_key=supersecretvalue";
    let record = SessionRecord::ContextLedger {
        schema_version: 1,
        seq: 2,
        time: "2026-07-10T12:00:00Z".to_string(),
        turn_id: "turn_1".to_string(),
        ledger: ContextLedgerMeta::from(&context_ledger(secret_content)),
    };

    let json = record.to_json().expect("serialize ledger");
    let restored = SessionRecord::from_json(&json).expect("deserialize ledger");

    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"context_ledger\""));
    assert!(json.contains("archived after budget selection"));
    assert!(!json.contains(secret_content));
    assert!(!json.contains("supersecretvalue"));
    assert!(
        !json.contains("repository credentials"),
        "labels are not persisted as context metadata"
    );
}

#[test]
fn context_action_records_round_trip_without_content() {
    let item = context_item("password=supersecretvalue");
    let actions = [
        (
            "context_pin",
            SessionRecord::ContextPin {
                schema_version: 1,
                seq: 3,
                time: "2026-07-10T12:00:01Z".to_string(),
                item: ContextItemMeta::from(&item),
                reason: "user requested pin".to_string(),
            },
        ),
        (
            "context_drop",
            SessionRecord::ContextDrop {
                schema_version: 1,
                seq: 4,
                time: "2026-07-10T12:00:02Z".to_string(),
                item: ContextItemMeta::from(&item),
                reason: "user requested drop".to_string(),
            },
        ),
        (
            "context_recovery",
            SessionRecord::ContextRecovery {
                schema_version: 1,
                seq: 5,
                time: "2026-07-10T12:00:03Z".to_string(),
                item: ContextItemMeta::from(&item),
                reason: "user requested recovery".to_string(),
            },
        ),
    ];

    for (record_type, record) in actions {
        let json = record.to_json().expect("serialize context action");
        let restored = SessionRecord::from_json(&json).expect("deserialize context action");

        assert_eq!(record, restored);
        assert!(json.contains(&format!("\"type\":\"{record_type}\"")));
        assert!(json.contains("user requested"));
        assert!(!json.contains("supersecretvalue"));
    }
}

#[test]
fn lifecycle_action_records_round_trip_with_relation_and_protection_metadata() {
    let relation = ContextRelation::proposed_verification("rel-1", "ctx-write", "ctx-test");
    let mut item = context_item("write evidence");
    item.lifecycle = ContextLifecycle::new(ContextProtection::from_reason(
        ContextProtectionReason::UnverifiedWriteEdit,
    ))
    .apply(ContextLifecycleAction::ProposeVerification { relation: relation.clone() })
    .expect("proposal");
    let record = SessionRecord::ContextLifecycle {
        schema_version: 1,
        seq: 6,
        time: "2026-07-10T12:00:04Z".to_string(),
        audit: ContextLifecycleAudit {
            action: ContextLifecycleAction::ProposeVerification { relation },
            item: ContextItemMeta::from(&item),
            reason: "agent proposed a verification relation".to_string(),
        },
    };

    let json = record.to_json().expect("serialize lifecycle action");
    let restored = SessionRecord::from_json(&json).expect("deserialize lifecycle action");
    assert_eq!(record, restored);
    assert!(json.contains("context_lifecycle"));
    assert!(json.contains("unverified_write_edit"));
    assert!(json.contains("verified_by"));
    assert!(!json.contains("write evidence"));
}

#[test]
fn writer_appends_context_ledger_and_actions() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "context-actions-session");
    let ledger = context_ledger("access_token=supersecretvalue");
    let item = ledger.items.first().expect("ledger item");

    writer
        .append_context_ledger("turn_1", &ledger)
        .expect("append context ledger");
    writer.append_context_pin(item, "user chose pin").expect("append pin");
    writer
        .append_context_drop(item, "user chose drop")
        .expect("append drop");
    writer
        .append_context_recovery(item, "user chose recovery")
        .expect("append recovery");

    let records = SessionReader::read_records(writer.path());
    assert!(matches!(records.get(1), Some(SessionRecord::ContextLedger { turn_id, .. }) if turn_id == "turn_1"));
    assert!(matches!(records.get(2), Some(SessionRecord::ContextPin { .. })));
    assert!(matches!(records.get(3), Some(SessionRecord::ContextDrop { .. })));
    assert!(matches!(records.get(4), Some(SessionRecord::ContextRecovery { .. })));

    let content = std::fs::read_to_string(writer.path()).expect("read session");
    assert!(!content.contains("supersecretvalue"));
}

#[test]
fn reader_skips_malformed_optional_context_metadata_and_keeps_other_records() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("context-actions.jsonl");
    let first = SessionRecord::Cancelled {
        schema_version: 1,
        seq: 0,
        time: "2026-07-10T12:00:00Z".to_string(),
        turn_id: "turn_1".to_string(),
        reason: "before malformed metadata".to_string(),
    }
    .to_json()
    .expect("serialize first record");
    let last = SessionRecord::Cancelled {
        schema_version: 1,
        seq: 2,
        time: "2026-07-10T12:00:02Z".to_string(),
        turn_id: "turn_1".to_string(),
        reason: "after malformed metadata".to_string(),
    }
    .to_json()
    .expect("serialize last record");
    let malformed = r#"{"type":"context_pin","schema_version":1,"seq":1,"time":"2026-07-10T12:00:01Z","item":{"id":"ctx_1","kind":"pinned_file","source_path":42,"byte_count":1,"token_estimate":17,"visibility":"pinned","reason":"pin"},"reason":"pin"}"#;

    std::fs::write(&path, format!("{first}\n{malformed}\n{last}\n")).expect("write session");

    let records = SessionReader::read_records(&path);
    assert_eq!(records.len(), 2);
    assert!(matches!(&records[0], SessionRecord::Cancelled { reason, .. } if reason == "before malformed metadata"));
    assert!(matches!(&records[1], SessionRecord::Cancelled { reason, .. } if reason == "after malformed metadata"));
}

#[test]
fn compaction_records_round_trip_for_manual_and_automatic_triggers() {
    for (seq, trigger) in [(5, CompactionTrigger::Manual), (6, CompactionTrigger::Automatic)] {
        let audit = compaction_audit(trigger);
        let record = SessionRecord::Compaction {
            schema_version: 1,
            seq,
            time: "2026-07-10T12:00:00Z".to_string(),
            audit: audit.clone(),
        };

        let json = record.to_json().expect("serialize compaction");
        let restored = SessionRecord::from_json(&json).expect("deserialize compaction");

        assert_eq!(record, restored);
        assert!(json.contains("\"type\":\"compaction\""));
        assert!(json.contains(match trigger {
            CompactionTrigger::Manual => "\"trigger\":\"manual\"",
            CompactionTrigger::Automatic => "\"trigger\":\"automatic\"",
        }));
        assert!(json.contains("Kept the build failure"));
        assert!(json.contains("ctx_tool_archive_47"));
        assert!(json.contains("\"input_tokens\":1024"));
        assert_eq!(audit.covered_start_seq, 12);
        assert_eq!(audit.covered_end_seq, 47);
    }
}

#[test]
fn compaction_review_record_round_trips_without_source_content() {
    let record = SessionRecord::CompactionReview {
        schema_version: SCHEMA_VERSION,
        seq: 4,
        time: "2026-07-11T12:00:00Z".to_string(),
        recovery_handle: "session:1..4".to_string(),
        review: CompactionReviewResult::Approved,
    };
    let json = record.to_json().expect("serialize review");
    let restored = SessionRecord::from_json(&json).expect("deserialize review");
    assert_eq!(restored, record);
    assert!(!json.contains("source"));
}

#[test]
fn writer_redacts_compaction_summary_and_preserves_transcript_reader() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "compaction-session");
    let mut audit = compaction_audit(CompactionTrigger::Automatic);
    audit.summary = "api_key=supersecretvalue; preserve the test failure".to_string();

    writer
        .append_entry(&Entry::User { text: "compact this".to_string() }, "turn_1")
        .expect("append user");
    writer.append_compaction(&audit).expect("append compaction");

    let records = SessionReader::read_records(writer.path());
    assert!(matches!(
        records.get(2),
        Some(SessionRecord::Compaction { audit: stored, .. })
            if stored.summary.contains("api_key=[REDACTED]") && stored.summary.contains("test failure")
    ));
    let transcript = SessionReader::read_transcript(writer.path());
    assert_eq!(transcript, vec![Entry::User { text: "compact this".to_string() }]);

    let content = std::fs::read_to_string(writer.path()).expect("read session");
    assert!(!content.contains("supersecretvalue"));
}

#[test]
fn reader_skips_corrupt_optional_compaction_fields_and_keeps_other_records() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("compaction.jsonl");
    let first = SessionRecord::Cancelled {
        schema_version: 1,
        seq: 0,
        time: "2026-07-10T12:00:00Z".to_string(),
        turn_id: "turn_1".to_string(),
        reason: "before corrupt compaction".to_string(),
    }
    .to_json()
    .expect("serialize first record");
    let last = SessionRecord::Cancelled {
        schema_version: 1,
        seq: 2,
        time: "2026-07-10T12:00:02Z".to_string(),
        turn_id: "turn_1".to_string(),
        reason: "after corrupt compaction".to_string(),
    }
    .to_json()
    .expect("serialize last record");
    let malformed = r#"{"type":"compaction","schema_version":1,"seq":1,"time":"2026-07-10T12:00:01Z","audit":{"summary":"summary","covered_start_seq":1,"covered_end_seq":3,"trigger":"automatic","risk":"low","model":"opencode/big-pickle","usage":"not an object"}}"#;

    std::fs::write(&path, format!("{first}\n{malformed}\n{last}\n")).expect("write session");

    let records = SessionReader::read_records(&path);
    assert_eq!(records.len(), 2);
    assert!(matches!(&records[0], SessionRecord::Cancelled { reason, .. } if reason == "before corrupt compaction"));
    assert!(matches!(&records[1], SessionRecord::Cancelled { reason, .. } if reason == "after corrupt compaction"));
}

#[test]
fn session_record_json_round_trip_assistant_finished() {
    let record = SessionRecord::AssistantFinished {
        schema_version: 1,
        seq: 6,
        time: "2026-06-29T12:00:08Z".to_string(),
        turn_id: "turn_1".to_string(),
        text: "Here is the entry point.".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"assistant_finished\""));
}

#[test]
fn session_record_json_round_trip_reasoning_finished() {
    let record = SessionRecord::ReasoningFinished {
        schema_version: 1,
        seq: 4,
        time: "2026-06-29T12:00:07Z".to_string(),
        turn_id: "turn_1".to_string(),
        text: "I should check src/main.rs first.".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"reasoning_finished\""));
}

#[test]
fn usage_record_json_round_trip() {
    let record = SessionRecord::Usage {
        schema_version: 1,
        seq: 6,
        time: "2026-06-29T12:00:08Z".to_string(),
        input_tokens: 123,
        output_tokens: 45,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"usage\""));
}

#[test]
fn request_accounting_record_round_trips_without_payload_content() {
    let mut accounting = ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:0",
        1,
        "anthropic",
        "model",
        b"{\"messages\":[]}",
        Vec::new(),
    );
    accounting.provider_usage =
        Some(ProviderUsageComponents::new(100, 12).normalize("anthropic", ProviderUsageRule::AnthropicMessages));
    let record = SessionRecord::RequestAccounting {
        schema_version: 1,
        seq: 7,
        time: "2026-06-29T12:00:09Z".to_string(),
        turn_id: "turn_1".to_string(),
        accounting: Box::new(accounting),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(!json.contains(r#"{"messages":[]}"#));
    assert!(json.contains("serialized_bytes"));
    assert!(json.contains("anthropic_input_plus_cache_components"));
}

#[test]
fn session_record_json_round_trip_tool_started() {
    let record = SessionRecord::ToolStarted {
        schema_version: 1,
        seq: 4,
        time: "2026-06-29T12:00:06Z".to_string(),
        turn_id: "turn_1".to_string(),
        call_id: "call_1".to_string(),
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"fn main"}"#.to_string(),
        mcp: None,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"tool_started\""));
}

#[test]
fn session_record_json_round_trip_cancelled() {
    let record = SessionRecord::Cancelled {
        schema_version: 1,
        seq: 7,
        time: "2026-06-29T12:00:09Z".to_string(),
        turn_id: "turn_1".to_string(),
        reason: "user pressed Escape".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"cancelled\""));
}

#[test]
fn session_record_json_round_trip_failed() {
    let record = SessionRecord::Failed {
        schema_version: 1,
        seq: 7,
        time: "2026-06-29T12:00:09Z".to_string(),
        turn_id: "turn_1".to_string(),
        error: "OPENCODE_ZEN_KEY is not set".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"failed\""));
}

#[test]
fn session_record_json_round_trip_session_renamed() {
    let record = SessionRecord::SessionRenamed {
        schema_version: 1,
        seq: 8,
        time: "2026-06-29T12:00:10Z".to_string(),
        title: "new session name".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"session_renamed\""));
}

#[test]
fn session_record_json_round_trip_context() {
    let record = SessionRecord::Context {
        schema_version: 1,
        seq: 1,
        time: "2026-06-29T12:00:02Z".to_string(),
        sources: vec![
            ContextSourceMeta {
                path: "/repo/AGENTS.md".to_string(),
                scope: ".".to_string(),
                content_hash: 12345,
                truncated: false,
                byte_count: 25,
            },
            ContextSourceMeta {
                path: "/repo/sub/AGENTS.md".to_string(),
                scope: "sub".to_string(),
                content_hash: 67890,
                truncated: true,
                byte_count: 40_000,
            },
        ],
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"context\""));
    assert!(json.contains("AGENTS.md"));
    assert!(json.contains("12345"));
    assert!(json.contains("67890"));
}

#[test]
fn session_record_from_json_rejects_malformed() {
    assert!(SessionRecord::from_json("not json").is_err());
}

#[test]
fn session_record_from_entry_skips_streaming() {
    let entry = Entry::Agent { text: "partial".to_string(), streaming: true };
    assert!(SessionRecord::from_entry(&entry, 1, "t", "turn_1").is_none());

    let entry = Entry::Agent { text: "done".to_string(), streaming: false };
    let record = SessionRecord::from_entry(&entry, 1, "t", "turn_1");
    assert!(record.is_some());
}

#[test]
fn session_record_to_entry_round_trip() {
    let entry = Entry::User { text: "hello".to_string() };
    let record = SessionRecord::from_entry(&entry, 0, "t", "turn_1").expect("record");
    let restored = record.to_entry().expect("entry");
    assert_eq!(entry, restored);
}

#[test]
fn writer_creates_file_and_appends_records() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "test-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains("\"type\":\"session_meta\""));
    assert!(content.contains("test-session"));

    writer
        .append_entry(&Entry::User { text: "hello".to_string() }, "turn_1")
        .expect("append");

    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains("\"type\":\"user\""));
    assert!(content.contains("hello"));
}

#[test]
fn writer_creates_session_file_in_custom_session_dir() {
    let workspace = tempfile::tempdir().expect("workspace");
    let custom_dir = workspace.path().join("state").join("sessions");
    let writer = SessionWriter::create(
        &custom_dir,
        "custom-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        Some(SessionConfigMeta { session_dir: Some(custom_dir.display().to_string()), ..SessionConfigMeta::default() }),
    )
    .expect("create writer");

    assert_eq!(writer.path(), custom_dir.join("custom-session.jsonl"));
    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains(&format!("\"session_dir\":\"{}\"", custom_dir.display())));
}

#[test]
fn writer_appends_acp_session_metadata() {
    let dir = tempfile::tempdir().expect("session dir");
    let mut writer = test_writer(dir.path(), "acp-session");
    writer
        .append_acp_session(&AcpSessionMetadata {
            agent_name: "local".to_string(),
            acp_session_id: "external-session".to_string(),
            command: "agent --acp".to_string(),
            protocol_version: "V1".to_string(),
            agent_info_name: Some("fake-agent".to_string()),
            agent_info_version: Some("0.0.0".to_string()),
            client_info_name: None,
            client_info_version: None,
        })
        .expect("append acp session");

    let records = SessionReader::read_records(writer.path());

    assert!(matches!(
        records.last(),
        Some(SessionRecord::AcpSession {
            local_session_id,
            agent_name,
            acp_session_id,
            command,
            ..
        }) if local_session_id == "acp-session"
            && agent_name == "local"
            && acp_session_id == "external-session"
            && command == "agent --acp"
    ));
}

#[test]
fn reader_transcript_shows_acp_client_metadata_when_present() {
    let dir = tempfile::tempdir().expect("session dir");
    let mut writer = test_writer(dir.path(), "acp-client-session");
    writer
        .append_acp_session(&AcpSessionMetadata {
            agent_name: "thndrs".to_string(),
            acp_session_id: "acp-session-00000001".to_string(),
            command: "thndrs acp serve".to_string(),
            protocol_version: "V1".to_string(),
            agent_info_name: None,
            agent_info_version: None,
            client_info_name: Some("zed".to_string()),
            client_info_version: Some("1.0.0".to_string()),
        })
        .expect("append acp session");

    let transcript = SessionReader::read_transcript(writer.path());

    assert!(
        transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text.contains("client zed")))
    );
}

#[test]
fn writer_appends_context_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "ctx-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let sources = vec![ContextSource {
        path: PathBuf::from("/repo/AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 999,
        truncated: false,
        byte_count: 10,
    }];
    writer.append_context(&sources).expect("append context");

    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains("\"type\":\"context\""));
    assert!(content.contains("AGENTS.md"));
    assert!(content.contains("999"));
}

#[test]
fn context_metadata_write_read_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "ctx-rt-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let sources = vec![
        ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: ".".to_string(),
            content: "# Project\nBuild with cargo.".to_string(),
            content_hash: 4242,
            truncated: false,
            byte_count: 25,
        },
        ContextSource {
            path: PathBuf::from("/repo/sub/AGENTS.md"),
            scope: "sub".to_string(),
            content: "x".repeat(100),
            content_hash: 17171,
            truncated: true,
            byte_count: 40_000,
        },
    ];
    writer.append_context(&sources).expect("append context");

    let path = writer.path().to_path_buf();
    drop(writer);

    let records = SessionReader::read_records(&path);
    let context_record = records
        .iter()
        .find(|r| matches!(r, SessionRecord::Context { .. }))
        .expect("should find a context record");

    let SessionRecord::Context { sources: metas, .. } = context_record else {
        panic!("expected Context record");
    };
    assert_eq!(metas.len(), 2);

    assert_eq!(metas[0].path, "/repo/AGENTS.md");
    assert_eq!(metas[0].scope, ".");
    assert_eq!(metas[0].content_hash, 4242);
    assert!(!metas[0].truncated);
    assert_eq!(metas[0].byte_count, 25);

    assert_eq!(metas[1].path, "/repo/sub/AGENTS.md");
    assert_eq!(metas[1].scope, "sub");
    assert_eq!(metas[1].content_hash, 17171);
    assert!(metas[1].truncated);
    assert_eq!(metas[1].byte_count, 40_000);

    let json = context_record.to_json().expect("serialize");
    assert!(
        !json.contains("Build with cargo"),
        "context record must not contain AGENTS.md content"
    );
}

#[test]
fn reader_reconstructs_transcript() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "replay-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    writer
        .append_entry(&Entry::User { text: "hello".to_string() }, "turn_1")
        .expect("append");
    writer
        .append_entry(
            &Entry::Agent { text: "hi there".to_string(), streaming: false },
            "turn_1",
        )
        .expect("append");

    let path = writer.path().to_path_buf();
    drop(writer);

    let transcript = SessionReader::read_transcript(&path);
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript[0], Entry::User { text: "hello".to_string() });
    assert_eq!(
        transcript[1],
        Entry::Agent { text: "hi there".to_string(), streaming: false }
    );
}

#[test]
fn reader_projects_tool_write_shell_and_status_rows() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "projection-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    writer
        .append_entry(&Entry::User { text: "run checks".to_string() }, "turn_1")
        .expect("append user");
    writer
        .append_tool_started("turn_1", "call_1", "run_shell", r#"{"program":"cargo"}"#)
        .expect("append tool start");
    writer
        .append_entry(
            &Entry::Tool {
                name: "run_shell#call_1".to_string(),
                arguments: r#"{"program":"cargo"}"#.to_string(),
                status: ToolStatus::Ok,
                output: vec!["ok".to_string()],
            },
            "turn_1",
        )
        .expect("append tool finish");

    let write = tools::WriteResult {
        op: tools::WriteOp::Create,
        path: PathBuf::from("/repo/new.txt"),
        before_hash: None,
        before_bytes: None,
        after_hash: 42,
        after_bytes: 3,
    };
    writer
        .append_file_write("turn_1", &write, ToolStatus::Ok)
        .expect("append file write");

    let shell = tools::shell::ProcessResult {
        process_id: None,
        command: vec!["cargo".to_string(), "test".to_string()],
        cwd: PathBuf::from("/repo"),
        status: tools::shell::ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec!["ignored by shell_exec record".to_string()],
        stderr: Vec::new(),
        output_truncated: false,
        elapsed: Duration::from_millis(123),
        kind: tools::shell::ProcessKind::OneShot,
    };
    writer.append_shell_exec("turn_1", &shell).expect("append shell exec");
    writer
        .append(SessionRecord::Cancelled {
            schema_version: 1,
            seq: 0,
            time: "t".to_string(),
            turn_id: "turn_1".to_string(),
            reason: "manual status".to_string(),
        })
        .expect("append status");
    writer
        .append_entry(&Entry::Agent { text: "done".to_string(), streaming: false }, "turn_1")
        .expect("append assistant");

    let path = writer.path().to_path_buf();
    drop(writer);

    let transcript = SessionReader::read_transcript(&path);
    assert_eq!(
        transcript.len(),
        6,
        "tool_started should not project to a transcript row"
    );
    assert_eq!(transcript[0], Entry::User { text: "run checks".to_string() });
    assert_eq!(
        transcript[1],
        Entry::Tool {
            name: "#call_1".to_string(),
            arguments: String::new(),
            status: ToolStatus::Ok,
            output: vec!["ok".to_string()],
        }
    );
    assert!(
        matches!(&transcript[2], Entry::Status { text } if text.contains("wrote create: /repo/new.txt")),
        "file write should project to a status row: {:?}",
        transcript[2]
    );
    assert!(
        matches!(&transcript[3], Entry::Status { text } if text == "shell ok: cargo test (123ms)"),
        "shell exec should project to a status row: {:?}",
        transcript[3]
    );
    assert_eq!(transcript[4], Entry::Status { text: "manual status".to_string() });
    assert_eq!(
        transcript[5],
        Entry::Agent { text: "done".to_string(), streaming: false }
    );
}

#[test]
fn reader_skips_corrupt_lines() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("corrupt.jsonl");

    std::fs::write(
        &path,
        "{\"type\":\"session_meta\",\"schema_version\":1,\"seq\":0,\"time\":\"t\",\"session_id\":\"c\",\"cwd\":\"/\",\"title\":\"x\",\"provider\":\"u\",\"model\":\"m\",\"websearch\":\"n\",\"app_version\":\"0\"}\n\
         this is not json\n\
         {\"type\":\"user\",\"schema_version\":1,\"seq\":1,\"time\":\"t\",\"turn_id\":\"t1\",\"text\":\"hello\"}\n",
    )
    .expect("write file");

    let records = SessionReader::read_records(&path);
    assert_eq!(records.len(), 2);

    let transcript = SessionReader::read_transcript(&path);
    assert_eq!(transcript.len(), 1);
    assert_eq!(transcript[0], Entry::User { text: "hello".to_string() });
}

#[test]
fn reader_returns_empty_for_missing_file() {
    assert!(SessionReader::read_records(Path::new("/nonexistent/file.jsonl")).is_empty());
}

#[test]
fn reader_preserves_record_order() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "order-session",
        "/repo",
        "scratch",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    writer
        .append_entry(&Entry::User { text: "first".to_string() }, "turn_1")
        .expect("append");
    writer
        .append_entry(&Entry::Agent { text: "second".to_string(), streaming: false }, "turn_1")
        .expect("append");
    writer
        .append_entry(&Entry::User { text: "third".to_string() }, "turn_2")
        .expect("append");

    let path = writer.path().to_path_buf();
    drop(writer);

    let transcript = SessionReader::read_transcript(&path);
    assert_eq!(transcript.len(), 3);
    assert_eq!(transcript[0], Entry::User { text: "first".to_string() });
    assert_eq!(
        transcript[1],
        Entry::Agent { text: "second".to_string(), streaming: false }
    );
    assert_eq!(transcript[2], Entry::User { text: "third".to_string() });
}

#[test]
fn reader_reads_title_from_session_meta() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = SessionWriter::create(
        dir.path(),
        "title-session",
        "/repo",
        "my title",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let path = writer.path().to_path_buf();
    drop(writer);

    let title = SessionReader::read_title(&path);
    assert_eq!(title, "my title");
}

#[test]
fn reader_reads_latest_renamed_title() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "rename-session",
        "/repo",
        "original",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    writer
        .append(SessionRecord::SessionRenamed {
            schema_version: 1,
            seq: 1,
            time: "t".to_string(),
            title: "renamed".to_string(),
        })
        .expect("append");

    let path = writer.path().to_path_buf();
    drop(writer);

    let title = SessionReader::read_title(&path);
    assert_eq!(title, "renamed");
}

#[test]
fn writer_appends_validated_rename_without_changing_session_identity() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "rename-session");
    let path = writer.path().to_path_buf();

    writer.append_rename("  focused work  ").expect("append rename");

    assert_eq!(writer.session_id(), "rename-session");
    assert_eq!(writer.path(), path);
    assert_eq!(SessionReader::read_title(&path), "focused work");
    assert!(matches!(
        SessionReader::read_records(&path).last(),
        Some(SessionRecord::SessionRenamed { title, .. }) if title == "focused work"
    ));
}

#[test]
fn writer_rejects_empty_oversized_and_control_character_names() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "rename-session");
    let original_sequence = writer.next_sequence();

    for invalid in [
        "   ".to_string(),
        "x".repeat(MAX_SESSION_NAME_CHARS + 1),
        "two\nlines".to_string(),
    ] {
        let error = writer.append_rename(&invalid).expect_err("invalid name");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
    assert_eq!(writer.next_sequence(), original_sequence);
}

#[test]
fn list_session_files_returns_jsonl_sorted_newest_first() {
    let dir = tempfile::tempdir().expect("temp dir");

    let _w1 = SessionWriter::create(
        dir.path(),
        "older",
        "/repo",
        "first",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    std::thread::sleep(std::time::Duration::from_millis(50));

    let _w2 = SessionWriter::create(
        dir.path(),
        "newer",
        "/repo",
        "second",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let files = list_session_files(dir.path());
    assert_eq!(files.len(), 2);
    assert!(files[0].to_string_lossy().contains("newer"));
    assert!(files[1].to_string_lossy().contains("older"));
}

#[test]
fn list_session_files_empty_for_missing_dir() {
    let files = list_session_files(Path::new("/nonexistent/dir"));
    assert!(files.is_empty());
}

#[test]
fn list_session_titles_returns_titles_newest_first() {
    let dir = tempfile::tempdir().expect("temp dir");
    let _w1 = SessionWriter::create(
        dir.path(),
        "s1",
        "/repo",
        "first session",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    std::thread::sleep(std::time::Duration::from_millis(50));

    let _w2 = SessionWriter::create(
        dir.path(),
        "s2",
        "/repo",
        "second session",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let titles = list_session_titles(dir.path());
    assert_eq!(titles.len(), 2);
    assert_eq!(titles[0], "second session");
    assert_eq!(titles[1], "first session");
}

#[test]
fn latest_session_file_returns_newest() {
    let dir = tempfile::tempdir().expect("temp dir");
    let _w1 = SessionWriter::create(
        dir.path(),
        "old",
        "/repo",
        "old",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let _w2 = SessionWriter::create(
        dir.path(),
        "new",
        "/repo",
        "new",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    let latest = latest_session_file(dir.path()).expect("should find latest");
    assert!(latest.to_string_lossy().contains("new"));
}

#[test]
fn latest_session_file_none_for_empty_dir() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(latest_session_file(dir.path()).is_none());
}

#[test]
fn generate_session_id_has_timestamp_prefix() {
    let id = generate_session_id();
    assert!(id.starts_with("session-"), "id should start with session-");
    assert_eq!(
        id.len(),
        23,
        "id should be 23 chars (session-YYYYMMDD-HHMMSS), got: {id} len={}",
        id.len()
    );
}

#[test]
fn sessions_dir_is_under_thndrs() {
    assert_eq!(
        sessions_dir(Path::new("/repo")),
        PathBuf::from("/repo/.thndrs/sessions")
    );
}

#[test]
fn resolve_session_file_accepts_exact_and_unique_prefixes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = test_writer(dir.path(), "session-alpha");
    let second = test_writer(dir.path(), "session-beta");
    drop(first);
    drop(second);

    let exact = resolve_session_file(dir.path(), "session-alpha").expect("exact lookup");
    let prefix = resolve_session_file(dir.path(), "session-b").expect("prefix lookup");

    assert_eq!(exact.file_stem().and_then(|stem| stem.to_str()), Some("session-alpha"));
    assert_eq!(prefix.file_stem().and_then(|stem| stem.to_str()), Some("session-beta"));
}

#[test]
fn resolve_session_file_rejects_ambiguous_and_missing_prefixes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = test_writer(dir.path(), "session-alpha");
    let second = test_writer(dir.path(), "session-alpine");
    drop(first);
    drop(second);

    assert!(matches!(
        resolve_session_file(dir.path(), "session-al"),
        Err(SessionLookupError::Ambiguous { matches, .. }) if matches.len() == 2
    ));
    assert!(matches!(
        resolve_session_file(dir.path(), "missing"),
        Err(SessionLookupError::NotFound { .. })
    ));
}

#[test]
fn resume_requires_an_exclusive_writer_lock() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = test_writer(dir.path(), "locked-session");
    let path = writer.path().to_path_buf();

    let error = SessionWriter::resume(&path, "locked-session").expect_err("second writer must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);

    drop(writer);
    let resumed = SessionWriter::resume(&path, "locked-session").expect("writer can resume after release");
    assert_eq!(resumed.session_id(), "locked-session");
}

#[test]
fn resume_rejects_corrupt_records_and_releases_the_writer_lock() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = test_writer(dir.path(), "corrupt-session");
    let path = writer.path().to_path_buf();
    drop(writer);
    std::fs::write(&path, "not json\n").expect("corrupt session");

    let error = SessionWriter::resume(&path, "corrupt-session").expect_err("corrupt session must fail");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!path.with_file_name("corrupt-session.jsonl.lock").exists());
}

#[test]
fn resume_rejects_a_session_identity_mismatch() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = test_writer(dir.path(), "stored-session");
    let path = writer.path().to_path_buf();
    drop(writer);

    let error = SessionWriter::resume(&path, "different-session").expect_err("identity mismatch must fail");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("identity mismatch"));
}

#[test]
fn redacted_records_and_log_tails_hide_secret_shaped_values_and_stay_bounded() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "redacted-session");
    writer
        .append_entry(&Entry::User { text: "token=sk-secretvalue123".to_string() }, "turn_1")
        .expect("append user");
    let records = SessionReader::read_redacted_records(writer.path());
    let rendered = serde_json::to_string(&records).expect("serialize records");
    assert!(rendered.contains("sk-[REDACTED]"));
    assert!(!rendered.contains("sk-secretvalue123"));

    let log = dir.path().join("debug.log");
    std::fs::write(&log, "one\ntoken=sk-secretvalue123\nthree\n").expect("write log");
    let tail = read_redacted_log_tail(&log, 2);
    assert_eq!(tail.len(), 2);
    assert!(tail[0].contains("sk-[REDACTED]"));
    assert!(!tail[0].contains("sk-secretvalue123"));
    assert!(read_redacted_log_tail(&log, 0).is_empty());
}

/// Helper: create a session writer in a temp dir.
fn test_writer(dir: &Path, name: &str) -> SessionWriter {
    SessionWriter::create(
        dir,
        name,
        "/repo",
        "test",
        "opencode-zen",
        "opencode/big-pickle",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer")
}

#[test]
fn file_write_record_json_round_trip() {
    let record = SessionRecord::FileWrite {
        schema_version: 1,
        seq: 5,
        time: "2026-06-29T12:00:06Z".to_string(),
        turn_id: "turn_1".to_string(),
        op: WriteOp::Create,
        path: "/repo/src/new.rs".to_string(),
        before_hash: None,
        before_bytes: None,
        after_hash: 99999,
        after_bytes: 42,
        status: ToolStatus::Ok,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"file_write\""));
    assert!(json.contains("\"create\""));
}

#[test]
fn file_write_record_round_trip_edit_op() {
    let record = SessionRecord::FileWrite {
        schema_version: 1,
        seq: 6,
        time: "2026-06-29T12:00:07Z".to_string(),
        turn_id: "turn_1".to_string(),
        op: WriteOp::Edit,
        path: "/repo/src/existing.rs".to_string(),
        before_hash: Some(11111),
        before_bytes: Some(100),
        after_hash: 22222,
        after_bytes: 120,
        status: ToolStatus::Ok,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"edit\""));
    assert!(json.contains("11111"));
    assert!(json.contains("22222"));
}

#[test]
fn file_write_record_round_trip_failed_status() {
    let record = SessionRecord::FileWrite {
        schema_version: 1,
        seq: 7,
        time: "2026-06-29T12:00:08Z".to_string(),
        turn_id: "turn_1".to_string(),
        op: WriteOp::Replace,
        path: "/repo/failed.txt".to_string(),
        before_hash: Some(33333),
        before_bytes: Some(50),
        after_hash: 44444,
        after_bytes: 60,
        status: ToolStatus::Failed,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"replace\""));
    assert!(json.contains("\"Failed\""));
}

#[test]
fn append_file_write_persists_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "fw-session");

    let result = tools::WriteResult {
        op: WriteOp::Create,
        path: PathBuf::from("/repo/src/new.rs"),
        before_hash: None,
        before_bytes: None,
        after_hash: 99999,
        after_bytes: 42,
    };

    writer
        .append_file_write("turn_1", &result, ToolStatus::Ok)
        .expect("append file write");

    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains("\"type\":\"file_write\""));
    assert!(content.contains("\"create\""));
    assert!(content.contains("/repo/src/new.rs"));
    assert!(content.contains("99999"));
    assert!(content.contains("42"));
    assert!(content.contains("\"Ok\""));
}

#[test]
fn registry_write_side_effect_persists_to_session_record() {
    let dir = tempfile::tempdir().expect("temp dir");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir(&workspace).expect("create workspace");
    let sessions = dir.path().join("sessions");
    std::fs::create_dir(&sessions).expect("create sessions");

    let request = tools::ToolUseRequest::new(
        "write_patch".to_string(),
        r#"{"patches":[{"op":"create","path":"audit.txt","content":"hello\n"}]}"#.to_string(),
        "call_1".to_string(),
    );
    let (output, write_result, shell_result) = tools::dispatch_full(&request, &workspace);
    let write_result = write_result.expect("registry write should return audit metadata");
    assert_eq!(output.status, ToolStatus::Ok);
    assert!(shell_result.is_none());

    let mut writer = test_writer(&sessions, "registry-fw-session");
    writer
        .append_file_write("turn_1", &write_result, output.status)
        .expect("append file write");

    let content = std::fs::read_to_string(writer.path()).expect("read session");
    assert!(content.contains("\"type\":\"file_write\""));
    assert!(content.contains("\"create\""));
    assert!(content.contains("audit.txt"));
    assert!(content.contains("\"Ok\""));
}

#[test]
fn append_file_write_persists_before_and_after_for_edit() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "fw-edit-session");

    let result = tools::WriteResult {
        op: WriteOp::Edit,
        path: PathBuf::from("/repo/src/existing.rs"),
        before_hash: Some(11111),
        before_bytes: Some(100),
        after_hash: 22222,
        after_bytes: 120,
    };

    writer
        .append_file_write("turn_1", &result, ToolStatus::Ok)
        .expect("append file write");

    let path = writer.path().to_path_buf();
    drop(writer);

    let records = SessionReader::read_records(&path);
    let fw = records
        .iter()
        .find(|r| matches!(r, SessionRecord::FileWrite { .. }))
        .expect("should find file_write record");

    let SessionRecord::FileWrite { op, path, before_hash, before_bytes, after_hash, after_bytes, status, .. } = fw
    else {
        panic!("expected FileWrite record");
    };

    assert_eq!(*op, WriteOp::Edit);
    assert_eq!(path, "/repo/src/existing.rs");
    assert_eq!(*before_hash, Some(11111));
    assert_eq!(*before_bytes, Some(100));
    assert_eq!(*after_hash, 22222);
    assert_eq!(*after_bytes, 120);
    assert_eq!(*status, ToolStatus::Ok);
}

#[test]
fn file_write_record_does_not_persist_file_content() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "fw-no-content");

    let result = tools::WriteResult {
        op: WriteOp::Create,
        path: PathBuf::from("/repo/output.txt"),
        before_hash: None,
        before_bytes: None,
        after_hash: 12345,
        after_bytes: 30,
    };

    writer
        .append_file_write("turn_1", &result, ToolStatus::Ok)
        .expect("append file write");

    let json = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(
        !json.contains("TOP_SECRET_CONTENT"),
        "file_write record must not contain file content"
    );
    assert!(json.contains("12345"), "should contain after_hash");
    assert!(json.contains("30"), "should contain after_bytes");
}

#[test]
fn file_write_record_maps_to_status_entry_on_resume() {
    let record = SessionRecord::FileWrite {
        schema_version: 1,
        seq: 3,
        time: "t".to_string(),
        turn_id: "turn_1".to_string(),
        op: WriteOp::Create,
        path: "/repo/new.txt".to_string(),
        before_hash: None,
        before_bytes: None,
        after_hash: 1,
        after_bytes: 5,
        status: ToolStatus::Ok,
    };
    let entry = record.to_entry().expect("should map to entry");
    match entry {
        Entry::Status { text } => {
            assert!(text.contains("create"), "status text should mention the op");
            assert!(text.contains("/repo/new.txt"), "status text should mention the path");
        }
        _ => panic!("expected Status entry for file_write, got {entry:?}"),
    }
}

#[test]
fn file_write_failed_record_maps_to_status_entry_on_resume() {
    let record = SessionRecord::FileWrite {
        schema_version: 1,
        seq: 3,
        time: "t".to_string(),
        turn_id: "turn_1".to_string(),
        op: WriteOp::Edit,
        path: "/repo/failed.txt".to_string(),
        before_hash: Some(1),
        before_bytes: Some(10),
        after_hash: 2,
        after_bytes: 10,
        status: ToolStatus::Failed,
    };
    let entry = record.to_entry().expect("should map to entry");
    match entry {
        Entry::Status { text } => {
            assert!(text.contains("write failed"), "failed write should mention failure");
        }
        _ => panic!("expected Status entry for failed file_write, got {entry:?}"),
    }
}

#[test]
fn append_tool_started_persists_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "tool-started-session");

    writer
        .append_tool_started(
            "turn_1",
            "call_1",
            "run_shell",
            r#"{"program":"cargo","args":["test"]}"#,
        )
        .expect("append tool started");

    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains("\"type\":\"tool_started\""));
    assert!(content.contains("run_shell"));
    assert!(content.contains("call_1"));
    assert!(content.contains("cargo"));
}

#[test]
fn append_mcp_tool_records_include_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "mcp-tool-session");

    writer
        .append_tool_started("turn_1", "call_1", "mcp__docs__search", r#"{"query":"rust"}"#)
        .expect("append tool started");

    let entry = Entry::Tool {
        name: "mcp__docs__search#call_1".to_string(),
        arguments: r#"{"query":"rust"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["ok".to_string()],
    };
    writer.append_entry(&entry, "turn_1").expect("append finished");

    let records = SessionReader::read_records(writer.path());
    assert!(records.iter().any(|record| matches!(
        record,
        SessionRecord::ToolStarted {
            mcp: Some(McpToolSessionMeta { server_name, original_tool_name }),
            ..
        } if server_name == "docs" && original_tool_name == "search"
    )));
    assert!(records.iter().any(|record| matches!(
        record,
        SessionRecord::ToolFinished {
            mcp: Some(McpToolSessionMeta { server_name, original_tool_name }),
            ..
        } if server_name == "docs" && original_tool_name == "search"
    )));
}

#[test]
fn append_tool_started_round_trip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "tool-started-rt");

    writer
        .append_tool_started("turn_1", "call_1", "search_text", r#"{"pattern":"fn main"}"#)
        .expect("append tool started");

    let path = writer.path().to_path_buf();
    drop(writer);

    let records = SessionReader::read_records(&path);
    let ts = records
        .iter()
        .find(|r| matches!(r, SessionRecord::ToolStarted { .. }))
        .expect("should find tool_started record");

    let SessionRecord::ToolStarted { call_id, name, arguments, .. } = ts else {
        panic!("expected ToolStarted record");
    };

    assert_eq!(call_id, "call_1");
    assert_eq!(name, "search_text");
    assert!(arguments.contains("fn main"));
}

#[test]
fn tool_started_record_json_round_trip() {
    let record = SessionRecord::ToolStarted {
        schema_version: 1,
        seq: 4,
        time: "2026-06-29T12:00:06Z".to_string(),
        turn_id: "turn_1".to_string(),
        call_id: "call_1".to_string(),
        name: "run_shell".to_string(),
        arguments: r#"{"program":"echo","args":["hello"]}"#.to_string(),
        mcp: None,
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"tool_started\""));
    assert!(json.contains("run_shell"));
}

#[test]
fn tool_started_does_not_map_to_entry_on_resume() {
    let record = SessionRecord::ToolStarted {
        schema_version: 1,
        seq: 3,
        time: "t".to_string(),
        turn_id: "turn_1".to_string(),
        call_id: "call_1".to_string(),
        name: "run_shell".to_string(),
        arguments: "{}".to_string(),
        mcp: None,
    };
    assert!(record.to_entry().is_none(), "tool_started should not map to an entry");
}

#[test]
fn shell_exec_record_json_round_trip() {
    let record = SessionRecord::ShellExec {
        schema_version: 1,
        seq: 5,
        time: "2026-06-29T12:00:06Z".to_string(),
        turn_id: "turn_1".to_string(),
        process_id: None,
        command: "cargo test".to_string(),
        cwd: "/repo".to_string(),
        process_status: "ok".to_string(),
        exit_code: Some(0),
        elapsed_ms: 1200,
        kind: "one-shot".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"shell_exec\""));
    assert!(json.contains("cargo test"));
    assert!(json.contains("\"ok\""));
    assert!(json.contains("1200"));
}

#[test]
fn shell_exec_record_round_trip_timeout() {
    let record = SessionRecord::ShellExec {
        schema_version: 1,
        seq: 6,
        time: "2026-06-29T12:00:07Z".to_string(),
        turn_id: "turn_1".to_string(),
        process_id: None,
        command: "sleep 30".to_string(),
        cwd: "/repo".to_string(),
        process_status: "timeout".to_string(),
        exit_code: None,
        elapsed_ms: 10000,
        kind: "one-shot".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"timeout\""));
}

#[test]
fn shell_exec_record_round_trip_cancelled() {
    let record = SessionRecord::ShellExec {
        schema_version: 1,
        seq: 7,
        time: "2026-06-29T12:00:08Z".to_string(),
        turn_id: "turn_1".to_string(),
        process_id: Some(7),
        command: "cargo build".to_string(),
        cwd: "/repo".to_string(),
        process_status: "cancelled".to_string(),
        exit_code: None,
        elapsed_ms: 500,
        kind: "background".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"cancelled\""));
    assert!(json.contains("\"background\""));
}

#[test]
fn shell_exec_record_round_trip_failed() {
    let record = SessionRecord::ShellExec {
        schema_version: 1,
        seq: 8,
        time: "2026-06-29T12:00:09Z".to_string(),
        turn_id: "turn_1".to_string(),
        process_id: None,
        command: "false".to_string(),
        cwd: "/repo".to_string(),
        process_status: "failed".to_string(),
        exit_code: Some(1),
        elapsed_ms: 10,
        kind: "one-shot".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"failed\""));
    assert!(json.contains("1"));
}

#[test]
fn append_shell_exec_preserves_background_start_and_terminal_lifecycle() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "background-lifecycle-session");
    let running = tools::shell::ProcessResult {
        process_id: Some(11),
        command: vec!["sleep".to_string(), "30".to_string()],
        cwd: PathBuf::from("/repo"),
        status: tools::shell::ProcessStatus::Running,
        exit_code: None,
        stdout: vec![],
        stderr: vec![],
        output_truncated: false,
        elapsed: Duration::from_millis(2),
        kind: tools::shell::ProcessKind::Background,
    };
    let terminal = tools::shell::ProcessResult {
        status: tools::shell::ProcessStatus::Failed,
        exit_code: Some(3),
        elapsed: Duration::from_millis(40),
        ..running.clone()
    };

    writer.append_shell_exec("turn_1", &running).expect("append start");
    writer.append_shell_exec("turn_1", &terminal).expect("append terminal");

    let records = SessionReader::read_records(writer.path());
    let shell_records = records
        .iter()
        .filter_map(|record| match record {
            SessionRecord::ShellExec { process_id, process_status, .. } => Some((*process_id, process_status.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shell_records, vec![(Some(11), "running"), (Some(11), "failed")]);
}

#[test]
fn append_shell_exec_persists_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "shell-session");

    let result = tools::shell::ProcessResult {
        process_id: None,
        command: vec!["cargo".to_string(), "test".to_string()],
        cwd: PathBuf::from("/repo"),
        status: tools::shell::ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec![],
        stderr: vec![],
        output_truncated: false,
        elapsed: Duration::from_millis(1200),
        kind: tools::shell::ProcessKind::OneShot,
    };

    writer.append_shell_exec("turn_1", &result).expect("append shell exec");

    let content = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(content.contains("\"type\":\"shell_exec\""));
    assert!(content.contains("cargo test"));
    assert!(content.contains("/repo"));
    assert!(content.contains("\"ok\""));
    assert!(content.contains("1200"));
    assert!(content.contains("\"one-shot\""));
}

#[test]
fn registry_shell_side_effect_persists_to_session_record() {
    let dir = tempfile::tempdir().expect("temp dir");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir(&workspace).expect("create workspace");
    let sessions = dir.path().join("sessions");
    std::fs::create_dir(&sessions).expect("create sessions");

    let request = tools::ToolUseRequest::new(
        "run_shell".to_string(),
        r#"{"program":"echo","args":["audit"]}"#.to_string(),
        "call_1".to_string(),
    );
    let (output, write_result, shell_result) = tools::dispatch_full(&request, &workspace);
    let shell_result = shell_result.expect("registry shell should return audit metadata");
    assert_eq!(output.status, ToolStatus::Ok);
    assert!(write_result.is_none());

    let mut writer = test_writer(&sessions, "registry-shell-session");
    writer
        .append_shell_exec("turn_1", &shell_result)
        .expect("append shell exec");

    let content = std::fs::read_to_string(writer.path()).expect("read session");
    assert!(content.contains("\"type\":\"shell_exec\""));
    assert!(content.contains("echo audit"));
    assert!(content.contains("\"ok\""));
    assert!(content.contains("\"one-shot\""));
}

#[test]
fn append_shell_exec_round_trip_for_timeout() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "shell-timeout-session");

    let result = tools::shell::ProcessResult {
        process_id: None,
        command: vec!["sleep".to_string(), "30".to_string()],
        cwd: PathBuf::from("/repo"),
        status: tools::shell::ProcessStatus::Timeout,
        exit_code: None,
        stdout: vec![],
        stderr: vec![],
        output_truncated: false,
        elapsed: Duration::from_millis(10000),
        kind: tools::shell::ProcessKind::OneShot,
    };

    writer.append_shell_exec("turn_1", &result).expect("append shell exec");

    let path = writer.path().to_path_buf();
    drop(writer);

    let records = SessionReader::read_records(&path);
    let se = records
        .iter()
        .find(|r| matches!(r, SessionRecord::ShellExec { .. }))
        .expect("should find shell_exec record");

    let SessionRecord::ShellExec { command, process_status, exit_code, elapsed_ms, .. } = se else {
        panic!("expected ShellExec record");
    };

    assert_eq!(command, "sleep 30");
    assert_eq!(*process_status, "timeout");
    assert!(exit_code.is_none());
    assert_eq!(*elapsed_ms, 10000);
}

#[test]
fn shell_exec_record_does_not_store_stdout_stderr() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "shell-no-output");

    let result = tools::shell::ProcessResult {
        process_id: None,
        command: vec!["echo".to_string()],
        cwd: PathBuf::from("/repo"),
        status: tools::shell::ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec!["SECRET_OUTPUT_LINE".to_string()],
        stderr: vec!["SECRET_STDERR".to_string()],
        output_truncated: false,
        elapsed: Duration::from_millis(10),
        kind: tools::shell::ProcessKind::OneShot,
    };

    writer.append_shell_exec("turn_1", &result).expect("append shell exec");

    let json = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(
        !json.contains("SECRET_OUTPUT_LINE"),
        "shell_exec record must not store stdout"
    );
    assert!(
        !json.contains("SECRET_STDERR"),
        "shell_exec record must not store stderr"
    );
}

#[test]
fn shell_exec_record_maps_to_status_entry_on_resume() {
    let record = SessionRecord::ShellExec {
        schema_version: 1,
        seq: 3,
        time: "t".to_string(),
        turn_id: "turn_1".to_string(),
        process_id: None,
        command: "cargo test".to_string(),
        cwd: "/repo".to_string(),
        process_status: "ok".to_string(),
        exit_code: Some(0),
        elapsed_ms: 500,
        kind: "one-shot".to_string(),
    };
    let entry = record.to_entry().expect("should map to entry");
    match entry {
        Entry::Status { text } => {
            assert!(text.contains("shell"), "status text should mention shell");
            assert!(text.contains("cargo test"), "status text should mention command");
            assert!(text.contains("ok"), "status text should mention status");
            assert!(text.contains("500ms"), "status text should mention elapsed time");
        }
        _ => panic!("expected Status entry for shell_exec, got {entry:?}"),
    }
}

#[test]
fn shell_exec_failed_record_maps_to_status_entry_on_resume() {
    let record = SessionRecord::ShellExec {
        schema_version: 1,
        seq: 4,
        time: "t".to_string(),
        turn_id: "turn_1".to_string(),
        process_id: None,
        command: "false".to_string(),
        cwd: "/repo".to_string(),
        process_status: "failed".to_string(),
        exit_code: Some(1),
        elapsed_ms: 10,
        kind: "one-shot".to_string(),
    };
    let entry = record.to_entry().expect("should map to entry");
    match entry {
        Entry::Status { text } => {
            assert!(text.contains("failed"), "failed shell should mention failure");
        }
        _ => panic!("expected Status entry for failed shell_exec, got {entry:?}"),
    }
}

#[test]
fn append_skill_activation_persists_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "skill-activation");
    let activation = SkillActivation {
        name: "example-skill".to_string(),
        path: PathBuf::from("/repo/.thndrs/skills/example-skill/SKILL.md"),
        content_hash: 4242,
        byte_count: 128,
        rendered_content_hash: 9999,
        rendered_byte_count: 256,
        loaded_references: Vec::new(),
    };

    writer
        .append_skill_activation(&activation)
        .expect("append skill activation");

    let records = SessionReader::read_records(writer.path());
    assert!(records.iter().any(|record| matches!(
        record,
        SessionRecord::SkillActivated {
            name,
            content_hash: 4242,
            byte_count: 128,
            rendered_content_hash: 9999,
            rendered_byte_count: 256,
            ..
        }
            if name == "example-skill"
    )));
}

#[test]
fn skill_activation_record_defaults_rendered_metadata_for_old_sessions() {
    let json = r#"{"type":"skill_activated","schema_version":1,"seq":1,"time":"2026-07-03T00:00:00Z","name":"example-skill","path":"/repo/.thndrs/skills/example-skill/SKILL.md","content_hash":4242,"byte_count":128,"loaded_references":[]}"#;

    let restored = SessionRecord::from_json(json).expect("deserialize old skill activation");

    assert!(matches!(
        restored,
        SessionRecord::SkillActivated {
            content_hash: 4242,
            byte_count: 128,
            rendered_content_hash: 0,
            rendered_byte_count: 0,
            ..
        }
    ));
}

#[test]
fn read_records_from_tail_discards_partial_record_and_keeps_recent_records() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = test_writer(dir.path(), "tail-records");
    writer
        .append_entry(&Entry::Agent { text: "x".repeat(4_096), streaming: false }, "turn_1")
        .expect("append large response");
    writer
        .append_entry(&Entry::User { text: "recent prompt".to_string() }, "turn_2")
        .expect("append recent prompt");

    let records = SessionReader::read_records_from_tail(writer.path(), 512);

    assert!(records.iter().any(|record| matches!(
        record,
        SessionRecord::User { text, .. } if text == "recent prompt"
    )));
}
