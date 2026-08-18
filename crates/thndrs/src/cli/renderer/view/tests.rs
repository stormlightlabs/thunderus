use crate::acp::permissions::{PendingPermission, PermissionKindView, PermissionOptionView};
use crate::app::{
    App, BlockContentState, Entry, FilePickerSource, FirstRunRecovery, Mode, PickerItem, PickerState, PromptAccessory,
    QueueTarget, RecoveryStage, RunState, ToolLifecycleState, ToolStatus, TranscriptBlockKind, VISIBLE_ROWS,
};
use crate::cli::{Cli, Theme, commands::setup::SetupProviderArg};
use crate::renderer;
use crate::renderer::view::{
    FocusedSurfaceView, PromptStatusView, PromptSuggestionKind, RendererView, TranscriptRowKind, TruncationPolicy,
};
use std::path::PathBuf;
use std::sync::mpsc;

fn test_app() -> App {
    let mut app = App::from_cli(&Cli {
        cwd: PathBuf::from("."),
        model: "test-model".to_string(),
        reasoning_effort: Default::default(),
        reasoning_summary: Default::default(),
        tick_rate_ms: 100,
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
    app.session.id = "test-session".to_string();
    app.runtime.git_status =
        Some(crate::cli::git::GitStatusSummary { branch: Some("main".to_string()), added: 0, modified: 0, deleted: 0 });
    app.transcript.entries.clear();
    app.transcript.context_sources.clear();
    app.transcript.skills.clear();
    app.transcript.skill_diagnostics.clear();
    app
}

#[test]
fn build_view_idle_has_empty_transcript_and_banner() {
    let app = test_app();
    let view = RendererView::build(&app, 80, 24);

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
fn context_surface_stays_bounded_at_normal_narrow_and_small_height() {
    let mut app = test_app();
    app.refresh_context_ledger(None);
    app.overlay.show_context();

    for (width, height) in [(80, 24), (30, 8), (20, 3)] {
        let view = RendererView::build(&app, width, height);
        assert!(
            view.live.accessory_rows.iter().all(|row| row.width == width),
            "context rows must preserve width at {width}x{height}"
        );
        assert!(
            view.live.accessory_rows.len() <= VISIBLE_ROWS,
            "context rows must remain bounded at {width}x{height}"
        );
    }
}

#[test]
fn build_view_submitted_user_is_rendered() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript
        .entries
        .push(Entry::User { text: "do the thing".to_string() });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        view.transcript.banner_rows.is_empty(),
        "banner rows should not be exposed once transcript is non-empty"
    );
    assert!(
        !view.transcript.stable_rows.is_empty(),
        "submitted user rows should be stable while the first response is pending"
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
        "submitted input should be retained in the visible transcript"
    );
}

#[test]
fn semantic_view_exposes_stable_block_identity_kind_and_tool_state() {
    let mut app = test_app();
    app.transcript
        .entries
        .queue_tool("call_1", "search_text", r#"{"pattern":"needle"}"#)
        .expect("queue tool");
    app.transcript.entries.start_tool("call_1").expect("start tool");

    let view = RendererView::build(&app, 80, 24);
    let row = view.semantic.transcript.rows.first().expect("semantic tool row");
    let tool = row.tool.as_ref().expect("tool state");
    assert_eq!(row.block_id.as_ref().map(|id| id.as_str()), Some("tool:call_1"));
    assert_eq!(row.block_kind, Some(TranscriptBlockKind::ToolCall));
    assert_eq!(tool.action.as_deref(), Some("search_text"));
    assert_eq!(tool.target.as_deref(), Some("needle"));
    assert_eq!(tool.lifecycle, Some(ToolLifecycleState::Running));
    assert_eq!(tool.result_state, Some(BlockContentState::Unknown));
}

#[test]
fn build_view_streaming_assistant_is_all_live() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;

    let text = "line one. line two. line three. line four. line five. line six. line seven. line eight.";
    app.transcript
        .entries
        .push(Entry::Agent { text: text.to_string(), streaming: true });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        !view
            .transcript
            .stable_rows
            .iter()
            .any(|row| row.text().contains("Response") || row.text().contains("line one")),
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
        !view
            .transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Response")),
        "live_rows should not add a response header"
    );
}

#[test]
fn build_view_long_streaming_reasoning_is_all_live() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript.entries.push(Entry::Reasoning {
        text: (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"),
        streaming: true,
    });

    let view = RendererView::build(&app, 32, 24);

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
            .any(|row| row.text().contains("line 0")),
        "live rows should include the compact reasoning summary"
    );
}

#[test]
fn build_view_streaming_assistant_short_block_is_all_live() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript
        .entries
        .push(Entry::Agent { text: "short".to_string(), streaming: true });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        !view
            .transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Response")),
        "live_rows should not add a response header for a short block"
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
            .any(|row| row.text().contains("Response")),
        "stable_rows should not contain the response header for a short block"
    );
}

#[test]
fn build_view_running_tool_is_live_only() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec!["running tests".to_string()],
    });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        !view.transcript.live_rows.is_empty(),
        "running tool should produce live rows"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Testing cargo test")),
        "live_rows should contain the semantic current operation"
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
fn activity_timeline_has_no_standalone_heading() {
    let mut app = test_app();
    for name in ["find_files", "search_text"] {
        app.transcript.entries.push(Entry::Tool {
            name: name.to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["stored output".to_string()],
        });
    }

    let view = RendererView::build(&app, 80, 24);
    let activity_headings = view
        .transcript
        .stable_rows
        .iter()
        .filter(|row| row.text().contains("Activity"))
        .count();

    assert_eq!(
        activity_headings, 0,
        "activity rows should not repeat a standalone heading"
    );
}

#[test]
fn routine_exploration_collapses_into_one_activity_summary() {
    let mut app = test_app();
    for (name, arguments) in [
        ("find_files", r#"{"pattern":"Cargo.toml"}"#),
        ("read_file_range", r#"{"path":"Cargo.toml"}"#),
        ("search_text", r#"{"pattern":"workspace"}"#),
    ] {
        app.transcript.entries.push(Entry::Tool {
            name: name.to_string(),
            arguments: arguments.to_string(),
            status: ToolStatus::Ok,
            output: vec!["stored output".to_string()],
        });
    }

    let rendered = RendererView::build(&app, 80, 24)
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("✓ Explored · 1 read · 2 searches"), "{rendered}");
    assert!(rendered.contains("Ctrl+O details"), "{rendered}");
    assert!(!rendered.contains("find_files"), "{rendered}");
    assert!(!rendered.contains("read_file_range"), "{rendered}");
    assert!(!rendered.contains("search_text"), "{rendered}");
}

#[test]
fn activity_summary_keeps_disclosure_visible_when_narrow() {
    let mut app = test_app();
    for name in ["find_files", "read_file_range", "search_text"] {
        app.transcript.entries.push(Entry::Tool {
            name: name.to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["stored output".to_string()],
        });
    }

    let narrow = RendererView::build(&app, 40, 24);
    let narrow_text = narrow
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(narrow_text.contains("Ctrl+O details"), "{narrow_text}");
    assert!(narrow.transcript.rows.iter().all(|row| row.text_width() == 40));

    let tiny = RendererView::build(&app, 20, 24);
    assert!(tiny.transcript.rows.iter().all(|row| row.text_width() == 20));
    assert!(
        tiny.transcript.rows.iter().any(|row| row.text().contains("Explored")),
        "tiny activity summary should retain its semantic status"
    );
}

#[test]
fn activity_detail_discloses_individual_exploration_calls() {
    let mut app = test_app();
    for name in ["find_files", "search_text"] {
        app.transcript.entries.push(Entry::Tool {
            name: name.to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec![format!("{name} output")],
        });
    }
    app.overlay.show_detail(1);

    let rendered = RendererView::build(&app, 80, 24)
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("✓ Explored · 2 searches · Esc close"), "{rendered}");
    assert!(rendered.contains("find_files"), "{rendered}");
    assert!(rendered.contains("search_text"), "{rendered}");
    assert!(rendered.contains("search_text output"), "{rendered}");
}

#[test]
fn running_exploration_updates_one_live_summary_row() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "read_file_range".to_string(),
        arguments: r#"{"path":"src/lib.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["stored read".to_string()],
    });
    app.transcript.entries.push(Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"Activity"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec!["streaming match".to_string()],
    });

    let view = RendererView::build(&app, 80, 24);
    let live = view
        .transcript
        .live_rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            .iter()
            .any(|marker| live.contains(&format!("{marker} Searching Activity"))),
        "{live}"
    );
    assert!(live.contains("Searching Activity · 1 read · 1 search"), "{live}");
    assert!(!live.contains("read_file_range"), "{live}");
    assert!(!live.contains("search_text"), "{live}");
    assert!(!live.contains("streaming match"), "{live}");

    app.runtime.ui_tick = app.runtime.ui_tick.saturating_add(8);
    let animated = RendererView::build(&app, 80, 24)
        .transcript
        .live_rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(animated.contains("Searching Activity"), "{animated}");
    assert_ne!(animated, live, "the running marker should animate");
}

#[test]
fn failed_exploration_summarizes_the_diagnostic() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"missing"}"#.to_string(),
        status: ToolStatus::Failed,
        output: vec!["permission denied".to_string()],
    });

    let rendered = RendererView::build(&app, 80, 24)
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Exploration failed"), "{rendered}");
    assert!(rendered.contains("permission denied"), "{rendered}");
    assert!(rendered.contains("Ctrl+O details"), "{rendered}");
    assert!(!rendered.contains("search_text"), "{rendered}");
    assert!(!rendered.contains("Explored"), "{rendered}");
}

#[test]
fn significant_activities_use_semantic_timeline_rows() {
    let mut app = test_app();
    for entry in [
        Entry::Tool {
            name: "write_patch".to_string(),
            arguments: r#"{"patches":[{"op":"edit","path":"src/lib.rs"}]}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec![
                "--- a/src/lib.rs".to_string(),
                "+++ b/src/lib.rs".to_string(),
                "@@ -1 +1 @@".to_string(),
                "-old".to_string(),
                "+new".to_string(),
            ],
        },
        Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"argv":["cargo","test","renderer"]}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec![
                "test result: ok. 12 passed; 0 failed".to_string(),
                "test result: ok. 0 passed; 0 failed".to_string(),
            ],
        },
    ] {
        app.transcript.entries.push(entry);
    }

    let rendered = RendererView::build(&app, 80, 24)
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Edited src/lib.rs · +1 −1"), "{rendered}");
    assert!(rendered.contains("Tests passed · 12 passed"), "{rendered}");
    assert!(!rendered.contains("write_patch"), "{rendered}");
    assert!(!rendered.contains("run_shell"), "{rendered}");
}

#[test]
fn cancelled_activities_never_look_successful() {
    let mut app = test_app();
    for entry in [
        Entry::Tool {
            name: "search_text".to_string(),
            arguments: r#"{"pattern":"needle"}"#.to_string(),
            status: ToolStatus::Cancelled,
            output: vec!["search stopped".to_string()],
        },
        Entry::Tool {
            name: "write_patch".to_string(),
            arguments: r#"{"patches":[{"path":"src/lib.rs"}]}"#.to_string(),
            status: ToolStatus::Cancelled,
            output: vec!["edit stopped".to_string()],
        },
        Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"argv":["cargo","test"]}"#.to_string(),
            status: ToolStatus::Cancelled,
            output: vec!["tests stopped".to_string()],
        },
        Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"argv":["git","diff"]}"#.to_string(),
            status: ToolStatus::Cancelled,
            output: vec!["command stopped".to_string()],
        },
    ] {
        app.transcript.entries.push(entry);
    }

    let rendered = RendererView::build(&app, 80, 24)
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    for label in [
        "Exploration cancelled",
        "Edit cancelled src/lib.rs",
        "Tests cancelled",
        "Command cancelled git diff",
    ] {
        assert!(rendered.contains(label), "missing {label}:\n{rendered}");
    }
    assert!(!rendered.contains("Tests passed"), "{rendered}");
    assert!(!rendered.contains("Edited src/lib.rs"), "{rendered}");
}

#[test]
fn semantic_write_patch_summary_uses_nested_argument_path() {
    let entry = Entry::Tool {
        name: "write_patch".to_string(),
        arguments: r#"{"patches":[{"path":"src/lib.rs"}]}"#.to_string(),
        status: ToolStatus::Ok,
        output: Vec::new(),
    };

    let row = super::TranscriptRowView::from(&entry);

    assert_eq!(row.edit.and_then(|edit| edit.path), Some("src/lib.rs".to_string()));
}

#[test]
fn failed_tests_stay_loud_and_commands_stay_compact_when_narrow() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"argv":["cargo","test","renderer"]}"#.to_string(),
        status: ToolStatus::Failed,
        output: vec![
            "error: command failed (exit 101)".to_string(),
            "$ cargo test renderer [one-shot failed exit 101 4800ms]".to_string(),
            "── stdout ──".to_string(),
            "running 2 tests".to_string(),
            "test renderer::keeps_diagnostics ... FAILED".to_string(),
            "assertion failed: rendered.contains(\"diagnostic\")".to_string(),
            "  --> crates/thndrs/src/cli/renderer/view/tests.rs:42:5".to_string(),
            "test result: FAILED. 1 passed; 1 failed".to_string(),
        ],
    });
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"argv":["git","diff","--check"]}"#.to_string(),
        status: ToolStatus::Ok,
        output: Vec::new(),
    });

    let wide_rendered = RendererView::build(&app, 80, 24)
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");
    let narrow_view = RendererView::build(&app, 40, 24);
    let narrow_rendered = narrow_view
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        wide_rendered.contains("Tests failed · 1 failed · 1 passed · 4.8s · exit 101"),
        "{wide_rendered}"
    );
    assert!(wide_rendered.contains("$ cargo test renderer"), "{wide_rendered}");
    assert!(
        wide_rendered.contains("keeps_diagnostics ... FAILED"),
        "{wide_rendered}"
    );
    assert!(
        wide_rendered.contains("assertion failed: rendered.contains"),
        "{wide_rendered}"
    );
    assert!(wide_rendered.contains("view/tests.rs:42:5"), "{wide_rendered}");
    assert!(wide_rendered.contains("… +5 lines"), "{wide_rendered}");
    assert!(!wide_rendered.contains("[Failed] run_shell"), "{wide_rendered}");
    assert!(!wide_rendered.contains("Middle output hidden"), "{wide_rendered}");
    assert!(narrow_rendered.contains("Ran git diff --check"), "{narrow_rendered}");
    assert!(narrow_view.transcript.rows.iter().all(|row| row.text_width() == 40));
}

#[test]
fn build_view_narrow_changes_row_counts() {
    let mut app = test_app();
    app.composer
        .input
        .set_text("a longer prompt that should wrap at narrow width");
    app.transcript
        .entries
        .push(Entry::User { text: "This is a user message that will wrap differently at narrow width.".to_string() });

    let wide = RendererView::build(&app, 80, 24);
    let narrow = RendererView::build(&app, 40, 24);

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
    app.composer.input.set_text("hello world");
    let view = RendererView::build(&app, 80, 24);

    assert!(
        view.live.prompt_cursor.is_some(),
        "editable prompt should have a cursor in the view"
    );
}

#[test]
fn build_view_prompt_clipping_keeps_cursor_row() {
    let mut app = test_app();
    app.composer
        .input
        .set_text(&(0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));

    let view = RendererView::build(&app, 80, 24);
    let text = view
        .live
        .prompt_rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(view.live.prompt_rows.len(), renderer::live::MAX_PROMPT_ROWS);
    assert!(
        text.contains("test-session"),
        "clipping should preserve the metadata row:\n{text}"
    );
    assert!(text.contains(['╭', '╮', '╰', '╯', '│']));
    assert!(
        text.contains("line 19"),
        "prompt clipping should keep the editable tail:\n{text}"
    );
    assert_eq!(
        view.live.prompt_cursor.map(|cursor| cursor.row),
        Some(renderer::live::MAX_PROMPT_ROWS - 2),
        "cursor row should be rebased above the composer's bottom padding"
    );
}

#[test]
fn build_view_working_state_keeps_composer_status_separate_from_transcript() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript.entries.push(Entry::Agent {
        text: "streaming text that is currently being generated by the model".to_string(),
        streaming: true,
    });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        !view.transcript.live_rows.is_empty(),
        "working state should project mutable transcript rows"
    );
    assert!(!view.live.prompt_rows[0].text().contains("Working"));
    assert!(view.live.static_status.text().contains("Working"));
}

#[test]
fn build_view_streaming_with_tool_has_running_transcript_projection() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec!["test running ...".to_string()],
    });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        !view.transcript.live_rows.is_empty(),
        "running tool should have a mutable projection"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|r| r.text().contains("Testing cargo test")),
        "live tail should describe the active operation"
    );
}

#[test]
fn build_view_accessory_surfaces_are_present_when_active() {
    let mut app = test_app();
    app.composer.input.set_text("@src");
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        PickerState::new(vec![PickerItem::new("src/main.rs", "main entry")], 50),
    );

    let view = RendererView::build(&app, 80, 24);

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
    app.runtime.run_state = RunState::Working;
    app.composer
        .queue
        .push(QueueTarget::FollowUp, "next task".to_string(), "test".to_string());
    app.composer
        .queue
        .push(QueueTarget::Steering, "look at tests".to_string(), "test".to_string());

    let view = RendererView::build(&app, 80, 24);

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
fn build_view_steering_summary_uses_plain_label() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.composer
        .queue
        .push(QueueTarget::Steering, "look at tests".to_string(), "test".to_string());

    let view = RendererView::build(&app, 80, 24);
    let summary = view
        .live
        .queued_summary
        .as_ref()
        .expect("steering summary should be present");

    assert_eq!(summary.text().trim(), "Steering");
}

#[test]
fn build_view_queued_summary_absent_when_nothing_queued() {
    let app = test_app();
    let view = RendererView::build(&app, 80, 24);

    assert!(
        view.live.queued_summary.is_none(),
        "no queued summary when nothing queued"
    );
}

#[test]
fn build_view_pending_permission_takes_priority_over_focused_surface() {
    let mut app = test_app();
    let (tx, _rx) = mpsc::channel();
    app.overlay.show_help();
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
        selected: 0,
        responder: tx,
    });

    let view = RendererView::build(&app, 80, 24);
    let text = view
        .live
        .accessory_rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("permission"),
        "permission prompt should be visible:\n{text}"
    );
    assert!(
        text.contains("Write src/main.rs"),
        "permission title should be visible:\n{text}"
    );
    assert!(
        !text.contains("Ctrl+O"),
        "help rows must not mask a pending permission:\n{text}"
    );
}

#[test]
fn build_view_expands_tool_detail_inline() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["file_a.rs".to_string(), "file_b.rs".to_string()],
    });
    app.overlay.show_detail(0);

    let view = RendererView::build(&app, 80, 24);

    assert!(
        view.live.detail_pane.is_empty(),
        "tool detail must not occupy the composer accessory area"
    );
    let transcript = view
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        transcript.contains("run_shell") && transcript.contains("Esc close"),
        "expanded transcript row should retain its tool header: {transcript}"
    );
    assert!(
        transcript.contains("file_a.rs"),
        "expanded transcript row should contain tool output: {transcript}"
    );
}

#[test]
fn build_view_inline_tool_detail_scrolls_wrapped_rows() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "printf"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda".to_string(),
            "second output line".to_string(),
        ],
    });
    app.overlay.show_detail(0);
    app.overlay.detail_mut().expect("detail overlay").scroll = 1;

    let view = RendererView::build(&app, 30, 24);
    let body = view
        .transcript
        .rows
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
        body.contains("second output line"),
        "scrolling should reveal the next output line:\n{body}"
    );
}

#[test]
fn build_view_inline_tool_detail_scrolls_from_selected_line() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "seq 20"}"#.to_string(),
        status: ToolStatus::Ok,
        output: (0..20).map(|index| format!("line {index}")).collect(),
    });
    app.overlay.show_detail(0);
    app.overlay.detail_mut().expect("detail overlay").scroll = 3;

    let view = RendererView::build(&app, 80, 24);
    let body = view
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !body.contains("line 0") && body.contains("line 3") && body.contains("line 19"),
        "inline detail should begin at its scroll offset without a detached clipping footer:\n{body}"
    );
}

#[test]
fn build_view_handles_large_transcript_with_running_tool_and_inline_detail() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;

    for index in 0..300 {
        app.transcript
            .entries
            .push(Entry::User { text: format!("user message {index}") });
        app.transcript.entries.push(Entry::Agent {
            text: format!("assistant response {index} with enough prose to wrap at least sometimes"),
            streaming: false,
        });
    }

    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Ok,
        output: (0..200).map(|index| format!("finished output line {index}")).collect(),
    });
    let detail_index = app.transcript.entries.len() - 1;
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test renderer"}"#.to_string(),
        status: ToolStatus::Running,
        output: (0..200).map(|index| format!("running output line {index}")).collect(),
    });
    app.overlay.show_detail(detail_index);
    app.overlay.detail_mut().expect("detail overlay").scroll = 120;

    let view = RendererView::build(&app, 100, 32);

    assert!(
        view.transcript.stable_rows.len() > 300,
        "large transcript should still produce stable rows"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("Testing cargo test renderer")),
        "running tool should stay live as a compact semantic row"
    );
    assert!(
        view.transcript
            .live_rows
            .iter()
            .any(|row| row.text().contains("finished output line")),
        "expanded completed output must be live so opening and scrolling redraw it"
    );
}

#[test]
fn build_view_detail_pane_absent_when_closed() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["file_a.rs".to_string()],
    });

    let view = RendererView::build(&app, 80, 24);

    assert!(
        view.live.detail_pane.is_empty(),
        "detail pane should be empty when closed"
    );
}

#[test]
fn ctrl_o_affordance_only_appears_on_latest_eligible_tool() {
    let mut app = test_app();
    for (name, output) in [
        ("run_shell", vec!["old output".to_string()]),
        ("read_file", vec!["latest output".to_string()]),
    ] {
        app.transcript.entries.push(Entry::Tool {
            name: name.to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output,
        });
    }

    let view = RendererView::build(&app, 80, 24);
    let transcript = view
        .transcript
        .rows
        .iter()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(transcript.matches("Ctrl+O details").count(), 1, "{transcript}");
    let latest = transcript
        .lines()
        .find(|line| line.contains("Ctrl+O details"))
        .expect("latest activity summary row");
    assert!(latest.contains("Ran read_file"), "{latest}");
}

#[test]
fn build_view_narrow_width_still_has_prompt_and_footer() {
    let mut app = test_app();
    app.composer
        .input
        .set_text("a long prompt that wraps at narrow width definitely");
    app.transcript
        .entries
        .push(Entry::User { text: "user message".to_string() });

    let view = RendererView::build(&app, 20, 24);

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
fn build_view_tiny_height_keeps_composer_and_status_without_owning_transcript_layout() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;

    let text = "line one. line two. line three. line four. line five. line six. line seven. line eight.".to_string();
    app.transcript.entries.push(Entry::Agent { text, streaming: true });

    let view = RendererView::build(&app, 80, 8);

    assert!(
        !view.transcript.live_rows.is_empty(),
        "transcript projection remains independent from the bottom pane height"
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
    let view = RendererView::build(&app, 80, 3);

    assert!(!view.live.prompt_rows.is_empty(), "prompt should exist at height 3");
    assert!(
        !view.live.static_status.text().is_empty(),
        "static status should exist at height 3"
    );
}

#[test]
fn build_view_view_dimensions_match_input() {
    let app = test_app();
    let view = RendererView::build(&app, 72, 30);

    assert_eq!(view.width, 72);
    assert_eq!(view.height, 30);
}

#[test]
fn semantic_view_maps_transcript_row_kinds_and_tool_states() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::User { text: "hello".to_string() });
    app.transcript
        .entries
        .push(Entry::Agent { text: "answer".to_string(), streaming: false });
    app.transcript
        .entries
        .push(Entry::Reasoning { text: "thinking".to_string(), streaming: true });
    app.transcript.entries.push(Entry::Skill {
        name: "writing".to_string(),
        path: "/skills/writing/SKILL.md".to_string(),
        content: "instructions".to_string(),
        token_estimate: 842,
        context_percent: Some(1),
    });
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program":"cargo test"}"#.to_string(),
        status: ToolStatus::Failed,
        output: vec!["failure".to_string()],
    });
    app.transcript
        .entries
        .push(Entry::Status { text: "cancelled".to_string() });

    let view = RendererView::build(&app, 80, 24);
    let rows = &view.semantic.transcript.rows;

    assert_eq!(rows[0].kind, TranscriptRowKind::User);
    assert_eq!(rows[1].kind, TranscriptRowKind::Assistant);
    assert_eq!(rows[2].kind, TranscriptRowKind::Reasoning);
    assert!(!rows[2].stable, "streaming reasoning should be semantic-live");
    assert_eq!(rows[3].kind, TranscriptRowKind::Skill);
    assert!(rows[3].primary.contains("~842 tokens · 1% context"));
    assert_eq!(rows[4].kind, TranscriptRowKind::Tool);
    assert_eq!(rows[4].tool.as_ref().map(|tool| tool.status), Some(ToolStatus::Failed));
    assert_eq!(rows[5].kind, TranscriptRowKind::Cancelled);
}

#[test]
fn semantic_view_represents_edit_and_diff_summaries() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "replace_range#tool1".to_string(),
        arguments: r#"{"path":"src/lib.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "wrote update: src/lib.rs".to_string(),
            "--- a/src/lib.rs".to_string(),
            "+++ b/src/lib.rs".to_string(),
            "@@ -1 +1 @@".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ],
    });

    let view = RendererView::build(&app, 80, 24);
    let row = &view.semantic.transcript.rows[0];

    assert_eq!(row.kind, TranscriptRowKind::Diff);
    assert_eq!(
        row.edit.as_ref().and_then(|edit| edit.path.as_deref()),
        Some("src/lib.rs")
    );
    let diff = row.diff.as_ref().expect("diff summary");
    assert_eq!(diff.files, vec!["src/lib.rs"]);
    assert_eq!(diff.added, 1);
    assert_eq!(diff.removed, 1);
}

#[test]
fn semantic_prompt_has_queued_summary_without_queued_text() {
    let mut app = test_app();
    app.runtime.run_state = RunState::Working;
    app.composer.queue.push(
        QueueTarget::FollowUp,
        "do the private next thing".to_string(),
        "test".to_string(),
    );
    app.composer
        .queue
        .push(QueueTarget::Steering, "steer quietly".to_string(), "test".to_string());

    let view = RendererView::build(&app, 80, 24);
    let prompt = &view.semantic.prompt;
    let queued = prompt.queued.as_ref().expect("queued summary");

    assert_eq!(prompt.status, PromptStatusView::Queued);
    assert_eq!(queued.steering_count, 1);
    assert_eq!(queued.followup_count, 1);
    assert_eq!(queued.target, "follow-up");
    assert!(
        !format!("{prompt:?}").contains("do the private next thing"),
        "semantic queued state should be summary-only"
    );
}

#[test]
fn semantic_prompt_represents_command_suggestions() {
    let mut app = test_app();
    app.composer.mode = Mode::Command;
    app.composer.input.set_text("he");
    app.overlay.show_commands();

    let view = RendererView::build(&app, 80, 24);
    let suggestions = &view.semantic.prompt.suggestions;

    assert_eq!(view.semantic.prompt.status, PromptStatusView::Suggesting);
    assert!(
        suggestions
            .iter()
            .any(|suggestion| suggestion.kind == PromptSuggestionKind::Command && suggestion.label == "help")
    );
}

#[test]
fn semantic_prompt_represents_file_mention_suggestions() {
    let mut app = test_app();
    app.composer.input.set_text("read @src");
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Mention { token_start: 5 }),
        PickerState::new(vec![PickerItem::new("src/lib.rs", "library")], 50),
    );

    let view = RendererView::build(&app, 80, 24);
    let suggestions = &view.semantic.prompt.suggestions;

    assert_eq!(view.semantic.prompt.status, PromptStatusView::Suggesting);
    assert_eq!(suggestions[0].kind, PromptSuggestionKind::FileMention);
    assert_eq!(suggestions[0].label, "src/lib.rs");
}

#[test]
fn semantic_session_picker_projects_recent_session_metadata() {
    let mut app = test_app();
    let _ = app.overlay.show_picker(
        PromptAccessory::Sessions,
        PickerState::new(
            vec![PickerItem::new(
                "Named work",
                "session-20260809 · opencode/big-pickle · 12 in / 7 out",
            )],
            50,
        ),
    );

    let view = RendererView::build(&app, 80, 24);

    match &view.semantic.focused_surface {
        FocusedSurfaceView::CommandPicker(picker) => {
            assert_eq!(picker.title, "sessions");
            assert_eq!(picker.items[0].label, "Named work");
            assert!(picker.items[0].detail.contains("session-20260809"));
            assert!(picker.items[0].detail.contains("12 in / 7 out"));
        }
        surface => panic!("expected session picker, got {surface:?}"),
    }
}

#[test]
fn semantic_orientation_has_truncation_metadata() {
    let app = test_app();
    let view = RendererView::build(&app, 80, 24);
    let orientation = &view.semantic.orientation;

    assert!(
        orientation
            .fields
            .iter()
            .any(|field| field.label == "workspace" && field.truncate == TruncationPolicy::EllipsizeMiddle)
    );
    assert!(
        orientation
            .fields
            .iter()
            .any(|field| field.label == "access" && field.truncate == TruncationPolicy::Hide)
    );
}

#[test]
fn semantic_orientation_identifies_ephemeral_runs() {
    let mut app = test_app();
    app.session.run_persistence = crate::app::RunPersistence::Ephemeral;
    let view = RendererView::build(&app, 80, 24);

    assert!(
        view.semantic
            .orientation
            .fields
            .iter()
            .any(|field| field.label == "session" && field.value == "ephemeral")
    );
}

#[test]
fn semantic_focused_surface_represents_tool_detail() {
    let mut app = test_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell".to_string(),
        arguments: "{}".to_string(),
        status: ToolStatus::Ok,
        output: vec!["\u{1b}[31mone\u{1b}[0m".to_string(), "two".to_string()],
    });
    app.overlay.show_detail(0);
    app.overlay.detail_mut().expect("detail overlay").scroll = 1;

    let view = RendererView::build(&app, 80, 24);

    match &view.semantic.focused_surface {
        FocusedSurfaceView::ToolDetail(detail) => {
            assert_eq!(detail.entry_index, 0);
            assert_eq!(detail.status, ToolStatus::Ok);
            assert_eq!(detail.scroll, 1);
            assert_eq!(detail.output, vec!["one".to_string(), "two".to_string()]);
        }
        other => panic!("expected tool detail surface, got {other:?}"),
    }
}

#[test]
fn semantic_setup_surface_projects_selection_and_masks_credentials() {
    let mut app = test_app();
    app.overlay
        .show_setup(FirstRunRecovery::setup(SetupProviderArg::ChatgptCodex));

    let view = RendererView::build(&app, 80, 24);
    match &view.semantic.focused_surface {
        FocusedSurfaceView::SetupForm(form) => {
            assert_eq!(form.title, "setup");
            assert!(!form.attention);
            assert_eq!(form.fields[0].label, "provider");
            assert_eq!(form.fields[0].value, "choose provider");
            assert!(!form.fields[0].secret);
            assert_eq!(form.submit_label, "continue");
        }
        surface => panic!("expected setup surface, got {surface:?}"),
    }

    app.overlay.show_setup(FirstRunRecovery {
        intent: crate::app::RecoveryIntent::Setup,
        provider: Some(SetupProviderArg::OpencodeGo),
        stage: RecoveryStage::EnterKey,
        pending_provider_prompt: false,
        selected: 0,
        secret_input: "sk-view-secret".to_string(),
        chatgpt_oauth: None,
    });
    let view = RendererView::build(&app, 80, 24);
    match &view.semantic.focused_surface {
        FocusedSurfaceView::SetupForm(form) => {
            assert!(form.fields.is_empty());
            assert!(form.details.iter().any(|detail| detail.contains("Input is hidden")));
            assert!(!format!("{form:?}").contains("sk-view-secret"));
        }
        surface => panic!("expected setup surface, got {surface:?}"),
    }
}

#[test]
fn setup_surface_clips_actions_around_the_focused_selection_in_the_live_region() {
    let mut app = test_app();
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    for width in [80, 40] {
        let view = RendererView::build(&app, width, renderer::live::LIVE_REGION_HEIGHT);
        let text = view
            .live
            .accessory_rows
            .iter()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            text.contains("start browser PKCE login"),
            "focused setup action should stay visible at width {width}:\n{text}"
        );
        assert!(
            text.contains("rows above") && text.contains("below"),
            "overflowing setup actions should be clipped with an indicator at width {width}:\n{text}"
        );
        assert!(
            !text.contains("continue without setup"),
            "pending setup offers an unavailable continuation at width {width}:\n{text}"
        );
    }
}

#[test]
fn semantic_reauthentication_surface_names_environment_override() {
    let mut app = test_app();
    app.overlay
        .show_setup(FirstRunRecovery::rejected_environment(SetupProviderArg::OpencodeZen));

    let view = RendererView::build(&app, 80, 24);
    match &view.semantic.focused_surface {
        FocusedSurfaceView::SetupForm(form) => {
            assert_eq!(form.title, "sign in again");
            assert!(form.attention);
            assert_eq!(form.fields[0].label, "credential source");
            assert_eq!(form.fields[0].value, "OPENCODE_ZEN_KEY");
            assert!(form.details.iter().any(|detail| detail.contains("restart thndrs")));
            assert_eq!(form.actions[0].label, "switch model/provider");
        }
        surface => panic!("expected setup surface, got {surface:?}"),
    }
}
