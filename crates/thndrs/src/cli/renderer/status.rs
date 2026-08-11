//! Pure projection for the configurable, single-row operational status.

use std::path::Path;

use unicode_width::UnicodeWidthStr;

use crate::app::{App, Entry, RunState, ToolStatus};
use crate::config::StatusSegment;
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Span};
use crate::utils;

const STATUS_INSET: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Truncation {
    None,
    End,
    Middle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    text: String,
    priority: u8,
    min_width: usize,
    truncation: Truncation,
    urgent: bool,
}

/// Render the configured operational status without wrapping.
pub(super) fn status_row(app: &App, width: usize, anchored: bool) -> Row {
    let palette = super::style::palette();
    let background = CellStyle::new();
    if width < 8 {
        return Row::blank(width, background);
    }

    let row_inset = width.min(2);
    let body_width = super::layout::content_width(width);
    let inset = STATUS_INSET
        .saturating_sub(row_inset)
        .min(body_width.saturating_sub(4) / 2);
    let available = body_width.saturating_sub(inset * 2);
    let config = &app.runtime.cli.status_line;
    let mut left = fields(app, &config.left, anchored);
    let mut right = fields(app, &config.right, anchored);
    fit(&mut left, &mut right, available);

    let left_text = join(&left);
    let right_text = join(&right);
    let gap = available
        .saturating_sub(UnicodeWidthStr::width(left_text.as_str()))
        .saturating_sub(UnicodeWidthStr::width(right_text.as_str()));
    let urgent = left.first().is_some_and(|field| field.urgent);
    let state_color = if urgent { palette.red } else { palette.teal };
    let mut spans = vec![Span::styled(" ".repeat(inset), background)];
    if !left_text.is_empty() {
        spans.push(Span::styled(left_text, CellStyle::new().fg(state_color).bold()));
    }
    spans.push(Span::styled(" ".repeat(gap), background));
    if !right_text.is_empty() {
        spans.push(Span::styled(right_text, CellStyle::new().fg(palette.overlay1)));
    }
    spans.push(Span::styled(" ".repeat(inset), background));
    Row::padded(spans, width, background)
}

fn fields(app: &App, configured: &[StatusSegment], anchored: bool) -> Vec<Field> {
    configured
        .iter()
        .filter_map(|segment| project(app, *segment, anchored))
        .collect()
}

fn project(app: &App, segment: StatusSegment, anchored: bool) -> Option<Field> {
    let (text, priority, min_width, truncation, urgent) = match segment {
        StatusSegment::RunState => {
            let text = if app.overlay.permission().is_some() {
                "Waiting for permission".to_string()
            } else if app.compaction_in_flight() {
                "Compacting".to_string()
            } else {
                match app.runtime.run_state {
                    RunState::Stopping => "Cancelling".to_string(),
                    RunState::Error(_) => "Failed".to_string(),
                    RunState::Idle => match app
                        .transcript
                        .entries
                        .iter()
                        .rev()
                        .find(|entry| !matches!(entry, Entry::Status { .. }))
                    {
                        Some(Entry::Agent { streaming: false, .. })
                        | Some(Entry::Tool { status: ToolStatus::Ok, .. }) => "Complete".to_string(),
                        Some(Entry::Tool { name, status: ToolStatus::Failed, .. }) => {
                            format!("Failed: tool {}", utils::truncate_ellipsis(name, 32))
                        }
                        Some(Entry::Error { .. }) => "Failed".to_string(),
                        _ => "Idle".to_string(),
                    },
                    RunState::Working => "Thinking".to_string(),
                }
            };
            let urgent = text.starts_with("Failed") || matches!(text.as_str(), "Waiting for permission" | "Cancelling");
            (text, 0, 4, Truncation::End, urgent)
        }
        StatusSegment::ActiveTool => {
            let tool = app.transcript.entries.iter().rev().find_map(|entry| match entry {
                Entry::Tool { name, status: ToolStatus::Running, .. } => Some(name.as_str()),
                _ => None,
            })?;
            (format!("tool {tool}"), 1, 8, Truncation::End, false)
        }
        StatusSegment::Authority => (
            app.runtime.cli.authority.display_label().to_string(),
            1,
            9,
            Truncation::None,
            false,
        ),
        StatusSegment::Route => {
            let route = if app.runtime.model.trim().is_empty() {
                "route unavailable".to_string()
            } else {
                app.runtime.model.clone()
            };
            (route, 3, 8, Truncation::Middle, false)
        }
        StatusSegment::Workspace => (
            display_name(&app.runtime.cwd).to_string(),
            4,
            6,
            Truncation::Middle,
            false,
        ),
        StatusSegment::Session => (format!("session {}", app.session.id), 4, 10, Truncation::Middle, false),
        StatusSegment::QueueCount => {
            let count = app.composer.queue.pending_count(crate::app::QueueTarget::Steering)
                + app.composer.queue.pending_count(crate::app::QueueTarget::FollowUp);
            (format!("queue {count}"), 2, 7, Truncation::None, false)
        }
        StatusSegment::AnchoredAway if anchored => ("↑ away".to_string(), 2, 6, Truncation::None, false),
        StatusSegment::AnchoredAway => return None,
        StatusSegment::ActiveChildren => ("children 0".to_string(), 5, 10, Truncation::None, false),
    };
    Some(Field { text, priority, min_width, truncation, urgent })
}

fn fit(left: &mut Vec<Field>, right: &mut Vec<Field>, available: usize) {
    while total_width(left, right) > available {
        let worst = left
            .iter()
            .enumerate()
            .map(|(index, field)| (field.priority, true, index))
            .chain(
                right
                    .iter()
                    .enumerate()
                    .map(|(index, field)| (field.priority, false, index)),
            )
            .filter(|(priority, _, _)| *priority > 1)
            .max();
        let Some((_, is_left, index)) = worst else { break };
        if is_left {
            left.remove(index);
        } else {
            right.remove(index);
        }
    }
    for index in (0..right.len()).rev() {
        let excess = total_width(left, right).saturating_sub(available);
        if excess == 0 {
            break;
        }
        let field = &mut right[index];
        if field.truncation == Truncation::None {
            continue;
        }
        let current = UnicodeWidthStr::width(field.text.as_str());
        let target = current.saturating_sub(excess).max(field.min_width);
        field.text = match field.truncation {
            Truncation::End => utils::truncate_ellipsis(&field.text, target),
            Truncation::Middle => utils::truncate_ellipsis_start(&field.text, target),
            Truncation::None => field.text.clone(),
        };
    }
    for index in (0..left.len()).rev() {
        let excess = total_width(left, right).saturating_sub(available);
        if excess == 0 {
            break;
        }
        let field = &mut left[index];
        if field.truncation == Truncation::None {
            continue;
        }
        let current = UnicodeWidthStr::width(field.text.as_str());
        let target = current.saturating_sub(excess).max(field.min_width);
        field.text = match field.truncation {
            Truncation::End => utils::truncate_ellipsis(&field.text, target),
            Truncation::Middle => utils::truncate_ellipsis_start(&field.text, target),
            Truncation::None => field.text.clone(),
        };
    }
    while total_width(left, right) > available {
        if !right.is_empty() {
            right.pop();
        } else if left.len() > 1 {
            left.pop();
        } else {
            break;
        }
    }
}

fn total_width(left: &[Field], right: &[Field]) -> usize {
    UnicodeWidthStr::width(join(left).as_str())
        + UnicodeWidthStr::width(join(right).as_str())
        + usize::from(!left.is_empty() && !right.is_empty())
}

fn join(fields: &[Field]) -> String {
    fields
        .iter()
        .map(|field| field.text.as_str())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn display_name(path: &Path) -> &str {
    path.file_name().and_then(|name| name.to_str()).unwrap_or("workspace")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;

    #[test]
    fn status_is_one_row_at_normal_narrow_and_tiny_widths() {
        let app = App::from_cli(&Cli::default());
        for width in [80, 28, 8] {
            let row = status_row(&app, width, true);
            assert_eq!(row.width, width);
            assert!(!row.text().contains('\n'));
        }
    }

    #[test]
    fn permission_and_anchor_are_projected_semantically() {
        let mut app = App::from_cli(&Cli::default());
        let text = status_row(&app, 80, true).text();
        assert!(text.contains("Idle"));
        assert!(text.contains("↑ away"));
        assert!(text.contains("Editable"));

        app.runtime.cli.authority = crate::tools::ToolAuthority::ReadOnly;
        assert!(status_row(&app, 80, true).text().contains("Read-only"));
    }

    #[test]
    fn status_is_detached_from_the_composer_and_uses_symmetric_insets() {
        let app = App::from_cli(&Cli::default());
        let row = status_row(&app, 80, false);
        let text = row.text();

        assert!(
            row.spans
                .iter()
                .all(|span| span.style.bg == crate::renderer::style::Color::Reset)
        );
        assert_eq!(row.text_width(), 80);
        assert!(text.starts_with("    Idle"));
        assert!(text.ends_with("queue 0    "));
    }

    #[test]
    fn run_transitions_and_active_tool_are_projected() {
        let mut app = App::from_cli(&Cli::default());
        assert!(status_row(&app, 80, false).text().contains("Idle"));

        app.runtime.run_state = RunState::Working;
        app.transcript.entries.push(Entry::Tool {
            name: "search_text".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Running,
            output: Vec::new(),
        });
        let working = status_row(&app, 80, false).text();
        assert!(working.contains("Thinking"));
        assert!(working.contains("tool search_text"));

        app.runtime.run_state = RunState::Stopping;
        assert!(status_row(&app, 80, false).text().contains("Cancelling"));
        app.runtime.run_state = RunState::Error("provider detail".to_string());
        let failed = status_row(&app, 80, false).text();
        assert!(failed.contains("Failed"));
        assert!(!failed.contains("provider detail"));
    }

    #[test]
    fn failed_tool_status_names_the_operation_without_secret_context() {
        let mut app = App::from_cli(&Cli::default());
        app.transcript.entries.push(Entry::Tool {
            name: "run_shell".to_string(),
            arguments: "token=secret-argument".to_string(),
            status: ToolStatus::Failed,
            output: vec!["secret-output".to_string()],
        });

        let text = status_row(&app, 80, false).text();
        assert!(text.contains("Failed: tool run_shell"));
        assert!(!text.contains("secret-argument"));
        assert!(!text.contains("secret-output"));
    }
}
