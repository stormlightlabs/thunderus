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
    live.render_frame(&app, &mut backend, 60, 18).expect("width-resized render");
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
