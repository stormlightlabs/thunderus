use std::path::Path;

use crate::app::{Entry, ToolStatus};
use crate::renderer::row;

use super::{TranscriptRowContext, banner_rows, entry_rows};

fn ctx(width: usize) -> TranscriptRowContext<'static> {
    TranscriptRowContext { user_label: "User", cwd: Path::new("."), width }
}

fn render_entry_styled(entry: &Entry, width: usize) -> String {
    let rows = entry_rows(entry, &ctx(width));
    let frame = row::Frame { rows, width, cursor: None, cursor_visible: true };
    frame.render_styled()
}

fn render_banner_styled(app: &crate::app::App, width: usize) -> String {
    let rows = banner_rows(app, width);
    let frame = row::Frame { rows, width, cursor: None, cursor_visible: true };
    frame.render_styled()
}

fn assert_snapshot(name: &str, contents: String) {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(name, contents);
    });
}

fn test_app() -> crate::app::App {
    use crate::cli::{Cli, Theme, WebSearchMode};
    use std::path::PathBuf;

    let mut app = crate::app::App::from_cli(&Cli {
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
fn snapshot_user_message_normal() {
    let entry = Entry::User { text: "Hello, can you help me with this?".to_string() };
    assert_snapshot("transcript_user_message_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_user_message_narrow() {
    let entry = Entry::User { text: "Hello, can you help me with this?".to_string() };
    assert_snapshot("transcript_user_message_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_assistant_text_normal() {
    let entry =
        Entry::Assistant { text: "Sure! I can help with that. Let me take a look.".to_string(), streaming: false };
    assert_snapshot("transcript_assistant_text_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_assistant_text_narrow() {
    let entry =
        Entry::Assistant { text: "Sure! I can help with that. Let me take a look.".to_string(), streaming: false };
    assert_snapshot("transcript_assistant_text_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_assistant_code_fence_normal() {
    let entry = Entry::Assistant {
        text: "````md\nHere is the code:\n\n```rs\nfn main() {\n    println!(\"hello\");\n}\n```\n````".to_string(),
        streaming: false,
    };
    assert_snapshot(
        "transcript_assistant_code_fence_normal",
        render_entry_styled(&entry, 80),
    );
}

#[test]
fn snapshot_assistant_code_fence_narrow() {
    let entry = Entry::Assistant {
        text: "````md\nHere is the code:\n\n```rs\nfn main() {\n    println!(\"hello\");\n}\n```\n````".to_string(),
        streaming: false,
    };
    assert_snapshot(
        "transcript_assistant_code_fence_narrow",
        render_entry_styled(&entry, 40),
    );
}

#[test]
fn snapshot_reasoning_normal() {
    let entry = Entry::Reasoning { text: "I need to check the file structure first.".to_string(), streaming: false };
    assert_snapshot("transcript_reasoning_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_reasoning_narrow() {
    let entry = Entry::Reasoning { text: "I need to check the file structure first.".to_string(), streaming: false };
    assert_snapshot("transcript_reasoning_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_tool_running_normal() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec![
            "running 3 tests".to_string(),
            "test tests::foo ... ok".to_string(),
            "test tests::bar ... ok".to_string(),
        ],
    };
    assert_snapshot("transcript_tool_running_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_running_narrow() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec![
            "running 3 tests".to_string(),
            "test tests::foo ... ok".to_string(),
            "test tests::bar ... ok".to_string(),
        ],
    };
    assert_snapshot("transcript_tool_running_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_tool_running_partial_output() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Running,
        output: vec![
            "running 5 tests".to_string(),
            "test tests::foo ... ok".to_string(),
            "test tests::bar ... ok".to_string(),
            "test tests::baz ... ok".to_string(),
            "test tests::qux ... ok".to_string(),
        ],
    };
    assert_snapshot(
        "transcript_tool_running_partial_output",
        render_entry_styled(&entry, 80),
    );
}

#[test]
fn snapshot_tool_ok_normal() {
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
    assert_snapshot("transcript_tool_ok_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_ok_narrow() {
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
    assert_snapshot("transcript_tool_ok_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_tool_ok_highlighted() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo build"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "   Compiling thndrs v0.1.0".to_string(),
            "    Finished `dev` profile [unoptimized + debuginfo] target(s)".to_string(),
        ],
    };
    assert_snapshot("transcript_tool_ok_highlighted", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_failed_normal() {
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
    assert_snapshot("transcript_tool_failed_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_failed_narrow() {
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
    assert_snapshot("transcript_tool_failed_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_tool_failed_compiler() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
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
    assert_snapshot("transcript_tool_failed_compiler", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_status_entry_normal() {
    let entry = Entry::Status { text: "context  AGENTS.md (scope: .)".to_string() };
    assert_snapshot("transcript_status_entry_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_status_entry_narrow() {
    let entry = Entry::Status { text: "context  AGENTS.md (scope: .)".to_string() };
    assert_snapshot("transcript_status_entry_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_error_message_normal() {
    let entry = Entry::Error { text: "Provider request failed: connection refused".to_string() };
    assert_snapshot("transcript_error_message_normal", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_error_message_narrow() {
    let entry = Entry::Error { text: "Provider request failed: connection refused".to_string() };
    assert_snapshot("transcript_error_message_narrow", render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_startup_banner_normal() {
    let app = test_app();
    assert_snapshot("transcript_startup_banner_normal", render_banner_styled(&app, 80));
}

#[test]
fn snapshot_startup_banner_narrow() {
    let app = test_app();
    assert_snapshot("transcript_startup_banner_narrow", render_banner_styled(&app, 40));
}

#[test]
fn snapshot_tool_truncated_output() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: (0..20).map(|i| format!("file_{i}.rs")).collect(),
    };
    assert_snapshot("transcript_tool_truncated_output", render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_path_shortened() {
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
    let ctx = TranscriptRowContext { user_label: "User", cwd, width: 120 };
    let rows = entry_rows(&entry, &ctx);
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
