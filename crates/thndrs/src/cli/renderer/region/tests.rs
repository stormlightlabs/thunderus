use super::*;
use crate::app::{App, Entry};
use crate::cli::Cli;
use crate::renderer::backend::TerminalBackend;

fn test_app() -> App {
    let mut app = App::from_cli(&Cli::default());
    app.session_writer = None;
    app
}

#[test]
fn vt100_proves_native_history_live_repaint_and_prompt_cursor() {
    let mut app = test_app();
    app.first_run_recovery = None;
    app.session_id = "test-session".to_string();
    app.transcript
        .push(Entry::User { text: "committed history".to_string() });
    app.input.set_text("draft");

    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 40, 12);
    let expected = live.build_frame(&app, 40, 12).cursor.expect("editable prompt cursor");
    live.render_frame(&app, &mut backend, 40, 12).expect("initial render");

    let initial = backend.writer().clone();
    assert!(
        initial
            .windows(b"\x1b[?2026h".len())
            .any(|bytes| bytes == b"\x1b[?2026h")
    );
    assert!(
        initial
            .windows(b"\x1b[?2026l".len())
            .any(|bytes| bytes == b"\x1b[?2026l")
    );
    assert!(initial.windows(b"\x1b[5 q".len()).any(|bytes| bytes == b"\x1b[5 q"));

    let mut parser = vt100::Parser::new(12, 40, 128);
    parser.process(&initial);
    assert!(
        !parser.screen().alternate_screen(),
        "native mode must not enter the alternate screen"
    );
    assert_eq!(parser.screen().mouse_protocol_mode(), vt100::MouseProtocolMode::None);
    assert!(
        !parser.screen().hide_cursor(),
        "the editable prompt should show the hardware cursor"
    );
    assert_eq!(
        parser.screen().cursor_position(),
        (expected.row as u16, expected.col as u16)
    );
    if !crossterm::style::Colored::ansi_color_disabled_memoized() {
        let crossterm::style::Color::Rgb { r, g, b } = style::palette().panel_bg else {
            panic!("test palette should use an RGB panel background");
        };
        let composer_top = expected.row.saturating_sub(1) as u16;
        let status_row = composer_top + 3;
        for row in composer_top..=status_row {
            for col in 0..40 {
                assert_eq!(
                    parser
                        .screen()
                        .cell(row, col)
                        .expect("rendered physical cell")
                        .bgcolor(),
                    vt100::Color::Rgb(r, g, b),
                    "composer/status background changed at row {row}, column {col}"
                );
            }
        }
    }
    assert!(
        parser.screen().contents().contains("committed history"),
        "terminal screen after history commit:\n{}",
        parser.screen().contents()
    );

    backend.writer().clear();
    app.input.set_text("changed draft");
    let expected_after = live.build_frame(&app, 40, 12).cursor.expect("updated prompt cursor");
    live.render_frame(&app, &mut backend, 40, 12).expect("live repaint");
    let repaint = backend.writer().clone();
    assert!(
        !repaint
            .windows(b"committed history".len())
            .any(|bytes| bytes == b"committed history")
    );
    parser.process(&repaint);

    assert!(parser.screen().contents().contains("committed history"));
    assert!(parser.screen().contents().contains("changed draft"));
    assert_eq!(
        parser.screen().cursor_position(),
        (expected_after.row as u16, expected_after.col as u16)
    );
}

#[test]
fn vt100_can_scroll_back_to_settled_transcript_after_live_repaints() {
    let mut app = test_app();
    app.first_run_recovery = None;
    for index in 0..24 {
        app.transcript
            .push(Entry::User { text: format!("history row {index:02}") });
    }
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 40, 8);
    let mut parser = vt100::Parser::new(8, 40, 256);

    live.render_frame(&app, &mut backend, 40, 8).expect("initial render");
    parser.process(backend.writer());
    assert!(
        !parser.screen().contents().contains("history row 00"),
        "old settled rows should have moved above the live viewport"
    );

    parser.screen_mut().set_scrollback(usize::MAX);
    let scrollback_rows = parser.screen().scrollback();
    assert!(scrollback_rows > 0, "settled rows should populate native scrollback");
    assert!(
        (0..=scrollback_rows).any(|offset| {
            parser.screen_mut().set_scrollback(offset);
            parser.screen().contents().contains("history row 00")
        }),
        "terminal-owned history should be reachable by scrolling"
    );

    parser.screen_mut().set_scrollback(0);
    backend.writer().clear();
    app.input.set_text("changed draft");
    live.render_frame(&app, &mut backend, 40, 8).expect("live repaint");
    assert!(
        !backend
            .writer()
            .windows(b"history row 00".len())
            .any(|bytes| bytes == b"history row 00"),
        "live repaint must not replay settled history"
    );
    parser.process(backend.writer());
    parser.screen_mut().set_scrollback(usize::MAX);
    let scrollback_rows = parser.screen().scrollback();
    assert!(
        (0..=scrollback_rows).any(|offset| {
            parser.screen_mut().set_scrollback(offset);
            parser.screen().contents().contains("history row 00")
        }),
        "live repaint should preserve settled terminal history"
    );
}

#[test]
fn settled_transcript_is_inserted_once_into_native_scrollback() {
    let mut app = test_app();
    app.transcript
        .push(Entry::User { text: "committed message".to_string() });
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

    live.render_frame(&app, &mut backend, 80, 24).expect("initial render");
    let first = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert_eq!(first.matches("committed message").count(), 1);
    assert!(
        first.contains("\r\n"),
        "settled rows should enter scrollback through terminal newlines"
    );

    backend.writer().clear();
    live.render_frame(&app, &mut backend, 80, 24).expect("unchanged render");
    assert!(
        backend.writer().is_empty(),
        "settled transcript must not be repainted by the app"
    );
}

#[test]
fn replacement_transcript_starts_a_new_native_history_segment() {
    let mut app = test_app();
    app.transcript
        .push(Entry::User { text: "original session row".to_string() });
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

    live.render_frame(&app, &mut backend, 80, 24)
        .expect("initial session render");
    backend.writer().clear();

    app.transcript = vec![Entry::Agent { text: "resumed session row".to_string(), streaming: false }];
    live.begin_transcript_segment();
    live.render_frame(&app, &mut backend, 80, 24)
        .expect("resumed session render");

    let output = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert_eq!(output.matches("resumed session row").count(), 1);
    assert!(!output.contains("original session row"));
    assert!(!output.contains("Ask for change, run a command, or inspect the repo."));
    assert!(!output.contains("\x1b[3J"), "resuming must retain terminal history");
}

#[test]
fn changing_streaming_tail_does_not_repaint_settled_transcript() {
    let mut app = test_app();
    app.transcript.push(Entry::User { text: "settled".to_string() });
    app.transcript
        .push(Entry::Agent { text: "first chunk".to_string(), streaming: true });
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

    live.render_frame(&app, &mut backend, 80, 24).expect("initial render");
    backend.writer().clear();
    app.transcript[1] = Entry::Agent { text: "second chunk".to_string(), streaming: true };
    live.render_frame(&app, &mut backend, 80, 24).expect("stream update");

    let output = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert!(output.contains("second chunk"));
    assert!(
        !output.contains("settled"),
        "stable transcript should remain terminal-owned"
    );
}

#[test]
fn live_frame_places_a_real_terminal_cursor() {
    let mut app = test_app();
    app.input.insert_str("draft");
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

    live.render_frame(&app, &mut backend, 80, 24).expect("prompt render");
    let output = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert!(
        output.contains("\x1b[?25h"),
        "prompt rendering should show the terminal cursor"
    );
    assert!(
        output.contains("\x1b[5 q"),
        "prompt rendering should select a blinking terminal cursor"
    );
    assert!(
        output.contains("\x1b["),
        "prompt rendering should position the terminal cursor"
    );
}

#[test]
fn production_render_includes_the_startup_banner() {
    let app = test_app();
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

    live.render_frame(&app, &mut backend, 80, 24).expect("startup render");
    let output = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");

    assert!(
        output.contains("Ask for change, run a command, or inspect the repo."),
        "the production renderer must show the startup banner"
    );
}

#[test]
fn prompt_growth_and_height_changes_preserve_native_scrollback() {
    let mut app = test_app();
    app.transcript
        .push(Entry::User { text: "committed message".to_string() });
    let mut live = LiveRegion::new();
    let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

    live.render_frame(&app, &mut backend, 80, 24).expect("initial render");
    backend.writer().clear();

    app.input.insert_str("first line\nsecond line\nthird line");
    live.render_frame(&app, &mut backend, 80, 24)
        .expect("grown prompt render");
    let grown = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert!(grown.contains("third line"));
    assert!(
        !grown.contains("\x1b[3J"),
        "prompt growth must not purge native scrollback"
    );
    assert!(
        !grown.contains("committed message"),
        "settled transcript must not be replayed"
    );

    backend.writer().clear();
    backend.set_size(80, 18);
    live.render_frame(&app, &mut backend, 80, 18).expect("resized render");
    let resized = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert!(
        !resized.contains("\x1b[3J"),
        "height changes must not purge native scrollback"
    );
    assert!(
        !resized.contains("committed message"),
        "height changes must not replay settled transcript"
    );

    backend.writer().clear();
    backend.set_size(60, 18);
    live.render_frame(&app, &mut backend, 60, 18)
        .expect("width-resized render");
    let resized = String::from_utf8(backend.writer().clone()).expect("utf8 terminal output");
    assert!(
        !resized.contains("\x1b[3J"),
        "width changes must not purge native scrollback"
    );
    assert!(
        !resized.contains("committed message"),
        "width changes must not replay settled transcript"
    );
}
