//! Transcript row projection and activity summaries.

use super::*;
use crate::renderer::{path_display, style, tool_output};

/// Transcript portion of the view: committed banner rows, stable rows, and rows
/// that are still mutable (streaming or running).
#[derive(Clone)]
pub struct TranscriptView {
    /// Every transcript row in chronological application order.
    ///
    /// Unlike the stable/live partitions below, this projection never moves a
    /// settled entry ahead of an earlier mutable entry. Full-screen renderers
    /// should use this sequence as their source of truth.
    pub rows: Vec<Row>,
    /// Banner rows shown before the first transcript entry is committed.
    pub banner_rows: Vec<Row>,
    /// Settled rows retained for live-tail classification and compatibility
    /// snapshots.
    pub stable_rows: Vec<Row>,
    /// Rows that remain mutable until their entry settles.
    pub live_rows: Vec<Row>,
}

/// Inputs beyond the entry itself that affect its cached row projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptProjectionKey {
    tool_group_start: bool,
    detail_target: bool,
    detail_open: bool,
    detail_scroll: usize,
    activity: ActivityProjection,
}

/// Return all presentation state needed to decide whether an entry projection is reusable.
pub fn transcript_projection_key(app: &App, entry_index: usize) -> TranscriptProjectionKey {
    let previous_was_tool = entry_index
        .checked_sub(1)
        .and_then(|index| app.transcript.entries.get(index))
        .is_some_and(|entry| matches!(entry, Entry::Tool { .. }));
    let detail_target_index = crate::app::next_detail_target(app);
    let open_detail = app.overlay.detail();
    let activity = activity_projection(
        app,
        entry_index,
        detail_target_index,
        open_detail.map(|detail| detail.entry_index),
    );
    TranscriptProjectionKey {
        tool_group_start: !previous_was_tool,
        detail_target: detail_target_index == Some(entry_index),
        detail_open: open_detail.is_some_and(|detail| detail.entry_index == entry_index),
        detail_scroll: open_detail
            .filter(|detail| detail.entry_index == entry_index)
            .map_or(0, |detail| detail.scroll),
        activity,
    }
}

/// Project one transcript entry at a specific width.
///
/// Alternate-screen caching uses this boundary to invalidate a changing entry
/// without rebuilding settled entries above it.
pub fn project_transcript_entry(app: &App, entry_index: usize, width: usize) -> (Vec<Row>, Vec<Row>) {
    let Some(entry) = app.transcript.entries.get(entry_index) else {
        return (Vec::new(), Vec::new());
    };
    let key = transcript_projection_key(app, entry_index);
    TranscriptRowContext {
        user_label: &app.runtime.user_label,
        cwd: &app.runtime.cwd,
        width,
        entry_index: Some(entry_index),
        tool_group_start: key.tool_group_start,
        detail_target: key.detail_target,
        detail_open: key.detail_open,
        detail_scroll: key.detail_scroll,
        activity: key.activity,
    }
    .rows_for_entry_stable_and_live_rows(entry)
}

impl TranscriptView {
    pub(crate) fn build(app: &App, width: usize) -> Self {
        let banner_rows = app.render_banner_rows(width);

        if app.transcript.entries.is_empty() {
            return Self { rows: banner_rows.clone(), banner_rows, stable_rows: Vec::new(), live_rows: Vec::new() };
        }

        let mut rows = banner_rows.clone();
        let mut stable_rows = Vec::new();
        let mut live_rows = Vec::new();
        stable_rows.extend(banner_rows);

        for index in 0..app.transcript.entries.len() {
            let (entry_stable, entry_live) = project_transcript_entry(app, index, width);
            rows.extend(entry_stable.iter().cloned());
            rows.extend(entry_live.iter().cloned());
            if entry_stable.is_empty() {
                live_rows.extend(entry_live);
            } else {
                stable_rows.extend(entry_stable);
                live_rows.extend(entry_live);
            }
        }

        Self { rows, banner_rows: Vec::new(), stable_rows, live_rows }
    }
}

fn activity_projection(
    app: &App, entry_index: usize, detail_target: Option<usize>, open_detail: Option<usize>,
) -> ActivityProjection {
    let entries = &app.transcript.entries;
    let Some(Entry::Tool { name, arguments, status, output }) = entries.get(entry_index) else {
        return ActivityProjection::Regular;
    };

    if !is_routine_exploration(&entries[entry_index]) {
        let base_name = name.split('#').next().unwrap_or(name);
        let detail_open = open_detail == Some(entry_index);
        return ActivityProjection::Summary {
            summary: single_activity_summary(
                app,
                base_name,
                arguments,
                *status,
                output,
                detail_target == Some(entry_index),
                detail_open,
            ),
            show_tool: detail_open,
        };
    }

    let group_start = entry_index == 0 || !is_routine_exploration(&entries[entry_index - 1]);
    let disclosed = open_detail.is_some_and(|detail_index| same_exploration_group(entries, entry_index, detail_index));
    if !group_start {
        return if disclosed { ActivityProjection::DisclosedTool } else { ActivityProjection::Hidden };
    }

    let mut end = entry_index;
    while entries.get(end + 1).is_some_and(is_routine_exploration) {
        end += 1;
    }

    let mut reads = 0;
    let mut searches = 0;
    let mut running = false;
    let mut latest = String::new();
    let mut latest_name = "";
    for entry in &entries[entry_index..=end] {
        let Entry::Tool { name, arguments, status, .. } = entry else {
            continue;
        };
        let base_name = name.split('#').next().unwrap_or(name);
        match base_name {
            "read_file_range" | "read_url" | "sawk" => reads += 1,
            "find_files" | "list_searchable_files" | "search_text" | "web_search" => searches += 1,
            _ => {}
        }
        running |= *status == ToolStatus::Running;
        latest = summarize_tool_invocation(base_name, arguments, &app.runtime.cwd);
        latest_name = base_name;
    }
    let label = if running { exploration_label(latest_name, &latest) } else { "Explored".to_string() };
    let mut details = Vec::new();
    if reads > 0 {
        details.push(format!("{reads} {}", if reads == 1 { "read" } else { "reads" }));
    }
    if searches > 0 {
        details.push(format!(
            "{searches} {}",
            if searches == 1 { "search" } else { "searches" }
        ));
    }
    ActivityProjection::Summary {
        summary: ActivitySummary {
            kind: ActivityKind::Explore,
            importance: ActivityImportance::Routine,
            calls: end - entry_index + 1,
            reads,
            searches,
            running,
            failed: false,
            cancelled: false,
            label,
            marker: activity_marker(app, *if running { &ToolStatus::Running } else { &ToolStatus::Ok }),
            details,
            preview: Vec::new(),
            hidden_lines: 0,
            detail_target: detail_target.is_some_and(|index| (entry_index..=end).contains(&index)),
            detail_open: disclosed,
        },
        show_tool: disclosed,
    }
}

fn single_activity_summary(
    app: &App, name: &str, arguments: &str, status: ToolStatus, output: &[String], detail_target: bool,
    detail_open: bool,
) -> ActivitySummary {
    let command = (name == "run_shell").then(|| shell_command(arguments)).flatten();
    let verification = command.as_deref().and_then(verification_command);
    let kind = if is_edit_tool(name) {
        ActivityKind::Edit
    } else if verification.is_some() {
        ActivityKind::Test
    } else if is_routine_tool(name) {
        ActivityKind::Explore
    } else {
        ActivityKind::Command
    };
    let target = match kind {
        ActivityKind::Edit => edit_path_from_args(arguments)
            .map(|path| path_display::transcript_line(&path, &app.runtime.cwd))
            .unwrap_or_else(|| "files".to_string()),
        ActivityKind::Test | ActivityKind::Command => command
            .unwrap_or_else(|| summarize_tool_invocation(name, arguments, &app.runtime.cwd))
            .trim_start_matches("$ ")
            .to_string(),
        ActivityKind::Explore => summarize_tool_invocation(name, arguments, &app.runtime.cwd),
    };
    let target = if target.is_empty() { name.to_string() } else { target };
    let label = activity_label(kind, status, &target, verification);
    let mut details = Vec::new();
    if kind == ActivityKind::Edit
        && let Some(diff) = tool_output::projected_diff(name, output)
    {
        let (_, added, removed) = diff.summary();
        if added > 0 || removed > 0 {
            details.push(format!("+{added} −{removed}"));
        }
    }
    if kind == ActivityKind::Test {
        let (passed, failed) = test_counts(output);
        if failed > 0 {
            details.push(format!("{failed} failed"));
        }
        if passed > 0 {
            details.push(format!("{passed} passed"));
        }
    }
    if name == "run_shell" {
        let metadata = shell_result_metadata(output);
        if let Some(duration) = metadata.duration {
            details.push(duration);
        }
        if let Some(exit_code) = metadata.exit_code {
            details.push(format!("exit {exit_code}"));
        }
    }
    let (preview, hidden_lines) = activity_preview(kind, status, &target, output);
    ActivitySummary {
        kind,
        importance: ActivityImportance::Significant,
        calls: 1,
        reads: usize::from(kind == ActivityKind::Explore && matches!(name, "read_file_range" | "read_url" | "sawk")),
        searches: usize::from(
            kind == ActivityKind::Explore && !matches!(name, "read_file_range" | "read_url" | "sawk"),
        ),
        running: status == ToolStatus::Running,
        failed: status == ToolStatus::Failed,
        cancelled: status == ToolStatus::Cancelled,
        label,
        marker: activity_marker(app, status),
        details,
        preview,
        hidden_lines,
        detail_target,
        detail_open,
    }
}

fn activity_marker(app: &App, status: ToolStatus) -> String {
    match status {
        ToolStatus::Running => {
            style::spinner_frame(style::spinner_tick(app.runtime.ui_tick, app.runtime.cli.tick_rate_ms)).to_string()
        }
        ToolStatus::Ok => "✓".to_string(),
        ToolStatus::Failed => "✕".to_string(),
        ToolStatus::Cancelled => "○".to_string(),
    }
}

fn exploration_label(name: &str, target: &str) -> String {
    let target = target.split_once(": ").map_or(target, |(_, value)| value);
    let verb = if matches!(
        name,
        "find_files" | "list_searchable_files" | "search_text" | "web_search"
    ) {
        "Searching"
    } else {
        "Exploring"
    };
    if target.is_empty() { verb.to_string() } else { format!("{verb} {target}") }
}

#[derive(Clone, Copy)]
enum VerificationCommand {
    Test,
    Check,
    Build,
}

fn verification_command(command: &str) -> Option<VerificationCommand> {
    let words = command.split_whitespace().collect::<Vec<_>>();
    match words.as_slice() {
        ["cargo", "test" | "nextest", ..] | ["npm" | "pnpm", "test", ..] | ["pytest", ..] | ["go", "test", ..] => {
            Some(VerificationCommand::Test)
        }
        ["cargo", "check", ..] => Some(VerificationCommand::Check),
        ["cargo", "build", ..] => Some(VerificationCommand::Build),
        _ => None,
    }
}

fn activity_label(
    kind: ActivityKind, status: ToolStatus, target: &str, verification: Option<VerificationCommand>,
) -> String {
    let failed = status == ToolStatus::Failed;
    let running = status == ToolStatus::Running;
    let cancelled = status == ToolStatus::Cancelled;
    match kind {
        ActivityKind::Explore => {
            if failed {
                "Exploration failed".to_string()
            } else if cancelled {
                "Exploration cancelled".to_string()
            } else {
                exploration_label("", target)
            }
        }
        ActivityKind::Edit => match (failed, running, cancelled) {
            (true, _, _) => format!("Edit failed {target}"),
            (_, true, _) => format!("Editing {target}"),
            (_, _, true) => format!("Edit cancelled {target}"),
            _ => format!("Edited {target}"),
        },
        ActivityKind::Test => {
            let (active, passed, failed_label) = match verification.unwrap_or(VerificationCommand::Test) {
                VerificationCommand::Test => ("Testing", "Tests passed", "Tests failed"),
                VerificationCommand::Check => ("Checking", "Checks passed", "Checks failed"),
                VerificationCommand::Build => ("Building", "Build passed", "Build failed"),
            };
            if failed {
                failed_label.to_string()
            } else if running {
                format!("{active} {target}")
            } else if cancelled {
                match verification.unwrap_or(VerificationCommand::Test) {
                    VerificationCommand::Test => "Tests cancelled",
                    VerificationCommand::Check => "Checks cancelled",
                    VerificationCommand::Build => "Build cancelled",
                }
                .to_string()
            } else {
                passed.to_string()
            }
        }
        ActivityKind::Command => match (failed, running, cancelled) {
            (true, _, _) => format!("Command failed {target}"),
            (_, true, _) => format!("Running {target}"),
            (_, _, true) => format!("Command cancelled {target}"),
            _ => format!("Ran {target}"),
        },
    }
}

fn shell_command(arguments: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    if let Some(argv) = value.get("argv").and_then(serde_json::Value::as_array) {
        let command = argv
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        return (!command.is_empty()).then_some(command);
    }
    value
        .get("program")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn test_counts(output: &[String]) -> (usize, usize) {
    output.iter().fold((0usize, 0usize), |(passed, failed), line| {
        let clean = tool_output::sanitize_terminal_text(line);
        let words = clean.split(|ch: char| !ch.is_ascii_alphanumeric()).collect::<Vec<_>>();
        let count = |label| {
            words
                .windows(2)
                .find_map(|pair| (pair[1] == label).then(|| pair[0].parse::<usize>().ok()).flatten())
                .unwrap_or(0)
        };
        (
            passed.saturating_add(count("passed")),
            failed.saturating_add(count("failed")),
        )
    })
}

#[derive(Default)]
struct ShellResultMetadata {
    duration: Option<String>,
    exit_code: Option<i32>,
}

fn shell_result_metadata(output: &[String]) -> ShellResultMetadata {
    let Some(summary) = output
        .iter()
        .map(|line| tool_output::sanitize_terminal_text(line))
        .find(|line| line.starts_with("$ "))
    else {
        return ShellResultMetadata::default();
    };
    let words = summary
        .trim_end_matches(']')
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    let duration = words
        .iter()
        .rev()
        .find_map(|word| word.strip_suffix("ms")?.parse::<u64>().ok())
        .map(format_duration);
    let exit_code = words
        .windows(2)
        .find_map(|pair| (pair[0] == "exit").then(|| pair[1].parse::<i32>().ok()).flatten());
    ShellResultMetadata { duration, exit_code }
}

fn format_duration(milliseconds: u64) -> String {
    if milliseconds < 1_000 {
        return format!("{milliseconds}ms");
    }
    let seconds = milliseconds as f64 / 1_000.0;
    if milliseconds.is_multiple_of(1_000) { format!("{seconds:.0}s") } else { format!("{seconds:.1}s") }
}

fn activity_preview(kind: ActivityKind, status: ToolStatus, target: &str, output: &[String]) -> (Vec<String>, usize) {
    if !matches!(status, ToolStatus::Running | ToolStatus::Failed | ToolStatus::Cancelled) {
        return (Vec::new(), 0);
    }

    let lines = output
        .iter()
        .map(|line| tool_output::sanitize_terminal_text(line))
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("$ ")
                && !trimmed.to_ascii_lowercase().starts_with("error: command failed")
                && !matches!(trimmed, "── stdout ──" | "── stderr ──")
        })
        .collect::<Vec<_>>();
    let mut preview = Vec::new();
    if kind == ActivityKind::Test {
        preview.push(format!("$ {target}"));
    }

    if status == ToolStatus::Running {
        preview.extend(lines.iter().rev().take(2).rev().cloned());
    } else {
        if kind == ActivityKind::Test
            && let Some(failed_test) = lines.iter().find(|line| {
                let lower = line.to_ascii_lowercase();
                lower.starts_with("test ") && lower.contains("failed")
            })
        {
            preview.push(failed_test.clone());
        }
        let diagnostic = lines.iter().position(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("error")
                || lower.contains("panicked")
                || lower.contains("assertion")
                || lower.contains("not found")
                || lower.contains("permission denied")
        });
        if let Some(index) = diagnostic {
            if !preview.contains(&lines[index]) {
                preview.push(lines[index].clone());
            }
            if let Some(location) = lines.iter().skip(index + 1).find(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with("-->") || trimmed.starts_with("at ")
            }) {
                preview.push(location.clone());
            } else if preview.len() < 2
                && let Some(next) = lines.get(index + 1)
            {
                preview.push(next.clone());
            }
        } else if preview.len() == usize::from(kind == ActivityKind::Test) {
            preview.extend(lines.iter().take(2).cloned());
        }
    }

    let output_preview_lines = preview.len().saturating_sub(usize::from(kind == ActivityKind::Test));
    let hidden_lines = output.len().saturating_sub(output_preview_lines);
    (preview, hidden_lines)
}

fn is_edit_tool(name: &str) -> bool {
    matches!(name, "create_file" | "replace_range" | "write_patch")
}

fn is_routine_tool(name: &str) -> bool {
    matches!(
        name,
        "find_files" | "list_searchable_files" | "search_text" | "read_file_range" | "sawk" | "web_search" | "read_url"
    )
}

fn same_exploration_group(entries: &[Entry], left: usize, right: usize) -> bool {
    let Some(slice) = entries.get(left.min(right)..=left.max(right)) else {
        return false;
    };
    slice.iter().all(is_routine_exploration)
}

fn is_routine_exploration(entry: &Entry) -> bool {
    let Entry::Tool { name, status, .. } = entry else {
        return false;
    };
    matches!(status, ToolStatus::Running | ToolStatus::Ok)
        && matches!(
            name.split('#').next().unwrap_or(name),
            "find_files"
                | "list_searchable_files"
                | "search_text"
                | "read_file_range"
                | "sawk"
                | "web_search"
                | "read_url"
        )
}
