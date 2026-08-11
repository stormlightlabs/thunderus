use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyModifiers};

use helpers::*;

#[test]
fn left_arrow_moves_cursor_left() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("abc");
    update(&mut app, &key(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), 2);
}

#[test]
fn right_arrow_moves_cursor_right() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("abc");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), 1);
}

#[test]
fn ctrl_b_moves_cursor_left() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("abc");
    update(&mut app, &key(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.cursor(), 2);
}

#[test]
fn ctrl_f_moves_cursor_right() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("abc");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.cursor(), 1);
}

#[test]
fn home_moves_to_start() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), 0);
}

#[test]
fn end_moves_to_end() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), 5);
}

#[test]
fn ctrl_a_moves_to_start() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.cursor(), 0);
}

#[test]
fn ctrl_e_moves_to_end() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.cursor(), 5);
}

#[test]
fn alt_left_moves_word_left() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar");
    update(&mut app, &key(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn ctrl_left_moves_word_left() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar");
    update(&mut app, &key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn alt_b_moves_word_left() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar");
    update(&mut app, &key(KeyCode::Char('b'), KeyModifiers::ALT));
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn alt_right_moves_word_right() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn ctrl_right_moves_word_right() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn alt_f_moves_word_right() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Char('f'), KeyModifiers::ALT));
    assert_eq!(app.composer.input.cursor(), 4);
}
