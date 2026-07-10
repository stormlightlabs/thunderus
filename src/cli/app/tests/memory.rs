//! `/memory recall` slash command integration tests.

use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyModifiers};
use std::io::Write;

/// Write a memory note into the project memory root.
fn write_project_note(workspace: &Path, name: &str, id: &str, title: &str, body: &str) {
    let path = workspace.join(".thndrs").join("memory").join("notes").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).expect("create notes dir");
    let content = format!(
        "---\nid: {id}\ntitle: {title}\nkind: procedure\nscope: project\ncreated: 2026-07-03T00:00:00Z\nupdated: 2026-07-03T00:00:00Z\nsource: explicit-user\n---\n\n{body}\n"
    );
    let mut f = std::fs::File::create(&path).expect("create note");
    f.write_all(content.as_bytes()).expect("write note");
}

#[test]
fn slash_memory_recall_renders_matches() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&workspace).expect("create workspace");
    write_project_note(
        &workspace,
        "build.md",
        "mem_build",
        "Build",
        "run cargo test for unit tests",
    );

    let cli = Cli { cwd: workspace, ..Cli::default() };
    let mut app = with_home(&home, || App::from_cli(&cli));
    app.session_writer = None;
    app.input = PromptInput::from("/memory recall cargo test");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("mem_build"))),
        "recall should surface the matching memory id"
    );
    assert!(app.input.is_empty(), "input should clear after /memory recall");
}

#[test]
fn slash_memory_recall_no_match_shows_diagnostic() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&workspace).expect("create workspace");
    write_project_note(&workspace, "a.md", "mem_a", "A", "nothing relevant here");

    let cli = Cli { cwd: workspace, ..Cli::default() };
    let mut app = with_home(&home, || App::from_cli(&cli));
    app.session_writer = None;
    app.input = PromptInput::from("/memory recall zzznomatch");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("no memory matched"))),
        "no-match recall should surface a useful diagnostic"
    );
    assert!(app.input.is_empty());
}

#[test]
fn slash_memory_recall_without_query_shows_usage() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/memory recall");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Error { text } if text.contains("usage"))),
        "/memory recall without a query should show usage"
    );
    assert!(app.input.is_empty());
}

#[test]
fn slash_memory_recall_is_allowed_while_working() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&workspace).expect("create workspace");
    write_project_note(&workspace, "build.md", "mem_build", "Build", "run cargo test");

    let cli = Cli { cwd: workspace, ..Cli::default() };
    let mut app = with_home(&home, || App::from_cli(&cli));
    app.session_writer = None;
    app.run_state = RunState::Working;
    app.input = PromptInput::from("/memory recall cargo");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("mem_build"))),
        "read-only /memory recall should run while the agent is working"
    );
    assert!(
        !app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/memory recall must not be rejected while working"
    );
    assert!(app.input.is_empty());
}

#[test]
fn slash_memory_recall_suggests_in_command_list() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("memory");

    let suggestions = command_suggestions_for_app(&app);
    assert!(
        suggestions.iter().any(|(cmd, _)| *cmd == "memory recall"),
        "memory recall should appear in command suggestions"
    );
}
