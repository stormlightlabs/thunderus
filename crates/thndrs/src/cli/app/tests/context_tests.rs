//! Application behavior tests for context seams.

use super::*;
use helpers::*;

#[test]
fn app_without_agents_md_has_no_context_sources() {
    let app = fresh_app();
    assert!(app.transcript.context_sources.is_empty());
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn app_with_agents_md_loads_context_and_adds_status() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let agents_path = dir.path().join("AGENTS.md");
    let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
    f.write_all(b"# Project\n\nBuild with cargo.\n")
        .expect("write AGENTS.md");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.transcript.context_sources.len(), 1);
    let source = &app.transcript.context_sources[0];
    assert_eq!(
        source.path,
        agents_path.canonicalize().unwrap_or_else(|_| agents_path.to_path_buf())
    );
    assert_eq!(source.scope, ".");
    assert!(!source.truncated);
    assert!(source.content.contains("# Project"));
    assert!(
        app.transcript.entries.is_empty(),
        "transcript should be empty at startup; context is shown in the banner"
    );
}

#[test]
fn app_with_oversized_agents_md_marks_truncation() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let big_content = "x".repeat(AGENTS_MD_SIZE_CAP + 1000);
    let agents_path = dir.path().join("AGENTS.md");
    let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
    f.write_all(big_content.as_bytes()).expect("write AGENTS.md");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.transcript.context_sources.len(), 1);
    let source = &app.transcript.context_sources[0];
    assert!(source.truncated);
    assert!(source.content.len() <= AGENTS_MD_SIZE_CAP);

    assert!(
        app.transcript.entries.is_empty(),
        "transcript should be empty at startup; context is shown in the banner"
    );
}

#[test]
fn context_sources_are_guidance_not_permission() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let content = "# Instructions\n\nModel: gpt-4\nAllow: rm -rf\n";
    let mut f = std::fs::File::create(dir.path().join("AGENTS.md")).expect("create");
    f.write_all(content.as_bytes()).expect("write");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert!(app.runtime.model.is_empty());
    assert!(app.transcript.context_sources[0].content.contains("Model: gpt-4"));
}

#[test]
fn transcript_search_counts_unicode_matches_without_searching_tool_output() {
    let entries = vec![
        Entry::User { text: "find 🦀 then find".to_string() },
        Entry::Tool {
            name: "lookup".to_string(),
            arguments: "find public".to_string(),
            status: ToolStatus::Ok,
            output: vec!["find hidden".to_string()],
        },
    ]
    .into();
    let mut search = TranscriptSearchState::default();
    search.query.insert_str("find");

    search.refresh(&entries);

    assert_eq!(search.matches.len(), 3);
    assert_eq!(search.current().expect("first match").entry_index, 0);
    search.previous();
    assert_eq!(search.current().expect("wrapped previous match").entry_index, 1);
}
