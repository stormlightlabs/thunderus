use super::*;

#[test]
fn new_is_empty() {
    let p = PromptInput::new();
    assert!(p.is_empty());
    assert_eq!(p.cursor(), 0);
    assert_eq!(p.as_str(), "");
}

#[test]
fn from_str_places_cursor_at_end() {
    let p = PromptInput::from_str("hello");
    assert_eq!(p.as_str(), "hello");
    assert_eq!(p.cursor(), 5);
}

#[test]
fn insert_char_advances_cursor() {
    let mut p = PromptInput::from_str("helo");
    p.cursor_left();
    assert_eq!(p.cursor(), 3);

    p.insert_char('l');
    assert_eq!(p.as_str(), "hello");
    assert_eq!(p.cursor(), 4);
}

#[test]
fn insert_char_at_start() {
    let mut p = PromptInput::from_str("world");
    p.cursor_to_start();
    p.insert_char('!');
    assert_eq!(p.as_str(), "!world");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn insert_char_at_end() {
    let mut p = PromptInput::from_str("hi");
    p.insert_char('!');
    assert_eq!(p.as_str(), "hi!");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn backspace_deletes_left() {
    let mut p = PromptInput::from_str("hello");
    p.cursor_left();
    assert!(p.backspace());
    assert_eq!(p.as_str(), "helo");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn backspace_at_start_is_noop() {
    let mut p = PromptInput::from_str("hello");
    p.cursor_to_start();
    assert!(!p.backspace());
    assert_eq!(p.as_str(), "hello");
    assert_eq!(p.cursor(), 0);
}

#[test]
fn delete_forward_deletes_right() {
    let mut p = PromptInput::from_str("hello");
    p.cursor_to_start();
    p.cursor_right();
    assert!(p.delete_forward());
    assert_eq!(p.as_str(), "hllo");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn delete_forward_at_end_is_noop() {
    let mut p = PromptInput::from_str("hello");
    assert!(!p.delete_forward());
    assert_eq!(p.as_str(), "hello");
}

#[test]
fn cursor_left_clamped() {
    let mut p = PromptInput::from_str("ab");
    p.cursor_left();
    p.cursor_left();
    p.cursor_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn cursor_right_clamped() {
    let mut p = PromptInput::from_str("ab");
    p.cursor_right();
    assert_eq!(p.cursor(), 2);
}

#[test]
fn cursor_to_start_and_end() {
    let mut p = PromptInput::from_str("hello");
    p.cursor_to_start();
    assert_eq!(p.cursor(), 0);

    p.cursor_to_end();
    assert_eq!(p.cursor(), 5);
}

#[test]
fn cursor_up_moves_between_logical_lines() {
    let mut p = PromptInput::from_str("x\n x\n x");
    assert!(p.cursor_up());
    assert_eq!(p.cursor(), 4);

    assert!(p.cursor_up());
    assert_eq!(p.cursor(), 1);

    assert!(!p.cursor_up());
    assert_eq!(p.cursor(), 1);
}

#[test]
fn cursor_down_moves_between_logical_lines() {
    let mut p = PromptInput::from_str("x\n x\n x");
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
    let mut p = PromptInput::from_str("long\nx\nwide");
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
    let mut p = PromptInput::from_str("foo bar baz");
    p.cursor_word_left();
    assert_eq!(p.cursor(), 8);

    p.cursor_word_left();
    assert_eq!(p.cursor(), 4);

    p.cursor_word_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn word_left_from_within_word() {
    let mut p = PromptInput::from_str("foo bar");
    p.cursor_word_left();
    assert_eq!(p.cursor(), 4);

    p.cursor_left();
    assert_eq!(p.cursor(), 3);

    p.cursor_word_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn word_right_skips_word_then_whitespace() {
    let mut p = PromptInput::from_str("foo bar baz");
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
    let mut p = PromptInput::from_str("a   b");
    p.cursor_word_left();
    assert_eq!(p.cursor(), 4);
    p.cursor_word_left();
    assert_eq!(p.cursor(), 0);
}

#[test]
fn insert_str_at_cursor() {
    let mut p = PromptInput::from_str("hello world");
    p.cursor_to_start();
    p.cursor_word_right();
    p.cursor_left();
    p.insert_str(" big");
    assert_eq!(p.as_str(), "hello big world");
    assert_eq!(p.cursor(), 9);
}

#[test]
fn insert_str_empty_is_noop() {
    let mut p = PromptInput::from_str("hi");
    p.insert_str("");
    assert_eq!(p.as_str(), "hi");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn clear_resets_cursor() {
    let mut p = PromptInput::from_str("hello");
    p.clear();
    assert!(p.is_empty());
    assert_eq!(p.cursor(), 0);
}

#[test]
fn set_text_places_cursor_at_end() {
    let mut p = PromptInput::from_str("old");
    p.set_text("new value");
    assert_eq!(p.as_str(), "new value");
    assert_eq!(p.cursor(), 9);
}

#[test]
fn kill_to_end_of_line() {
    let mut p = PromptInput::from_str("hello world");
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
    let mut p = PromptInput::from_str("hello world");
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
    let mut p = PromptInput::from_str("foo bar baz");
    let killed = p.kill_word_left();
    assert_eq!(killed, "baz");
    assert_eq!(p.as_str(), "foo bar ");
    assert_eq!(p.cursor(), 8);
}

#[test]
fn kill_word_left_multiple_spaces() {
    let mut p = PromptInput::from_str("a   b");
    let killed = p.kill_word_left();
    assert_eq!(killed, "b");
    assert_eq!(p.as_str(), "a   ");
}

#[test]
fn kill_word_right() {
    let mut p = PromptInput::from_str("foo bar baz");
    p.cursor_to_start();
    let killed = p.kill_word_right();
    assert_eq!(killed, "foo ");
    assert_eq!(p.as_str(), "bar baz");
    assert_eq!(p.cursor(), 0);
}

#[test]
fn transpose_chars_at_cursor() {
    let mut p = PromptInput::from_str("ab");
    p.cursor_to_start();
    p.transpose_chars();
    assert_eq!(p.as_str(), "ba");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn transpose_chars_at_end() {
    let mut p = PromptInput::from_str("hello");
    p.transpose_chars();
    assert_eq!(p.as_str(), "helol");
}

#[test]
fn yank_pastes_at_cursor() {
    let mut p = PromptInput::from_str("hello");
    p.cursor_left();
    p.yank(" world");
    assert_eq!(p.as_str(), "hell worldo");
    assert_eq!(p.cursor(), 10);
}

#[test]
fn insert_newline() {
    let mut p = PromptInput::from_str("line1");
    p.insert_char('\n');
    p.insert_str("line2");
    assert_eq!(p.as_str(), "line1\nline2");
    assert_eq!(p.cursor(), 11);
}

#[test]
fn text_before_cursor() {
    let mut p = PromptInput::from_str("hello world");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.text_before_cursor(), "hello ");
}

#[test]
fn multibyte_char_handling() {
    let mut p = PromptInput::from_str("héllo");
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
    let mut p = PromptInput::from_str("héllo");
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "hélo");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn combining_mark_is_one_grapheme() {
    let text = "e\u{0301}llo";
    let p = PromptInput::from_str(text);
    assert_eq!(p.len_graphemes(), 4, "e + combining acute should be 1 grapheme");
    assert_eq!(p.cursor(), 4);
}

#[test]
fn combining_mark_cursor_left_right() {
    let text = "he\u{0301}llo";
    let mut p = PromptInput::from_str(text);
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
    let mut p = PromptInput::from_str(text);
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "he\u{0301}lo");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn combining_mark_delete_forward() {
    let text = "he\u{0301}llo";
    let mut p = PromptInput::from_str(text);
    p.cursor_to_start();
    p.cursor_right();
    p.delete_forward();
    assert_eq!(p.as_str(), "hllo");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn emoji_zwj_sequence_is_one_grapheme() {
    let text = "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b";
    let p = PromptInput::from_str(text);
    assert_eq!(p.len_graphemes(), 3, "a + family-emoji + b = 3 graphemes");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn emoji_zwj_backspace_deletes_whole_cluster() {
    let text = "x\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}y";
    let mut p = PromptInput::from_str(text);
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "xy");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn emoji_zwj_cursor_navigation() {
    let text = "ab\u{1F468}\u{200D}\u{1F469}cd";
    let mut p = PromptInput::from_str(text);
    assert_eq!(p.len_graphemes(), 5);

    p.cursor_to_start();
    p.cursor_right();
    p.cursor_right();
    p.cursor_right();
    assert_eq!(p.cursor(), 3);
}

#[test]
fn cjk_wide_char_graphemes() {
    let p = PromptInput::from_str("你好世界");
    assert_eq!(p.len_graphemes(), 4);
    assert_eq!(p.cursor(), 4);
}

#[test]
fn cjk_wide_char_backspace() {
    let mut p = PromptInput::from_str("你好世界");
    p.backspace();
    assert_eq!(p.as_str(), "你好世");
    assert_eq!(p.cursor(), 3);
}

#[test]
fn cjk_wide_char_mixed() {
    let mut p = PromptInput::from_str("hi你好");
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
    let p = PromptInput::from_str(text);
    assert_eq!(p.len_graphemes(), 2, "中+combining and 文 = 2 graphemes");
}

#[test]
fn emoji_skin_tone_modifier_is_one_grapheme() {
    let text = "a\u{1F44D}\u{1F3FF}b";
    let p = PromptInput::from_str(text);
    assert_eq!(p.len_graphemes(), 3, "a + thumbs-up-dark + b = 3 graphemes");
}

#[test]
fn emoji_skin_tone_backspace() {
    let text = "x\u{1F44D}\u{1F3FF}y";
    let mut p = PromptInput::from_str(text);
    p.cursor_left();
    p.backspace();
    assert_eq!(p.as_str(), "xy");
    assert_eq!(p.cursor(), 1);
}

#[test]
fn transpose_emoji_clusters() {
    let mut p = PromptInput::from_str("a\u{1F600}"); // a + 😀
    p.transpose_chars();
    assert_eq!(p.as_str(), "\u{1F600}a");
    assert_eq!(p.cursor(), 2);
}

#[test]
fn transpose_combining_mark() {
    let mut p = PromptInput::from_str("he\u{0301}x"); // h, é, x
    p.cursor_to_end();
    p.transpose_chars(); // swaps é and x
    assert_eq!(p.as_str(), "hxe\u{0301}");
}

#[test]
fn word_boundaries_with_punctuation() {
    let mut p = PromptInput::from_str("foo.bar baz");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.cursor(), 8);
}

#[test]
fn word_boundaries_with_cjk() {
    let mut p = PromptInput::from_str("hello 世界 test");
    p.cursor_to_start();
    p.cursor_word_right();
    assert_eq!(p.cursor(), 6);
}

#[test]
fn kill_word_left_with_emoji() {
    let text = "foo \u{1F600} bar";
    let mut p = PromptInput::from_str(text);
    let killed = p.kill_word_left();
    assert_eq!(killed, "bar");
    assert_eq!(p.as_str(), "foo \u{1F600} ");
}

#[test]
fn multiline_kill_to_end_of_line() {
    let mut p = PromptInput::from_str("line1\nline2\nline3");
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
    let mut p = PromptInput::from_str("line1\nline2\nline3");
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
