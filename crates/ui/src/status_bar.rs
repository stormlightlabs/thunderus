use crate::components;
use crate::{App, ScreenMode, colors};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(crate) fn draw_status_bar(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let left = format!(" {} ", screen_label(app.screen_mode));
    let right = if app.screen_mode == ScreenMode::Chat {
        app.chat
            .last_model
            .as_deref()
            .map(|model| format!(" model: {model} "))
            .unwrap_or_else(|| format!(" {} ", components::app_version_string()))
    } else {
        format!(" {} ", components::app_version_string())
    };

    let total_width = area.width as usize;
    let used_width = left.chars().count() + right.chars().count();
    let spacer = " ".repeat(total_width.saturating_sub(used_width).max(1));

    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::raw(spacer),
        Span::styled(right, Style::default().fg(colors::TEXT_MUTED)),
    ]);

    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(colors::BG_SECONDARY)),
        area,
    );
}

fn screen_label(mode: ScreenMode) -> &'static str {
    match mode {
        ScreenMode::Welcome => "WELCOME",
        ScreenMode::Chat => "CHAT",
        ScreenMode::Files => "FILES",
        ScreenMode::Settings => "SETTINGS",
        ScreenMode::Help => "HELP",
    }
}
