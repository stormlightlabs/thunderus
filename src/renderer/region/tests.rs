
use std::path::PathBuf;

use super::*;
use crate::app::{App, Entry, RunState, ToolStatus};
use crate::cli::{Cli, Theme, WebSearchMode};
use crate::renderer::row;

fn vt100_contents(bytes: &[u8], width: u16, height: u16) -> String {
    let mut parser = vt100::Parser::new(height, width, 200);
    parser.process(bytes);
    parser.screen().contents()
}

fn nonblank_lines(contents: &str) -> Vec<&str> {
    contents.lines().filter(|line| !line.trim().is_empty()).collect()
}

fn render_entry_styled(entry: &Entry, width: usize) -> String {
    let rows = entry_to_rows(entry, "User", width, Path::new("."));
    let frame = row::Frame { rows, width, cursor: None, cursor_visible: true };
    frame.render_styled()
}

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
    app
}

#[test]
fn live_region_starts_empty() {
    let lr = LiveRegion::new();
    assert_eq!(lr.rendered_width, None);
    assert_eq!(lr.rendered_height, None);
}

#[test]
fn build_frame_has_terminal_height() {
    let app = test_app();
    let frame = LiveRegion::new().build_frame(&app, 80, 24);
    assert_eq!(frame.len(), 24);
    assert!(frame.rows.iter().all(|row| row.width == 80));
}

#[test]
fn build_frame_contains_live_prompt_and_status() {
    let mut app = test_app();
    app.transcript.push(Entry::User { text: "hello".to_string() });
    app.transcript
        .push(Entry::Assistant { text: "hi there".to_string(), streaming: false });
    app.input.set_text("next question");

    let frame = LiveRegion::new().build_frame(&app, 80, 24);
    let combined = frame.render_text();
    assert!(
        combined.contains("next question"),
        "prompt should be part of the viewport"
    );
    assert!(combined.contains("model:"), "footer should be part of the viewport");
}

#[test]
fn build_frame_includes_streaming_when_active() {
    let mut app = test_app();
    app.transcript
        .push(Entry::Assistant { text: "streaming text".to_string(), streaming: true });
    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 80, 24);

    let combined: String = frame.rows.iter().map(|r| r.text()).collect();
    assert!(
        combined.contains("streaming text"),
        "streaming text should be in live frame"
    );
    assert!(
        combined.contains("Assistant"),
        "assistant label should be in live frame"
    );
}

#[test]
fn build_frame_keeps_live_rows_at_bottom() {
    let mut app = test_app();
    app.input.set_text("hello");

    let frame = LiveRegion::new().build_frame(&app, 80, 12);
    assert!(frame.rows[frame.len() - 1].text().trim().is_empty());
    assert!(frame.rows[frame.len() - 2].text().contains("model:"));
    assert!(frame.rows[frame.len() - 3].text().contains("hello"));
    assert!(frame.rows[frame.len() - 4].text().contains("test-session"));
    assert!(frame.rows[frame.len() - 5].text().trim().is_empty());
}

#[test]
fn build_frame_keeps_live_rows_at_bottom_with_status_notice() {
    let mut app = test_app();
    app.transcript
        .push(Entry::Status { text: "Press CTRL+D again to quit.".to_string() });

    let frame = LiveRegion::new().build_frame(&app, 80, 16);
    assert!(
        frame.render_text().contains("Press CTRL+D again to quit."),
        "status notice should still render"
    );
    assert!(frame.rows[frame.len() - 1].text().trim().is_empty());
    assert!(frame.rows[frame.len() - 2].text().contains("model:"));
    assert!(frame.rows[frame.len() - 3].text().contains("›"));
    assert!(frame.rows[frame.len() - 4].text().contains("test-session"));
    assert!(frame.rows[frame.len() - 5].text().trim().is_empty());
    assert_eq!(
        frame.cursor,
        Some(crate::renderer::row::CursorCoord::new(frame.len() - 3, 6)),
        "cursor should be on the bottom-pinned prompt row"
    );
}

#[test]
fn build_frame_height_matches_viewport() {
    let mut app = test_app();
    for _ in 0..12 {
        app.transcript.push(Entry::User { text: "message".to_string() });
    }

    let frame = LiveRegion::new().build_frame(&app, 80, 10);
    let combined = frame.render_text();
    assert_eq!(frame.len(), 10, "frame should fill the viewport height");
    assert!(combined.contains("model:"), "footer should remain visible");
}

#[test]
fn build_frame_cursor_set_for_editable_prompt() {
    let mut app = test_app();
    app.input.set_text("hello");

    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 80, 24);
    assert!(frame.cursor.is_some(), "cursor should be set for editable prompt");
}

#[test]
fn build_frame_cursor_visible_for_editable_prompt_across_ticks() {
    let mut app = test_app();
    app.input.set_text("hello");

    let lr = LiveRegion::new();
    for tick in 0..20u64 {
        app.ui_tick = tick;
        let frame = lr.build_frame(&app, 80, 24);
        assert!(
            frame.cursor_visible,
            "cursor should be visible on every tick for editable prompt (tick={tick})"
        );
    }
}

#[test]
fn build_frame_cursor_hidden_for_non_editable_prompt() {
    let mut app = test_app();
    app.input.set_text("hello");
    app.run_state = RunState::Working;
    app.transcript.push(Entry::User { text: "go".to_string() });
    app.transcript
        .push(Entry::Assistant { text: "working...".to_string(), streaming: false });

    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 80, 24);
    assert!(
        !frame.cursor_visible,
        "cursor should be hidden when prompt is not editable"
    );
}

#[test]
fn render_frame_diff_emits_no_hide_when_cursor_stays_visible() {
    let app = test_app();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();

    lr.render_frame(&app, &mut backend, 80, 24).unwrap();
    let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();

    lr.render_frame(&app, &mut backend, 80, 24).unwrap();
    let second_output = String::from_utf8(backend.writer().clone()).unwrap();
    let new_bytes = &second_output[first_len..];

    assert!(
        !new_bytes.contains("\x1b[?25l"),
        "re-render of identical visible-cursor frame should not emit Hide: {new_bytes:?}"
    );
}

#[test]
fn render_frame_diff_emits_no_show_on_unchanged_visible_cursor() {
    let app = test_app();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();

    lr.render_frame(&app, &mut backend, 80, 24).unwrap();
    let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();

    lr.render_frame(&app, &mut backend, 80, 24).unwrap();
    let second_output = String::from_utf8(backend.writer().clone()).unwrap();
    let new_bytes = &second_output[first_len..];

    assert_eq!(
        new_bytes.len(),
        0,
        "identical frame with unchanged cursor should produce zero output, got: {new_bytes:?}"
    );
}

#[test]
fn render_frame_writes_from_top() {
    let app = test_app();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();

    let out = String::from_utf8(backend.writer().clone()).unwrap();
    assert!(out.contains("\x1b[1;1H"), "viewport render should start at top-left");
    assert!(out.contains("\x1b[K"), "should clear each row to end-of-line");
    assert_eq!(lr.rendered_width, Some(80));
    assert_eq!(lr.rendered_height, Some(24));
    assert!(
        lr.rendered_frame.is_some(),
        "should store the rendered frame for diffing"
    );
}

#[test]
fn render_frame_diff_skips_unchanged_rows() {
    let app = test_app();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();

    let first_output_len = String::from_utf8(backend.writer().clone()).unwrap().len();
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();

    let second_output = String::from_utf8(backend.writer().clone()).unwrap();
    let second_new_bytes = second_output.len() - first_output_len;

    assert_eq!(
        second_new_bytes,
        0,
        "identical frame should produce no output, got: {:?}",
        &second_output[first_output_len..]
    );
}

#[test]
fn render_frame_diff_writes_only_changed_rows() {
    let mut app = test_app();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();
    app.ui_tick = app.ui_tick.wrapping_add(1);
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();

    let out = String::from_utf8(backend.writer().clone()).unwrap();
    assert!(!out.is_empty(), "changed frame should produce output");
}

#[test]
fn render_frame_full_redraw_on_resize() {
    let app = test_app();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();

    let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();
    lr.render_frame(&app, &mut backend, 100, 30).unwrap();

    let second_output = String::from_utf8(backend.writer().clone()).unwrap();
    let second_new_bytes = second_output.len() - first_len;

    assert!(second_new_bytes > 0, "resize should trigger a full redraw with output");
}

#[test]
fn render_frame_commits_submitted_user_to_scrollback() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::User { text: "start the task".to_string() });

    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 24).unwrap();

    let out = String::from_utf8(backend.writer().clone()).unwrap();
    assert!(
        out.contains("\x1b[1;"),
        "history rows should be inserted through a constrained scroll region"
    );
    assert!(
        out.contains("start the task"),
        "submitted prompt should be appended to native scrollback immediately"
    );
    assert!(lr.committed_row_count > 0);
}

#[test]
fn build_frame_places_streaming_output_above_status_line() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::Assistant { text: "streaming response text".to_string(), streaming: true });

    let frame = LiveRegion::new().build_frame(&app, 80, 24);
    let output_row = frame
        .rows
        .iter()
        .position(|row| row.text().contains("streaming response text"))
        .expect("streaming output should render");
    let status_row = frame
        .rows
        .iter()
        .position(|row| row.text().contains("test-session"))
        .expect("status line should render");

    assert!(
        output_row < status_row,
        "mutable transcript output should stay above the running/status line"
    );
}

#[test]
fn build_frame_keeps_user_text_visible_when_live_tail_grows() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::User { text: "consolidate the renderer milestones".to_string() });
    app.transcript.push(Entry::Reasoning {
        text: "reading TODO.md before summarizing the requested renderer milestones".to_string(),
        streaming: true,
    });
    app.transcript.push(Entry::Tool {
        name: "read_file_range".to_string(),
        arguments: r#"{"path": "TODO.md"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec![
            "1: # TODO".to_string(),
            "2:".to_string(),
            "3: ## Completed Summary".to_string(),
            "4:".to_string(),
            "5: ### Harness, Provider, And Event Loop".to_string(),
            "6:".to_string(),
        ],
    });

    let frame = LiveRegion::new().build_frame(&app, 120, 32);
    let lines: Vec<String> = frame.rows.iter().map(|row| row.text()).collect();
    let user_label = lines
        .iter()
        .position(|line| line.contains("User"))
        .expect("user label should remain visible");
    let user_text = lines
        .iter()
        .position(|line| line.contains("consolidate the renderer milestones"))
        .expect("user text should remain visible");
    let thinking = lines
        .iter()
        .position(|line| line.contains("Thinking"))
        .expect("live reasoning should render");

    assert!(
        user_label < user_text && user_text < thinking,
        "live rows should not overwrite the bottom of the user block:\n{}",
        frame.render_text()
    );
}

#[test]
fn build_frame_keeps_done_prompt_bottom_anchored_after_latest_assistant_message() {
    let mut app = test_app();
    app.transcript.push(Entry::User { text: "summarize TODO".to_string() });
    app.transcript
        .push(Entry::Assistant { text: "Here is the consolidated renderer summary.".to_string(), streaming: false });
    app.input.set_text("Please update @TODO.md with the updated content");

    let frame = LiveRegion::new().build_frame(&app, 120, 32);
    let lines: Vec<String> = frame.rows.iter().map(|row| row.text()).collect();
    let assistant_body = lines
        .iter()
        .position(|line| line.contains("Here is the consolidated renderer summary."))
        .expect("assistant body should render");
    let status = lines
        .iter()
        .position(|line| line.contains("test-session"))
        .expect("status row should render");

    assert!(
        assistant_body < status,
        "transcript should stay above live prompt/status:\n{}",
        frame.render_text()
    );
    assert!(frame.rows[frame.len() - 1].text().trim().is_empty());
    assert!(frame.rows[frame.len() - 2].text().contains("model:"));
    assert!(frame.rows[frame.len() - 3].text().contains("Please update @TODO.md"));
    assert!(frame.rows[frame.len() - 4].text().contains("test-session"));
    assert!(frame.rows[frame.len() - 5].text().trim().is_empty());
}

#[test]
fn render_frame_commits_streaming_stable_prefix_and_keeps_tail_live() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::Assistant { text: "first stable line second mutable tail".to_string(), streaming: true });

    let mut backend = TerminalBackend::new(Vec::new(), 24, 16);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 24, 16).unwrap();

    let out = String::from_utf8(backend.writer().clone()).unwrap();
    assert!(
        out.contains("Assistant"),
        "stable streaming block header should be committed to scrollback"
    );
    let frame_text = lr.rendered_frame.as_ref().unwrap().render_text();
    assert!(
        frame_text.contains("mutable tail") || frame_text.contains("tail"),
        "the mutable streaming tail should remain in the live frame"
    );
}

#[test]
fn vt100_submitted_prompt_survives_first_render_scrollback_round_trip() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::User { text: "start the task".to_string() });

    let mut backend = TerminalBackend::new(Vec::new(), 80, 18);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 18).unwrap();

    let contents = vt100_contents(backend.writer(), 80, 18);
    assert!(
        contents.contains("start the task"),
        "vt100 should interpret the scroll-region insert as visible/history content:\n{contents}"
    );
    assert!(
        contents.contains("sending") || contents.contains("submitted"),
        "live chrome should still render after the prompt commit:\n{contents}"
    );
}

#[test]
fn vt100_streaming_tail_moves_above_status_after_stable_prefix_commit() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::User { text: "describe the renderer".to_string() });

    let mut backend = TerminalBackend::new(Vec::new(), 32, 14);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 32, 14).unwrap();

    app.transcript.push(Entry::Assistant {
        text: "stable prefix keeps moving into scrollback while mutable tail stays live".to_string(),
        streaming: true,
    });
    lr.render_frame(&app, &mut backend, 32, 14).unwrap();

    let contents = vt100_contents(backend.writer(), 32, 14);
    let tail_line = contents
        .lines()
        .position(|line| line.contains("tail") || line.contains("live"))
        .expect("streaming tail should be visible in vt100 output");
    let status_line = contents
        .lines()
        .position(|line| line.contains("test-session"))
        .expect("status line should be visible in vt100 output");

    assert!(
        tail_line < status_line,
        "streaming tail should render above the status line after vt100 parsing:\n{contents}"
    );
}

#[test]
fn vt100_resize_replays_committed_rows_without_duplicate_prompt() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::User { text: "resize preserves exactly one submitted prompt".to_string() });

    let mut backend = TerminalBackend::new(Vec::new(), 48, 16);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 48, 16).unwrap();

    app.transcript.push(Entry::Assistant {
        text: "stable response rows should reflow when the terminal changes size".to_string(),
        streaming: false,
    });
    lr.render_frame(&app, &mut backend, 48, 16).unwrap();

    backend.set_size(30, 16);
    lr.render_frame(&app, &mut backend, 30, 16).unwrap();

    backend.set_size(48, 16);
    lr.render_frame(&app, &mut backend, 48, 16).unwrap();

    let contents = vt100_contents(backend.writer(), 48, 16);
    let prompt_count = nonblank_lines(&contents)
        .into_iter()
        .filter(|line| line.contains("resize preserves exactly one"))
        .count();
    assert_eq!(
        prompt_count, 1,
        "prompt should be replayed once after narrow/wide round trip:\n{contents}"
    );
    assert!(
        contents.contains("stable response rows"),
        "committed assistant rows should survive resize replay:\n{contents}"
    );
}

#[test]
fn vt100_resize_replays_startup_banner_with_committed_scrollback() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.transcript
        .push(Entry::User { text: "trigger scrollback replay".to_string() });

    let mut backend = TerminalBackend::new(Vec::new(), 80, 20);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 80, 20).unwrap();

    backend.set_size(40, 20);
    lr.render_frame(&app, &mut backend, 40, 20).unwrap();

    backend.set_size(80, 20);
    lr.render_frame(&app, &mut backend, 80, 20).unwrap();

    let contents = vt100_contents(backend.writer(), 80, 20);
    assert!(
        contents.contains("thndrs  coding agent"),
        "startup banner metadata should be replayed with committed scrollback after resize:\n{contents}"
    );
    assert!(
        contents.contains("trigger scrollback replay"),
        "committed prompt should still be present with replayed banner:\n{contents}"
    );
}

#[test]
fn vt100_resize_keeps_latest_git_statusline_without_duplicates() {
    let mut app = test_app();
    app.git_status = Some(crate::renderer::git::GitStatusSummary {
        branch: Some("main".to_string()),
        added: 1,
        modified: 0,
        deleted: 0,
    });

    let mut backend = TerminalBackend::new(Vec::new(), 100, 18);
    let mut lr = LiveRegion::new();
    lr.render_frame(&app, &mut backend, 100, 18).unwrap();

    app.git_status = Some(crate::renderer::git::GitStatusSummary {
        branch: Some("main".to_string()),
        added: 1,
        modified: 2,
        deleted: 1,
    });
    lr.render_frame(&app, &mut backend, 100, 18).unwrap();

    backend.set_size(72, 18);
    lr.render_frame(&app, &mut backend, 72, 18).unwrap();

    backend.set_size(100, 18);
    lr.render_frame(&app, &mut backend, 100, 18).unwrap();

    let contents = vt100_contents(backend.writer(), 100, 18);
    let git_lines: Vec<&str> = contents.lines().filter(|line| line.contains("git: main")).collect();
    assert_eq!(
        git_lines.len(),
        1,
        "exactly one git statusline should remain after resize replay:\n{contents}"
    );
    assert!(
        git_lines[0].contains("git: main +1 ~2 -1"),
        "latest git summary should survive resize replay:\n{contents}"
    );
}

#[test]
fn reset_clears_state() {
    let mut lr = LiveRegion::new();
    lr.rendered_frame = Some(Frame::new(80));
    lr.rendered_width = Some(80);
    lr.rendered_height = Some(24);

    lr.reset();

    assert!(lr.rendered_frame.is_none());
    assert_eq!(lr.rendered_width, None);
    assert_eq!(lr.rendered_height, None);
}

#[test]
fn resize_reflows_viewport() {
    let mut app = test_app();
    app.input
        .set_text("some prompt text here that should occupy more rows when the viewport narrows");
    let lr = LiveRegion::new();

    let wide = lr.build_frame(&app, 80, 16);
    let narrow = lr.build_frame(&app, 32, 16);

    assert_eq!(wide.len(), 16);
    assert_eq!(narrow.len(), 16);
    assert_ne!(wide.render_text(), narrow.render_text());
    assert!(narrow.cursor.is_some());
}

#[test]
fn snapshot_empty_live_frame() {
    let app = test_app();
    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 80, 24);
    insta::assert_snapshot!("empty_live_frame", frame.render_styled());
}

#[test]
fn snapshot_streaming_live_frame() {
    let mut app = test_app();
    app.transcript
        .push(Entry::Assistant { text: "streaming response text".to_string(), streaming: true });
    app.input.set_text("follow up");
    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 80, 24);
    insta::assert_snapshot!("streaming_live_frame", frame.render_styled());
}

#[test]
fn snapshot_narrow_live_frame() {
    let mut app = test_app();
    app.input.set_text("a longer prompt that should wrap at narrow width");
    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 40, 20);
    insta::assert_snapshot!("narrow_live_frame", frame.render_styled());
}

#[test]
fn snapshot_startup_banner() {
    let app = test_app();
    let rows = banner_rows(&app, 80);
    let frame = row::Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("startup_banner", frame.render_styled());
}

#[test]
fn snapshot_narrow_startup_banner() {
    let app = test_app();
    let rows = banner_rows(&app, 40);
    let frame = row::Frame { rows, width: 40, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("narrow_startup_banner", frame.render_styled());
}

#[test]
fn snapshot_user_message() {
    let entry = Entry::User { text: "Hello, can you help me with this?".to_string() };
    insta::assert_snapshot!("user_message", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_assistant_text() {
    let entry =
        Entry::Assistant { text: "Sure! I can help with that. Let me take a look.".to_string(), streaming: false };
    insta::assert_snapshot!("assistant_text", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_assistant_with_code_fence() {
    let entry = Entry::Assistant {
        text: "````md\nHere is the code:\n\n```rs\nfn main() {\n    println!(\"hello\");\n}\n```\n````".to_string(),
        streaming: false,
    };
    insta::assert_snapshot!("assistant_code_fence", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_reasoning() {
    let entry = Entry::Reasoning { text: "I need to check the file structure first.".to_string(), streaming: false };
    insta::assert_snapshot!("reasoning_block", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_ok() {
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern": "fn main", "path": "src/main.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "src/main.rs:1:fn main() {".to_string(),
            "src/main.rs:2:    println!(\"hello\");".to_string(),
            "src/main.rs:3:}".to_string(),
        ],
    };
    insta::assert_snapshot!("tool_ok", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_failed() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo build"}"#.to_string(),
        status: ToolStatus::Failed,
        output: vec![
            "error[E0308]: mismatched types".to_string(),
            "  --> src/main.rs:5:14".to_string(),
            "   |".to_string(),
            "5 |     let x: i32 = \"hello\";".to_string(),
            "   |               ^^^^^^^^".to_string(),
        ],
    };
    insta::assert_snapshot!("tool_failed", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_error_message() {
    let entry = Entry::Error { text: "Provider request failed: connection refused".to_string() };
    insta::assert_snapshot!("error_message", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_rust_compiler_output() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo build"}"#.to_string(),
        status: ToolStatus::Failed,
        output: vec![
            "   Compiling thndrs v0.1.0".to_string(),
            "error[E0277]: the trait bound `X: Y` is not satisfied".to_string(),
            "  --> src/lib.rs:42:10".to_string(),
            "   |".to_string(),
            "42 |     fn foo() -> impl Y {".to_string(),
            "   |                    ^^^^^".to_string(),
        ],
    };
    insta::assert_snapshot!("rust_compiler_output", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_json_output() {
    let entry = Entry::Tool {
        name: "read_file_range".to_string(),
        arguments: r#"{"path": "config.json"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "{".to_string(),
            "  \"name\": \"thndrs\",".to_string(),
            "  \"version\": \"0.1.0\"".to_string(),
            "}".to_string(),
        ],
    };
    insta::assert_snapshot!("json_output", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_plain_prose() {
    let entry = Entry::Assistant {
            text: "This is a plain prose response without any code or special formatting. It should wrap nicely across multiple lines when the terminal is narrow enough.".to_string(),
            streaming: false,
        };
    insta::assert_snapshot!("plain_prose", render_entry_styled(&entry, 60));
}

#[test]
fn snapshot_diff_output() {
    let entry = Entry::Tool {
        name: "replace_range".to_string(),
        arguments: r#"{"path": "src/main.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "--- src/main.rs".to_string(),
            "+++ src/main.rs".to_string(),
            "@@ -1,3 +1,3 @@".to_string(),
            " fn main() {".to_string(),
            "-    println!(\"old\");".to_string(),
            "+    println!(\"new\");".to_string(),
            " }".to_string(),
        ],
    };
    insta::assert_snapshot!("diff_output", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_with_truncated_output() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: (0..20).map(|i| format!("file_{i}.rs")).collect(),
    };
    insta::assert_snapshot!("tool_truncated_output", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_status_entry() {
    let entry = Entry::Status { text: "context  AGENTS.md (scope: .)".to_string() };
    insta::assert_snapshot!("status_entry", render_entry_styled(&entry, 80));
}

#[test]
fn plain_status_entries_render_as_system() {
    let entry = Entry::Status { text: "manual status".to_string() };
    let rendered = render_entry_styled(&entry, 80);

    assert!(
        rendered.contains("System"),
        "plain status label should be System:\n{rendered}"
    );
    assert!(
        !rendered.contains("Notice"),
        "plain status label should not be Notice:\n{rendered}"
    );
}

#[test]
fn tool_output_shortens_workspace_absolute_paths() {
    let cwd = Path::new("/Users/owais/Projects/StormlightLabs/OpenSource/thndrs");
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"Entry::Status"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "/Users/owais/Projects/StormlightLabs/OpenSource/thndrs/src/session/tests.rs:1420: Entry::Status"
                .to_string(),
        ],
    };
    let rows = entry_to_rows(&entry, "User", 120, cwd);
    let frame = row::Frame { rows, width: 120, cursor: None, cursor_visible: true };
    let rendered = frame.render_text();

    assert!(
        rendered.contains("src/session/tests.rs:1420:"),
        "path should be project-relative:\n{rendered}"
    );
    assert!(
        !rendered.contains("/Users/owais/Projects/StormlightLabs/OpenSource/thndrs"),
        "workspace prefix should be hidden:\n{rendered}"
    );
}

#[test]
fn snapshot_streaming_tool_with_output() {
    let mut app = test_app();
    app.transcript.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec![
            "running 3 tests".to_string(),
            "test tests::foo ... ok".to_string(),
            "test tests::bar ... ok".to_string(),
        ],
    });
    let lr = LiveRegion::new();
    let frame = lr.build_frame(&app, 80, 24);
    insta::assert_snapshot!("streaming_tool_with_output", frame.render_styled());
}
