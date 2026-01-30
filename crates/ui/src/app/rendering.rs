use super::App;
use crate::components::{
    Footer, FuzzyFinderComponent, Header, Inspector, MemoryHitsPanel, Sidebar, TeachingHintPopup,
    Transcript as TranscriptComponent, WelcomeView,
};
use crate::layout::{LayoutMode, TuiLayout};
use crate::state::MainView;
use crate::theme::Theme;
use crate::transcript::RenderOptions;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Result;

pub fn draw(app: &mut App, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    if app.state.is_test_mode() {
        capture_tui_snapshot(app, "draw", "TUI state update");
    }

    if app.state.is_generating() || app.state.approval_ui.pending_approval.is_some() {
        app.state.advance_animation_frame();
    }
    if app.state.ui.sidebar_animation.is_some() {
        app.state.ui.advance_sidebar_animation();
    }

    terminal.draw(|frame| {
        let size = frame.area();
        let theme = Theme::palette(app.state.theme_variant());
        let content_area = inset_area(size, 1, 1, 1, 1);

        frame.render_widget(
            ratatui::widgets::Block::default().style(ratatui::style::Style::default().bg(theme.bg)),
            size,
        );

        if app.state.is_first_session() {
            let welcome = WelcomeView::new(&app.state, content_area);
            welcome.render(frame);
            if app.state.is_fuzzy_finder_active() {
                let fuzzy_finder = FuzzyFinderComponent::new(&app.state);
                fuzzy_finder.render(frame);
            }

            return;
        }

        let layout = if matches!(app.state.ui.active_view, MainView::Inspector) {
            TuiLayout::calculate_inspector(content_area)
        } else {
            TuiLayout::calculate(
                content_area,
                app.state.ui.sidebar_visible,
                app.state.ui.sidebar_width_override(),
            )
        };
        let header = Header::with_theme(&app.state.session_header, app.state.theme_variant());
        header.render(frame, layout.header);

        if matches!(app.state.ui.active_view, MainView::Inspector) {
            let inspector = Inspector::new(&app.state);
            inspector.render(
                frame,
                layout.evidence_list.unwrap_or_default(),
                layout.evidence_detail.unwrap_or_default(),
            );
        } else {
            let theme = Theme::palette(app.state.theme_variant());
            let options = RenderOptions {
                centered: false,
                max_bubble_width: if layout.mode == LayoutMode::Full { None } else { Some(60) },
                animation_frame: app.state.ui.animation_frame,
            };
            let ellipsis = app.state.streaming_ellipsis();
            let transcript_component = if app.state.is_generating() {
                TranscriptComponent::with_streaming_ellipsis(
                    &app.transcript,
                    app.state.ui.scroll_vertical,
                    ellipsis,
                    theme,
                    options,
                )
            } else {
                TranscriptComponent::with_vertical_scroll(&app.transcript, app.state.ui.scroll_vertical, theme, options)
            };
            transcript_component.render(frame, layout.transcript);

            if let Some(sidebar_area) = layout.sidebar {
                let sidebar = Sidebar::new(&app.state);
                sidebar.render(frame, sidebar_area);
                render_sidebar_divider(app, frame, layout.clone(), sidebar_area);
            }
        }

        let footer = Footer::new(&app.state);
        footer.render(frame, layout.footer);

        if app.state.is_fuzzy_finder_active() {
            let fuzzy_finder = FuzzyFinderComponent::new(&app.state);
            fuzzy_finder.render(frame);
        }

        if app.state.memory_hits.is_visible() {
            let panel_area = ratatui::layout::Rect {
                x: size.width / 4,
                y: size.height / 8,
                width: size.width / 2,
                height: size.height * 3 / 4,
            };
            let memory_panel = MemoryHitsPanel::new(&app.state.memory_hits);
            memory_panel.render(frame, panel_area);
        }

        if let Some(ref hint) = app.state.approval_ui.pending_hint {
            let theme = Theme::palette(app.state.theme_variant());
            let hint_popup = TeachingHintPopup::new(hint, theme);
            hint_popup.render(frame, content_area);
        }
    })?;

    Ok(())
}

fn capture_tui_snapshot(app: &mut App, event_type: &str, description: &str) {
    if let Some(ref mut capture) = app.snapshot_capture {
        let snapshot_content = format!(
            "Active View: {:?}\n\
             Theme Variant: {:?}\n\
             Is Generating: {}\n\
             Is Paused: {}\n\
             Is First Session: {}\n\
             Sidebar Visible: {}\n\
             Inspector Visible: {}\n\
             Fuzzy Finder Active: {}\n\
             Memory Hits Visible: {}\n\
             Approval Pending: {}\n",
            app.state.ui.active_view,
            app.state.ui.theme_variant,
            app.state.is_generating(),
            app.state.is_paused(),
            app.state.is_first_session(),
            app.state.sidebar_visible(),
            matches!(app.state.ui.active_view, MainView::Inspector),
            app.state.is_fuzzy_finder_active(),
            app.state.memory_hits.is_visible(),
            app.state.pending_approval().is_some()
        );

        let _ = capture.capture(&snapshot_content, event_type, description);
    }
}

fn render_sidebar_divider(
    app: &App, frame: &mut ratatui::Frame<'_>, layout: TuiLayout, sidebar: ratatui::layout::Rect,
) {
    let theme = Theme::palette(app.state.theme_variant());
    let x = sidebar.x + sidebar.width;
    let top = layout.header.y;
    let bottom = layout.footer.y;
    if x >= layout.footer.x + layout.footer.width || bottom <= top {
        return;
    }

    for y in top..bottom {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(ratatui::text::Line::from(vec![ratatui::text::Span::styled(
                "│",
                ratatui::style::Style::default().fg(theme.border).bg(theme.bg),
            )])),
            ratatui::layout::Rect::new(x, y, 1, 1),
        );
    }
}

fn inset_area(area: ratatui::layout::Rect, left: u16, right: u16, top: u16, bottom: u16) -> ratatui::layout::Rect {
    let width = area.width.saturating_sub(left + right);
    let height = area.height.saturating_sub(top + bottom);
    if width == 0 || height == 0 {
        return area;
    }
    ratatui::layout::Rect { x: area.x + left, y: area.y + top, width, height }
}
