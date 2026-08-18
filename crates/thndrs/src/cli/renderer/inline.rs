//! Native-scrollback transcript coordination for the inline terminal surface.
//!
//! Completed semantic blocks are checkpointed by their stable application
//! identity. The runtime writes the resulting rows with Ratatui
//! `Terminal::insert_before`; this module neither owns terminal I/O nor retains
//! a reconstructed history viewport.

use std::collections::HashSet;

use crate::app::{App, Entry, ToolLifecycleState, ToolStatus, TranscriptBlock, TranscriptBlockId, TranscriptBlockKind};
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};
use crate::renderer::transcript::{ACTIVITY_RAIL, TranscriptRowContext, summarize_tool_invocation};
use crate::utils;

/// A transcript operation with a stable reader-facing glyph and label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    Skill,
    Run,
    Search,
    Read,
    Explore,
    Edit,
    Create,
    Delete,
    Fetch,
    Retry,
    Tool,
    Subagent,
    Warning,
}

impl OperationKind {
    /// Symbol shown beside the readable operation label.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Skill => "§",
            Self::Run => "$",
            Self::Search => "/",
            Self::Read => "›",
            Self::Explore => "⌁",
            Self::Edit => "∆",
            Self::Create => "+",
            Self::Delete => "−",
            Self::Fetch => "↗",
            Self::Retry => "⟳",
            Self::Tool => "@",
            Self::Subagent => "∥",
            Self::Warning => "!",
        }
    }

    /// Readable label adjacent to the operation symbol.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Run => "Ran",
            Self::Search => "Searched",
            Self::Read => "Read",
            Self::Explore => "Explored",
            Self::Edit => "Edited",
            Self::Create => "Wrote",
            Self::Delete => "Removed",
            Self::Fetch => "Fetched",
            Self::Retry => "Retried",
            Self::Tool => "Tool/MCP",
            Self::Subagent => "Agent",
            Self::Warning => "Blocked",
        }
    }

    /// Classify a semantic block from its structured kind, action, and arguments.
    ///
    /// Renderer labels are never inspected to recover operation semantics.
    pub fn for_block(block: TranscriptBlock<'_>) -> Option<Self> {
        match block.kind {
            TranscriptBlockKind::SkillActivation => Some(Self::Skill),
            TranscriptBlockKind::Permission => Some(Self::Warning),
            TranscriptBlockKind::ChildActivity => Some(Self::Subagent),
            TranscriptBlockKind::ToolCall | TranscriptBlockKind::Edit | TranscriptBlockKind::Diff => Some(
                Self::for_tool(block.action().unwrap_or_default(), tool_arguments(block.entry)),
            ),
            _ => None,
        }
    }

    /// Classify a tool from its structured action and argument payload.
    pub(crate) fn for_tool(action: &str, arguments: &str) -> Self {
        match crate::mcp::adapter::tool_presentation(action) {
            crate::mcp::adapter::McpToolPresentation::Search => return Self::Search,
            crate::mcp::adapter::McpToolPresentation::Fetch => return Self::Fetch,
            crate::mcp::adapter::McpToolPresentation::Generic => {}
        }
        match action {
            "run_shell" => Self::Run,
            "find_files" | "list_searchable_files" => Self::Explore,
            "search_text" => Self::Search,
            "read_file_range" | "sawk" => Self::Read,
            "read_url" => Self::Fetch,
            "explore" => Self::Explore,
            "create_file" => Self::Create,
            "replace_range" => Self::Edit,
            "write_patch" => write_patch_operation(arguments).unwrap_or(Self::Edit),
            "retry" | "refresh" => Self::Retry,
            "spawn_agent" | "run_subagent" => Self::Subagent,
            _ => Self::Tool,
        }
    }
}

/// One stable block ready to insert above the inline viewport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptCommit {
    /// Application session that owns this terminal-scrollback checkpoint.
    pub session_id: String,
    /// Semantic identity used for exact-once checkpointing.
    pub id: TranscriptBlockId,
    /// Rows projected at the terminal width when the block first stabilizes.
    pub rows: Vec<Row>,
}

/// Ordered native-scrollback work for one terminal transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScrollbackPlan {
    /// New stable blocks, in transcript order.
    pub commits: Vec<TranscriptCommit>,
}

/// Tracks semantic blocks already committed to native terminal history.
#[derive(Clone, Debug, Default)]
pub struct ScrollbackCommitter {
    committed: HashSet<CommitCheckpoint>,
}

/// Exact-once checkpoint for one semantic block in one application session.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CommitCheckpoint {
    session_id: String,
    id: TranscriptBlockId,
}

impl ScrollbackCommitter {
    /// Return stable semantic blocks that have not been committed for this session.
    pub fn newly_stable(&self, app: &App, width: usize) -> ScrollbackPlan {
        let mut plan = ScrollbackPlan::default();
        let mut waiting_for_stable = false;

        for (index, block) in app.transcript.entries.blocks().enumerate() {
            let checkpoint = CommitCheckpoint { session_id: app.session.id.clone(), id: block.id.clone() };
            if self.committed.contains(&checkpoint) {
                continue;
            }
            if !waiting_for_stable && block_is_stable(app, &block) {
                plan.commits.push(TranscriptCommit {
                    session_id: app.session.id.clone(),
                    id: block.id.clone(),
                    rows: project_inline_block(app, index, width),
                });
                continue;
            }

            waiting_for_stable = true;
        }
        plan
    }

    /// Reproject every stable block for a fresh terminal width.
    ///
    /// Native terminal history cannot reflow at word boundaries after it has
    /// been painted. A resize therefore purges and rebuilds the app-owned
    /// history instead of asking the terminal to wrap old cells.
    pub fn replayable_rows(&self, app: &App, width: usize) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut waiting_for_stable = false;

        for (index, block) in app.transcript.entries.blocks().enumerate() {
            if !waiting_for_stable && block_is_stable(app, &block) {
                rows.extend(project_inline_block(app, index, width));
            } else {
                waiting_for_stable = true;
            }
        }
        rows
    }

    /// Project the incomplete ordered suffix that Ratatui still owns.
    pub fn mutable_tail_rows(&self, app: &App, width: usize) -> Vec<Row> {
        let mut tail = Vec::new();
        let mut waiting_for_stable = false;

        for (index, block) in app.transcript.entries.blocks().enumerate() {
            let checkpoint = CommitCheckpoint { session_id: app.session.id.clone(), id: block.id.clone() };
            if self.committed.contains(&checkpoint) {
                continue;
            }
            if !waiting_for_stable && block_is_stable(app, &block) {
                continue;
            }

            waiting_for_stable = true;
            tail.extend(project_inline_block(app, index, width));
        }
        tail
    }

    /// Record commits only after their terminal transaction has succeeded.
    pub fn mark_committed(&mut self, commits: &[TranscriptCommit]) {
        self.committed.extend(
            commits
                .iter()
                .map(|commit| CommitCheckpoint { session_id: commit.session_id.clone(), id: commit.id.clone() }),
        );
    }

    /// Forget checkpoints after an explicit terminal clear.
    pub fn clear(&mut self) {
        self.committed.clear();
    }
}

fn block_is_stable(app: &App, block: &TranscriptBlock<'_>) -> bool {
    if block.kind == TranscriptBlockKind::Permission && app.overlay.permission().is_some() {
        return false;
    }
    match block.entry {
        Entry::Agent { streaming, .. } | Entry::Reasoning { streaming, .. } => !streaming,
        Entry::Tool { status, .. } => {
            block.lifecycle().is_none_or(|state| {
                matches!(
                    state,
                    ToolLifecycleState::Succeeded | ToolLifecycleState::Failed | ToolLifecycleState::Cancelled
                )
            }) && *status != ToolStatus::Running
        }
        _ => true,
    }
}

fn project_inline_block(app: &App, entry_index: usize, width: usize) -> Vec<Row> {
    let Some(entry) = app.transcript.entries.get(entry_index) else {
        return Vec::new();
    };
    let semantic_operation = app
        .transcript
        .entries
        .block(entry_index)
        .and_then(OperationKind::for_block);
    let mut rows = TranscriptRowContext {
        user_label: &app.runtime.user_label,
        cwd: &app.runtime.cwd,
        width,
        entry_index: Some(entry_index),
        detail_target: false,
        detail_open: false,
        detail_scroll: 0,
        activity: super::transcript::ActivityProjection::Regular,
    }
    .rows_for_entry(entry);
    match entry {
        Entry::Tool { name, arguments, status, .. } => {
            let action = name.split('#').next().unwrap_or(name);
            rewrite_tool_header(&mut rows, width, action, arguments, *status, &app.runtime.cwd);
        }
        Entry::Skill { name, path, token_estimate, context_percent, .. } => {
            rows = skill_row(width, name, path, *token_estimate, *context_percent);
        }
        Entry::Status { text } => {
            if let Some(operation) = semantic_operation {
                rows = vec![
                    Row::blank(width, CellStyle::new().bg(Color::Reset)),
                    operation_status_row(width, operation, text),
                ];
            }
        }
        _ => {}
    }
    trim_transcript_row_padding(&mut rows);
    rows
}

/// Avoid copying renderer-only trailing cells as part of native transcript
/// history. The row width remains terminal-wide so later live rendering still
/// clears the complete line.
fn trim_transcript_row_padding(rows: &mut [Row]) {
    for row in rows {
        while let Some(last) = row.spans.last_mut() {
            let trimmed = last.text.trim_end_matches(' ');
            if trimmed.len() == last.text.len() {
                break;
            }
            last.text.truncate(trimmed.len());
            if last.text.is_empty() {
                row.spans.pop();
            }
        }
    }
}

/// Inline projection always starts a standalone tool block, whose header
/// follows its one-row group spacer.
///
/// Classification uses structured tool data above; it never inspects rendered labels.
fn rewrite_tool_header(
    rows: &mut [Row], width: usize, action: &str, arguments: &str, status: ToolStatus, cwd: &std::path::Path,
) {
    let Some(row) = rows.get_mut(1) else {
        return;
    };
    let operation = OperationKind::for_tool(action, arguments);
    let palette = super::style::palette();
    let lifecycle = match status {
        ToolStatus::Running => ("Running", palette.active),
        ToolStatus::Ok => ("Done", palette.success),
        ToolStatus::Failed => ("Failed", palette.failure),
        ToolStatus::Cancelled => ("Stopped", palette.active),
    };
    let prefix = format!("{} {} · {}", operation.symbol(), operation.label(), lifecycle.0);
    let target = summarize_tool_invocation(action, arguments, cwd);
    let target_width = width.saturating_sub(utils::text_width(ACTIVITY_RAIL) + utils::text_width(&prefix) + 2);
    let target = utils::truncate_ellipsis(&target, target_width);
    let mut spans = vec![
        Span::styled(ACTIVITY_RAIL, CellStyle::new().fg(palette.warning).bg(Color::Reset)),
        Span::styled(
            format!("{} ", operation.symbol()),
            CellStyle::new().fg(lifecycle.1).bg(Color::Reset),
        ),
        Span::styled(
            operation.label(),
            CellStyle::new().fg(palette.primary).bg(Color::Reset).bold(),
        ),
        Span::styled(
            format!(" · {}", lifecycle.0),
            CellStyle::new().fg(lifecycle.1).bg(Color::Reset),
        ),
    ];
    if !target.is_empty() {
        spans.push(Span::styled("  ", CellStyle::new().bg(Color::Reset)));
        spans.push(Span::styled(
            target,
            CellStyle::new().fg(palette.secondary).bg(Color::Reset),
        ));
    }
    *row = Row::padded(spans, width, CellStyle::new().bg(Color::Reset));
}

fn operation_status_row(width: usize, operation: OperationKind, text: &str) -> Row {
    let palette = super::style::palette();
    Row::padded(
        vec![
            Span::styled("  ", CellStyle::new().fg(palette.warning).bg(Color::Reset)),
            Span::styled(
                format!("{} ", operation.symbol()),
                CellStyle::new().fg(palette.warning).bg(Color::Reset),
            ),
            Span::styled(
                operation.label(),
                CellStyle::new().fg(palette.primary).bg(Color::Reset).bold(),
            ),
            Span::styled(
                format!("  {text}"),
                CellStyle::new().fg(palette.secondary).bg(Color::Reset),
            ),
        ],
        width,
        CellStyle::new().bg(Color::Reset),
    )
}

fn skill_row(width: usize, name: &str, path: &str, token_estimate: usize, context_percent: Option<u8>) -> Vec<Row> {
    let palette = super::style::palette();
    let summary = crate::renderer::view::skill_activation_summary(name, path, token_estimate, context_percent);
    vec![
        Row::blank(width, CellStyle::new().bg(Color::Reset)),
        Row::padded(
            vec![
                Span::styled("  ", CellStyle::new().fg(palette.warning).bg(Color::Reset)),
                Span::styled("§ Skill", CellStyle::new().fg(palette.accent).bg(Color::Reset).bold()),
                Span::styled(
                    format!("  {summary}"),
                    CellStyle::new().fg(palette.secondary).bg(Color::Reset),
                ),
            ],
            width,
            CellStyle::new().bg(Color::Reset),
        ),
    ]
}

fn tool_arguments(entry: &Entry) -> &str {
    match entry {
        Entry::Tool { arguments, .. } => arguments,
        _ => "",
    }
}

fn write_patch_operation(arguments: &str) -> Option<OperationKind> {
    let value = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    let patches = value.get("patches")?.as_array()?;
    let operation = patches.first()?.get("op")?.as_str()?;
    match operation {
        "create" => Some(OperationKind::Create),
        "delete" => Some(OperationKind::Delete),
        "edit" | "replace" => Some(OperationKind::Edit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;

    fn app() -> App {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.transcript.entries.clear();
        app
    }

    #[test]
    fn checkpoints_stable_blocks_once_and_keeps_streaming_tail_mutable() {
        let mut app = app();
        app.transcript
            .entries
            .push(Entry::User { text: "inspect this".to_string() });
        app.transcript
            .entries
            .push(Entry::Agent { text: "streaming".to_string(), streaming: true });
        let mut committer = ScrollbackCommitter::default();

        let first = committer.newly_stable(&app, 80);
        assert_eq!(first.commits.len(), 1);
        assert!(
            committer
                .mutable_tail_rows(&app, 80)
                .iter()
                .any(|row| row.text().contains("streaming"))
        );
        committer.mark_committed(&first.commits);

        let unchanged = committer.newly_stable(&app, 40);
        assert!(unchanged.commits.is_empty(), "width changes must not replay history");

        let Some(Entry::Agent { streaming, .. }) = app.transcript.entries.last_mut() else {
            panic!("streaming response should remain present");
        };
        *streaming = false;
        let finalized = committer.newly_stable(&app, 40);
        assert_eq!(finalized.commits.len(), 1);
        committer.mark_committed(&finalized.commits);
        assert!(
            committer.mutable_tail_rows(&app, 40).is_empty(),
            "finalization removes the mutable copy"
        );
    }

    #[test]
    fn new_and_resumed_sessions_have_distinct_checkpoint_namespaces() {
        let mut app = app();
        app.transcript.entries.push(Entry::User { text: "first".to_string() });
        let mut committer = ScrollbackCommitter::default();
        let first = committer.newly_stable(&app, 80);
        committer.mark_committed(&first.commits);

        app.start_new_session();
        app.transcript.entries.clear();
        app.transcript.entries.push(Entry::User { text: "second".to_string() });
        let next = committer.newly_stable(&app, 80);

        assert_eq!(next.commits.len(), 1, "block identities may repeat after /new");
        committer.mark_committed(&next.commits);

        // `/resume` replaces the transcript with a distinct persisted session.
        // The committer only needs its first-class application session identity
        // to keep a repeated `block:1` from colliding with either prior session.
        app.session.id = "resumed-session".to_string();
        let resumed = committer.newly_stable(&app, 80);
        assert_eq!(resumed.commits.len(), 1, "block identities may repeat after /resume");
    }

    #[test]
    fn replayable_rows_rewrap_stable_history_at_the_new_width() {
        let mut app = app();
        app.transcript
            .entries
            .push(Entry::Agent { text: "alpha beta gamma delta epsilon zeta".to_string(), streaming: false });
        let mut committer = ScrollbackCommitter::default();
        let initial = committer.newly_stable(&app, 80);
        committer.mark_committed(&initial.commits);

        let rows = committer.replayable_rows(&app, 24);
        let text = rows.iter().map(Row::text).collect::<Vec<_>>();

        assert!(text.iter().any(|row| row.contains("alpha beta gamma")));
        assert!(text.iter().any(|row| row.contains("delta epsilon")));
        assert!(text.iter().all(|row| crate::utils::text_width(row) <= 24));
        assert!(
            text.iter().all(|row| !row.ends_with(' ')),
            "history rows should not carry copy padding"
        );
    }

    #[test]
    fn classifies_structured_operation_vocabulary() {
        let cases = [
            ("run_shell", r#"{"argv":["cargo","test"]}"#, OperationKind::Run),
            ("search_text", r#"{"pattern":"needle"}"#, OperationKind::Search),
            ("read_file_range", r#"{"path":"src/lib.rs"}"#, OperationKind::Read),
            ("read_url", r#"{"url":"https://example.com"}"#, OperationKind::Fetch),
            (
                "mcp__research__web_search",
                r#"{"query":"needle"}"#,
                OperationKind::Search,
            ),
            (
                "mcp__research__web_fetch",
                r#"{"url":"https://example.com"}"#,
                OperationKind::Fetch,
            ),
            ("explore", "{}", OperationKind::Explore),
            ("create_file", r#"{"path":"new.rs"}"#, OperationKind::Create),
            ("replace_range", r#"{"path":"lib.rs"}"#, OperationKind::Edit),
            (
                "write_patch",
                r#"{"patches":[{"op":"delete","path":"old.rs"}]}"#,
                OperationKind::Delete,
            ),
            ("retry", "{}", OperationKind::Retry),
            ("run_subagent", "{}", OperationKind::Subagent),
            ("external_mcp", "{}", OperationKind::Tool),
        ];
        for (action, arguments, expected) in cases {
            let mut app = app();
            app.transcript.entries.push(Entry::Tool {
                name: action.to_string(),
                arguments: arguments.to_string(),
                status: ToolStatus::Ok,
                output: Vec::new(),
            });
            let block = app.transcript.entries.block(0).expect("tool block");
            assert_eq!(OperationKind::for_block(block), Some(expected), "{action}");
            assert!(!expected.symbol().is_empty());
            assert!(!expected.label().is_empty());
        }
    }
}
