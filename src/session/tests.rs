use super::*;
use crate::cli::WebSearchMode;
use crate::prompt::PromptBundle;
use Path;
use PathBuf;

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

#[test]
fn session_record_json_round_trip_session_meta() {
    let record = SessionRecord::SessionMeta {
        schema_version: 1,
        seq: 0,
        time: "2026-06-29T12:00:00Z".to_string(),
        session_id: "test-1".to_string(),
        cwd: "/repo".to_string(),
        title: "scratch".to_string(),
        provider: "umans".to_string(),
        model: "umans-coder".to_string(),
        websearch: "native".to_string(),
        app_version: "0.1.0".to_string(),
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"session_meta\""));
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
    };
    let json = record.to_json().expect("serialize");
    let restored = SessionRecord::from_json(&json).expect("deserialize");
    assert_eq!(record, restored);
    assert!(json.contains("\"type\":\"tool_finished\""));
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
fn session_record_json_round_trip_tool_started() {
    let record = SessionRecord::ToolStarted {
        schema_version: 1,
        seq: 4,
        time: "2026-06-29T12:00:06Z".to_string(),
        turn_id: "turn_1".to_string(),
        call_id: "call_1".to_string(),
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"fn main"}"#.to_string(),
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
        error: "UMANS_API_KEY is not set".to_string(),
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
    let entry = Entry::Assistant { text: "partial".to_string(), streaming: true };
    assert!(SessionRecord::from_entry(&entry, 1, "t", "turn_1").is_none());

    let entry = Entry::Assistant { text: "done".to_string(), streaming: false };
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
fn writer_appends_context_metadata() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = SessionWriter::create(
        dir.path(),
        "ctx-session",
        "/repo",
        "scratch",
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
    )
    .expect("create writer");

    writer
        .append_entry(&Entry::User { text: "hello".to_string() }, "turn_1")
        .expect("append");
    writer
        .append_entry(
            &Entry::Assistant { text: "hi there".to_string(), streaming: false },
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
        Entry::Assistant { text: "hi there".to_string(), streaming: false }
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
    )
    .expect("create writer");

    writer
        .append_entry(&Entry::User { text: "first".to_string() }, "turn_1")
        .expect("append");
    writer
        .append_entry(
            &Entry::Assistant { text: "second".to_string(), streaming: false },
            "turn_1",
        )
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
        Entry::Assistant { text: "second".to_string(), streaming: false }
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
fn list_session_files_returns_jsonl_sorted_newest_first() {
    let dir = tempfile::tempdir().expect("temp dir");

    let _w1 = SessionWriter::create(
        dir.path(),
        "older",
        "/repo",
        "first",
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
    )
    .expect("create writer");

    std::thread::sleep(std::time::Duration::from_millis(50));

    let _w2 = SessionWriter::create(
        dir.path(),
        "newer",
        "/repo",
        "second",
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
    )
    .expect("create writer");

    std::thread::sleep(std::time::Duration::from_millis(50));

    let _w2 = SessionWriter::create(
        dir.path(),
        "s2",
        "/repo",
        "second session",
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
    )
    .expect("create writer");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let _w2 = SessionWriter::create(
        dir.path(),
        "new",
        "/repo",
        "new",
        "umans",
        "umans-coder",
        "native",
        "0.1.0",
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
