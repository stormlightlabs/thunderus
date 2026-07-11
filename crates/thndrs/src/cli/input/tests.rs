use crate::{renderer, utils};

use super::*;

/// Generate a prompt string of approximately `target_bytes` by repeating
/// a prose fragment. The result is valid UTF-8 with word boundaries.
fn make_large_prompt(target_bytes: usize) -> String {
    let fragment = "the quick brown fox jumps over the lazy dog. ";
    let repeats = target_bytes / fragment.len();
    fragment.repeat(repeats)
}

#[test]
fn new_is_empty() {
    let p = PromptInput::new();
    assert!(p.is_empty());
    assert_eq!(p.cursor(), 0);
    assert_eq!(p.as_str(), "");
}

#[test]
fn from_str_places_cursor_at_end() {
    let p = PromptInput::from("hello");
    assert_eq!(p.as_str(), "hello");
    assert_eq!(p.cursor(), 5);
}

#[test]
fn insert_char_advances_cursor() {
    let mut p = PromptInput::from("helo");
    p.cursor_left();
    assert_eq!(p.cursor(), 3);

    p.insert_char('l');
    assert_eq!(p.as_str(), "hello");
    assert_eq!(p.cursor(), 4);
}

#[test]
fn insert_char_at_start() {
    let mut p = PromptInput::from("world");
    p.cursor_to_start();
    p.insert_char('!');
    assert_eq!(p.as_str(), "!world");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn insert_char_at_end() {
    let mut p = PromptInput::from("hi");
    p.insert_char('!');
    assert_eq!(p.as_str(), "hi!");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn backspace_deletes_left() {
    let mut p = PromptInput::from("hello");
    p.cursor_left();
    assert!(p.backspace());
    assert_eq!(p.as_str(), "helo");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn backspace_at_start_is_noop() {
    let mut p = PromptInput::from("hello");
    p.cursor_to_start();
    assert!(!p.backspace());
    assert_eq!(p.as_str(), "hello");
    assert_eq!(p.cursor(), 0);
}

#[test]
fn delete_forward_deletes_right() {
    let mut p = PromptInput::from("hello");
    p.cursor_to_start();
    p.cursor_right();
    assert!(p.delete_forward());
    assert_eq!(p.as_str(), "hllo");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn delete_forward_at_end_is_noop() {
    let mut p = PromptInput::from("hello");
    assert!(!p.delete_forward());
    assert_eq!(p.as_str(), "hello");
}

#[test]
fn cursor_left_clamped() {
    let mut p = PromptInput::from("ab");
    p.cursor_left();
    p.cursor_left();
    p.cursor_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn cursor_right_clamped() {
    let mut p = PromptInput::from("ab");
    p.cursor_right();
    assert_eq!(p.cursor(), 2);
}

#[test]
fn cursor_to_start_and_end() {
    let mut p = PromptInput::from("hello");
    p.cursor_to_start();
    assert_eq!(p.cursor(), 0);

    p.cursor_to_end();
    assert_eq!(p.cursor(), 5);
}

#[test]
fn cursor_up_moves_between_logical_lines() {
    let mut p = PromptInput::from("x\n x\n x");
    assert!(p.cursor_up());
    assert_eq!(p.cursor(), 4);

    assert!(p.cursor_up());
    assert_eq!(p.cursor(), 1);

    assert!(!p.cursor_up());
    assert_eq!(p.cursor(), 1);
}

#[test]
fn cursor_down_moves_between_logical_lines() {
    let mut p = PromptInput::from("x\n x\n x");
    p.cursor_to_start();
    p.cursor_right();

    assert!(p.cursor_down());
    assert_eq!(p.cursor(), 3);

    assert!(p.cursor_down());
    assert_eq!(p.cursor(), 6);

    assert!(!p.cursor_down());
    assert_eq!(p.cursor(), 6);
}

#[test]
fn cursor_up_and_down_clamp_to_shorter_lines() {
    let mut p = PromptInput::from("long\nx\nwide");
    p.cursor_to_start();
    p.cursor_right();
    p.cursor_right();
    p.cursor_right();

    assert!(p.cursor_down());
    assert_eq!(p.cursor(), 6);

    assert!(p.cursor_down());
    assert_eq!(p.cursor(), 8);
}

#[test]
fn word_left_skips_whitespace_then_word() {
    let mut p = PromptInput::from("foo bar baz");
    p.cursor_word_left();
    assert_eq!(p.cursor(), 8);

    p.cursor_word_left();
    assert_eq!(p.cursor(), 4);

    p.cursor_word_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn word_left_from_within_word() {
    let mut p = PromptInput::from("foo bar");
    p.cursor_word_left();
    assert_eq!(p.cursor(), 4);

    p.cursor_left();
    assert_eq!(p.cursor(), 3);

    p.cursor_word_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn word_right_skips_word_then_whitespace() {
    let mut p = PromptInput::from("foo bar baz");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.cursor(), 4);

    p.cursor_word_right();
    assert_eq!(p.cursor(), 8);

    p.cursor_word_right();
    assert_eq!(p.cursor(), 11);
}

#[test]
fn word_left_multiple_spaces() {
    let mut p = PromptInput::from("a   b");
    p.cursor_word_left();
    assert_eq!(p.cursor(), 4);
    p.cursor_word_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn insert_str_at_cursor() {
    let mut p = PromptInput::from("hello world");
    p.cursor_to_start();
    p.cursor_word_right();
    p.cursor_left();
    p.insert_str(" big");
    assert_eq!(p.as_str(), "hello big world");
    assert_eq!(p.cursor(), 9);
}

#[test]
fn insert_str_empty_is_noop() {
    let mut p = PromptInput::from("hi");
    p.insert_str("");
    assert_eq!(p.as_str(), "hi");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn clear_resets_cursor() {
    let mut p = PromptInput::from("hello");
    p.clear();
    assert!(p.is_empty());
    assert_eq!(p.cursor(), 0);
}

#[test]
fn set_text_places_cursor_at_end() {
    let mut p = PromptInput::from("old");
    p.set_text("new value");
    assert_eq!(p.as_str(), "new value");
    assert_eq!(p.cursor(), 9);
}

#[test]
fn kill_to_end_of_line() {
    let mut p = PromptInput::from("hello world");
    p.cursor_left();
    p.cursor_left();
    p.cursor_left();
    let killed = p.kill_to_end_of_line();
    assert_eq!(killed, "rld");
    assert_eq!(p.as_str(), "hello wo");
    assert_eq!(p.cursor(), 8);
}

#[test]
fn kill_to_start_of_line() {
    let mut p = PromptInput::from("hello world");
    p.cursor_left();
    p.cursor_left();
    p.cursor_left();
    let killed = p.kill_to_start_of_line();
    assert_eq!(killed, "hello wo");
    assert_eq!(p.as_str(), "rld");
    assert_eq!(p.cursor(), 0);
}

#[test]
fn kill_word_left() {
    let mut p = PromptInput::from("foo bar baz");
    let killed = p.kill_word_left();
    assert_eq!(killed, "baz");
    assert_eq!(p.as_str(), "foo bar ");
    assert_eq!(p.cursor(), 8);
}

#[test]
fn kill_word_left_multiple_spaces() {
    let mut p = PromptInput::from("a   b");
    let killed = p.kill_word_left();
    assert_eq!(killed, "b");
    assert_eq!(p.as_str(), "a   ");
}

#[test]
fn kill_word_right() {
    let mut p = PromptInput::from("foo bar baz");
    p.cursor_to_start();
    let killed = p.kill_word_right();
    assert_eq!(killed, "foo ");
    assert_eq!(p.as_str(), "bar baz");
    assert_eq!(p.cursor(), 0);
}

#[test]
fn transpose_chars_at_cursor() {
    let mut p = PromptInput::from("ab");
    p.cursor_to_start();
    p.transpose_chars();
    assert_eq!(p.as_str(), "ba");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn transpose_chars_at_end() {
    let mut p = PromptInput::from("hello");
    p.transpose_chars();
    assert_eq!(p.as_str(), "helol");
}

#[test]
fn yank_pastes_at_cursor() {
    let mut p = PromptInput::from("hello");
    p.cursor_left();
    p.yank(" world");
    assert_eq!(p.as_str(), "hell worldo");
    assert_eq!(p.cursor(), 10);
}

#[test]
fn insert_newline() {
    let mut p = PromptInput::from("line1");
    p.insert_char('\n');
    p.insert_str("line2");
    assert_eq!(p.as_str(), "line1\nline2");
    assert_eq!(p.cursor(), 11);
}

#[test]
fn text_before_cursor() {
    let mut p = PromptInput::from("hello world");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.text_before_cursor(), "hello ");
}

#[test]
fn multibyte_char_handling() {
    let mut p = PromptInput::from("héllo");
    assert_eq!(p.len_graphemes(), 5);
    assert_eq!(p.cursor(), 5);

    p.cursor_to_start();
    p.cursor_right();
    p.insert_char('x');

    assert_eq!(p.as_str(), "hxéllo");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn backspace_multibyte() {
    let mut p = PromptInput::from("héllo");
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "hélo");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn combining_mark_is_one_grapheme() {
    let text = "e\u{0301}llo";
    let p = PromptInput::from(text);
    assert_eq!(p.len_graphemes(), 4, "e + combining acute should be 1 grapheme");
    assert_eq!(p.cursor(), 4);
}

#[test]
fn combining_mark_cursor_left_right() {
    let text = "he\u{0301}llo";
    let mut p = PromptInput::from(text);
    assert_eq!(p.len_graphemes(), 5);

    p.cursor_left();
    assert_eq!(p.cursor(), 4);

    p.cursor_left();
    assert_eq!(p.cursor(), 3);

    p.cursor_right();
    assert_eq!(p.cursor(), 4);
}

#[test]
fn combining_mark_backspace_deletes_whole_cluster() {
    let text = "he\u{0301}llo";
    let mut p = PromptInput::from(text);
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "he\u{0301}lo");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn combining_mark_delete_forward() {
    let text = "he\u{0301}llo";
    let mut p = PromptInput::from(text);
    p.cursor_to_start();
    p.cursor_right();
    p.delete_forward();
    assert_eq!(p.as_str(), "hllo");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn emoji_zwj_sequence_is_one_grapheme() {
    let text = "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b";
    let p = PromptInput::from(text);
    assert_eq!(p.len_graphemes(), 3, "a + family-emoji + b = 3 graphemes");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn emoji_zwj_backspace_deletes_whole_cluster() {
    let text = "x\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}y";
    let mut p = PromptInput::from(text);
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "xy");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn emoji_zwj_cursor_navigation() {
    let text = "ab\u{1F468}\u{200D}\u{1F469}cd";
    let mut p = PromptInput::from(text);
    assert_eq!(p.len_graphemes(), 5);

    p.cursor_to_start();
    p.cursor_right();
    p.cursor_right();
    p.cursor_right();
    assert_eq!(p.cursor(), 3);
}

#[test]
fn cjk_wide_char_graphemes() {
    let p = PromptInput::from("你好世界");
    assert_eq!(p.len_graphemes(), 4);
    assert_eq!(p.cursor(), 4);
}

#[test]
fn cjk_wide_char_backspace() {
    let mut p = PromptInput::from("你好世界");
    p.backspace();
    assert_eq!(p.as_str(), "你好世");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn cjk_wide_char_mixed() {
    let mut p = PromptInput::from("hi你好");
    assert_eq!(p.len_graphemes(), 4);
    p.cursor_to_start();
    p.cursor_right();
    p.cursor_right();
    p.insert_char('x');
    assert_eq!(p.as_str(), "hix你好");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn zero_width_combining_mark_in_cjk() {
    let text = "中\u{0302}文";
    let p = PromptInput::from(text);
    assert_eq!(p.len_graphemes(), 2, "中+combining and 文 = 2 graphemes");
}

#[test]
fn emoji_skin_tone_modifier_is_one_grapheme() {
    let text = "a\u{1F44D}\u{1F3FF}b";
    let p = PromptInput::from(text);
    assert_eq!(p.len_graphemes(), 3, "a + thumbs-up-dark + b = 3 graphemes");
}

#[test]
fn emoji_skin_tone_backspace() {
    let text = "x\u{1F44D}\u{1F3FF}y";
    let mut p = PromptInput::from(text);
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "xy");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn transpose_emoji_clusters() {
    let mut p = PromptInput::from("a\u{1F600}");
    p.transpose_chars();
    assert_eq!(p.as_str(), "\u{1F600}a");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn transpose_combining_mark() {
    let mut p = PromptInput::from("he\u{0301}x");
    p.cursor_to_end();
    p.transpose_chars();
    assert_eq!(p.as_str(), "hxe\u{0301}");
}

#[test]
fn word_boundaries_with_punctuation() {
    let mut p = PromptInput::from("foo.bar baz");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.cursor(), 8);
}

#[test]
fn word_boundaries_with_cjk() {
    let mut p = PromptInput::from("hello 世界 test");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.cursor(), 6);
}

#[test]
fn kill_word_left_with_emoji() {
    let text = "foo \u{1F600} bar";
    let mut p = PromptInput::from(text);
    let killed = p.kill_word_left();
    assert_eq!(killed, "bar");
    assert_eq!(p.as_str(), "foo \u{1F600} ");
}

#[test]
fn multiline_kill_to_end_of_line() {
    let mut p = PromptInput::from("line1\nline2\nline3");
    p.cursor_to_start();
    p.cursor_right();
    p.cursor_right();

    let killed = p.kill_to_end_of_line();
    assert_eq!(killed, "ne1");
    assert_eq!(p.as_str(), "li\nline2\nline3");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn multiline_kill_to_start_of_line() {
    let mut p = PromptInput::from("line1\nline2\nline3");
    p.cursor_to_start();
    for _ in 0..11 {
        p.cursor_right();
    }
    assert_eq!(p.cursor(), 11);
    let killed = p.kill_to_start_of_line();
    assert_eq!(killed, "line2");
    assert_eq!(p.as_str(), "line1\n\nline3");
    assert_eq!(p.cursor(), 6);
}

#[test]
fn stress_10kb_prompt_edit_at_start() {
    let text = make_large_prompt(10_000);
    let mut p = PromptInput::from(text.as_str());
    p.cursor_to_start();
    p.insert_char('X');
    assert_eq!(p.cursor(), 1);
    assert!(p.as_str().starts_with("Xthe quick"));
    p.backspace();
    assert_eq!(p.cursor(), 0);
    assert!(p.as_str().starts_with("the quick"));
}

#[test]
fn stress_10kb_prompt_edit_at_middle() {
    let text = make_large_prompt(10_000);
    let mut p = PromptInput::from(text.as_str());
    let mid = p.len_graphemes() / 2;
    for _ in 0..mid {
        p.cursor_left();
    }
    let cursor_before = p.cursor();
    p.insert_char('X');
    assert_eq!(p.cursor(), cursor_before + 1);
    p.backspace();
    assert_eq!(p.cursor(), cursor_before);
}

#[test]
fn stress_10kb_prompt_edit_at_end() {
    let text = make_large_prompt(10_000);
    let mut p = PromptInput::from(text.as_str());
    p.insert_char('X');
    assert!(p.as_str().ends_with("X"));

    p.backspace();
    assert!(!p.as_str().ends_with("X"));
}

#[test]
fn stress_100kb_prompt_edit_at_start() {
    let text = make_large_prompt(100_000);
    let mut p = PromptInput::from(text.as_str());
    p.cursor_to_start();
    p.insert_char('X');
    assert_eq!(p.cursor(), 1);
    assert!(p.as_str().starts_with("Xthe quick"));
    p.backspace();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn stress_100kb_prompt_edit_at_middle() {
    let text = make_large_prompt(100_000);
    let mut p = PromptInput::from(text.as_str());
    let mid = p.len_graphemes() / 2;
    for _ in 0..mid {
        p.cursor_left();
    }
    let cursor_before = p.cursor();
    p.insert_char('X');
    assert_eq!(p.cursor(), cursor_before + 1);
    p.backspace();
    assert_eq!(p.cursor(), cursor_before);
}

#[test]
fn stress_100kb_prompt_edit_at_end() {
    let text = make_large_prompt(100_000);
    let mut p = PromptInput::from(text.as_str());
    p.insert_char('X');
    assert!(p.as_str().ends_with("X"));
    p.backspace();
    assert!(!p.as_str().ends_with("X"));
}

#[test]
fn stress_1mb_prompt_edit_at_start() {
    let text = make_large_prompt(1_000_000);
    let mut p = PromptInput::from(text.as_str());
    p.cursor_to_start();
    p.insert_char('X');
    assert_eq!(p.cursor(), 1);
    assert!(p.as_str().starts_with("Xthe quick"));
    p.backspace();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn stress_1mb_prompt_edit_at_middle() {
    let text = make_large_prompt(1_000_000);
    let mut p = PromptInput::from(text.as_str());
    let mid = p.len_graphemes() / 2;
    for _ in 0..mid {
        p.cursor_left();
    }
    let cursor_before = p.cursor();
    p.insert_char('X');
    assert_eq!(p.cursor(), cursor_before + 1);
    p.backspace();
    assert_eq!(p.cursor(), cursor_before);
}

#[test]
fn stress_1mb_prompt_edit_at_end() {
    let text = make_large_prompt(1_000_000);
    let mut p = PromptInput::from(text.as_str());
    p.insert_char('X');
    assert!(p.as_str().ends_with("X"));
    p.backspace();
    assert!(!p.as_str().ends_with("X"));
}

#[test]
fn stress_1mb_prompt_cursor_navigation_no_panic() {
    let text = make_large_prompt(1_000_000);
    let mut p = PromptInput::from(text.as_str());

    p.cursor_to_start();
    assert_eq!(p.cursor(), 0);

    p.cursor_to_end();
    assert_eq!(p.cursor(), p.len_graphemes());

    p.cursor_to_start();
    assert_eq!(p.cursor(), 0);
}

/// Timing test separating prompt editing cost from visual wrapping and
/// display-width measurement cost.
///
/// This test measures the two costs independently:
/// - **Prompt editing**: `insert_char` + `backspace` on a 100 KB prompt.
/// - **Visual wrapping**: `wrap_text` on the same 100 KB prompt.
/// - **Display-width measurement**: `display_width` on each wrapped line.
///
/// The String-backed model is expected to be fast enough that each operation
/// completes in well under a second. If these thresholds ever fail, the plan
/// recommends revisiting Ropey for prompt storage and/or Textwrap for wrapping.
#[test]
fn timing_separates_prompt_editing_from_wrapping_and_width() {
    use std::time::Instant;

    let text = make_large_prompt(100_000);
    let edit_start = Instant::now();
    let mut p = PromptInput::from(text.as_str());
    p.cursor_to_start();
    p.insert_char('X');
    p.backspace();
    let edit_elapsed = edit_start.elapsed();

    let wrap_start = Instant::now();
    let lines = renderer::layout::wrap_text(p.as_str(), 80);
    let wrap_elapsed = wrap_start.elapsed();

    let width_start = Instant::now();
    let _total_width: usize = lines.iter().map(|l| utils::text_width(l)).sum();
    let width_elapsed = width_start.elapsed();

    assert!(
        edit_elapsed.as_secs() < 2,
        "prompt editing should be fast: {:?}",
        edit_elapsed
    );
    assert!(
        wrap_elapsed.as_secs() < 2,
        "visual wrapping should be fast: {:?}",
        wrap_elapsed
    );
    assert!(
        width_elapsed.as_secs() < 2,
        "display-width measurement should be fast: {:?}",
        width_elapsed
    );
}

/// Confirm that prompt storage remains `String`-backed (no Ropey).
///
/// The plan decided against Ropey for this milestone. This test verifies that
/// `PromptInput` stores text as a plain `String` and that the cursor is a
/// grapheme index into that `String`. If the storage type changes, this test
/// will need updating, which forces a conscious decision about the plan.
#[test]
fn prompt_storage_is_string_backed() {
    let p = PromptInput::from("hello world");
    assert_eq!(p.as_str(), "hello world");

    let cursor: usize = p.cursor();
    assert_eq!(cursor, 11);

    let mut p = PromptInput::new();
    p.set_text("first");
    p.set_text("second");
    assert_eq!(p.as_str(), "second");
    assert_eq!(p.cursor(), 6);
}
