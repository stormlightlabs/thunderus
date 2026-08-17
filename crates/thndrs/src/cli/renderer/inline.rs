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
    /// Semantic identity used for exact-once checkpointing.
    pub id: TranscriptBlockId,
    /// Rows projected at the terminal width when the block first stabilizes.
    pub rows: Vec<Row>,
}

/// Ordered work for one terminal transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InlineTranscriptPlan {
    /// New stable blocks, in transcript order.
    pub commits: Vec<TranscriptCommit>,
    /// Reflowable blocks which have not become terminal history yet.
    pub live_rows: Vec<Row>,
}

/// Tracks the semantic transcript blocks committed to this terminal session.
#[derive(Clone, Debug, Default)]
pub struct InlineTranscript {
    generation: u64,
    committed: HashSet<CommitCheckpoint>,
}

/// Exact-once checkpoint for one semantic block in one application generation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CommitCheckpoint {
    generation: u64,
    id: TranscriptBlockId,
}

impl InlineTranscript {
    /// Build the next ordered insertion and mutable tail for the current app state.
    pub fn plan(&self, app: &App, width: usize) -> InlineTranscriptPlan {
        let mut plan = InlineTranscriptPlan::default();
        let mut waiting_for_stable = false;

        for (index, block) in app.transcript.entries.blocks().enumerate() {
            if self
                .committed
                .contains(&CommitCheckpoint { generation: self.generation, id: block.id.clone() })
            {
                continue;
            }
            if !waiting_for_stable && block_is_stable(app, &block) {
                plan.commits
                    .push(TranscriptCommit { id: block.id.clone(), rows: project_inline_block(app, index, width) });
                continue;
            }

            waiting_for_stable = true;
            plan.live_rows.extend(project_inline_block(app, index, width));
        }

        if app.transcript.entries.is_empty() {
            plan.live_rows = app.render_banner_rows(width);
        }
        plan
    }

    /// Record commits only after their terminal transaction has succeeded.
    pub fn mark_committed(&mut self, commits: &[TranscriptCommit]) {
        self.committed.extend(
            commits
                .iter()
                .map(|commit| CommitCheckpoint { generation: self.generation, id: commit.id.clone() }),
        );
    }

    /// Start a fresh application-history generation without purporting to erase
    /// terminal-emulator scrollback.
    pub fn reset(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.committed.clear();
    }

    /// Current generation for tests and terminal-coordinator diagnostics.
    pub fn generation(&self) -> u64 {
        self.generation
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
        tool_group_start: true,
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
            rows = vec![skill_row(width, name, path, *token_estimate, *context_percent)];
        }
        Entry::Status { text } => {
            if let Some(operation) = semantic_operation {
                rows = vec![operation_status_row(width, operation, text)];
            }
        }
        _ => {}
    }
    rows
}

fn rewrite_tool_header(
    rows: &mut [Row], width: usize, action: &str, arguments: &str, status: ToolStatus, cwd: &std::path::Path,
) {
    // Inline projection always starts a standalone tool block, whose header
    // follows its one-row group spacer. Classification uses structured tool
    // data above; it never inspects rendered labels.
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

fn skill_row(width: usize, name: &str, path: &str, token_estimate: usize, context_percent: Option<u8>) -> Row {
    let palette = super::style::palette();
    let summary = crate::renderer::view::skill_activation_summary(name, path, token_estimate, context_percent);
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
    )
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
        let mut transcript = InlineTranscript::default();

        let first = transcript.plan(&app, 80);
        assert_eq!(first.commits.len(), 1);
        assert!(first.live_rows.iter().any(|row| row.text().contains("streaming")));
        transcript.mark_committed(&first.commits);

        let unchanged = transcript.plan(&app, 40);
        assert!(unchanged.commits.is_empty(), "width changes must not replay history");

        let Some(Entry::Agent { streaming, .. }) = app.transcript.entries.last_mut() else {
            panic!("streaming response should remain present");
        };
        *streaming = false;
        let finalized = transcript.plan(&app, 40);
        assert_eq!(finalized.commits.len(), 1);
        assert!(finalized.live_rows.is_empty(), "finalization removes the mutable copy");
    }

    #[test]
    fn clear_starts_a_new_generation() {
        let mut transcript = InlineTranscript::default();
        transcript.reset();

        assert_eq!(transcript.generation(), 1);
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
