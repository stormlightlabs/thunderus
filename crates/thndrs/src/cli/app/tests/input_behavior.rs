//! Application behavior tests for input behavior seams.

use super::*;
use helpers::*;

#[test]
fn file_picker_selection_inserts_selected_path() {
    let mut app = fresh_app();
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        picker_from_paths(vec!["src/main.rs".to_string(), "src/app.rs".to_string()]),
    );

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "src/app.rs");
    assert!(app.overlay.picker().is_none());
}

#[test]
fn file_picker_arrows_and_pages_are_scrollable() {
    let mut app = fresh_app();
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        picker_from_paths((0..20).map(|i| format!("src/file_{i:02}.rs")).collect()),
    );

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
    );
    let picker = app.overlay.picker().expect("picker");
    assert_eq!(picker.selected, VISIBLE_ROWS);
    assert!(picker.scroll > 0);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)));
    let picker = app.overlay.picker().expect("picker");
    assert_eq!(picker.selected, 0);
    assert_eq!(picker.scroll, 0);
}

#[test]
fn backspace_removes_last_char() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("abc");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.input.as_str(), "ab");
}

#[test]
fn enter_trims_whitespace_before_submit() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("  hello  ");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input.as_str(), "");
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(app.transcript.entries[0], Entry::User { text: String::from("hello") });
}

#[test]
fn typing_after_recalled_history_edits_copy() {
    let mut app = fresh_app();
    submit_user_turn(&mut app, String::from("previous"));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE)),
    );

    assert_eq!(app.composer.input.as_str(), "previous!");
    assert_eq!(app.composer.input_history, vec![String::from("previous")]);
    assert_eq!(app.composer.history_cursor, None);
}

#[test]
fn remembering_input_keeps_bounded_in_memory_history() {
    let mut app = fresh_app();
    app.composer.input_history = (0..INPUT_HISTORY_LIMIT)
        .map(|index| format!("prompt {index}"))
        .collect();

    remember_input(&mut app, "newest prompt");

    assert_eq!(app.composer.input_history.len(), INPUT_HISTORY_LIMIT);
    assert_eq!(app.composer.input_history.first().map(String::as_str), Some("prompt 1"));
    assert_eq!(
        app.composer.input_history.last().map(String::as_str),
        Some("newest prompt")
    );
}

#[test]
fn question_key_enters_help_mode() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::Help);
}

#[test]
fn esc_exits_help_mode() {
    let mut app = fresh_app();
    app.overlay.show_help();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
}

#[test]
fn question_key_keeps_inline_help_open() {
    let mut app = fresh_app();
    app.overlay.show_help();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::Help);
}

#[test]
fn question_key_does_not_enter_help_when_input_nonempty() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.composer.input.as_str(), "hello?");
}

#[test]
fn ctrl_k_kills_to_end_of_line() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
    app.composer.input.cursor_to_start();
    app.composer.input.cursor_word_right();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "hello ");
    assert_eq!(app.composer.kill_ring, vec!["world"]);
}

#[test]
fn ctrl_u_kills_to_start_of_line() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
    app.composer.input.cursor_to_start();
    app.composer.input.cursor_word_right();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "world");
    assert_eq!(app.composer.kill_ring, vec!["hello "]);
}

#[test]
fn ctrl_w_kills_previous_word() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "foo bar ");
    assert_eq!(app.composer.kill_ring, vec!["baz"]);
}

#[test]
fn ctrl_y_yanks_last_kill() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello ");
    app.composer.kill_ring.push("world".to_string());
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "hello world");
}

#[test]
fn ctrl_t_transposes_chars() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("ab");
    app.composer.input.cursor_to_start();

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "ba");
}

#[test]
fn alt_d_kills_next_word() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar baz");
    app.composer.input.cursor_to_start();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT)),
    );
    assert_eq!(app.composer.input.as_str(), "bar baz");
    assert_eq!(app.composer.kill_ring, vec!["foo "]);
}

#[test]
fn alt_backspace_kills_previous_word() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
    );
    assert_eq!(app.composer.input.as_str(), "foo bar ");
    assert_eq!(app.composer.kill_ring, vec!["baz"]);
}

#[test]
fn shift_enter_inserts_newline() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("line1");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(app.composer.input.as_str(), "line1\n");
    assert_eq!(app.composer.input.cursor(), 6);
}

#[test]
fn ctrl_j_inserts_newline() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("line1");
    update(&mut app, &key(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.as_str(), "line1\n");
}

#[test]
fn delete_key_deletes_forward() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "ello");
    assert_eq!(app.composer.input.cursor(), 0);
}

#[test]
fn backspace_deletes_before_cursor() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "hell");
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn typing_inserts_at_cursor_not_at_end() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("helo");
    app.composer.input.cursor_left();
    update(&mut app, &key(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "hello");
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn at_token_opens_file_picker_and_accepts_mention() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.runtime.cwd = dir.path().to_path_buf();
    let _ = std::fs::write(app.runtime.cwd.join("readme.md"), "readme");

    for ch in "inspect @read".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    assert_eq!(
        app.overlay.accessory(),
        PromptAccessory::Files(FilePickerSource::Mention { token_start: 8 })
    );
    let picker = app.overlay.picker().expect("file picker");
    assert!(picker.matches.iter().any(|item| item.label == "readme.md"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "inspect @readme.md ");
}

#[test]
fn file_mention_picker_routes_cursor_navigation_to_the_prompt() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.runtime.cwd = dir.path().to_path_buf();
    let _ = std::fs::write(app.runtime.cwd.join("README.md"), "readme");

    for ch in "inspect @READ".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let end = app.composer.input.cursor();

    update(&mut app, &key(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), end - 1);
    update(&mut app, &key(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), end);

    update(&mut app, &key(KeyCode::Left, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "inspect @REA");
    assert_eq!(app.overlay.picker().expect("file picker").query, "REA");
}

#[test]
fn at_token_accepts_directory_mention() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.runtime.cwd = dir.path().to_path_buf();
    std::fs::create_dir(app.runtime.cwd.join("src")).expect("create source directory");

    for ch in "inspect @src".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    let picker = app.overlay.picker().expect("path picker");
    assert_eq!(picker.selected().map(|item| item.label.as_str()), Some("src/"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "inspect @src/ ");
}

#[test]
fn backspace_while_streaming_deletes_char() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(
        app.composer.input.as_str(),
        "hell",
        "backspace should work while streaming"
    );
}

#[test]
fn history_recall_while_streaming_works() {
    let mut app = working_app_with_streaming();
    app.composer.input_history.push("previous prompt".to_string());
    app.composer.input = PromptInput::from("current draft");

    update(&mut app, &key(KeyCode::Up, KeyModifiers::NONE));

    assert_eq!(
        app.composer.input.as_str(),
        "previous prompt",
        "Up should recall history while streaming"
    );
}

#[test]
fn file_mention_activation_while_working() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("check @src");

    update(&mut app, &key(KeyCode::Char('r'), KeyModifiers::NONE));

    assert!(
        app.composer.input.as_str().contains("@srcr"),
        "typing should append after @mention"
    );
    assert!(
        matches!(app.overlay.accessory(), PromptAccessory::Files(_)),
        "@mention should activate file picker while working"
    );
}
