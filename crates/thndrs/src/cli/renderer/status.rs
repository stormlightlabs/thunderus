//! Pure projection for the configurable, single-row operational status.

use std::path::Path;

use unicode_width::UnicodeWidthStr;

use crate::app::{App, Entry, StatusToastKind, ToolStatus};
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
    Route,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    text: String,
    priority: u8,
    min_width: usize,
    truncation: Truncation,
    urgent: bool,
    required: bool,
}

/// Render the configured operational status without wrapping.
pub(super) fn status_row(app: &App, width: usize, anchored: bool) -> Row {
    let palette = super::style::palette();
    let background = CellStyle::new();
    if let Some(toast) = &app.runtime.status_toast {
        let color = match toast.kind {
            StatusToastKind::Success => palette.success,
            StatusToastKind::Error => palette.failure,
        };
        let body_width = super::layout::content_width(width);
        let inset = width.min(2);
        let text = utils::truncate_ellipsis(&toast.text, body_width.saturating_sub(inset).max(1));
        return Row::padded(
            vec![
                Span::styled(" ".repeat(inset), background),
                Span::styled(text, CellStyle::new().fg(color).bold()),
            ],
            width,
            background,
        );
    }
    if width < 8 {
        let state = operational_state(app);
        let available = width.saturating_sub(width.min(2));
        return Row::padded(
            vec![Span::styled(
                utils::truncate_ellipsis(&state, available),
                CellStyle::new()
                    .fg(super::style::status_color(&app.status_label()))
                    .bold(),
            )],
            width,
            background,
        );
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
    if !config.left.contains(&StatusSegment::Route) && !config.right.contains(&StatusSegment::Route) {
        right.extend(project(app, StatusSegment::Route, anchored));
    }
    fit(&mut left, &mut right, available);

    let left_text = join(&left);
    let right_text = join(&right);
    let gap = available
        .saturating_sub(UnicodeWidthStr::width(left_text.as_str()))
        .saturating_sub(UnicodeWidthStr::width(right_text.as_str()));
    let urgent = left.first().is_some_and(|field| field.urgent);
    let state_color = if urgent { palette.failure } else { super::style::status_color(&app.status_label()) };
    let mut spans = vec![Span::styled(" ".repeat(inset), background)];
    if !left_text.is_empty() {
        spans.push(Span::styled(left_text, CellStyle::new().fg(state_color).bold()));
    }
    spans.push(Span::styled(" ".repeat(gap), background));
    if !right_text.is_empty() {
        spans.push(Span::styled(right_text, CellStyle::new().fg(palette.secondary)));
    }
    spans.push(Span::styled(" ".repeat(inset), background));
    Row::padded(spans, width, background)
}

fn fields(app: &App, configured: &[StatusSegment], anchored: bool) -> Vec<Field> {
    let run_state_names_active_tool =
        configured.contains(&StatusSegment::RunState) && app.status_label().starts_with("Running ");
    configured
        .iter()
        .filter(|segment| !(run_state_names_active_tool && **segment == StatusSegment::ActiveTool))
        .filter_map(|segment| project(app, *segment, anchored))
        .collect()
}

fn project(app: &App, segment: StatusSegment, anchored: bool) -> Option<Field> {
    let (text, priority, min_width, truncation, urgent) = match segment {
        StatusSegment::RunState => {
            let text = operational_state(app);
            let urgent = text.contains("Failed") || text.contains("Waiting for permission");
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
        StatusSegment::Route => (active_model(app), 0, 1, Truncation::Route, false),
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
        StatusSegment::ContextRemaining => {
            let projection = app.transcript.context_ledger.as_ref()?.projection();
            let remaining = projection.remaining_percent?;
            (format!("{remaining}% ctx left"), 5, 12, Truncation::None, false)
        }
    };
    Some(Field { text, priority, min_width, truncation, urgent, required: segment == StatusSegment::RunState })
}

fn operational_state(app: &App) -> String {
    let label = if app.overlay.permission().is_some() {
        "Waiting for permission".to_string()
    } else {
        app.status_label()
    };
    let icon = super::style::status_icon(
        &label,
        super::style::spinner_tick(app.runtime.ui_tick, app.runtime.cli.tick_rate_ms),
    );
    format!("{icon} {label}")
}

fn active_model(app: &App) -> String {
    let model = model_name(app);
    format!("{model} · {}", app.runtime.cli.reasoning_effort.label())
}

fn model_name(app: &App) -> &str {
    if app.runtime.model.trim().is_empty() { "model unavailable" } else { &app.runtime.model }
}

fn truncate_field(field: &Field, target: usize) -> String {
    match field.truncation {
        Truncation::End => utils::truncate_ellipsis(&field.text, target),
        Truncation::Middle => utils::truncate_ellipsis_start(&field.text, target),
        Truncation::Route => {
            let model = field
                .text
                .split_once(" · ")
                .map_or(field.text.as_str(), |(model, _)| model);
            utils::truncate_ellipsis_start(model, target)
        }
        Truncation::None => field.text.clone(),
    }
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
            .filter(|(_, is_left, index)| {
                let field = if *is_left { &left[*index] } else { &right[*index] };
                field.priority > 1 && !field.required
            })
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
        if field.required || field.truncation == Truncation::None {
            continue;
        }
        let current = UnicodeWidthStr::width(field.text.as_str());
        let target = current.saturating_sub(excess).max(field.min_width);
        field.text = truncate_field(field, target);
    }
    for index in (0..left.len()).rev() {
        let excess = total_width(left, right).saturating_sub(available);
        if excess == 0 {
            break;
        }
        let field = &mut left[index];
        if field.required || field.truncation == Truncation::None {
            continue;
        }
        let current = UnicodeWidthStr::width(field.text.as_str());
        let target = current.saturating_sub(excess).max(field.min_width);
        field.text = truncate_field(field, target);
    }
    while total_width(left, right) > available {
        let optional = left
            .iter()
            .enumerate()
            .map(|(index, field)| (field.priority, true, index, field.required))
            .chain(
                right
                    .iter()
                    .enumerate()
                    .map(|(index, field)| (field.priority, false, index, field.required)),
            )
            .filter(|(_, _, _, required)| !required)
            .max();
        let Some((_, is_left, index, _)) = optional else { break };
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
        if !field.required || field.truncation == Truncation::None {
            continue;
        }
        let current = UnicodeWidthStr::width(field.text.as_str());
        let target = current.saturating_sub(excess).max(field.min_width);
        field.text = truncate_field(field, target);
    }
    for index in (0..left.len()).rev() {
        let excess = total_width(left, right).saturating_sub(available);
        if excess == 0 {
            break;
        }
        let field = &mut left[index];
        if !field.required || field.truncation == Truncation::None {
            continue;
        }
        let current = UnicodeWidthStr::width(field.text.as_str());
        let target = current.saturating_sub(excess).max(field.min_width);
        field.text = truncate_field(field, target);
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
    use crate::cli::app::RunState;

    #[test]
    fn status_is_one_row_at_normal_narrow_and_tiny_widths() {
        let mut app = App::from_cli(&Cli::default());
        app.runtime.model = "fake-agent".to_string();
        for width in [80, 28, 8, 4] {
            let row = status_row(&app, width, true);
            assert_eq!(row.width, width);
            assert!(!row.text().contains('\n'));
            assert!(!row.text().trim().is_empty());
            if width == 28 {
                assert!(row.text().contains("fake-agent"));
            } else if width == 4 {
                assert_eq!(row.text(), "  ✓…");
            }
        }
    }

    #[test]
    fn transient_toast_replaces_operational_status() {
        let mut app = App::from_cli(&Cli::default());
        app.show_status_toast("Copied transcript selection", StatusToastKind::Success);

        let row = status_row(&app, 40, false);
        assert!(row.text().contains("Copied transcript selection"));
        assert!(!row.text().contains("queue 0"));
        assert!(
            row.spans
                .iter()
                .any(|span| span.style.fg == super::super::style::palette().success)
        );
    }

    #[test]
    fn active_model_is_default_and_movable() {
        let mut app = App::from_cli(&Cli::default());
        app.runtime.model = "opencode-go/glm-5.2".to_string();
        app.runtime.cli.reasoning_effort = crate::cli::ReasoningEffort::High;

        let default = status_row(&app, 80, false).text();
        assert!(default.contains("opencode-go/glm-5.2 · high"));
        assert!(!default.contains("Editable"));

        app.runtime.cli.status_line.left =
            vec![StatusSegment::RunState, StatusSegment::Authority, StatusSegment::Route];
        app.runtime.cli.status_line.right = vec![StatusSegment::QueueCount];
        let moved = status_row(&app, 80, false).text();
        assert!(moved.find("opencode-go/glm-5.2").unwrap() < moved.find("queue 0").unwrap());
    }

    #[test]
    fn current_state_displaces_model_under_width_pressure() {
        let mut app = App::from_cli(&Cli::default());
        app.runtime.model = "opencode-go/glm-5.2".to_string();
        app.runtime
            .cli
            .status_line
            .right
            .retain(|field| *field != StatusSegment::Route);

        assert!(status_row(&app, 80, false).text().contains("opencode-go/glm-5.2"));
        let cramped = status_row(&app, 12, false).text();
        assert!(cramped.contains('✓'));
        assert!(!cramped.contains("5.2"));
    }

    #[test]
    fn optional_authority_and_anchor_are_projected_semantically() {
        let mut app = App::from_cli(&Cli::default());
        let text = status_row(&app, 80, true).text();
        assert!(text.contains("✓ Ready"));
        assert!(text.contains("↑ away"));
        assert!(!text.contains("Editable"));

        app.runtime.cli.status_line.left.push(StatusSegment::Authority);
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
        assert!(text.starts_with("    ✓ Ready"));
        assert!(text.ends_with("model unavailable · auto    "));
    }

    #[test]
    fn run_transitions_and_active_tool_are_projected() {
        let mut app = App::from_cli(&Cli::default());
        assert!(status_row(&app, 80, false).text().contains("✓ Ready"));

        app.runtime.run_state = RunState::Working;
        app.transcript.entries.push(Entry::Tool {
            name: "search_text".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Running,
            output: Vec::new(),
        });
        let working = status_row(&app, 80, false).text();
        assert!(working.contains("Running search text"));
        assert!(working.contains('⠋'));
        assert!(!working.contains("tool search_text"));

        app.runtime.run_state = RunState::Stopping;
        assert!(status_row(&app, 80, false).text().contains("Stopping"));
        app.runtime.run_state = RunState::Error("provider detail".to_string());
        let failed = status_row(&app, 80, false).text();
        assert!(failed.contains("Failed"));
        assert!(!failed.contains("provider detail"));
    }

    #[test]
    fn failed_tool_status_is_authoritative_without_secret_context() {
        let mut app = App::from_cli(&Cli::default());
        app.transcript.entries.push(Entry::Tool {
            name: "run_shell".to_string(),
            arguments: "token=secret-argument".to_string(),
            status: ToolStatus::Failed,
            output: vec!["secret-output".to_string()],
        });

        let text = status_row(&app, 80, false).text();
        assert!(text.contains("✕ Failed"));
        assert!(!text.contains("secret-argument"));
        assert!(!text.contains("secret-output"));
    }

    #[test]
    fn context_remaining_is_default_live_ordered_and_removed_under_width_pressure() {
        let mut app = App::from_cli(&Cli::default());
        app.runtime.model = "fixture-model".to_string();
        app.refresh_context_ledger(None);

        let wide = status_row(&app, 100, false).text();
        let route = wide.find("fixture-model · auto").expect("model and reasoning");
        let context = wide.find("% ctx left").expect("context remaining");
        assert!(route < context);
        assert!(!wide.contains("queue"));
        assert!(!status_row(&app, 24, false).text().contains("ctx left"));

        let ledger = app.transcript.context_ledger.as_mut().expect("refreshed ledger");
        ledger.budget.used = ledger.budget.available_input;
        let exhausted = status_row(&app, 100, false).text();
        assert!(exhausted.contains("0% ctx left"));
        assert!(!exhausted.contains("compact"));
    }

    #[test]
    fn context_remaining_hides_unknown_capacity() {
        let mut app = App::from_cli(&Cli::default());
        app.refresh_context_ledger(None);
        app.transcript
            .context_ledger
            .as_mut()
            .expect("refreshed ledger")
            .budget
            .available_input = 0;

        assert!(!status_row(&app, 100, false).text().contains("ctx left"));
    }
}
