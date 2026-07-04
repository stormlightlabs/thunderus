use std::path::{Path, PathBuf};

use crate::app::{Entry, ToolStatus};
use crate::renderer::{self, row};
use crate::skills;

use super::{TranscriptRowContext, banner_rows, entry_rows, startup_loaded_skill_lines};

fn ctx(width: usize) -> TranscriptRowContext<'static> {
    TranscriptRowContext::for_test("User", Path::new("."), width)
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

fn assert_snapshot(name: &str, contents: &str) {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(name, contents);
    });
}

fn test_app() -> crate::app::App {
    use crate::cli::{Cli, Theme, WebSearchMode};

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
    app.git_status =
        Some(renderer::git::GitStatusSummary { branch: Some("main".to_string()), added: 0, modified: 0, deleted: 0 });
    app.transcript.clear();
    app.context_sources.clear();
    app.skills.clear();
    app.skill_diagnostics.clear();
    app
}

fn test_skill(name: &str) -> skills::SkillMetadata {
    skills::SkillMetadata {
        name: name.to_string(),
        description: "test skill".to_string(),
        path: PathBuf::from(format!("/tmp/{name}/SKILL.md")),
        root: PathBuf::from(format!("/tmp/{name}")),
        content_hash: 0,
        byte_count: 0,
        source: skills::SkillSource::Project,
        allowed_tools: Vec::new(),
        license: None,
        compatibility: None,
        metadata: None,
        references: Vec::new(),
    }
}

#[test]
fn snapshot_user_message_normal() {
    let entry = Entry::User { text: "Hello, can you help me with this?".to_string() };
    assert_snapshot("transcript_user_message_normal", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_user_message_narrow() {
    let entry = Entry::User { text: "Hello, can you help me with this?".to_string() };
    assert_snapshot("transcript_user_message_narrow", &render_entry_styled(&entry, 40));
}

#[test]
fn user_message_has_balanced_vertical_padding() {
    let entry = Entry::User { text: "hello".to_string() };
    let rows = entry_rows(&entry, &ctx(80));

    assert!(
        rows.first().is_some_and(|row| row.text().trim().is_empty()),
        "user block should start with vertical padding"
    );
    assert!(
        rows.last().is_some_and(|row| row.text().trim().is_empty()),
        "user block should end with vertical padding"
    );
}

#[test]
fn snapshot_assistant_text_normal() {
    let entry = Entry::Agent { text: "Sure! I can help with that. Let me take a look.".to_string(), streaming: false };
    assert_snapshot("transcript_assistant_text_normal", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_assistant_text_narrow() {
    let entry = Entry::Agent { text: "Sure! I can help with that. Let me take a look.".to_string(), streaming: false };
    assert_snapshot("transcript_assistant_text_narrow", &render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_assistant_code_fence_normal() {
    let entry = Entry::Agent {
        text: "````md\nHere is the code:\n\n```rs\nfn main() {\n    println!(\"hello\");\n}\n```\n````".to_string(),
        streaming: false,
    };
    assert_snapshot(
        "transcript_assistant_code_fence_normal",
        &render_entry_styled(&entry, 80),
    );
}

#[test]
fn snapshot_assistant_code_fence_narrow() {
    let entry = Entry::Agent {
        text: "````md\nHere is the code:\n\n```rs\nfn main() {\n    println!(\"hello\");\n}\n```\n````".to_string(),
        streaming: false,
    };
    assert_snapshot(
        "transcript_assistant_code_fence_narrow",
        &render_entry_styled(&entry, 40),
    );
}

#[test]
fn snapshot_reasoning_normal() {
    let entry = Entry::Reasoning { text: "I need to check the file structure first.".to_string(), streaming: false };
    assert_snapshot("transcript_reasoning_normal", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_reasoning_narrow() {
    let entry = Entry::Reasoning { text: "I need to check the file structure first.".to_string(), streaming: false };
    assert_snapshot("transcript_reasoning_narrow", &render_entry_styled(&entry, 40));
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
    assert_snapshot("transcript_tool_running_normal", &render_entry_styled(&entry, 80));
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
    assert_snapshot("transcript_tool_running_narrow", &render_entry_styled(&entry, 40));
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
        &render_entry_styled(&entry, 80),
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
    assert_snapshot("transcript_tool_ok_normal", &render_entry_styled(&entry, 80));
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
    assert_snapshot("transcript_tool_ok_narrow", &render_entry_styled(&entry, 40));
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
    assert_snapshot("transcript_tool_ok_highlighted", &render_entry_styled(&entry, 80));
}

#[test]
fn highlighted_tool_output_marks_horizontal_truncation() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Failed,
        output: vec![format!(
            "error: {}",
            "this compiler diagnostic is far wider than the terminal body width ".repeat(3)
        )],
    };
    let rendered = render_entry_styled(&entry, 48);

    assert!(
        rendered.contains('…'),
        "wide highlighted tool output should include visible truncation marker:\n{rendered}"
    );
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
    assert_snapshot("transcript_tool_failed_normal", &render_entry_styled(&entry, 80));
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
    assert_snapshot("transcript_tool_failed_narrow", &render_entry_styled(&entry, 40));
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
    assert_snapshot("transcript_tool_failed_compiler", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_status_entry_normal() {
    let entry = Entry::Status { text: "context  AGENTS.md (scope: .)".to_string() };
    assert_snapshot("transcript_status_entry_normal", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_status_entry_narrow() {
    let entry = Entry::Status { text: "context  AGENTS.md (scope: .)".to_string() };
    assert_snapshot("transcript_status_entry_narrow", &render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_error_message_normal() {
    let entry = Entry::Error { text: "Provider request failed: connection refused".to_string() };
    assert_snapshot("transcript_error_message_normal", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_error_message_narrow() {
    let entry = Entry::Error { text: "Provider request failed: connection refused".to_string() };
    assert_snapshot("transcript_error_message_narrow", &render_entry_styled(&entry, 40));
}

#[test]
fn snapshot_startup_banner_normal() {
    let app = test_app();
    assert_snapshot("transcript_startup_banner_normal", &render_banner_styled(&app, 80));
}

#[test]
fn snapshot_startup_banner_narrow() {
    let app = test_app();
    assert_snapshot("transcript_startup_banner_narrow", &render_banner_styled(&app, 40));
}

#[test]
fn snapshot_startup_banner_with_context_and_diagnostics() {
    let mut app = test_app();
    app.context_sources = vec![crate::context::ContextSource {
        path: app.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: false,
        byte_count: 9,
    }];
    app.skill_diagnostics = vec![crate::skills::SkillDiagnostic {
        path: std::path::PathBuf::from("/Users/test/.thndrs/skills/bad/SKILL.md"),
        message: "invalid YAML frontmatter".to_string(),
    }];
    assert_snapshot(
        "transcript_startup_banner_with_context_and_diagnostics",
        &render_banner_styled(&app, 80),
    );
}

#[test]
fn banner_context_section_shows_agents_md_not_full_path() {
    let mut app = test_app();
    app.context_sources = vec![crate::context::ContextSource {
        path: app.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: false,
        byte_count: 9,
    }];

    let rendered = render_banner_styled(&app, 80);

    assert!(
        rendered.contains("AGENTS.md"),
        "Context section should show AGENTS.md:\n{rendered}"
    );
}

#[test]
fn banner_context_section_shows_truncation() {
    let mut app = test_app();
    app.context_sources = vec![crate::context::ContextSource {
        path: app.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: true,
        byte_count: 40_000,
    }];

    let rendered = render_banner_styled(&app, 80);

    assert!(
        rendered.contains("AGENTS.md (truncated, 40000") && rendered.contains("bytes)"),
        "Context section should preserve AGENTS.md truncation metadata:\n{rendered}"
    );
}

#[test]
fn banner_cwd_uses_statusline_truncation_without_wrapping() {
    let app = test_app();
    let rendered = render_banner_styled(&app, 40);

    assert!(
        rendered.contains("cwd ~/Pr/St/O/thndrs"),
        "cwd row should use statusline-style path truncation:\n{rendered}"
    );
    assert!(
        !rendered.contains("ource/thndrs"),
        "cwd row should not wrap onto a second row:\n{rendered}"
    );
}

#[test]
fn banner_diagnostics_section_shortens_home_paths() {
    let mut app = test_app();
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .expect("HOME should be set for startup banner path shortening test");
    let home_path = home.join(".thndrs/skills/bad/SKILL.md");
    app.skill_diagnostics = vec![skills::SkillDiagnostic {
        path: home_path.clone(),
        message: "invalid YAML frontmatter: unknown field".to_string(),
    }];

    let rendered = render_banner_styled(&app, 80);

    assert!(
        rendered.contains("~/.thndrs/skills/bad/SKILL.md"),
        "Diagnostics section should shorten HOME paths:\n{rendered}"
    );
    assert!(
        !rendered.contains(&home_path.display().to_string()),
        "Diagnostics section should not show the full HOME path:\n{rendered}"
    );
}

#[test]
fn banner_no_duplicate_context_loaded_status_entry() {
    let mut app = test_app();
    app.context_sources = vec![crate::context::ContextSource {
        path: app.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: false,
        byte_count: 9,
    }];

    assert!(
        app.transcript.is_empty(),
        "transcript should not contain a context-loaded status entry"
    );

    let rendered = render_banner_styled(&app, 80);
    assert!(
        rendered.contains("project"),
        "banner should have a project workbench section:\n{rendered}"
    );
    assert!(
        rendered.contains("AGENTS.md"),
        "Context section should list the AGENTS.md source:\n{rendered}"
    );
    assert!(
        !rendered.contains("loaded AGENTS.md"),
        "banner should not show context as a duplicate loaded status message:\n{rendered}"
    );
}

#[test]
fn startup_loaded_skills_wrap_at_hyphens_and_hide_extra_rows() {
    let mut app = test_app();
    app.skills = [
        "make-interfaces-feel-better",
        "code-change-status",
        "copywriting",
        "fallow",
        "frontend-design",
        "grill-me",
        "notetaking",
        "opentui",
    ]
    .into_iter()
    .map(test_skill)
    .collect();

    let snapshot = app.self_knowledge_snapshot();
    let lines = startup_loaded_skill_lines(&snapshot, 31);
    let rendered = lines.join("\n");

    assert!(
        lines.len() <= 4,
        "loaded skills should be capped at four rows:\n{rendered}"
    );
    assert!(
        rendered.contains("make-interfaces-feel-\n     better,"),
        "long hyphenated skill names should wrap at a hyphen:\n{rendered}"
    );
    assert!(
        rendered.contains("...6 skills hidden"),
        "loaded skills should report hidden skill count:\n{rendered}"
    );
    assert!(
        !rendered.contains("feel-bette"),
        "loaded skills should not split a hyphenated word mid-segment:\n{rendered}"
    );
}

#[test]
fn snapshot_tool_truncated_output() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "ls"}"#.to_string(),
        status: ToolStatus::Ok,
        output: (0..20).map(|i| format!("file_{i}.rs")).collect(),
    };
    assert_snapshot("transcript_tool_truncated_output", &render_entry_styled(&entry, 80));
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
    let ctx = TranscriptRowContext::for_test("User", cwd, 120);
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

#[test]
fn snapshot_tool_search_results() {
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern": "fn main", "path": "src"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "src/main.rs:1:fn main() {".to_string(),
            "src/main.rs:2:    println!(\"hello\");".to_string(),
            "src/main.rs:3:}".to_string(),
            "src/lib.rs:10:fn helper() {}".to_string(),
            "src/lib.rs:25:    helper();".to_string(),
        ],
    };
    assert_snapshot("transcript_tool_search_results", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_search_results_narrow() {
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern": "fn main", "path": "src"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "src/main.rs:1:fn main() {".to_string(),
            "src/main.rs:2:    println!(\"hello\");".to_string(),
        ],
    };
    assert_snapshot(
        "transcript_tool_search_results_narrow",
        &render_entry_styled(&entry, 40),
    );
}

#[test]
fn plain_tool_output_preserves_code_indentation_after_search_prefix() {
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern": "println", "path": "src/main.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["src/main.rs:2:    println!(\"hello\");".to_string()],
    };
    let rendered = render_entry_styled(&entry, 80);

    assert!(
        rendered.contains("src/main.rs:2:    println!"),
        "plain tool output should preserve repeated spaces after path prefix:\n{rendered}"
    );
}

#[test]
fn snapshot_tool_cancelled() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Cancelled,
        output: vec!["running 3 tests".to_string(), "test tests::foo ... ok".to_string()],
    };
    assert_snapshot("transcript_tool_cancelled", &render_entry_styled(&entry, 80));
}

#[test]
fn snapshot_tool_cancelled_narrow() {
    let entry = Entry::Tool {
        name: "run_shell".to_string(),
        arguments: r#"{"program": "cargo test"}"#.to_string(),
        status: ToolStatus::Cancelled,
        output: vec!["running 3 tests".to_string(), "test tests::foo ... ok".to_string()],
    };
    assert_snapshot("transcript_tool_cancelled_narrow", &render_entry_styled(&entry, 40));
}

#[test]
fn entry_rows_tag_group_id_when_entry_index_set() {
    let entry = Entry::User { text: "hello world".to_string() };
    let mut ctx = TranscriptRowContext::for_test("User", Path::new("."), 80);
    ctx.entry_index = Some(42);

    let rows = entry_rows(&entry, &ctx);

    assert!(
        rows.iter().all(|row| row.group_id.is_some()),
        "all rows should carry a group_id when entry_index is set"
    );
    let group_ids: Vec<_> = rows.iter().filter_map(|r| r.group_id).collect();
    assert!(
        group_ids.iter().all(|g| g.entry_index == 42),
        "all group_ids should reference entry_index 42"
    );
}

#[test]
fn entry_rows_omit_group_id_when_entry_index_none() {
    let entry = Entry::User { text: "hello world".to_string() };
    let ctx = TranscriptRowContext::for_test("User", Path::new("."), 80);

    let rows = entry_rows(&entry, &ctx);

    assert!(
        rows.iter().all(|row| row.group_id.is_none()),
        "rows should not carry a group_id when entry_index is None"
    );
}

#[test]
fn banner_normal_viewport_shows_all_sections() {
    let app = test_app();
    let rendered = render_banner_styled(&app, 80);

    for section in ["THNDRS", "workbench", "system", "project", "search", "attention"] {
        assert!(
            rendered.contains(section),
            "normal viewport should show {section}:\n{rendered}"
        );
    }

    assert!(rendered.contains("help"), "banner should show help row");
    assert!(rendered.contains("/model"), "banner should show model switcher row");
}

#[test]
fn banner_narrow_viewport_preserves_sections() {
    let app = test_app();
    let rendered = render_banner_styled(&app, 40);

    assert!(
        rendered.contains("system"),
        "narrow viewport should show system section"
    );
    assert!(
        rendered.contains("project"),
        "narrow viewport should show project section"
    );
}
