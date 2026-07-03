use crate::app::{App, Entry, RunState, ToolStatus};
use crate::cli::{Cli, Theme, WebSearchMode};
use crate::renderer::view::build_view;
use std::path::PathBuf;

fn test_app() -> App {
    let mut app = App::from_cli(&Cli {
        cwd: PathBuf::from("."),
        model: "test-model".to_string(),
        websearch: WebSearchMode::Native,
        tick_rate_ms: 100,
        no_alt_screen: true,
        no_mouse: false,
        mouse: false,
        verbose: false,
        theme: Theme::EldritchMinimal,
        print_prompt: false,
        skill_dirs: Vec::new(),
    });
    app.session_id = "test-session".to_string();
    app.git_status = Some(crate::renderer::git::GitStatusSummary {
        branch: Some("main".to_string()),
        added: 0,
        modified: 0,
        deleted: 0,
    });
    app.transcript.clear();
    app.context_sources.clear();
    app.skills.clear();
    app.skill_diagnostics.clear();
    app
}

#[test]
fn build_view_idle_has_empty_transcript_and_banner() {
    let app = test_app();
    let view = build_view(&app, 80, 24);

    assert!(
        !view.transcript.banner_rows.is_empty(),
        "empty transcript should produce banner rows"
    );
    assert!(
        view.transcript.stable_rows.is_empty(),
        "stable_rows should be empty when transcript is empty"
    );
    assert!(
        view.transcript.live_rows.is_empty(),
        "live_rows should be empty when transcript is empty"
    );
    assert!(!view.live.prompt_rows.is_empty(), "live view should have prompt rows");
    assert!(
        !view.live.static_status.text().is_empty(),
        "live view should have a static status row"
    );
}

#[test]
fn build_view_submitted_user_is_stable() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::User { text: "do the thing".to_string() });

    let view = build_view(&app, 80, 24);

    assert!(
        view.transcript.banner_rows.is_empty(),
        "banner rows should not be exposed once transcript is non-empty"
    );
    assert!(
        !view.transcript.stable_rows.is_empty(),
        "submitted user entry should appear in stable_rows"
    );
    assert!(
        view.transcript.live_rows.is_empty(),
        "finished user entry should have no live rows"
    );
    assert!(
        view.transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("do the thing")),
        "stable_rows should contain the user text"
    );
}

#[test]
fn build_view_streaming_assistant_splits_stable_and_live() {
    let mut app = test_app();
    app.run_state = RunState::Working;

    let text = "line one. line two. line three. line four. line five. line six. line seven. line eight.";
    app.transcript
        .push(Entry::Assistant { text: text.to_string(), streaming: true });

    let view = build_view(&app, 80, 24);

    assert!(
        view.transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("Assistant")),
        "stable_rows should contain the assistant header"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("line seven") || row.text().contains("line eight")),
        "live_rows should contain the mutable tail"
    );
    assert!(
        !view
            .transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Assistant")),
        "live_rows should not contain the assistant header"
    );
}

#[test]
fn build_view_streaming_assistant_short_block_is_all_live() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::Assistant { text: "short".to_string(), streaming: true });

    let view = build_view(&app, 80, 24);

    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Assistant")),
        "live_rows should contain the assistant header for a short block"
    );
    assert!(
        view.transcript.live_rows.iter().any(|row| row.text().contains("short")),
        "live_rows should contain the short streaming text"
    );
    assert!(
        !view
            .transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("Assistant")),
        "stable_rows should not contain the assistant header for a short block"
    );
}

#[test]
fn build_view_running_tool_is_live_only() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec!["running tests".to_string()],
    });

    let view = build_view(&app, 80, 24);

    assert!(
        !view.transcript.live_rows.is_empty(),
        "running tool should produce live rows"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("running tests")),
        "live_rows should contain the tool output"
    );
    assert!(
        !view
            .transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("run_shell")),
        "stable_rows should not contain the running tool header"
    );
}

#[test]
fn build_view_narrow_changes_row_counts() {
    let mut app = test_app();
    app.input.set_text("a longer prompt that should wrap at narrow width");
    app.transcript
        .push(Entry::User { text: "This is a user message that will wrap differently at narrow width.".to_string() });

    let wide = build_view(&app, 80, 24);
    let narrow = build_view(&app, 40, 24);

    assert_ne!(
        wide.transcript.stable_rows.len(),
        narrow.transcript.stable_rows.len(),
        "narrow width should change stable row count"
    );
    assert_ne!(
        wide.live.prompt_rows.len(),
        narrow.live.prompt_rows.len(),
        "narrow width should change prompt row count"
    );
    assert!(
        narrow.live.prompt_cursor.is_some(),
        "prompt cursor should still be present at narrow width"
    );
}

#[test]
fn build_view_preserves_cursor_for_editable_prompt() {
    let mut app = test_app();
    app.input.set_text("hello world");
    let view = build_view(&app, 80, 24);

    assert!(
        view.live.prompt_cursor.is_some(),
        "editable prompt should have a cursor in the view"
    );
}
