use super::*;
use crate::acp::permissions::{PendingPermission, PermissionKindView, PermissionOptionView};
use crate::app::{App, FilePickerSource, FirstRunRecovery, Mode, PickerItem, PickerState, RecoveryStage, RunState};
use crate::cli::commands::setup::SetupProviderArg;
use crate::cli::{Cli, Theme, WebSearchMode};
use crate::renderer::git::GitStatusSummary;
use crate::renderer::layout::truncate_spans;
use crate::renderer::row::Frame;
use std::path::PathBuf;
use std::sync::mpsc;

fn test_app() -> App {
    let mut app = App::from_cli(&Cli {
        cwd: PathBuf::from("."),
        model: "test-model".to_string(),
        websearch: WebSearchMode::Native,
        reasoning_effort: Default::default(),
        reasoning_summary: Default::default(),
        tick_rate_ms: 100,
        no_alt_screen: true,
        no_mouse: false,
        mouse: false,
        verbose: false,
        theme: Theme::EldritchMinimal,
        print_prompt: false,
        skill_dirs: Vec::new(),
        session_dir: None,
        config_diagnostics: Vec::new(),
        config_layers: Vec::new(),
        config_origins: std::collections::BTreeMap::new(),
        acp_agents: std::collections::BTreeMap::new(),
        command: None,
    });
    app.git_status = Some(GitStatusSummary { branch: Some("main".to_string()), added: 0, modified: 0, deleted: 0 });
    app
}

#[test]
fn dynamic_status_row_has_session_and_label() {
    let app = test_app();
    let row = dynamic_status_row(&app, 80);
    let text = row.text();
    assert!(!text.is_empty(), "status row should have content");
    assert!(text.contains("idle"), "status label should appear");
}

#[test]
fn dynamic_status_row_shows_queue_when_working() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.queued_steering.push("steer".to_string());
    let row = dynamic_status_row(&app, 80);
    let text = row.text();
    assert!(text.contains("target:"), "queue target should appear when working");
    assert!(text.contains("queued: 1/0"), "queue counts should appear");
}

#[test]
fn prompt_rows_empty_input() {
    let app = test_app();
    let (rows, cursor) = prompt_rows_for(&app, 80);
    assert_eq!(rows.len(), 1, "empty input should produce one row");
    assert!(cursor.is_some(), "cursor should be present");
    let text = rows[0].text();
    assert!(text.contains("›"), "prompt icon should appear");
}

#[test]
fn prompt_rows_with_text() {
    let mut app = test_app();
    app.input.set_text("hello world");
    let (rows, cursor) = prompt_rows_for(&app, 80);
    assert_eq!(rows.len(), 1, "short text should fit on one row");
    assert!(cursor.is_some());
    assert!(rows[0].text().contains("hello world"));
}

#[test]
fn prompt_rows_multiline() {
    let mut app = test_app();
    app.input.set_text("line one\nline two");
    let (rows, _cursor) = prompt_rows_for(&app, 80);
    assert_eq!(rows.len(), 2, "two logical lines should produce two rows");
}

#[test]
fn prompt_rows_wraps_long_text() {
    let mut app = test_app();
    app.input.set_text(&"x".repeat(100));
    let (rows, _cursor) = prompt_rows_for(&app, 20);
    assert!(rows.len() > 1, "long text should wrap to multiple rows");
}

#[test]
fn prompt_rows_wrap_at_visible_content_width() {
    let mut app = test_app();
    app.input.set_text(&"x".repeat(11));
    let (rows, cursor) = prompt_rows_for(&app, 20);

    assert_eq!(rows.len(), 1);
    assert_eq!(cursor, Some(CursorCoord::new(0, 18)));

    app.input.insert_char('x');
    let (rows, cursor) = prompt_rows_for(&app, 20);

    assert_eq!(rows.len(), 2);
    assert_eq!(cursor, Some(CursorCoord::new(1, 8)));
}

#[test]
fn prompt_rows_command_mode_shows_colon() {
    let mut app = test_app();
    app.mode = Mode::Command;
    let (rows, _) = prompt_rows_for(&app, 80);
    assert!(rows[0].text().contains(':'), "command mode should show colon prefix");
}

#[test]
fn prompt_rows_submitted_shows_queue_icon() {
    let mut app = test_app();
    app.run_state = RunState::Working;

    let (rows, _) = prompt_rows_for(&app, 80);
    assert!(
        rows[0].text().contains("»"),
        "submitted state should show queue composer icon"
    );
}

#[test]
fn static_status_row_shows_model_at_narrow_width() {
    let app = test_app();
    let row = static_status_row(&app, 30);
    assert!(row.text().contains("model:"), "model should show at width 30");
}

#[test]
fn static_status_row_hides_everything_at_tiny_width() {
    let app = test_app();
    let row = static_status_row(&app, 10);
    let text = row.text();
    assert!(text.trim().is_empty(), "nothing should show at width 10");
}

#[test]
fn static_status_row_width_thresholds_control_segments() {
    let app = test_app();
    let cases = [
        (23, false, false, false, false, false, false, false),
        (24, true, false, false, false, false, false, false),
        (41, true, false, false, false, false, false, false),
        (42, true, true, false, false, false, false, false),
        (55, true, true, false, false, false, false, false),
        (56, true, true, true, false, false, false, false),
        (71, true, true, true, false, false, false, false),
        (72, true, true, true, false, true, false, false),
        (87, true, true, true, false, true, false, false),
        (88, true, true, true, false, true, false, false),
        (95, true, true, true, false, true, false, false),
        (96, true, true, true, false, true, false, false),
        (97, true, true, true, false, true, true, false),
        (159, true, true, true, false, true, true, false),
        (160, true, true, true, false, true, true, true),
    ];

    for (width, model, search, tokens, ttft, git, cwd, trust) in cases {
        let text = static_status_row(&app, width).text();
        assert_eq!(
            text.contains("model:"),
            model,
            "model visibility at width {width}: {text}"
        );
        assert_eq!(
            text.contains("search:"),
            search,
            "search visibility at width {width}: {text}"
        );
        assert_eq!(
            text.contains("tok:"),
            tokens,
            "token visibility at width {width}: {text}"
        );
        assert_eq!(text.contains("ttft:"), ttft, "TTFT visibility at width {width}: {text}");
        assert_eq!(text.contains("git:"), git, "git visibility at width {width}: {text}");
        assert_eq!(text.contains("cwd:"), cwd, "cwd visibility at width {width}: {text}");
        assert_eq!(
            text.contains("local user"),
            trust,
            "trust visibility at width {width}: {text}"
        );
    }
}

#[test]
fn static_status_row_shows_trust_at_very_wide_width() {
    let app = test_app();
    let text = static_status_row(&app, 220).text();

    assert!(
        text.contains("local user · workspace-contained tools · no TUI sandbox"),
        "trust label should be visible on very wide terminals"
    );
}

#[test]
fn static_status_row_shows_pending_ttft_when_width_allows() {
    let mut app = test_app();
    app.ttft.set_pending_for_test();
    let text = static_status_row(&app, 90).text();

    assert!(text.contains("ttft: pending"));
}

#[test]
fn static_status_row_formats_measured_ttft() {
    let mut app = test_app();
    app.ttft
        .set_last_completed_for_test(std::time::Duration::from_millis(987));
    assert!(static_status_row(&app, 90).text().contains("ttft: 987ms"));

    app.ttft
        .set_last_completed_for_test(std::time::Duration::from_millis(1_234));
    assert!(static_status_row(&app, 90).text().contains("ttft: 1.2s"));
}

#[test]
fn static_status_row_hides_ttft_at_narrow_width() {
    let mut app = test_app();
    app.ttft.set_pending_for_test();
    let text = static_status_row(&app, 80).text();

    assert!(text.contains("tok:"), "core token status should remain visible");
    assert!(!text.contains("ttft:"), "TTFT should hide before core status");
}

#[test]
fn static_status_row_shows_all_at_wide_width() {
    let app = test_app();
    let row = static_status_row(&app, 128);
    let text = row.text();
    assert!(text.contains("model:"));
    assert!(text.contains("search:"));
    assert!(text.contains("tok:"));
    assert!(text.contains("git: main clean"));
}

#[test]
fn static_status_row_uses_codex_model_label_and_shows_reasoning_when_supported() {
    let mut app = test_app();
    app.model = "chatgpt-codex/gpt-5.6-terra".to_string();
    app.cli.reasoning_effort = crate::cli::ReasoningEffort::High;

    let text = static_status_row(&app, 128).text();

    assert!(text.contains("model: codex/gpt-5.6-terra"));
    assert!(text.contains("reasoning: high"));
    assert!(!text.contains("chatgpt-codex/"));
}

#[test]
fn static_status_row_shows_umans_reasoning_toggle() {
    let mut app = test_app();
    app.model = "umans-glm-5.2".to_string();
    app.cli.reasoning_effort = crate::cli::ReasoningEffort::On;

    let text = static_status_row(&app, 128).text();

    assert!(text.contains("reasoning: on"));
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
    app.prompt_accessory = PromptAccessory::Help;

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
    app.picker = Some(PickerState::new(items, 200));
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Forced);
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
    app.pending_permission = Some(PendingPermission {
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
    if let Some(picker) = app.picker.as_mut() {
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
    if let Some(picker) = app.picker.as_mut() {
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
    if let Some(picker) = app.picker.as_mut() {
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
    app.picker = Some(PickerState::new(
        vec![
            PickerItem::new("umans-coder", "Default route to Kimi K2.7-Code"),
            PickerItem::new("umans-glm-5.2", "Largest context window"),
        ],
        50,
    ));
    if let Some(picker) = app.picker.as_mut() {
        picker.selected = 1;
    }
    app.prompt_accessory = PromptAccessory::Models;
    let rows = accessory_rows(&app, 80, 12);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("model_picker", frame.render_styled());
}

#[test]
fn snapshot_mention_styling_in_prompt() {
    let mut app = test_app();
    app.input.set_text("check @src/main.rs for details");
    let (rows, _) = prompt_rows_for(&app, 80);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("mention_styling", frame.render_styled());
}

#[test]
fn snapshot_help_rows() {
    let mut app = test_app();
    app.prompt_accessory = PromptAccessory::Help;
    let rows = accessory_rows(&app, 80, 16);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("help_rows", frame.render_styled());
}

#[test]
fn help_rows_show_running_ctrl_t_binding() {
    let mut app = test_app();
    app.run_state = RunState::Working;
    app.prompt_accessory = PromptAccessory::Help;
    let rows = accessory_rows(&app, 80, 16);
    let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

    assert!(
        text.contains("Ctrl+T          toggle queue target"),
        "running help should describe queue-target toggle: {text}"
    );
}

#[test]
fn snapshot_command_suggestions() {
    let mut app = test_app();
    app.input.set_text("c");
    app.mode = Mode::Command;
    app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
    let rows = accessory_rows(&app, 80, 8);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("command_suggestions", frame.render_styled());
}

#[test]
fn snapshot_first_run_recovery_normal() {
    let mut app = test_app();
    app.model = "umans-coder".to_string();
    app.first_run_recovery = Some(FirstRunRecovery {
        provider: Some(SetupProviderArg::Umans),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 80, 8);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("first_run_recovery_normal", frame.render_styled());
}

#[test]
fn snapshot_first_run_recovery_narrow() {
    let mut app = test_app();
    app.model = "opencode-go/kimi-k2.7-code".to_string();
    app.first_run_recovery = Some(FirstRunRecovery {
        provider: Some(SetupProviderArg::OpencodeGo),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 40, 8);
    let frame = Frame { rows, width: 40, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("first_run_recovery_narrow", frame.render_styled());
}

#[test]
fn snapshot_first_run_recovery_tiny() {
    let mut app = test_app();
    app.model = "umans-coder".to_string();
    app.first_run_recovery = Some(FirstRunRecovery {
        provider: Some(SetupProviderArg::Umans),
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
    app.model = "chatgpt-codex/gpt-5.5".to_string();
    app.first_run_recovery = Some(FirstRunRecovery {
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 80, 8);
    let frame = Frame { rows, width: 80, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("chatgpt_recovery_normal", frame.render_styled());
}

#[test]
fn snapshot_chatgpt_recovery_narrow() {
    let mut app = test_app();
    app.model = "chatgpt-codex/gpt-5.5".to_string();
    app.first_run_recovery = Some(FirstRunRecovery {
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::MissingCredential,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
    let rows = accessory_rows(&app, 40, 8);
    let frame = Frame { rows, width: 40, cursor: None, cursor_visible: true };
    insta::assert_snapshot!("chatgpt_recovery_narrow", frame.render_styled());
}

#[test]
fn snapshot_chatgpt_recovery_tiny() {
    let mut app = test_app();
    app.model = "chatgpt-codex/gpt-5.5".to_string();
    app.first_run_recovery = Some(FirstRunRecovery {
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::ChatGptOAuthPolling,
        pending_provider_prompt: true,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: Some(crate::app::ChatGptOAuthRecovery {
            code: crate::thndrs_core::auth::ChatGptCodexDeviceCode {
                device_auth_id: "device-auth-secret-from-renderer-test".to_string(),
                user_code: "ABCD-EFGH".to_string(),
                verification_uri: Some("https://auth.example.test/device".to_string()),
                verification_uri_complete: None,
                expires_in: Some(900),
                interval: Some(5),
            },
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
        app.input.set_text(text);
        let (rows, _) = prompt_rows_for(&app, width);
        let frame = Frame { rows, width, cursor: None, cursor_visible: true };
        combined.push_str(&format!("width={width}:\n"));
        combined.push_str(&frame.render_styled());
        combined.push('\n');
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
    app.picker = Some(PickerState::new(items, 200));
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Forced);

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
    app.cwd = std::path::PathBuf::from("/Users/owais/日本語プロジェクト");

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
    pending.ttft.set_pending_for_test();
    combined.push_str("pending:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&pending, 96)], width: 96, cursor: None, cursor_visible: true }
            .render_styled(),
    );
    combined.push('\n');

    let mut measured = test_app();
    measured
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
        .ttft
        .set_last_completed_for_test(std::time::Duration::from_millis(1_340));
    combined.push_str("retained:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&retained, 96)], width: 96, cursor: None, cursor_visible: true }
            .render_styled(),
    );
    combined.push('\n');

    let mut narrow = test_app();
    narrow.ttft.set_pending_for_test();
    combined.push_str("narrow:\n");
    combined.push_str(
        &Frame { rows: vec![static_status_row(&narrow, 80)], width: 80, cursor: None, cursor_visible: true }
            .render_styled(),
    );

    insta::assert_snapshot!("ttft_statusline", combined);
}
