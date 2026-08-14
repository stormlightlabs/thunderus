use super::*;
use crate::acp::permissions::{PendingPermission, PermissionKindView, PermissionOptionView};
use crate::app::PromptAccessory;
use crate::app::{
    App, ChatGptOAuthMethod, FilePickerSource, FirstRunRecovery, Mode, PickerItem, PickerState, RecoveryStage, RunState,
};
use crate::cli::commands::setup::SetupProviderArg;
use crate::cli::{Cli, Theme, WebSearchMode};
use crate::renderer::git::GitStatusSummary;
use crate::renderer::layout::truncate_spans;
use crate::renderer::row::Frame;
use crate::thndrs_core::auth::ChatGptCodexDeviceCode;
use std::path::PathBuf;
use std::sync::mpsc;

fn test_app() -> App {
    let mut app = App::from_cli(&Cli {
        cwd: PathBuf::from("."),
        model: "test-model".to_string(),
        websearch: WebSearchMode::DuckDuckGo,
        websearch_url: None,
        reasoning_effort: Default::default(),
        reasoning_summary: Default::default(),
        tick_rate_ms: 100,
        no_mouse: false,
        mouse: false,
        verbose: false,
        theme: Theme::EldritchMinimal,
        print_prompt: false,
        skill_dirs: Vec::new(),
        session_dir: None,
        ephemeral: false,
        capture_context_content: false,
        config_diagnostics: Vec::new(),
        config_layers: Vec::new(),
        config_origins: std::collections::BTreeMap::new(),
        acp_agents: std::collections::BTreeMap::new(),
        context: thndrs_agent::context::ContextConfig::default(),
        status_line: Default::default(),
        session_retention: Default::default(),
        authority: Default::default(),
        command: None,
    });
    app.overlay.close();
    app.runtime.git_status =
        Some(GitStatusSummary { branch: Some("main".to_string()), added: 0, modified: 0, deleted: 0 });
    app
}

#[test]
fn prompt_rows_empty_input() {
    let app = test_app();
    let (rows, cursor) = prompt_rows_for(&app, 80);
    assert_eq!(rows.len(), 1, "empty input should produce one row");
    assert!(cursor.is_some(), "cursor should be present");
    let text = rows[0].text();
    assert!(text.contains("❯"), "prompt icon should appear");
}

#[test]
fn prompt_rows_with_text() {
    let mut app = test_app();
    app.composer.input.set_text("hello world");
    let (rows, cursor) = prompt_rows_for(&app, 80);
    assert_eq!(rows.len(), 1, "short text should fit on one row");
    assert!(cursor.is_some());
    assert!(rows[0].text().contains("hello world"));
}

#[test]
fn credential_prompt_uses_one_responsive_masked_input() {
    let mut app = test_app();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::OpencodeGo),
        stage: RecoveryStage::EnterKey,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: "sk-view-secret".to_string(),
        chatgpt_oauth: None,
    });

    let (rows, cursor) = prompt_rows_for(&app, 80);
    let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

    assert!(text.contains("API key: ••••••••••••…"));
    assert!(!text.contains("sk-view-secret"));
    assert!(!text.contains("[hidden]"));
    assert!(cursor.is_none());
}

#[test]
fn prompt_rows_multiline() {
    let mut app = test_app();
    app.composer.input.set_text("line one\nline two");
    let (rows, _cursor) = prompt_rows_for(&app, 80);
    assert_eq!(rows.len(), 2, "two logical lines should produce two rows");
}

#[test]
fn prompt_rows_wraps_long_text() {
    let mut app = test_app();
    app.composer.input.set_text(&"x".repeat(100));
    let (rows, _cursor) = prompt_rows_for(&app, 20);
    assert!(rows.len() > 1, "long text should wrap to multiple rows");
}

#[test]
fn prompt_rows_wrap_at_visible_content_width() {
    let mut app = test_app();
    app.composer.input.set_text(&"x".repeat(11));
    let (rows, cursor) = prompt_rows_for(&app, 20);

    assert_eq!(rows.len(), 1);
    assert_eq!(cursor, Some(CursorCoord::new(0, 18)));

    app.composer.input.insert_char('x');
    let (rows, cursor) = prompt_rows_for(&app, 20);

    assert_eq!(rows.len(), 2);
    assert_eq!(cursor, Some(CursorCoord::new(1, 8)));
}

#[test]
fn comfortable_prompt_keeps_the_readable_measure() {
    let mut app = test_app();
    app.composer.input.set_text(&"x".repeat(150));

    let (rows_at_120, _) = prompt_rows_for(&app, 120);
    let (rows_at_160, _) = prompt_rows_for(&app, 160);

    assert_eq!(rows_at_120.len(), rows_at_160.len());
    assert_eq!(rows_at_120[0].text().trim_end(), rows_at_160[0].text().trim_end());
}

#[test]
fn frame_prompt_rows_adds_borderless_metadata_and_offsets_cursor() {
    let mut app = test_app();
    app.session.id = "test-session".to_string();
    app.composer.input.set_text("hello");
    let (body_rows, cursor) = prompt_rows_for(&app, 80);
    let (rows, cursor) = frame_prompt_rows(&app, 80, body_rows, cursor);

    assert_eq!(rows.len(), 4);
    assert!(rows[0].text().contains("test-session"));
    assert!(!rows[0].text().contains("prompt"));
    assert!(!rows[0].text().contains("idle"));
    assert!(rows[2].text().contains("❯  hello"));
    assert!(!rows.iter().any(|row| row.text().contains(['╭', '╮', '╰', '╯', '│'])));
    assert_eq!(cursor, Some(CursorCoord::new(2, 12)));
    let content_column = rows[0].text().find("test-session").expect("session label");
    assert_eq!(rows[2].text().find('❯'), Some(content_column));
    assert_eq!(static_status_row(&app, 80).text().find('✓'), Some(content_column));
    assert_eq!(
        app.render_banner_rows(80)[0].text().find("thndrs"),
        Some(content_column)
    );
    assert!(
        rows[0].spans.iter().all(|span| span.style.bg == Color::Reset),
        "session metadata should use the terminal background"
    );
    assert!(
        rows[2].spans.first().is_some_and(|span| span.style.bg == Color::Reset)
            && rows[2].spans.last().is_some_and(|span| span.style.bg == Color::Reset),
        "the editable input surface should be inset from the terminal edges"
    );
    assert!(
        rows[2]
            .spans
            .iter()
            .any(|span| span.style.bg == renderer::style::palette().input),
        "the editable input surface should retain the composer background"
    );
    for row in [&rows[1], &rows[3]] {
        assert!(
            row.spans.first().is_some_and(|span| span.style.bg == Color::Reset)
                && row.spans.last().is_some_and(|span| span.style.bg == Color::Reset)
                && row
                    .spans
                    .iter()
                    .any(|span| span.style.bg == renderer::style::palette().input),
            "vertical composer padding should share the inset input background"
        );
    }
}

#[test]
fn frame_prompt_rows_identifies_ephemeral_runs() {
    let mut app = test_app();
    app.session.run_persistence = crate::app::RunPersistence::Ephemeral;
    let (body_rows, cursor) = prompt_rows_for(&app, 80);
    let (rows, _) = frame_prompt_rows(&app, 80, body_rows, cursor);

    assert!(rows[0].text().contains("ephemeral"));
}

#[test]
fn frame_prompt_rows_right_aligns_nonzero_queue_count_when_space_allows() {
    let mut app = test_app();
    app.session.id = "test-session".to_string();
    app.composer.queue.push(
        crate::app::QueueTarget::FollowUp,
        "first".to_string(),
        "test".to_string(),
    );
    app.composer.queue.push(
        crate::app::QueueTarget::Steering,
        "second".to_string(),
        "test".to_string(),
    );

    let (body_rows, cursor) = prompt_rows_for(&app, 80);
    let (rows, _) = frame_prompt_rows(&app, 80, body_rows, cursor);
    let header = rows[0].text();

    assert!(header.contains("test-session"));
    assert!(header.trim_end().ends_with("2 queued"));
    assert!(!static_status_row(&app, 80).text().contains("queue"));

    app.runtime.cli.status_line.right = vec![
        crate::config::StatusSegment::Route,
        crate::config::StatusSegment::QueueCount,
    ];
    let (rows, _) = frame_prompt_rows(&app, 80, prompt_rows_for(&app, 80).0, None);
    assert!(!rows[0].text().contains("queued"));
    assert!(static_status_row(&app, 80).text().contains("queue 2"));
}

#[test]
fn frame_prompt_rows_hides_queue_count_at_zero_and_under_width_pressure() {
    let mut app = test_app();
    app.session.id = "test-session".to_string();

    let (body_rows, cursor) = prompt_rows_for(&app, 80);
    let (rows, _) = frame_prompt_rows(&app, 80, body_rows, cursor);
    assert!(!rows[0].text().contains("queued"));

    app.composer.queue.push(
        crate::app::QueueTarget::FollowUp,
        "later".to_string(),
        "test".to_string(),
    );
    let (body_rows, cursor) = prompt_rows_for(&app, 24);
    let (rows, _) = frame_prompt_rows(&app, 24, body_rows, cursor);
    assert!(rows[0].text().contains("test-session"));
    assert!(!rows[0].text().contains("queued"));
}

#[test]
fn frame_prompt_rows_keeps_runtime_status_out_of_composer_header() {
    let mut app = test_app();
    app.session.id = "test-session".to_string();
    app.runtime.run_state = RunState::Working;
    app.transcript
        .entries
        .push(Entry::Agent { text: "working".to_string(), streaming: true });
    let (body_rows, cursor) = prompt_rows_for(&app, 80);
    let (rows, _) = frame_prompt_rows(&app, 80, body_rows, cursor);

    assert!(rows[0].text().contains("test-session"));
    assert!(!rows[0].text().contains("Responding"));
}

#[test]
fn prompt_rows_command_mode_shows_colon() {
    let mut app = test_app();
    app.composer.mode = Mode::Command;
    let (rows, _) = prompt_rows_for(&app, 80);
    assert!(rows[0].text().contains(':'), "command mode should show colon prefix");
}

#[test]
fn prompt_rows_submitted_shows_queue_icon() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;

    let (rows, _) = prompt_rows_for(&app, 80);
    assert!(
        rows[0].text().contains("»"),
        "submitted state should show queue composer icon"
    );
}

#[test]
fn static_status_row_keeps_state_at_tiny_width() {
    let app = test_app();
    let row = static_status_row(&app, 10);
    let text = row.text();
    assert!(text.contains('✓'));
    assert!(!text.contains("model"));
}

#[test]
fn static_status_row_prioritizes_immediate_state() {
    let app = test_app();
    let text = static_status_row(&app, 80).text();
    assert!(text.contains("✓ Ready"));
    assert!(!text.contains("Editable"));
    assert!(!text.contains("queue"));
    assert!(!text.contains("model:"));
    assert!(!text.contains("tok:"));
    assert!(!text.contains("quota"));
}

#[test]
fn static_status_row_names_active_work() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript
        .entries
        .push(Entry::Reasoning { text: "checking".to_string(), streaming: true });

    let text = static_status_row(&app, 80).text();
    assert!(text.contains("Thinking"));
    assert!(text.contains('⠋'));
}

#[test]
fn accessory_rows_none_when_no_accessory() {
    let app = test_app();
    let rows = accessory_rows(&app, 80, 8);
    assert!(rows.is_empty(), "no accessory should produce no rows");
}

#[test]
fn accessory_rows_help_has_entries() {
    let mut app = test_app();
    app.overlay.show_help();

    let rows = accessory_rows(&app, 80, 16);
    assert!(!rows.is_empty(), "help should produce rows");
    let combined: String = rows.iter().map(|r| r.text()).collect();
    assert!(combined.contains("Navigation"), "help should have Navigation section");
    assert!(combined.contains("Enter"), "help should include Enter key");
    assert!(combined.contains("Escape"), "help should include Escape key");
}

#[test]
fn truncate_row_helper_works() {
    let spans = vec![Span::plain("hello world")];
    let out = truncate_spans(&spans, 5, CellStyle::default());
    assert_eq!(out.iter().map(|s| s.text.chars().count()).sum::<usize>(), 5);
}

fn picker_app(files: &[String]) -> App {
    let mut app = test_app();
    let items: Vec<PickerItem> = files.iter().map(|file| PickerItem::new(file.clone(), "")).collect();
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        PickerState::new(items, 200),
    );
    app
}

#[test]
fn snapshot_file_picker_empty_query() {
    let app = picker_app(&[
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "Cargo.toml".to_string(),
    ]);
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("file_picker_empty_query", frame.render_styled());
}

#[test]
fn snapshot_permission_prompt() {
    let mut app = test_app();
    let (tx, _rx) = mpsc::channel();
    app.overlay.show_permission(PendingPermission {
        tool_call_id: "call_1".to_string(),
        title: "Write src/main.rs".to_string(),
        options: vec![
            PermissionOptionView {
                id: "allow".to_string(),
                name: "Allow once".to_string(),
                kind: PermissionKindView::AllowOnce,
            },
            PermissionOptionView {
                id: "reject".to_string(),
                name: "Reject".to_string(),
                kind: PermissionKindView::RejectOnce,
            },
        ],
        selected: 1,
        responder: tx,
    });

    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("permission_prompt", frame.render_styled());
}

#[test]
fn snapshot_file_picker_filtered_results() {
    let mut app = picker_app(&[
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "Cargo.toml".to_string(),
    ]);
    if let Some(picker) = app.overlay.picker_mut() {
        picker.query = "main".to_string();
        picker.matches = vec![PickerItem::new("src/main.rs", "")];
        picker.match_indices = vec![vec![4, 5, 6, 7]];
    }
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("file_picker_filtered", frame.render_styled());
}

#[test]
fn snapshot_file_picker_no_matches() {
    let mut app = picker_app(&["src/main.rs".to_string()]);
    if let Some(picker) = app.overlay.picker_mut() {
        picker.query = "xyz".to_string();
        picker.matches = Vec::new();
        picker.match_indices = Vec::new();
    }
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("file_picker_no_matches", frame.render_styled());
}

#[test]
fn snapshot_file_picker_long_path_clipping() {
    let app = picker_app(&["src/very/deeply/nested/path/to/some/module/file.rs".to_string()]);
    let rows = accessory_rows(&app, 30, 12);
    let frame = Frame { rows, width: 30, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("file_picker_long_path", frame.render_styled());
}

#[test]
fn snapshot_file_picker_scrolled_selection() {
    let files: Vec<String> = (0..15).map(|i| format!("src/file_{i:02}.rs")).collect();
    let mut app = picker_app(&files);
    if let Some(picker) = app.overlay.picker_mut() {
        picker.selected = 5;
        picker.scroll = 3;
    }
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("file_picker_scrolled", frame.render_styled());
}

#[test]
fn snapshot_model_picker() {
    let mut app = test_app();
    let _ = app.overlay.show_picker(
        PromptAccessory::Models,
        PickerState::new(
            vec![
                PickerItem::new("opencode/big-pickle", "Recommended route to Kimi K2.7-Code"),
                PickerItem::new("opencode/gpt-5.6-luna", "Largest context window"),
            ],
            50,
        ),
    );
    if let Some(picker) = app.overlay.picker_mut() {
        picker.selected = 1;
    }
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("model_picker", frame.render_styled());
}

#[test]
fn snapshot_mention_styling_in_prompt() {
    let mut app = test_app();
    app.composer.input.set_text("check @src/main.rs for details");
    let (rows, _) = prompt_rows_for(&app, 80);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("mention_styling", frame.render_styled());
}

#[test]
fn snapshot_help_rows() {
    let mut app = test_app();
    app.overlay.show_help();
    let rows = accessory_rows(&app, 80, 16);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("help_rows", frame.render_styled());
}

#[test]
fn help_rows_show_running_steering_binding() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.overlay.show_help();
    let rows = accessory_rows(&app, 80, 16);
    let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

    let steering_key = if cfg!(target_os = "macos") { "Cmd+Enter" } else { "Ctrl+Enter" };
    assert!(
        text.contains(steering_key),
        "running help should describe the steering chord: {text}"
    );
    assert!(text.contains("steer the running turn"));
}

#[test]
fn snapshot_command_suggestions() {
    let mut app = test_app();
    app.composer.input.set_text("c");
    app.composer.mode = Mode::Command;
    app.overlay.show_commands();
    let rows = accessory_rows(&app, 80, 8);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("command_suggestions", frame.render_styled());
}

#[test]
fn snapshot_first_run_recovery_normal() {
    let mut app = test_app();
    app.runtime.model = "opencode/big-pickle".to_string();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::OpencodeGo),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("first_run_recovery_normal", frame.render_styled());
}

#[test]
fn snapshot_first_run_recovery_narrow() {
    let mut app = test_app();
    app.runtime.model = "opencode-go/kimi-k2.7-code".to_string();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::OpencodeGo),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 40, 12);
    let frame = Frame { rows, width: 40, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("first_run_recovery_narrow", frame.render_styled());
}

#[test]
fn snapshot_first_run_recovery_tiny() {
    let mut app = test_app();
    app.runtime.model = "opencode/big-pickle".to_string();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::OpencodeGo),
        stage: RecoveryStage::ConfirmStore,
        pending_provider_prompt: true,
        selected: 1,
        secret_input: "sk-hidden".to_string(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 24, 3);
    let frame = Frame { rows, width: 24, cursor: None, cursor_visible: true };
    let rendered = frame.render_styled();
    assert!(!rendered.contains("sk-hidden"));
    insta::assert_snapshot!("first_run_recovery_tiny", rendered);
}

#[test]
fn snapshot_chatgpt_recovery_normal() {
    let mut app = test_app();
    app.runtime.model = "chatgpt-codex/gpt-5.5".to_string();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("chatgpt_recovery_normal", frame.render_styled());
}

#[test]
fn snapshot_chatgpt_recovery_narrow() {
    let mut app = test_app();
    app.runtime.model = "chatgpt-codex/gpt-5.5".to_string();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 40, 12);
    let frame = Frame { rows, width: 40, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("chatgpt_recovery_narrow", frame.render_styled());
}

#[test]
fn snapshot_chatgpt_recovery_tiny() {
    let mut app = test_app();
    app.runtime.model = "chatgpt-codex/gpt-5.5".to_string();
    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::ChatGptOAuthPolling,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: Some(crate::app::ChatGptOAuthRecovery {
            method: ChatGptOAuthMethod::DeviceCode,
            authorization_url: None,
            code: Some(ChatGptCodexDeviceCode {
                device_auth_id: "device-auth-secret-from-renderer-test".to_string(),
                user_code: "ABCD-EFGH".to_string(),
                verification_uri: Some("https://auth.example.test/device".to_string()),
                verification_uri_complete: None,
                expires_in: Some(900),
                interval: Some(5),
            }),
            next_poll_tick: 10,
            expires_at_tick: 9000,
            status: "Waiting for ChatGPT authorization.".to_string(),
        }),
    });
    let rows = accessory_rows(&app, 24, 3);
    let frame = Frame { rows, width: 24, cursor: None, cursor_visible: true };
    let rendered = frame.render_styled();
    assert!(!rendered.contains("device-token-secret-from-renderer-test"));
    insta::assert_snapshot!("chatgpt_recovery_tiny", rendered);
}

fn snapshot_prompt_at_widths(name: &str, text: &str) {
    let mut combined = String::new();
    for width in [80, 40] {
        let mut app = test_app();
        app.composer.input.set_text(text);
        let (rows, _) = prompt_rows_for(&app, width);
        let frame = Frame { rows, width, cursor: None, cursor_visible: true };
        combined.push_str(&format!("width={width}:\n"));
        for line in frame.render_styled().lines() {
            combined.push_str(line.trim_end());
            combined.push('\n');
        }
    }
    insta::assert_snapshot!(name, combined);
}

#[test]
fn snapshot_prompt_combining_marks() {
    snapshot_prompt_at_widths("prompt_combining_marks", "ab\u{0327}cd");
}

#[test]
fn snapshot_prompt_zwj_emoji() {
    snapshot_prompt_at_widths("prompt_zwj_emoji", "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}");
}

#[test]
fn snapshot_prompt_regional_indicators() {
    snapshot_prompt_at_widths("prompt_regional_indicators", "\u{1f1fa}\u{1f1f8}\u{1f1ec}\u{1f1e7}");
}

#[test]
fn snapshot_prompt_cjk() {
    snapshot_prompt_at_widths("prompt_cjk", "日本語テキスト");
}

#[test]
fn snapshot_prompt_zero_width() {
    snapshot_prompt_at_widths("prompt_zero_width", "a\u{200b}b\u{200d}c");
}

#[test]
fn snapshot_prompt_long_word() {
    snapshot_prompt_at_widths("prompt_long_word", &"a".repeat(120));
}

#[test]
fn snapshot_prompt_explicit_newline() {
    snapshot_prompt_at_widths("prompt_explicit_newline", "line one\nline two\nline three");
}

#[test]
fn snapshot_picker_cjk() {
    let mut app = test_app();
    let items = vec![
        PickerItem::new("src/日本語.rs".to_string(), ""),
        PickerItem::new("src/テスト.rs".to_string(), ""),
        PickerItem::new("Cargo.toml".to_string(), ""),
    ];
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        PickerState::new(items, 200),
    );

    let mut combined = String::new();
    for width in [80, 40] {
        let rows = accessory_rows(&app, width, 12);
        let frame = Frame { rows, width, cursor: None, cursor_visible: true };
        combined.push_str(&format!("width={width}:\n"));
        combined.push_str(&frame.render_styled());
        combined.push('\n');
    }
    insta::assert_snapshot!("picker_cjk", combined);
}

#[test]
fn snapshot_footer_cjk() {
    let mut app = test_app();
    app.runtime.cwd = std::path::PathBuf::from("/Users/owais/日本語プロジェクト");

    let mut combined = String::new();
    for width in [80, 40] {
        let row = static_status_row(&app, width);
        let frame = Frame { rows: vec![row], width, cursor: None, cursor_visible: true };
        combined.push_str(&format!("width={width}:\n"));
        combined.push_str(&frame.render_styled());
        combined.push('\n');
    }
    insta::assert_snapshot!("footer_cjk", combined);
}

#[test]
fn snapshot_ttft_statusline() {
    let mut combined = String::new();

    let mut pending = test_app();
    pending.runtime.ttft.set_pending_for_test();
    combined.push_str("pending:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&pending, 96)], width: 96, cursor: None, cursor_visible: true }
            .render_styled(),
    );
    combined.push('\n');

    let mut measured = test_app();
    measured
        .runtime
        .ttft
        .set_last_completed_for_test(std::time::Duration::from_millis(842));
    combined.push_str("measured:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&measured, 96)], width: 96, cursor: None, cursor_visible: true }
            .render_styled(),
    );
    combined.push('\n');

    let mut retained = test_app();
    retained
        .runtime
        .ttft
        .set_last_completed_for_test(std::time::Duration::from_millis(1_340));
    combined.push_str("retained:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&retained, 96)], width: 96, cursor: None, cursor_visible: true }
            .render_styled(),
    );
    combined.push('\n');

    let mut narrow = test_app();
    narrow.runtime.ttft.set_pending_for_test();
    combined.push_str("narrow:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&narrow, 80)], width: 80, cursor: None, cursor_visible: true }
            .render_styled(),
    );

    insta::assert_snapshot!("ttft_statusline", combined);
}
