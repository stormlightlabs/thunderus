use crate::state::{AppState, ConfigEditorField, ConfigEditorState};
use crate::theme::{Theme, ThemePalette};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Config editor UI component
pub struct ConfigEditorComponent<'a> {
    state: &'a AppState,
}

impl<'a> ConfigEditorComponent<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render the config editor as a modal overlay
    ///
    /// Form Content layout: 5 Fields, Spacer, Hints
    pub fn render(&self, frame: &mut Frame<'_>) {
        let Some(editor) = &self.state.config_editor else {
            return;
        };

        let size = frame.area();
        let theme = Theme::palette(self.state.theme_variant());
        let overlay_width = 50.min(size.width.saturating_sub(4));
        let overlay_height = 14.min(size.height.saturating_sub(4));
        let overlay = Rect {
            x: (size.width - overlay_width) / 2,
            y: (size.height - overlay_height) / 2,
            width: overlay_width,
            height: overlay_height,
        };

        frame.render_widget(Clear, overlay);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.blue))
            .title(Span::styled(
                " Config Editor ",
                Style::default().fg(theme.blue).add_modifier(Modifier::BOLD),
            ))
            .bg(theme.panel_bg);

        let inner = block.inner(overlay);
        frame.render_widget(block, overlay);

        let chunks = Layout::new(
            Direction::Vertical,
            [
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(2),
            ],
        )
        .split(inner);

        for (idx, field) in ConfigEditorField::ALL.iter().enumerate() {
            if idx < 5 {
                self.render_field(frame, chunks[idx], editor, *field, idx, theme);
            }
        }

        self.render_hints(frame, chunks[6], editor, theme);
    }

    fn render_field(
        &self, frame: &mut Frame<'_>, area: Rect, editor: &ConfigEditorState, field: ConfigEditorField,
        field_idx: usize, theme: ThemePalette,
    ) {
        let is_focused = editor.focused_field_index == field_idx;
        let is_editable = field.is_editable();

        let label_style = if is_focused {
            Style::default().fg(theme.blue).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };

        let value_style = if is_focused {
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
        } else if is_editable {
            Style::default().fg(theme.fg)
        } else {
            Style::default().fg(theme.muted)
        };

        let focus_indicator = if is_focused { "> " } else { "  " };
        let edit_hint = if is_focused && is_editable { " [Enter to toggle]" } else { "" };

        let line = Line::from(vec![
            Span::styled(focus_indicator, label_style),
            Span::styled(format!("{}: ", field.label()), label_style),
            Span::styled(editor.field_value(field), value_style),
            Span::styled(edit_hint, Style::default().fg(theme.muted)),
        ]);

        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_hints(&self, frame: &mut Frame<'_>, area: Rect, editor: &ConfigEditorState, theme: ThemePalette) {
        let save_hint = if editor.has_changes { "Ctrl+S: Save" } else { "" };
        let hints = format!("Tab: Next  Shift+Tab: Prev  Esc: Cancel  {}", save_hint);
        let paragraph = Paragraph::new(Line::from(Span::styled(hints, Style::default().fg(theme.muted))))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_editor_component_new() {
        let state = AppState::default();
        let component = ConfigEditorComponent::new(&state);
        assert!(component.state.config_editor.is_none());
    }
}
