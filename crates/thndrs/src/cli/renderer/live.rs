//! Live region row builders for the direct renderer.
//!
//! The live chrome is rebuilt each tick and composed into the full viewport by [`super::region::LiveRegion`].

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use crate::app::{
    App, Entry, Mode, PromptAccessory, PromptState, RecoveryStage, RunState, ToolStatus, setup_model_options,
};
use crate::cli::commands;
use crate::providers::{codex, umans};
use crate::renderer::cursor::{prompt_cursor, prompt_rows};
use crate::renderer::row::{CursorCoord, Row};
use crate::renderer::style::{CellStyle, Color, Span};
use crate::renderer::transcript::GUTTER;
use crate::{renderer, utils};

/// Maximum rows the prompt input can occupy before scrolling within the live region.
pub const MAX_PROMPT_ROWS: usize = 8;

/// Maximum accessory rows (help/commands/files) shown in the live region.
pub const MAX_ACCESSORY_ROWS: usize = 8;

const LIVE_INSET: usize = 2;

#[derive(Copy, Clone)]
struct ActionDimensions(usize, usize);

impl ActionDimensions {
    fn width(&self) -> usize {
        self.0
    }

    fn max_height(&self) -> usize {
        self.1
    }
}

#[derive(Copy, Clone)]
struct ActionStyles {
    selected_style: CellStyle,
    muted_style: CellStyle,
    text_style: CellStyle,
    bg: Color,
}

impl ActionStyles {
    fn new(selected: CellStyle, muted: CellStyle, text: CellStyle, bg: Color) -> Self {
        Self { selected_style: selected, muted_style: muted, text_style: text, bg }
    }
}

/// Build the dynamic status row: session id + status icon + queue info.
///
/// Sits above the prompt input in the live region.
pub fn dynamic_status_row(app: &App, width: usize) -> Row {
    let p = super::style::palette();
    let bg = p.surface0;

    let label = app.status_label();
    let status_color = super::style::status_color(label);
    let icon = super::style::status_icon(label, app.ui_tick);
    let session = if app.session_id.is_empty() { "thndrs" } else { &app.session_id };

    let mut spans = vec![
        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
        Span::styled(session.to_string(), CellStyle::new().fg(p.accent).bg(bg).bold()),
        Span::styled("  ", CellStyle::new().bg(bg)),
        Span::styled(format!("{icon} {label}"), CellStyle::new().fg(status_color).bg(bg)),
    ];

    if matches!(app.run_state, RunState::Working) {
        spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
        spans.push(Span::styled(
            format!(
                "target: {}  queued: {}/{}",
                app.queue_target.label(),
                app.queued_steering.len(),
                app.queued_followups.len()
            ),
            CellStyle::new().fg(p.subtext0).bg(bg),
        ));
    }

    Row::padded(spans, width, CellStyle::new().bg(bg))
}

/// Build prompt input rows from app state.
///
/// Returns the rows and the cursor coordinate (relative to the first row).
pub fn prompt_rows_for(app: &App, width: usize) -> (Vec<Row>, Option<CursorCoord>) {
    let p = super::style::palette();
    let surface = p.surface0;
    let prompt_state = app.prompt_state();

    let (prompt_color, _, icon) = match prompt_state {
        PromptState::Editable => (p.yellow, true, "›"),
        PromptState::Submitted => (p.teal, true, "»"),
        PromptState::Streaming | PromptState::RunningTool => (p.teal, true, "»"),
        PromptState::Stopped => (p.teal, true, "○"),
        PromptState::Errored => (p.red, true, "✕"),
    };

    let prefix_width = if app.mode == Mode::Command { 4 } else { 3 };
    let row_body_width = super::layout::content_width(width);
    let body_width = row_body_width.saturating_sub(LIVE_INSET + prefix_width).max(1);
    let cursor_indent = width.min(2) + LIVE_INSET + prefix_width;
    let hidden_entry_active = app
        .first_run_recovery
        .as_ref()
        .is_some_and(|recovery| recovery.stage == RecoveryStage::EnterKey);
    let hidden_display = String::from("credential: [hidden]");
    let input_text = if hidden_entry_active { hidden_display.as_str() } else { app.input.as_str() };
    let cursor_pos = if hidden_entry_active { input_text.len() } else { app.input.cursor() };

    let visual_rows = prompt_rows(input_text, body_width);
    let cursor = prompt_cursor(input_text, cursor_pos, body_width, cursor_indent);

    let text_style = CellStyle::new().fg(p.text).bg(surface);
    let mention_style = CellStyle::new().fg(p.accent).bg(surface).bold();

    let mut rows = Vec::with_capacity(visual_rows.len());
    for (idx, line) in visual_rows.into_iter().enumerate() {
        let mut spans: Vec<Span> = if idx == 0 {
            let mut s = vec![
                Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(surface)),
                Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
                Span::styled("  ", CellStyle::new().bg(surface)),
            ];
            if app.mode == Mode::Command {
                s.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
            }
            s
        } else {
            vec![Span::styled(
                " ".repeat(LIVE_INSET + prefix_width),
                CellStyle::new().bg(surface),
            )]
        };

        if !line.is_empty() {
            spans.extend(mention_styled_spans(&line, text_style, mention_style, surface));
        }
        rows.push(Row::padded(spans, width, CellStyle::new().bg(surface)));
    }

    if rows.is_empty() {
        let mut spans = vec![
            Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(surface)),
            Span::styled(icon, CellStyle::new().fg(prompt_color).bg(surface)),
            Span::styled("  ", CellStyle::new().bg(surface)),
        ];
        if app.mode == Mode::Command {
            spans.push(Span::styled(":", CellStyle::new().fg(p.accent).bg(surface)));
        }
        rows.push(Row::padded(spans, width, CellStyle::new().bg(surface)));
    }

    (rows, if hidden_entry_active { None } else { Some(cursor) })
}

/// Build accessory rows (help, commands, or file picker) if active.
///
/// Returns an empty vec when no accessory is visible.
pub fn accessory_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    if max_height == 0 {
        return Vec::new();
    }

    if app.pending_permission.is_some() {
        return permission_rows(app, width, max_height);
    }

    if app.first_run_recovery.is_some() {
        return first_run_recovery_rows(app, width, max_height);
    }

    match app.prompt_accessory {
        PromptAccessory::None => Vec::new(),
        PromptAccessory::Help => help_rows(app, width, max_height),
        PromptAccessory::Commands { selected } => command_rows(app, selected, width, max_height),
        PromptAccessory::Files(_) => picker_rows(app, "files", width, max_height),
        PromptAccessory::Models => picker_rows(app, "models", width, max_height),
        PromptAccessory::ReasoningEffort => picker_rows(app, "reasoning effort", width, max_height),
        PromptAccessory::Skills => picker_rows(app, "skills", width, max_height),
        PromptAccessory::Context => Vec::new(),
    }
}

/// Build a queued-prompt summary row when steering or follow-up prompts are pending.
///
/// Returns `None` when the queue is empty or the agent is idle.
pub fn queued_summary_row(app: &App, width: usize) -> Option<Row> {
    let steering = app.queued_steering.len();
    let followups = app.queued_followups.len();
    if steering == 0 && followups == 0 {
        return None;
    }

    let p = super::style::palette();
    let bg = p.surface0;
    let label_style = CellStyle::new().fg(p.peach).bg(bg).bold();
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let mut spans = vec![
        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
        Span::styled("queued", label_style),
    ];

    if steering > 0 {
        spans.push(Span::styled(format!("  {steering} steering"), muted_style));
    }
    if followups > 0 {
        spans.push(Span::styled(format!("  {followups} follow-up"), muted_style));
    }
    Some(Row::padded(spans, width, CellStyle::new().bg(bg)))
}

/// Build detail pane rows for the expanded tool entry.
///
/// Shows a title bar with the tool name and status, then the full output
/// wrapped into visual rows. The scroll offset is applied to those rendered
/// rows so long lines scroll by visible terminal row rather than by raw output
/// line.
pub fn detail_pane_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    if max_height == 0 {
        return Vec::new();
    }

    let Some(entry) = app.transcript.get(app.detail_pane.entry_index) else {
        return Vec::new();
    };
    let Entry::Tool { name, arguments, status, output } = entry else {
        return Vec::new();
    };

    let p = super::style::palette();
    let bg = p.surface0;
    let title_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let status_color = match status {
        ToolStatus::Running => p.peach,
        ToolStatus::Ok => p.green,
        ToolStatus::Failed => p.red,
        ToolStatus::Cancelled => p.peach,
    };
    let status_style = CellStyle::new().fg(status_color).bg(bg);
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let body_style = CellStyle::new().fg(p.text).bg(bg);
    let gutter_style = CellStyle::new().fg(p.overlay0).bg(bg);

    let status_label = match status {
        ToolStatus::Running => "running",
        ToolStatus::Ok => "ok",
        ToolStatus::Failed => "failed",
        ToolStatus::Cancelled => "cancelled",
    };

    let mut title_spans = vec![
        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
        Span::styled(name.to_string(), title_style),
        Span::styled(format!(" [{status_label}]"), status_style),
    ];

    let args_summary = renderer::transcript::summarize_tool_args(arguments, &app.cwd);
    if !args_summary.is_empty() {
        title_spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
        title_spans.push(Span::styled(args_summary, muted_style));
    }

    let body_width = super::layout::content_width(width).saturating_sub(utils::text_width(GUTTER));
    let mut body_rows = Vec::new();

    for line in output {
        let line = renderer::path_display::transcript_line(line, &app.cwd);
        for wrapped in super::layout::wrap_text(&line, body_width) {
            let spans = vec![Span::styled(GUTTER, gutter_style), Span::styled(wrapped, body_style)];
            body_rows.push(Row::padded(spans, width, CellStyle::new().bg(bg)));
        }
    }

    let mut rows = Vec::with_capacity(max_height);
    let scroll = app.detail_pane.scroll.min(body_rows.len().saturating_sub(1));
    let body_budget = max_height.saturating_sub(1);
    let hidden_above = scroll;
    let hidden_below = body_rows.len().saturating_sub(scroll + body_budget);

    rows.push(Row::padded(title_spans, width, CellStyle::new().bg(bg)));
    rows.extend(body_rows.into_iter().skip(scroll).take(body_budget));
    if (hidden_above > 0 || hidden_below > 0)
        && let Some(row) = rows.last_mut()
    {
        *row = clipped_detail_indicator_row(width, bg, muted_style, hidden_above, hidden_below);
    }
    rows
}

/// Build the static status row (model/reasoning/search/tokens/cwd) below the prompt.
///
/// Width-aware clipping hides segments that don't fit.
pub fn static_status_row(app: &App, width: usize) -> Row {
    let p = super::style::palette();
    let bg = p.surface0;
    let subtext = CellStyle::new().fg(p.subtext0).bg(bg);
    let trust_text = "local user · workspace-contained tools · no TUI sandbox";
    let muted = CellStyle::new().fg(p.overlay0).bg(bg);
    let model_label = format!("model: {}", codex::display_model_id(&app.model));
    let reasoning_text =
        supports_reasoning_status(&app.model).then(|| format!("reasoning: {}", app.cli.reasoning_effort.label()));
    let search_label = app.websearch.label();
    let search_text = format!("search: {search_label}");
    let token_text = format!("tok: ↑{} ↓{}", app.session_tokens_in, app.session_tokens_out);
    let ttft_text = ttft_status_text(app);
    let git_text = app.git_status.as_ref().map(|summary| summary.display());
    let token_style = CellStyle::new().fg(p.peach).bg(bg);
    let ttft_style = CellStyle::new().fg(p.teal).bg(bg);
    let git_style = CellStyle::new().fg(p.green).bg(bg);

    let (show_model, show_reasoning, show_search, show_tokens, show_ttft, show_git, show_cwd, show_trust) = match width
    {
        w if w < 24 => (false, false, false, false, false, false, false, false),
        w if w < 42 => (true, false, false, false, false, false, false, false),
        w if w < 56 => (true, false, true, false, false, false, false, false),
        w if w < 72 => (true, false, true, true, false, false, false, false),
        w if w < 88 => (true, true, true, true, false, true, false, false),
        w if w < 97 => (true, true, true, true, true, true, false, false),
        w if w < 160 => (true, true, true, true, true, true, true, false),
        _ => (true, true, true, true, true, true, true, true),
    };

    let mut spans: Vec<Span> = Vec::new();
    let mut used = LIVE_INSET;
    spans.push(Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)));

    let mut push_segment = |text: &str, style: CellStyle, used: &mut usize| {
        let segment_len = utils::text_width(text);
        if *used == LIVE_INSET {
            *used = used.saturating_add(segment_len);
            spans.push(Span::styled(text.to_string(), style));
            return;
        }

        let separator_len = 3;
        if *used + separator_len + segment_len > width {
            return;
        }

        spans.push(Span::styled("   ", CellStyle::new().bg(bg)));
        spans.push(Span::styled(text.to_string(), style));
        *used = used.saturating_add(separator_len + segment_len);
    };

    if show_model {
        push_segment(&model_label, subtext, &mut used);
    }
    if show_reasoning && let Some(reasoning_text) = reasoning_text.as_deref() {
        push_segment(reasoning_text, CellStyle::new().fg(p.mauve).bg(bg), &mut used);
    }
    if show_search {
        push_segment(&search_text, subtext, &mut used);
    }
    if show_tokens {
        push_segment(&token_text, token_style, &mut used);
    }
    if show_ttft && let Some(ttft_text) = ttft_text {
        push_segment(&ttft_text, ttft_style, &mut used);
    }
    if show_git && let Some(git_text) = git_text {
        push_segment(&git_text, git_style, &mut used);
    }
    if show_trust {
        push_segment(trust_text, subtext, &mut used);
    }
    if show_cwd {
        let mut used = used + 6;
        let cwd_display = super::path_display::footer_segment(&app.cwd, width, used);
        push_segment(&cwd_display, muted, &mut used);
    }

    Row::padded(spans, width, CellStyle::new().bg(bg))
}

fn first_run_recovery_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    let Some(recovery) = app.first_run_recovery.as_ref() else {
        return Vec::new();
    };
    let p = super::style::palette();
    let bg = p.surface0;
    let title_style = CellStyle::new().fg(p.peach).bg(bg).bold();
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let text_style = CellStyle::new().fg(p.text).bg(bg);
    let selected_style = CellStyle::new().fg(p.text).bg(bg).bold();

    let provider = if recovery.stage == RecoveryStage::ChooseProvider {
        "choose"
    } else {
        recovery.provider.map(|provider| provider.label()).unwrap_or("acp")
    };
    let missing = recovery.missing_label();

    let mut rows = Vec::new();
    rows.push(Row::padded(
        vec![
            Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
            Span::styled("setup", title_style),
            Span::styled(
                format!("  provider={provider} model={} missing={missing}", app.model),
                muted_style,
            ),
        ],
        width,
        CellStyle::new().bg(bg),
    ));

    match recovery.stage {
        RecoveryStage::ChooseProvider => {
            rows.push(recovery_body_row(
                width,
                bg,
                text_style,
                "Choose the provider to configure.",
            ));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &[
                    "ChatGPT Codex  [first class]",
                    "Umans  [first class]",
                    "advanced providers / ACP",
                ],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::ModelSelection => {
            rows.push(recovery_body_row(
                width,
                bg,
                text_style,
                "Choose a model for this authenticated provider.",
            ));
            let actions = recovery.provider.map(setup_model_options).unwrap_or_default();
            let action_labels: Vec<String> = actions.into_iter().map(|item| item.label).collect();
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &action_labels,
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::ModelConfigScope => {
            rows.push(recovery_body_row(
                width,
                bg,
                text_style,
                "Save the selected model to config?",
            ));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["project config", "global config", "skip model config", "cancel setup"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::MissingCredential => {
            let body = match recovery.provider {
                Some(commands::setup::SetupProviderArg::ChatgptCodex) => {
                    "Missing ChatGPT OAuth credential. Sign in with your ChatGPT account before submitting this prompt."
                }
                Some(commands::setup::SetupProviderArg::Umans) => {
                    "Missing Umans API key. Enter it to store it outside config before submitting this prompt."
                }
                Some(commands::setup::SetupProviderArg::OpencodeGo) => {
                    "Missing OpenCode Go API key. Run CLI setup or enter the key before submitting this prompt."
                }
                Some(commands::setup::SetupProviderArg::OpencodeZen) => {
                    "Missing OpenCode Zen API key. Run CLI setup or enter the key before submitting this prompt."
                }
                None => {
                    "Missing provider configuration. Choose an available setup route before submitting this prompt."
                }
            };
            rows.push(recovery_body_row(width, bg, text_style, body));
            let actions = if recovery.provider == Some(commands::setup::SetupProviderArg::ChatgptCodex) {
                &[
                    "start ChatGPT OAuth login",
                    "switch model/provider",
                    "show setup instructions",
                    "continue without setup",
                    "quit",
                ][..]
            } else {
                &[
                    "enter API key",
                    "switch model/provider",
                    "show setup instructions",
                    "continue without setup",
                    "quit",
                ][..]
            };
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                actions,
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::EnterKey => {
            let credential_label = recovery
                .provider
                .map(|provider| format!("{} API key", provider.label()))
                .unwrap_or_else(|| String::from("API key"));
            let body = format!("Type the {credential_label}. Input is hidden. Enter continues, Esc cancels.");
            rows.push(recovery_body_row(width, bg, text_style, &body));
        }
        RecoveryStage::ConfirmStore => {
            rows.push(recovery_body_row(width, bg, text_style, "Store this credential where?"));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["global credentials", "project credentials", "cancel"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::Instructions => {
            // FIXME: pattern matching
            let text = if recovery.provider == Some(commands::setup::SetupProviderArg::ChatgptCodex) {
                "Run `thndrs setup --provider chatgpt-codex` or `thndrs login chatgpt-codex` outside the TUI."
            } else if recovery.provider.is_none() {
                "Advanced providers remain available: use `thndrs setup --provider opencode-zen|opencode-go` or configure an ACP model."
            } else {
                "Run `thndrs setup` or `thndrs login <provider>` outside the TUI."
            };
            rows.push(recovery_body_row(width, bg, text_style, text));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["back", "close"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::ChatGptOAuthRequesting => {
            rows.push(recovery_body_row(
                width,
                bg,
                text_style,
                "Requesting a ChatGPT OAuth device code.",
            ));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["cancel"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::ChatGptOAuthPolling => {
            if let Some(oauth) = recovery.chatgpt_oauth.as_ref() {
                let verification_uri = oauth
                    .code
                    .verification_uri
                    .as_deref()
                    .unwrap_or("https://auth.openai.com/codex/device");
                rows.push(recovery_body_row(
                    width,
                    bg,
                    text_style,
                    &format!("Open {verification_uri} and enter code {}", oauth.code.user_code),
                ));
                rows.push(recovery_body_row(width, bg, muted_style, &oauth.status));
            } else {
                rows.push(recovery_body_row(width, bg, text_style, "Waiting for ChatGPT OAuth."));
            }
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["cancel"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::ChatGptOAuthFailed => {
            let text = recovery
                .chatgpt_oauth
                .as_ref()
                .map(|oauth| oauth.status.as_str())
                .unwrap_or("ChatGPT OAuth failed. Run `thndrs login chatgpt-codex` for browser fallback.");
            rows.push(recovery_body_row(width, bg, text_style, text));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["retry ChatGPT OAuth login", "back"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::LogoutConfirm => {
            rows.push(recovery_body_row(
                width,
                bg,
                text_style,
                "Remove the stored credential from which store?",
            ));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &["global credentials", "project credentials", "cancel"],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
        RecoveryStage::AcpMissing => {
            rows.push(recovery_body_row(
                width,
                bg,
                text_style,
                "ACP models use ACP agent config, not provider API keys.",
            ));
            push_recovery_actions(
                &mut rows,
                ActionDimensions(width, max_height),
                recovery.selected,
                &[
                    "switch model/provider",
                    "show ACP setup",
                    "continue without setup",
                    "quit",
                ],
                ActionStyles::new(selected_style, muted_style, text_style, bg),
            );
        }
    }

    rows.truncate(max_height);
    rows
}

fn recovery_body_row(width: usize, bg: Color, style: CellStyle, text: &str) -> Row {
    Row::padded(
        vec![
            Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
            Span::styled(text.to_string(), style),
        ],
        width,
        CellStyle::new().bg(bg),
    )
}

fn push_recovery_actions<T: AsRef<str>>(
    rows: &mut Vec<Row>, dims: ActionDimensions, selected: usize, actions: &[T], styles: ActionStyles,
) {
    for (index, action) in actions.iter().enumerate() {
        if rows.len() >= dims.max_height() {
            break;
        }
        let is_selected = index == selected;
        let marker = if is_selected { "›" } else { " " };
        rows.push(Row::padded(
            vec![
                Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(styles.bg)),
                Span::styled(
                    marker,
                    if is_selected { styles.selected_style } else { styles.muted_style },
                ),
                Span::styled(" ", CellStyle::new().bg(styles.bg)),
                Span::styled(
                    action.as_ref().to_string(),
                    if is_selected { styles.selected_style } else { styles.text_style },
                ),
            ],
            dims.width(),
            CellStyle::new().bg(styles.bg),
        ));
    }
}

fn permission_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    match app.pending_permission.as_ref() {
        Some(permission) => {
            let mut rows = Vec::new();
            let p = super::style::palette();
            let bg = p.surface0;
            let title_style = CellStyle::new().fg(p.peach).bg(bg).bold();
            let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
            let selected_style = CellStyle::new().fg(p.text).bg(bg).bold();
            let option_style = CellStyle::new().fg(p.text).bg(bg);

            rows.push(Row::padded(
                vec![
                    Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
                    Span::styled("permission", title_style),
                    Span::styled(format!("  {}", permission.title), muted_style),
                ],
                width,
                CellStyle::new().bg(bg),
            ));

            for (index, option) in permission.options.iter().enumerate() {
                if rows.len() >= max_height {
                    break;
                }
                let selected = index == permission.selected;
                let marker = if selected { "›" } else { " " };
                rows.push(Row::padded(
                    vec![
                        Span::styled(" ".repeat(LIVE_INSET), CellStyle::new().bg(bg)),
                        Span::styled(marker, if selected { selected_style } else { muted_style }),
                        Span::styled(" ", CellStyle::new().bg(bg)),
                        Span::styled(
                            option.name.clone(),
                            if selected { selected_style } else { option_style },
                        ),
                        Span::styled(format!("  {}", option.kind.label()), muted_style),
                    ],
                    width,
                    CellStyle::new().bg(bg),
                ));
            }
            rows
        }
        None => Vec::new(),
    }
}

fn clipped_detail_indicator_row(
    width: usize, bg: Color, style: CellStyle, hidden_above: usize, hidden_below: usize,
) -> Row {
    let text = match (hidden_above, hidden_below) {
        (0, below) => format!("   │ … {below} rows below"),
        (above, 0) => format!("   │ … {above} rows above"),
        (above, below) => format!("   │ … {above} rows above, {below} below"),
    };
    Row::padded(vec![Span::styled(text, style)], width, CellStyle::new().bg(bg))
}

fn ttft_status_text(app: &App) -> Option<String> {
    if app.ttft.is_pending() {
        return Some(String::from("ttft: pending"));
    }

    app.ttft.last_completed().map(|duration| {
        let millis = duration.as_millis();
        if millis < 1_000 {
            format!("ttft: {millis}ms")
        } else {
            format!("ttft: {:.1}s", millis as f64 / 1_000.0)
        }
    })
}

fn supports_reasoning_status(model: &str) -> bool {
    codex::supports_reasoning_effort(model) || umans::reasoning_options(model).len() > 1
}

fn command_rows(app: &App, selected: usize, width: usize, max_height: usize) -> Vec<Row> {
    let p = super::style::palette();
    let bg = p.surface0;
    let commands = crate::app::command_suggestions_for_app(app);

    if commands.is_empty() {
        return vec![Row::padded(
            vec![Span::styled(
                "no matching commands",
                CellStyle::new().fg(p.overlay0).bg(bg),
            )],
            width,
            CellStyle::new().bg(bg),
        )];
    }

    commands
        .into_iter()
        .enumerate()
        .take(max_height)
        .map(|(i, (cmd, desc))| {
            let is_selected = i == selected;
            let row_bg = if is_selected { p.surface1 } else { bg };
            let marker = if is_selected { "›" } else { " " };
            let marker_style =
                if is_selected { CellStyle::new().fg(p.peach).bg(row_bg).bold() } else { CellStyle::new().bg(bg) };
            let cmd_style = if is_selected {
                CellStyle::new().fg(p.text).bg(row_bg).bold()
            } else {
                CellStyle::new().fg(p.subtext0).bg(bg)
            };
            let desc_style = CellStyle::new().fg(p.overlay0).bg(row_bg);
            let spans = vec![
                Span::styled(marker, marker_style),
                Span::styled(" ", CellStyle::new().bg(row_bg)),
                Span::styled(cmd.to_string(), cmd_style),
                Span::styled(format!("  {desc}"), desc_style),
            ];
            Row::padded(spans, width, CellStyle::new().bg(row_bg))
        })
        .collect()
}

/// Build styled spans for a prompt line, highlighting `@path` mentions.
///
/// Mention patterns are `@` followed by path-like characters (word chars,
/// `/`, `.`, `-`, `_`).
///
/// The `@` and path are styled with `mention_style` whereas all other text uses `text_style`
fn mention_styled_spans(line: &str, text_style: CellStyle, mention_style: CellStyle, _bg: Color) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '@' {
            if !current.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut current), text_style));
            }

            let mut mention = String::from('@');
            i += 1;
            while i < chars.len() && is_mention_char(chars[i]) {
                mention.push(chars[i]);
                i += 1;
            }

            if mention.len() > 1 {
                spans.push(Span::styled(mention, mention_style));
            } else {
                spans.push(Span::styled("@", text_style));
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, text_style));
    }

    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), text_style));
    }

    spans
}

/// Whether a character is valid in a file mention path after `@`.
fn is_mention_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | '~')
}

/// Build fuzzy picker rows for the live region.
///
/// Renders the query header, match list with selection marker + fuzzy highlight
/// indices + long label clipping, "no matches" row, and footer hints.
fn picker_rows(app: &App, title: &str, width: usize, max_height: usize) -> Vec<Row> {
    let p = super::style::palette();
    let bg = p.surface0;
    let surface1 = p.surface1;
    let label_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let muted_style = CellStyle::new().fg(p.overlay0).bg(bg);
    let text_style = CellStyle::new().fg(p.text).bg(bg);
    let highlight_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let selected_style = CellStyle::new().fg(p.text).bg(surface1).bold();
    let selected_marker_style = CellStyle::new().fg(p.peach).bg(surface1).bold();

    let Some(picker) = app.picker.as_ref() else {
        return vec![Row::padded(
            vec![Span::styled(format!("{title} loading"), muted_style)],
            width,
            CellStyle::new().bg(bg),
        )];
    };

    let mut rows = Vec::new();

    let query_display = if picker.query.is_empty() { "type to filter".to_string() } else { picker.query.clone() };
    rows.push(Row::padded(
        vec![
            Span::styled(title.to_string(), label_style),
            Span::styled("  ", CellStyle::new().bg(bg)),
            Span::styled(query_display, muted_style),
        ],
        width,
        CellStyle::new().bg(bg),
    ));

    if picker.matches.is_empty() {
        rows.push(Row::padded(
            vec![Span::styled("no matches", muted_style)],
            width,
            CellStyle::new().bg(bg),
        ));
    } else {
        let visible_rows = picker.matches.len().clamp(1, crate::app::VISIBLE_ROWS);
        let end = (picker.scroll + visible_rows).min(picker.matches.len());
        let available = width.saturating_sub(6);

        for (idx, item) in picker.matches[picker.scroll..end].iter().enumerate() {
            let absolute_idx = picker.scroll + idx;
            let is_selected = absolute_idx == picker.selected;
            let row_bg = if is_selected { surface1 } else { bg };
            let marker = if is_selected { "›" } else { " " };
            let marker_style = if is_selected { selected_marker_style } else { CellStyle::new().bg(bg) };

            let detail_len = if item.detail.is_empty() { 0 } else { utils::text_width(&item.detail).min(24) + 2 };
            let label_available = available.saturating_sub(detail_len).max(8);
            let truncated = utils::truncate_ellipsis(&item.label, label_available);
            let indices = picker.match_indices.get(absolute_idx).cloned().unwrap_or_default();

            let label_spans = build_fuzzy_highlight_spans(
                &truncated,
                &indices,
                if is_selected { selected_style } else { text_style },
                highlight_style.with_bg(row_bg),
            );
            let detail_style = CellStyle::new().fg(p.overlay0).bg(row_bg);

            let mut spans = vec![
                Span::styled(marker, marker_style),
                Span::styled("  ", CellStyle::new().bg(row_bg)),
            ];
            spans.extend(label_spans);
            if !item.detail.is_empty() {
                spans.push(Span::styled("  ", CellStyle::new().bg(row_bg)));
                spans.push(Span::styled(
                    utils::truncate_ellipsis(
                        &item.detail,
                        available.saturating_sub(utils::text_width(&truncated) + 2),
                    ),
                    detail_style,
                ));
            }
            rows.push(Row::padded(spans, width, CellStyle::new().bg(row_bg)));
        }
    }

    rows.push(Row::padded(
        vec![
            Span::styled("Enter", label_style),
            Span::styled(" select   ", muted_style),
            Span::styled("Esc", label_style),
            Span::styled(" close", muted_style),
        ],
        width,
        CellStyle::new().bg(bg),
    ));

    rows.truncate(max_height.max(1));
    rows
}

/// Build styled spans for a path with fuzzy match indices highlighted.
///
/// Characters at the given indices are rendered with `highlight_style`; all
/// others use `base_style`.
///
/// The indices refer to char positions in the original path, but since we
/// truncate with ellipsis, indices beyond the truncated length are skipped.
fn build_fuzzy_highlight_spans(
    text: &str, indices: &[usize], base_style: CellStyle, highlight_style: CellStyle,
) -> Vec<Span> {
    if indices.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    let chars: Vec<char> = text.chars().collect();
    let index_set: HashSet<usize> = indices.iter().copied().collect();

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_is_highlight = None;

    for (i, ch) in chars.iter().enumerate() {
        let is_highlight = index_set.contains(&i);
        match current_is_highlight {
            None => {
                current.push(*ch);
                current_is_highlight = Some(is_highlight);
            }
            Some(prev) if prev == is_highlight => {
                current.push(*ch);
            }
            Some(_) => {
                let style = if current_is_highlight.unwrap() { highlight_style } else { base_style };
                spans.push(Span::styled(std::mem::take(&mut current), style));
                current.push(*ch);
                current_is_highlight = Some(is_highlight);
            }
        }
    }

    if !current.is_empty() {
        let style = if current_is_highlight.unwrap_or(false) { highlight_style } else { base_style };
        spans.push(Span::styled(current, style));
    }
    spans
}

fn help_rows(app: &App, width: usize, max_height: usize) -> Vec<Row> {
    let p = super::style::palette();
    let bg = p.surface0;
    let label_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let desc_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let section_style = CellStyle::new().fg(p.overlay1).bg(bg).bold();

    let ctrl_t_desc = if matches!(app.run_state, RunState::Working) {
        "toggle queue target"
    } else {
        "transpose characters"
    };
    let entries: &[(&str, &str)] = &[
        ("── Navigation ──", ""),
        ("Up/Down", "move cursor or recall history"),
        ("Enter", "accept highlighted item"),
        ("Escape", "close help, files, or commands"),
        ("── Editing ──", ""),
        ("Shift+Enter", "insert newline"),
        ("Ctrl+A/E", "move to start/end"),
        ("Ctrl+B/F", "move cursor left/right"),
        ("Ctrl+W", "delete previous word"),
        ("Ctrl+K", "delete to end of line"),
        ("Ctrl+U", "delete to start of line"),
        ("Ctrl+Y", "yank (paste) last kill"),
        ("Ctrl+T", ctrl_t_desc),
        ("Alt+B/F", "move word left/right"),
        ("Alt+D", "delete next word"),
        ("Alt+Bksp", "delete previous word"),
        ("── Files ──", ""),
        ("@path", "mention a file from fuzzy search"),
        ("── App ──", ""),
        ("Ctrl+D", "quit after double-press"),
    ];

    entries
        .iter()
        .take(max_height)
        .map(|&(key, desc)| {
            if desc.is_empty() {
                Row::padded(
                    vec![Span::styled(key.to_string(), section_style)],
                    width,
                    CellStyle::new().bg(bg),
                )
            } else {
                Row::padded(
                    vec![
                        Span::styled(format!("{key:<16}"), label_style),
                        Span::styled(desc.to_string(), desc_style),
                    ],
                    width,
                    CellStyle::new().bg(bg),
                )
            }
        })
        .collect()
}
