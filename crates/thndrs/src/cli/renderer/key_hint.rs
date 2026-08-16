//! Compact contextual key hints rendered with filled keycaps.

use crate::renderer::row::Row;
use crate::renderer::style::{self, CellStyle, Span};
use crate::utils;

const ROW_INSET: usize = 2;
const KEY_HORIZONTAL_PADDING: usize = 1;
const KEY_ACTION_GAP: usize = 1;
const HINT_GAP: usize = 2;

/// One immediate contextual action and the key that invokes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyHint {
    /// Key label shown inside the filled keycap.
    pub key: String,
    /// Immediate action described beside the keycap.
    pub action: String,
}

impl KeyHint {
    /// Create a contextual key hint.
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self { key: key.into(), action: action.into() }
    }
}

/// Render as many complete hints as fit, truncating the first action only when
/// a narrow surface cannot display it in full.
pub fn render_key_hints(hints: &[KeyHint], width: usize) -> Row {
    let p = style::palette();
    let normal = CellStyle::new();
    let key_style = CellStyle::new().fg(p.primary).bg(p.selection).bold();
    let action_style = CellStyle::new().fg(p.secondary);
    let frame_padding = width.min(ROW_INSET * 2);
    let mut remaining = width.saturating_sub(frame_padding);
    let mut spans = Vec::new();

    for (index, hint) in hints.iter().enumerate() {
        let gap = usize::from(index > 0) * HINT_GAP;
        let key_width = utils::text_width(&hint.key) + KEY_HORIZONTAL_PADDING * 2;
        let action_width = utils::text_width(&hint.action);
        let needed = gap + key_width + KEY_ACTION_GAP + action_width;

        if needed > remaining && index > 0 {
            break;
        }
        if gap > 0 {
            spans.push(Span::styled(" ".repeat(gap), normal));
            remaining = remaining.saturating_sub(gap);
        }
        if remaining < KEY_HORIZONTAL_PADDING * 2 + 1 {
            break;
        }

        let key_budget = remaining.saturating_sub(KEY_HORIZONTAL_PADDING * 2);
        let key = utils::truncate_ellipsis(&hint.key, key_budget.max(1));
        let rendered_key_width = utils::text_width(&key) + KEY_HORIZONTAL_PADDING * 2;
        spans.push(Span::styled(format!(" {key} "), key_style));
        remaining = remaining.saturating_sub(rendered_key_width);

        if remaining > KEY_ACTION_GAP {
            spans.push(Span::styled(" ".to_string(), normal));
            remaining -= KEY_ACTION_GAP;
            let action = utils::truncate_ellipsis(&hint.action, remaining);
            remaining = remaining.saturating_sub(utils::text_width(&action));
            if !action.is_empty() {
                spans.push(Span::styled(action, action_style));
            }
        }
    }

    Row::padded(spans, width, normal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::style::Color;

    #[test]
    fn one_hint_uses_a_filled_keycap_and_muted_action() {
        let row = render_key_hints(&[KeyHint::new("↑↓", "navigate")], 30);
        let key = row.spans.iter().find(|span| span.text.contains("↑↓")).expect("keycap");
        let action = row
            .spans
            .iter()
            .find(|span| span.text.contains("navigate"))
            .expect("action");

        assert_ne!(key.style.bg, Color::Reset);
        assert_eq!(action.style.bg, Color::Reset);
        assert_eq!(action.style.fg, style::palette().secondary);
    }

    #[test]
    fn multiple_hints_include_long_keys_and_larger_inter_action_gaps() {
        let row = render_key_hints(&[KeyHint::new("↑↓", "navigate"), KeyHint::new("enter", "select")], 50);
        let text = row.text();

        assert!(text.contains(" ↑↓  navigate   enter  select"));
        assert_eq!(row.spans.iter().filter(|span| span.style.bg != Color::Reset).count(), 2);
    }

    #[test]
    fn constrained_width_keeps_the_first_keycap_and_truncates_its_action() {
        let row = render_key_hints(
            &[KeyHint::new("enter", "select a model"), KeyHint::new("esc", "close")],
            14,
        );

        assert!(row.text().contains("enter"));
        assert!(!row.text().contains("esc"));
        assert_eq!(row.text_width(), 14);
    }
}
