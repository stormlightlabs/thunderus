//! View geometry computation and rendering.
//!
//! Layout follows the Herdr lesson: compute all rectangles in [`compute_view`]
//! before drawing, then [`render`] reads the stored [`ViewState`].
//!
//! This keeps draw code stateless and makes layout testable with plain [`Rect`]
//! assertions.

use crate::app::App;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

/// Fixed sidebar width in columns.
pub const SIDEBAR_WIDTH: u16 = 22;

/// Below this total width the sidebar is hidden so prompt/status text
/// does not wrap or overlap.
pub const SIDEBAR_HIDE_THRESHOLD: u16 = 50;

/// Prompt region height: one divider line plus one input line.
const PROMPT_HEIGHT: u16 = 2;

/// Footer region height: one status line.
const FOOTER_HEIGHT: u16 = 1;

/// Precomputed layout rectangles plus display flags.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ViewState {
    /// Full screen rect handed to `compute_view`.
    pub area: Rect,
    /// Sidebar rect (zero-sized when hidden).
    pub sidebar: Rect,
    /// Transcript rect.
    pub transcript: Rect,
    /// Prompt rect.
    pub prompt: Rect,
    /// Footer rect.
    pub footer: Rect,
    /// Whether the sidebar is visible at this width.
    pub sidebar_visible: bool,
}

/// Compute view geometry from a terminal rect. Pure function.
///
/// - Vertical: body (fills), prompt, footer.
/// - Horizontal within body: sidebar, transcript.
pub fn compute_view(area: Rect) -> ViewState {
    let sidebar_visible = area.width >= SIDEBAR_HIDE_THRESHOLD;
    let sidebar_width = if sidebar_visible { SIDEBAR_WIDTH } else { 0 };

    let [body, prompt, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(PROMPT_HEIGHT),
        Constraint::Length(FOOTER_HEIGHT),
    ])
    .areas(area);

    let [sidebar, transcript] =
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Fill(1)]).areas(body);

    ViewState { area, sidebar, transcript, prompt, footer, sidebar_visible }
}

/// Render the whole screen from `app` state into `frame`.
pub fn render(frame: &mut Frame, app: &App) {
    let view = compute_view(frame.area());
    render_sidebar(frame, app, view.sidebar);
    render_transcript(frame, app, view.transcript);
    render_prompt(frame, app, view.prompt);
    render_footer(frame, app, view.footer);
}

/// Sessions list on top, status at the bottom.
fn render_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = Block::bordered().title("thndrs");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let [sessions_area, status_area] = Layout::vertical([Constraint::Fill(1), Constraint::Length(2)]).areas(inner);
    let items: Vec<ListItem> = app
        .sidebar
        .sessions
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let prefix = if app.sidebar.active == Some(i) { "> " } else { "  " };
            ListItem::new(format!("{prefix}{name}"))
        })
        .collect();

    let list = List::new(items)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::new().title(Line::from("Sessions")));

    frame.render_widget(list, sessions_area);

    let status_text = Line::from(format!("Status: {}", app.run_state.label()));

    frame.render_widget(
        Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray)),
        status_area,
    );
}

/// Render newest entries fitting the viewport.
fn render_transcript(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = Block::bordered().title("Transcript");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if app.transcript.is_empty() {
        let placeholder = Paragraph::new("No messages yet. Type a prompt below.");
        frame.render_widget(placeholder, inner);
        return;
    }

    let lines: Vec<Line> = app.transcript.iter().map(|e| e.to_line()).collect();
    let available = inner.height as usize;
    let start = lines.len().saturating_sub(available);
    let visible: Vec<Line> = lines[start..].to_vec();
    frame.render_widget(Paragraph::new(Text::from(visible)), inner);
}

fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let prompt_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::raw(app.input.as_str()),
    ]);

    frame.render_widget(Paragraph::new(prompt_line), inner);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let cwd_display = app.cwd.display().to_string();
    let footer = Line::from(vec![
        Span::styled(
            format!("model: {} ", app.model),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("cwd: {}", cwd_display), Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(footer), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Entry, RunState, ToolStatus};
    use crate::cli::Cli;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app() -> App {
        App::from_cli(&Cli::default())
    }

    #[test]
    fn compute_view_normal_width_shows_sidebar() {
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(area);
        assert!(view.sidebar_visible);
        assert_eq!(view.sidebar.width, SIDEBAR_WIDTH);
        assert_eq!(view.sidebar.height, 24 - PROMPT_HEIGHT - FOOTER_HEIGHT);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_eq!(view.transcript.width, 80 - SIDEBAR_WIDTH);
        assert!(view.transcript.height > 0);
    }

    #[test]
    fn compute_view_narrow_width_hides_sidebar() {
        let area = Rect::new(0, 0, 40, 24);
        let view = compute_view(area);
        assert!(!view.sidebar_visible);
        assert_eq!(view.sidebar.width, 0);
        assert_eq!(view.transcript.width, 40);
    }

    #[test]
    fn compute_view_tiny_terminal_does_not_panic() {
        let area = Rect::new(0, 0, 20, 5);
        let view = compute_view(area);
        assert!(!view.sidebar_visible);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
    }

    #[test]
    fn compute_view_at_threshold_hides_sidebar() {
        let area = Rect::new(0, 0, SIDEBAR_HIDE_THRESHOLD - 1, 24);
        assert!(!compute_view(area).sidebar_visible);

        let area = Rect::new(0, 0, SIDEBAR_HIDE_THRESHOLD, 24);
        assert!(compute_view(area).sidebar_visible);
    }

    #[test]
    fn compute_view_normal_rect_full_layout() {
        let area = Rect::new(0, 0, 80, 24);
        let view = compute_view(area);
        assert!(view.sidebar_visible);

        assert_eq!(view.sidebar.x, 0);
        assert_eq!(view.sidebar.width, SIDEBAR_WIDTH);
        assert_eq!(view.transcript.x, SIDEBAR_WIDTH);
        assert_eq!(view.transcript.width, 80 - SIDEBAR_WIDTH);

        assert!(view.transcript.y + view.transcript.height <= view.prompt.y);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.prompt.y + view.prompt.height, view.footer.y);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_eq!(view.footer.y + view.footer.height, area.height);
    }

    #[test]
    fn compute_view_narrow_rect_hides_sidebar_full_width() {
        let area = Rect::new(0, 0, 40, 24);
        let view = compute_view(area);
        assert!(!view.sidebar_visible);
        assert_eq!(view.sidebar.width, 0);
        assert_eq!(view.transcript.x, 0);
        assert_eq!(view.transcript.width, 40);
    }

    #[test]
    fn compute_view_tiny_rect_reserves_prompt_and_footer() {
        let area = Rect::new(0, 0, 20, 5);
        let view = compute_view(area);
        assert!(!view.sidebar_visible);
        assert_eq!(view.prompt.height, PROMPT_HEIGHT);
        assert_eq!(view.footer.height, FOOTER_HEIGHT);
        assert_no_overlap(&view);
    }

    #[test]
    fn compute_view_extreme_tiny_height() {
        let area = Rect::new(0, 0, 30, PROMPT_HEIGHT + FOOTER_HEIGHT);
        let view = compute_view(area);
        assert_no_overlap(&view);
        assert_eq!(view.prompt.height + view.footer.height, area.height);
    }

    #[test]
    fn compute_view_below_prompt_plus_footer() {
        let area = Rect::new(0, 0, 30, 1);
        let view = compute_view(area);
        assert_no_overlap(&view);
    }

    #[test]
    fn compute_view_single_column_width() {
        let area = Rect::new(0, 0, 1, 24);
        let view = compute_view(area);
        assert!(!view.sidebar_visible);
        assert_no_overlap(&view);
    }

    /// Assert that prompt and footer rects do not overlap each other or
    /// the transcript, and that no rect extends past the area boundary.
    fn assert_no_overlap(view: &ViewState) {
        assert!(view.sidebar.right() <= view.area.right());
        assert!(view.transcript.right() <= view.area.right());
        assert!(view.prompt.right() <= view.area.right());
        assert!(view.footer.right() <= view.area.right());
        assert!(view.sidebar.bottom() <= view.area.bottom());
        assert!(view.transcript.bottom() <= view.area.bottom());
        assert!(view.prompt.bottom() <= view.area.bottom());
        assert!(view.footer.bottom() <= view.area.bottom());

        assert!(
            view.prompt.y >= view.transcript.y + view.transcript.height,
            "prompt overlaps transcript"
        );

        assert!(
            view.footer.y >= view.prompt.y + view.prompt.height,
            "footer overlaps prompt"
        );
    }

    #[test]
    fn empty_shell_snapshot_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        let app = app();
        terminal.draw(|f| render(f, &app)).expect("draw empty shell");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn submitted_prompt_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw submitted prompt");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn streaming_assistant_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Assistant { text: String::from("This is a fake streaming res..."), streaming: true });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw streaming assistant");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn reasoning_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript.push(Entry::Reasoning {
            text: String::from("Let me think about this... The repo is a Rust + Ratatui harness."),
            streaming: true,
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw reasoning");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn running_tool_snapshot_80x24() {
        let mut app = app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Reasoning { text: String::from("Let me read the Cargo.toml first."), streaming: false });

        app.transcript.push(Entry::Tool {
            name: String::from("read_file"),
            status: ToolStatus::Running,
            output: vec![String::from("Cargo.toml: 47 lines")],
        });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw running tool");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn finished_state_snapshot_80x24() {
        let mut app = app();
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript.push(Entry::Reasoning {
            text: String::from("Let me think about this... The repo is a Rust + Ratatui harness."),
            streaming: false,
        });

        app.transcript.push(Entry::Tool {
            name: String::from("read_file"),
            status: ToolStatus::Ok,
            output: vec![String::from("Cargo.toml: 47 lines")],
        });

        app.transcript
            .push(Entry::Assistant { text: String::from("This is a fake streaming response."), streaming: false });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw finished state");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn normal_width_layout_snapshot_80x24() {
        let mut app = app();
        app.input = String::from("hello");
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Assistant { text: String::from("response"), streaming: false });

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw normal width layout");
        insta::assert_snapshot!(terminal.backend().to_string());
    }

    #[test]
    fn narrow_width_layout_snapshot_40x24() {
        let mut app = app();
        app.input = String::from("hello");
        app.transcript
            .push(Entry::User { text: String::from("explain this repo") });

        app.transcript
            .push(Entry::Assistant { text: String::from("response"), streaming: false });

        let backend = TestBackend::new(40, 24);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|f| render(f, &app)).expect("draw narrow width layout");
        insta::assert_snapshot!(terminal.backend().to_string());
    }
}
