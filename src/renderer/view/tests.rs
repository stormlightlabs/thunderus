use crate::app::{
    App, DetailPane, Entry, FilePickerSource, PickerItem, PickerState, PromptAccessory, RunState, ToolStatus,
};
use crate::cli::{Cli, Theme, WebSearchMode};
use crate::renderer;
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
        session_dir: None,
    });
    app.session_id = "test-session".to_string();
    app.git_status =
        Some(renderer::git::GitStatusSummary { branch: Some("main".to_string()), added: 0, modified: 0, deleted: 0 });
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
fn build_view_streaming_assistant_is_all_live() {
    let mut app = test_app();
    app.run_state = RunState::Working;

    let text = "line one. line two. line three. line four. line five. line six. line seven. line eight.";
    app.transcript
        .push(Entry::Agent { text: text.to_string(), streaming: true });

    let view = build_view(&app, 80, 24);

    assert!(
        !view
            .transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("Agent") || row.text().contains("line one")),
        "streaming assistant rows should not be stable"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("line seven") || row.text().contains("line eight")),
        "live_rows should contain the mutable tail"
    );
    assert!(
        view.transcript.live_rows.iter().any(|row| row.text().contains("Agent")),
        "live_rows should contain the agent header"
    );
}

#[test]
fn build_view_long_streaming_reasoning_is_all_live() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::Reasoning {
        text: (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"),
        streaming: true,
    });

    let view = build_view(&app, 32, 24);

    assert!(
        !view
            .transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("Thinking") || row.text().contains("line 0")),
        "long streaming reasoning should not commit a stable prefix"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Thinking")),
        "live rows should include the reasoning header"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("line 39")),
        "live rows should include the mutable reasoning tail"
    );
}

#[test]
fn build_view_streaming_assistant_short_block_is_all_live() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::Agent { text: "short".to_string(), streaming: true });

    let view = build_view(&app, 80, 24);

    assert!(
        view.transcript.live_rows.iter().any(|row| row.text().contains("Agent")),
        "live_rows should contain the agent header for a short block"
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
            .any(|row| row.text().contains("Agent")),
        "stable_rows should not contain the agent header for a short block"
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

#[test]
fn build_view_prompt_clipping_keeps_cursor_row() {
    let mut app = test_app();
    app.input
        .set_text(&(0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));

    let view = build_view(&app, 80, 24);
    let text = view
        .live
        .prompt_rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(view.live.prompt_rows.len(), renderer::live::MAX_PROMPT_ROWS);
    assert!(
        text.contains("line 19"),
        "prompt clipping should keep the editable tail:\n{text}"
    );
    assert_eq!(
        view.live.prompt_cursor.map(|cursor| cursor.row),
        Some(renderer::live::MAX_PROMPT_ROWS - 1),
        "cursor row should be rebased into the clipped prompt rows"
    );
}

#[test]
fn build_view_working_state_has_live_tail_and_status() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::Agent {
        text: "streaming text that is currently being generated by the model".to_string(),
        streaming: true,
    });

    let view = build_view(&app, 80, 24);

    assert!(!view.live.live_tail.is_empty(), "working state should have a live tail");
    assert!(
        !view.live.dynamic_status.text().is_empty(),
        "dynamic status should be present during working state"
    );
}

#[test]
fn build_view_streaming_with_tool_has_live_tail_and_running_status() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec!["test running ...".to_string()],
    });

    let view = build_view(&app, 80, 24);

    assert!(
        !view.live.live_tail.is_empty(),
        "running tool should appear in live tail"
    );
    assert!(
        view.live.live_tail.iter().any(|r| r.text().contains("run_shell")),
        "live tail should contain the tool name"
    );
}

#[test]
fn build_view_accessory_surfaces_are_present_when_active() {
    let mut app = test_app();
    app.input.set_text("@src");
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Forced);
    app.picker = Some(PickerState::new(vec![PickerItem::new("src/main.rs", "main entry")], 50));

    let view = build_view(&app, 80, 24);

    assert!(
        !view.live.accessory_rows.is_empty(),
        "file picker accessory should produce rows"
    );
    assert!(
        view.live
            .accessory_rows
            .iter()
            .any(|r| r.text().contains("src/main.rs")),
        "accessory rows should contain picker items"
    );
}

#[test]
fn build_view_queued_summary_appears_when_prompts_queued() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.queued_followups.push("next task".to_string());
    app.queued_steering.push("look at tests".to_string());

    let view = build_view(&app, 80, 24);

    let summary = view
        .live
        .queued_summary
        .as_ref()
        .expect("queued summary should be present when prompts are queued");
    assert!(summary.text().contains("queued"), "summary should say 'queued'");
    assert!(
        summary.text().contains("1 steering"),
        "summary should show steering count"
    );
    assert!(
        summary.text().contains("1 follow-up"),
        "summary should show follow-up count"
    );
}

#[test]
fn build_view_queued_summary_absent_when_nothing_queued() {
    let app = test_app();
    let view = build_view(&app, 80, 24);

    assert!(
        view.live.queued_summary.is_none(),
        "no queued summary when nothing queued"
    );
}

#[test]
fn build_view_detail_pane_appears_when_open() {
    let mut app = test_app();
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["file_a.rs".to_string(), "file_b.rs".to_string()],
    });
    app.detail_pane = DetailPane { entry_index: 0, scroll: 0, open: true };

    let view = build_view(&app, 80, 24);

    assert!(
        !view.live.detail_pane.is_empty(),
        "detail pane should produce rows when open"
    );
    assert!(
        view.live.detail_pane.iter().any(|r| r.text().contains("run_shell")),
        "detail pane should contain the tool name"
    );
    assert!(
        view.live.detail_pane.iter().any(|r| r.text().contains("file_a.rs")),
        "detail pane should contain tool output"
    );
}

#[test]
fn build_view_detail_pane_scrolls_wrapped_rows() {
    let mut app = test_app();
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "printf"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["alpha beta gamma delta epsilon zeta eta theta iota kappa lambda".to_string()],
    });
    app.detail_pane = DetailPane { entry_index: 0, scroll: 1, open: true };

    let view = build_view(&app, 30, 24);
    let body = view
        .live
        .detail_pane
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        body.contains("run_shell"),
        "title should remain fixed above scrolled output"
    );
    assert!(
        !body.contains("alpha beta"),
        "scroll offset should skip the first wrapped visual row:\n{body}"
    );
    assert!(
        body.contains("delta") || body.contains("epsilon"),
        "scrolling should reveal later wrapped content:\n{body}"
    );
}

#[test]
fn build_view_handles_large_transcript_with_running_tool_and_detail_pane() {
    let mut app = test_app();
    app.run_state = RunState::Working;

    for index in 0..300 {
        app.transcript
            .push(Entry::User { text: format!("user message {index}") });
        app.transcript.push(Entry::Agent {
            text: format!("assistant response {index} with enough prose to wrap at least sometimes"),
            streaming: false,
        });
    }

    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Ok,
        output: (0..200).map(|index| format!("finished output line {index}")).collect(),
    });
    let detail_index = app.transcript.len() - 1;
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test renderer"}"#.to_string(),
        status: ToolStatus::Running,
        output: (0..200).map(|index| format!("running output line {index}")).collect(),
    });
    app.detail_pane = DetailPane { entry_index: detail_index, scroll: 120, open: true };

    let view = build_view(&app, 100, 32);

    assert!(
        view.transcript.stable_rows.len() > 300,
        "large transcript should still produce stable rows"
    );
    assert!(
        view.live
            .live_tail
            .iter()
            .any(|row| row.text().contains("running output line")),
        "running tool output should stay live"
    );
    assert!(
        view.live
            .detail_pane
            .iter()
            .any(|row| row.text().contains("finished output line")),
        "detail pane should render from the large completed tool output"
    );
}

#[test]
fn build_view_detail_pane_absent_when_closed() {
    let mut app = test_app();
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["file_a.rs".to_string()],
    });

    let view = build_view(&app, 80, 24);

    assert!(
        view.live.detail_pane.is_empty(),
        "detail pane should be empty when closed"
    );
}

#[test]
fn build_view_narrow_width_still_has_prompt_and_footer() {
    let mut app = test_app();
    app.input
        .set_text("a long prompt that wraps at narrow width definitely");
    app.transcript.push(Entry::User { text: "user message".to_string() });

    let view = build_view(&app, 20, 24);

    assert!(
        !view.live.prompt_rows.is_empty(),
        "prompt rows should exist at narrow width"
    );
    assert!(
        view.live.prompt_cursor.is_some(),
        "prompt cursor should exist at narrow width"
    );
    assert!(
        !view.live.static_status.text().is_empty(),
        "static status should exist at narrow width"
    );
}

#[test]
fn build_view_tiny_height_clips_live_tail() {
    let mut app = test_app();
    app.run_state = RunState::Working;

    let text = "line one. line two. line three. line four. line five. line six. line seven. line eight.".to_string();
    app.transcript.push(Entry::Agent { text, streaming: true });

    let view = build_view(&app, 80, 8);

    assert!(
        !view.live.live_tail.is_empty(),
        "live tail should still be present on tiny height"
    );
    assert!(
        view.live.live_tail.len() <= 4,
        "live tail should be clipped for tiny height: got {} rows",
        view.live.live_tail.len()
    );

    assert!(!view.live.prompt_rows.is_empty(), "prompt should survive tiny height");
    assert!(
        !view.live.static_status.text().is_empty(),
        "static status should survive tiny height"
    );
}

#[test]
fn build_view_tiny_height_still_has_prompt_and_status() {
    let app = test_app();
    let view = build_view(&app, 80, 3);

    assert!(!view.live.prompt_rows.is_empty(), "prompt should exist at height 3");
    assert!(
        !view.live.static_status.text().is_empty(),
        "static status should exist at height 3"
    );
}

#[test]
fn build_view_view_dimensions_match_input() {
    let app = test_app();
    let view = build_view(&app, 72, 30);

    assert_eq!(view.width, 72);
    assert_eq!(view.height, 30);
}
