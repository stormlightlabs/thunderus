//! iocraft adapter for bounded surface rendering.
//!
//! This module is the only place the direct TUI renderer calls iocraft. It
//! renders declarative iocraft elements into an inspectable canvas, then
//! converts the canvas text back into the existing [`Row`] contract. It does not
//! call iocraft render loops, fullscreen mode, stdout, or stderr.

use iocraft::prelude::*;

use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color as RendererColor, Span};

/// Render a bounded transcript/detail lens with iocraft.
pub fn transcript_lens_rows(title: &str, body: &[String], width: usize, height: usize) -> Vec<Row> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let content = body.join("\n");
    let view_width = width.min(u32::MAX as usize) as u32;
    let view_height = height.min(u32::MAX as usize) as u32;
    let body_height = height.saturating_sub(1).min(u32::MAX as usize) as u32;
    let mut element = element! {
        View(
            flex_direction: FlexDirection::Column,
            width: view_width,
            height: view_height,
        ) {
            Text(content: title.to_string(), weight: Weight::Bold)
            View(
                height: body_height,
            ) {
                ScrollView {
                    Text(content: content)
                }
            }
        }
    };

    let canvas = element.render(Some(width));
    canvas_text_to_rows(&canvas.get_text(0, 0, canvas.width(), canvas.height()), width)
}

fn canvas_text_to_rows(text: &str, width: usize) -> Vec<Row> {
    let style = CellStyle::new().fg(RendererColor::Reset).bg(RendererColor::Reset);
    text.lines()
        .map(|line| Row::padded(vec![Span::styled(line.to_string(), style)], width, style))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_lens_uses_iocraft_canvas_without_render_loop() {
        let rows = transcript_lens_rows(
            "details",
            &["one".to_string(), "two".to_string(), "three".to_string()],
            24,
            3,
        );
        let text = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(
            text.contains("details"),
            "title should render through iocraft canvas:\n{text}"
        );
        assert!(
            text.contains("one"),
            "body should render through iocraft canvas:\n{text}"
        );
        assert!(
            rows.iter().all(|row| row.width == 24),
            "converted rows should preserve the existing row contract"
        );
    }
}
