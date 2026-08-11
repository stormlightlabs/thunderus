use std::path::{Path, PathBuf};

use crate::app::{App, Entry, ToolStatus};
use crate::context::ContextSource;
use crate::renderer::{self, row};
use crate::skills::{self, SkillDiagnostic};

use super::{GUTTER, TranscriptRowContext};

fn ctx(width: usize) -> TranscriptRowContext<'static> {
    TranscriptRowContext::for_test("User", Path::new("."), width)
}

fn render_entry_styled(entry: &Entry, width: usize) -> String {
    let rows = ctx(width).rows_for_entry(entry);
    let frame = row::Frame { rows, width, cursor: None, cursor_visible: true };
    frame.render_styled()
}

fn render_banner_styled(app: &App, width: usize) -> String {
    let rows = app.render_banner_rows(width);
    let frame = row::Frame { rows, width, cursor: None, cursor_visible: true };
    frame
        .render_styled()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ordinary_transcript_rows_use_terminal_default_background() {
    let entries = [
        Entry::User { text: "inspect the renderer".to_string() },
        Entry::Agent { text: "I found the projection.".to_string(), streaming: false },
        Entry::Reasoning { text: "checking layout".to_string(), streaming: false },
    ];

    for entry in entries {
        let rows = ctx(80).rows_for_entry(&entry);
        assert!(
            rows.iter()
                .flat_map(|row| &row.spans)
                .all(|span| span.style.bg == renderer::style::Color::Reset),
            "ordinary transcript entries should inherit the terminal background: {entry:?}"
        );
    }
}

fn assert_snapshot(name: &str, contents: &str) {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(name, contents);
    });
}

fn test_app() -> App {
    use crate::cli::{Cli, Theme, WebSearchMode};

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
        config_diagnostics: Vec::new(),
        config_layers: Vec::new(),
        config_origins: std::collections::BTreeMap::new(),
        acp_agents: std::collections::BTreeMap::new(),
        context: thndrs_agent::context::ContextConfig::default(),
        status_line: Default::default(),
        authority: Default::default(),
        command: None,
    });
    app.overlay.close();
    app.session.id = "test-session".to_string();
    app.runtime.git_status =
        Some(renderer::git::GitStatusSummary { branch: Some("main".to_string()), added: 0, modified: 0, deleted: 0 });
    app.transcript.entries.clear();
    app.transcript.context_sources.clear();
    app.transcript.skills.clear();
    app.transcript.skill_diagnostics.clear();
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
    let rows = ctx(80).rows_for_entry(&entry);
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
fn user_and_activity_labels_share_the_same_rail_column() {
    let user = Entry::User { text: "hello".to_string() };
    let tool = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"hello"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["src/lib.rs:1:hello".to_string()],
    };

    let user_rows = ctx(80).rows_for_entry(&user);
    let tool_rows = ctx(80).rows_for_entry(&tool);
    let user_label = user_rows
        .iter()
        .find(|row| row.text().contains("User"))
        .expect("user label row");
    let activity_label = tool_rows
        .iter()
        .find(|row| row.text().contains("Activity"))
        .expect("activity label row");

    assert_eq!(user_label.text().find("User"), activity_label.text().find("Activity"));
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
fn ordinary_markdown_code_fence_is_highlighted_and_wrapped() {
    let entry = Entry::Agent {
        text: "Here is the code:\n```rs\nfn a_very_long_function_name() { println!(\"long line\"); }\n```".to_string(),
        streaming: false,
    };
    let rendered = render_entry_styled(&entry, 32);

    assert!(
        rendered.contains("fn a_very_long"),
        "code should be wrapped: {rendered}"
    );
    assert!(
        rendered.contains("[fg=#b48ead]=fn"),
        "ordinary Markdown code fences should highlight Rust keywords: {rendered}"
    );
}

#[test]
fn markdown_provider_wrapper_does_not_render_as_a_nested_code_block() {
    let entry =
        Entry::Agent { text: "```markdown\r\n1. This is an ordinary response.\r\n```".to_string(), streaming: false };
    let rows = ctx(80).rows_for_entry(&entry);

    assert!(
        rows.iter().all(|row| !row.text().contains(GUTTER)),
        "an outer Markdown wrapper must not add a code gutter"
    );
    assert!(
        rows.iter().any(|row| row.text().contains("ordinary response")),
        "wrapped Markdown should remain visible as response text"
    );
}

#[test]
fn assistant_markdown_table_renders_as_structured_rows() {
    let entry = Entry::Agent {
        text: "````md\n| File | Added | Removed |\n| :--- | ---: | ---: |\n| src/lib.rs | 10 | 2 |\n| README.md | 1 | 0 |\n````"
            .to_string(),
        streaming: false,
    };
    let rendered = render_entry_styled(&entry, 80);

    assert!(rendered.contains("File"), "table header should render:\n{rendered}");
    assert!(
        rendered.contains("src/lib.rs") && rendered.contains("README.md"),
        "table rows should render:\n{rendered}"
    );
    assert!(
        rendered.contains("u]="),
        "header cells should use underline styling:\n{rendered}"
    );
}

#[test]
fn assistant_markdown_table_has_narrow_fallback() {
    let entry = Entry::Agent {
        text: "````md\n| File | Added | Removed |\n| :--- | ---: | ---: |\n| src/lib.rs | 10 | 2 |\n````".to_string(),
        streaming: false,
    };
    let rendered = render_entry_styled(&entry, 28);

    assert!(
        rendered.contains("File: src/lib.rs"),
        "narrow table fallback should render label/value rows:\n{rendered}"
    );
    assert!(
        rendered.contains("Added:") && rendered.contains("10; Removed: 2"),
        "narrow table fallback should preserve numeric values:\n{rendered}"
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
fn completed_shell_tool_shows_command_without_opaque_call_id() {
    let entry = Entry::Tool {
        name: "run_shell#call_szT01Chdh05ZL1XMRRkV29xa".to_string(),
        arguments: r#"{"argv":["mv","docs/design/thndrs-ui-concepts.html",".sandbox/"] ,"cwd":"."}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["Process exited with code 0".to_string()],
    };

    let rendered = render_entry_styled(&entry, 120);

    assert!(
        rendered.contains("run_shell"),
        "tool name should remain visible:\n{rendered}"
    );
    assert!(
        rendered.contains("$ mv docs/design/thndrs-ui-concepts.html .sandbox/"),
        "shell activity should summarize its argv:\n{rendered}"
    );
    assert!(
        !rendered.contains("call_szT01Chdh05ZL1XMRRkV29xa") && !rendered.contains("cwd: ."),
        "opaque call ids and low-value cwd-only summaries should be hidden:\n{rendered}"
    );
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
fn settled_tool_collapses_output_behind_detail_affordance() {
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern":"fn main"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["src/main.rs:1:fn main()".to_string()],
    };
    let rendered = render_entry_styled(&entry, 80);

    assert!(
        rendered.contains("Ctrl+O details"),
        "settled tool should advertise detail: {rendered}"
    );
    assert!(
        !rendered.contains("src/main.rs:1:fn main()"),
        "settled output should stay in the detail surface: {rendered}"
    );
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
    let _guard = crate::test_env::lock();
    let app = test_app();
    assert_snapshot("transcript_startup_banner_normal", &render_banner_styled(&app, 80));
}

#[test]
fn snapshot_startup_banner_narrow() {
    let _guard = crate::test_env::lock();
    let app = test_app();
    assert_snapshot("transcript_startup_banner_narrow", &render_banner_styled(&app, 40));
}

#[test]
fn snapshot_startup_banner_with_context_and_diagnostics() {
    let _guard = crate::test_env::lock();
    let mut app = test_app();
    app.transcript.context_sources = vec![ContextSource {
        path: app.runtime.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: false,
        byte_count: 9,
    }];
    app.transcript.skill_diagnostics = vec![SkillDiagnostic {
        path: std::path::PathBuf::from("/Users/test/.thndrs/skills/bad/SKILL.md"),
        message: "invalid YAML frontmatter".to_string(),
    }];
    assert_snapshot(
        "transcript_startup_banner_with_context_and_diagnostics",
        &render_banner_styled(&app, 80),
    );
}

#[test]
fn banner_keeps_skill_diagnostics_out_of_the_attention_block() {
    let _guard = crate::test_env::lock();
    let mut app = test_app();
    app.transcript.skill_diagnostics = vec![SkillDiagnostic {
        path: std::path::PathBuf::from("/Users/test/.thndrs/skills/bad/SKILL.md"),
        message: "invalid YAML frontmatter".to_string(),
    }];

    let rendered = render_banner_styled(&app, 80);

    assert!(rendered.contains("skills available · 1 skipped"));
    assert!(!rendered.contains("ATTENTION"));
    assert!(!rendered.contains("invalid YAML frontmatter"));
}

#[test]
fn banner_context_section_shows_agents_md_not_full_path() {
    let _guard = crate::test_env::lock();
    let mut app = test_app();
    app.transcript.context_sources = vec![ContextSource {
        path: app.runtime.cwd.join("AGENTS.md"),
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
fn banner_context_section_keeps_truncated_source_compact() {
    let _guard = crate::test_env::lock();
    let mut app = test_app();
    app.transcript.context_sources = vec![ContextSource {
        path: app.runtime.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: true,
        byte_count: 40_000,
    }];

    let rendered = render_banner_styled(&app, 80);

    assert!(
        rendered.contains("AGENTS.md loaded"),
        "context source should remain readable:\n{rendered}"
    );
    assert!(
        rendered.contains("./AGENTS.md"),
        "context path should remain visible:\n{rendered}"
    );
    assert!(
        !rendered.contains("40000 bytes"),
        "readiness row should remain compact:\n{rendered}"
    );
}

#[test]
fn banner_keeps_skipped_skill_diagnostics_compact() {
    let _guard = crate::test_env::lock();
    let mut app = test_app();
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .expect("HOME should be set for startup banner path shortening test");
    let home_path = home.join(".thndrs/skills/bad/SKILL.md");
    app.transcript.skill_diagnostics = vec![skills::SkillDiagnostic {
        path: home_path.clone(),
        message: "invalid YAML frontmatter: unknown field".to_string(),
    }];

    let rendered = render_banner_styled(&app, 80);

    assert!(
        rendered.contains("1 skipped"),
        "banner should summarize skipped skills:\n{rendered}"
    );
    assert!(
        !rendered.contains(&home_path.display().to_string()),
        "banner should leave diagnostic paths to /skills:\n{rendered}"
    );
}

#[test]
fn banner_no_duplicate_context_loaded_status_entry() {
    let mut app = test_app();
    app.transcript.context_sources = vec![ContextSource {
        path: app.runtime.cwd.join("AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project".to_string(),
        content_hash: 42,
        truncated: false,
        byte_count: 9,
    }];

    assert!(
        app.transcript.entries.is_empty(),
        "transcript should not contain a context-loaded status entry"
    );

    let rendered = render_banner_styled(&app, 80);
    assert!(
        rendered.contains("AGENTS.md loaded"),
        "banner should show context readiness:\n{rendered}"
    );
    assert!(
        rendered.contains("AGENTS.md"),
        "Context section should list the AGENTS.md source:\n{rendered}"
    );
    assert!(
        !rendered.contains("loaded AGENTS.md"),
        "banner should not duplicate the old context status wording:\n{rendered}"
    );
}

#[test]
fn banner_summarizes_loaded_skills_as_readiness() {
    let mut app = test_app();
    app.transcript.skills = [
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

    let rendered = render_banner_styled(&app, 80);

    assert!(
        rendered.contains("8 skills available"),
        "banner should show the skill count:\n{rendered}"
    );
    assert!(
        rendered.contains("skills"),
        "banner should label skill readiness:\n{rendered}"
    );
    assert!(
        !rendered.contains("make-interfaces"),
        "banner should remain compact:\n{rendered}"
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
        status: ToolStatus::Running,
        output: vec![
            "/Users/owais/Projects/StormlightLabs/OpenSource/thndrs/src/session/tests.rs:1420: Entry::Status"
                .to_string(),
        ],
    };
    let ctx = TranscriptRowContext::for_test("User", cwd, 120);
    let rows = ctx.rows_for_entry(&entry);
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
fn edit_tool_rows_include_visible_edit_summary() {
    let entry = Entry::Tool {
        name: "replace_range#abc".to_string(),
        arguments: r#"{"path":"src/lib.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec!["wrote update: src/lib.rs".to_string()],
    };
    let rendered = render_entry_styled(&entry, 80);

    assert!(
        rendered.contains("edit") && rendered.contains("replace_range src/lib.rs [ok]"),
        "edit tools should include a compact edit summary:\n{rendered}"
    );
}

#[test]
fn diff_tool_rows_include_visible_diff_summary() {
    let entry = Entry::Tool {
        name: "write_patch".to_string(),
        arguments: r#"{"path":"src/lib.rs"}"#.to_string(),
        status: ToolStatus::Ok,
        output: vec![
            "--- a/src/lib.rs".to_string(),
            "+++ b/src/lib.rs".to_string(),
            "-old".to_string(),
            "+new".to_string(),
            "+newer".to_string(),
        ],
    };
    let rendered = render_entry_styled(&entry, 80);

    assert!(
        rendered.contains("diff") && rendered.contains("src/lib.rs +2 -1"),
        "diff output should include a changed-file summary:\n{rendered}"
    );
}

#[test]
fn plain_tool_output_preserves_code_indentation_after_search_prefix() {
    let entry = Entry::Tool {
        name: "search_text".to_string(),
        arguments: r#"{"pattern": "println", "path": "src/main.rs"}"#.to_string(),
        status: ToolStatus::Running,
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

    let rows = ctx.rows_for_entry(&entry);
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
    let rows = ctx.rows_for_entry(&entry);
    assert!(
        rows.iter().all(|row| row.group_id.is_none()),
        "rows should not carry a group_id when entry_index is None"
    );
}

#[test]
fn banner_normal_viewport_shows_all_sections() {
    let app = test_app();
    let rendered = render_banner_styled(&app, 80);
    for section in [
        "thndrs / ready",
        "Ask for change, run a command, or inspect the repo.",
        "No project instructions",
        "skills",
        "Web Search",
        "duckduckgo",
    ] {
        assert!(
            rendered.contains(section),
            "normal viewport should show {section}:\n{rendered}"
        );
    }

    assert!(rendered.contains("help"), "banner should show help row");
    assert!(rendered.contains("/model"), "banner should show model switcher row");
}

#[test]
fn banner_ends_with_a_transparent_spacer_below_shortcuts() {
    let app = test_app();
    let rows = app.render_banner_rows(80);
    let spacer = rows.last().expect("banner spacer row");

    assert!(spacer.text().trim().is_empty());
    assert!(
        spacer
            .spans
            .iter()
            .all(|span| span.style.bg == renderer::style::Color::Reset),
        "banner spacer should inherit the terminal background"
    );
}

#[test]
fn banner_search_metadata_uses_quiet_color() {
    let app = test_app();
    let rows = app.render_banner_rows(80);
    let search = rows
        .iter()
        .find(|row| row.text().contains("Web Search"))
        .expect("search readiness row");

    let label = search
        .spans
        .iter()
        .find(|span| span.text == "duckduckgo")
        .expect("search metadata span");
    assert_eq!(label.style.fg, renderer::style::palette().overlay1);
}

#[test]
fn banner_narrow_viewport_preserves_sections() {
    let app = test_app();
    let rendered = render_banner_styled(&app, 40);

    assert!(rendered.contains("thndrs"), "narrow viewport should show identity");
    assert!(
        rendered.contains("No project instructions"),
        "narrow viewport should show context readiness"
    );
}
