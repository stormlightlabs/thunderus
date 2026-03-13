use super::hint_bar::HintToken;
use super::theme::{Theme, resolve_theme};
use super::{HintBar, InputField};
use crate::ScreenAction;
use crate::chat::{
    ChatApp, ChatMessage, ChatMsg, IncomingStreamEvent, MessageRole, StreamingState, ToolCallStatus,
    map_chat_key_to_msg, u32_with_grouping, update as update_chat_model,
};
use ::iocraft::prelude::*;
use chrono::{DateTime, Local, Utc};
use std::cmp;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thndrs_core::{ResponseSections, estimate_token_cost_usd};

const DEFAULT_VIEWPORT_WIDTH: u16 = 80;
const DEFAULT_VIEWPORT_HEIGHT: u16 = 24;
const TOKEN_ROW_HEIGHT: u16 = 2;
const HINT_ROW_HEIGHT: u16 = 1;
const FILE_FINDER_HEIGHT: u16 = 12;
const FILE_FINDER_MAX_ITEMS: usize = 8;

#[derive(Default, Props)]
pub struct ChatScreenProps {
    pub initial_chat: Option<ChatApp>,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub stream_rx: Option<Arc<Mutex<Receiver<IncomingStreamEvent>>>>,
    pub on_submit: HandlerMut<'static, String>,
    pub on_command: HandlerMut<'static, String>,
    pub on_action: HandlerMut<'static, ScreenAction>,
}

struct ChatCallbacks {
    on_submit: HandlerMut<'static, String>,
    on_command: HandlerMut<'static, String>,
    on_action: HandlerMut<'static, ScreenAction>,
}

#[derive(Clone)]
struct RenderedLine {
    text: String,
    color: Color,
    weight: Weight,
}

impl RenderedLine {
    fn new(text: impl Into<String>, color: Color) -> Self {
        Self { text: text.into(), color, weight: Weight::Normal }
    }

    fn bold(text: impl Into<String>, color: Color) -> Self {
        Self { text: text.into(), color, weight: Weight::Bold }
    }

    fn blank(theme: Theme) -> Self {
        Self::new(" ", theme.text_muted)
    }
}

#[derive(Default, Props)]
struct TokenRowProps {
    content: String,
    width: u16,
}

#[component]
fn TokenRow(hooks: Hooks, props: &TokenRowProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let line = truncate_text(&props.content, props.width.saturating_sub(4) as usize);

    element! {
        View(height: TOKEN_ROW_HEIGHT, width: 100pct) {
            InputField(prompt: "", value: "", has_focus: false, multiline: false, on_change: |_| {}) {
                Text(content: line, color: theme.text_muted, wrap: TextWrap::NoWrap)
            }
        }
    }
}

#[derive(Default, Props)]
struct ChatInputProps {
    lines: Vec<RenderedLine>,
    height: u16,
}

#[component]
fn ChatInput(_hooks: Hooks, props: &ChatInputProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(height: props.height, width: 100pct) {
            InputField(prompt: "", value: "", has_focus: false, multiline: true, on_change: |_| {}) {
                #(props.lines.iter().cloned().map(|line| element! {
                    Text(
                        content: line.text,
                        color: line.color,
                        weight: line.weight,
                        wrap: TextWrap::NoWrap,
                    )
                }))
            }
        }
    }
}

#[component]
pub fn ChatScreen(mut hooks: Hooks, props: &mut ChatScreenProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let (terminal_width, terminal_height) = hooks.use_terminal_size();
    let viewport_width = resolve_dimension(props.viewport_width, terminal_width, DEFAULT_VIEWPORT_WIDTH);
    let viewport_height = resolve_dimension(props.viewport_height, terminal_height, DEFAULT_VIEWPORT_HEIGHT);
    let model = hooks.use_state({
        let initial_chat = props.initial_chat.clone();
        move || initial_chat.unwrap_or_default()
    });
    let mut scroll_rows = hooks.use_state(|| 0usize);
    let callbacks = hooks.use_ref(|| ChatCallbacks {
        on_submit: props.on_submit.take(),
        on_command: props.on_command.take(),
        on_action: props.on_action.take(),
    });

    hooks.use_terminal_events({
        let mut model = model;
        let mut callbacks = callbacks;
        let mut scroll_rows = scroll_rows;
        move |event| {
            if let TerminalEvent::Key(key) = event {
                match key.code {
                    KeyCode::PageUp if key.kind == KeyEventKind::Press => {
                        let snapshot = model.read().clone();
                        let transcript_width = viewport_width.saturating_sub(2).max(1);
                        let transcript_height = message_viewport_height(&snapshot, viewport_width, viewport_height);
                        let transcript_lines = transcript_lines(&snapshot, transcript_width, &theme, transcript_height);
                        let max_offset = transcript_lines.len().saturating_sub(transcript_height as usize);
                        let next = cmp::min(
                            scroll_rows.get().saturating_add(page_scroll_amount(transcript_height)),
                            max_offset,
                        );
                        scroll_rows.set(next);
                    }
                    KeyCode::PageDown if key.kind == KeyEventKind::Press => {
                        let next = scroll_rows
                            .get()
                            .saturating_sub(page_scroll_amount(message_viewport_height(
                                &model.read(),
                                viewport_width,
                                viewport_height,
                            )));
                        scroll_rows.set(next);
                    }
                    _ => {
                        let file_finder_active = model.read().is_file_finder_active();
                        if let Some(msg) = map_terminal_key_to_msg(&key, file_finder_active) {
                            dispatch_chat_message(&mut model, &mut callbacks, &mut scroll_rows, msg);
                        }
                    }
                }
            }
        }
    });

    hooks.use_future({
        let mut model = model;
        let mut callbacks = callbacks;
        let mut scroll_rows = scroll_rows;
        let stream_rx = props.stream_rx.clone();
        async move {
            let Some(stream_rx) = stream_rx else {
                return;
            };

            loop {
                let next_event = {
                    let receiver = stream_rx.lock().expect("stream receiver lock should not be poisoned");
                    match receiver.try_recv() {
                        Ok(event) => Some(Ok(event)),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => Some(Err(())),
                    }
                };

                match next_event {
                    Some(Ok(event)) => {
                        dispatch_chat_message(
                            &mut model,
                            &mut callbacks,
                            &mut scroll_rows,
                            ChatMsg::StreamEvent(event),
                        );
                    }
                    Some(Err(())) => break,
                    None => {
                        smol::Timer::after(Duration::from_millis(16)).await;
                    }
                }
            }
        }
    });

    let snapshot = model.read().clone();
    let transcript_width = viewport_width.saturating_sub(2).max(1);
    let transcript_height = message_viewport_height(&snapshot, viewport_width, viewport_height);
    let transcript = transcript_lines(&snapshot, transcript_width, &theme, transcript_height);
    let max_scroll = transcript.len().saturating_sub(transcript_height as usize);
    let clamped_scroll = scroll_rows.get().min(max_scroll);
    if clamped_scroll != scroll_rows.get() {
        scroll_rows.set(clamped_scroll);
    }
    let visible_transcript = visible_transcript_lines(&transcript, transcript_height as usize, clamped_scroll);
    let token_row = token_row_content(&snapshot);
    let input_lines = rendered_input_lines(&snapshot, viewport_width, &theme);
    let input_height = chat_input_height(input_lines.len());

    element! {
        View(
            width: viewport_width,
            height: viewport_height,
            flex_direction: FlexDirection::Column,
            background_color: theme.bg_terminal,
            position: Position::Relative,
        ) {
            #(if snapshot.has_messages() {
                Some(element! {
                    View(
                        height: transcript_height,
                        width: 100pct,
                        flex_direction: FlexDirection::Column,
                        padding_left: 1,
                        padding_right: 1,
                        padding_top: 1,
                    ) {
                        #(visible_transcript.into_iter().map(|line| {
                            element! {
                                Text(
                                    content: line.text,
                                    color: line.color,
                                    weight: line.weight,
                                    wrap: TextWrap::NoWrap,
                                )
                            }
                        }))
                    }
                }.into_any())
            } else {
                Some(element! {
                    View(
                        height: transcript_height,
                        width: 100pct,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding_left: 2,
                        padding_right: 2,
                    ) {
                        Text(
                            content: "Start a chat to see the transcript.",
                            color: theme.text_muted,
                            align: TextAlign::Center,
                            wrap: TextWrap::Wrap,
                        )
                    }
                }.into_any())
            })
            HintBar(tokens: hint_tokens(snapshot.is_file_finder_active()))
            TokenRow(content: token_row, width: viewport_width)
            ChatInput(lines: input_lines, height: input_height)
            #(if snapshot.is_file_finder_active() {
                Some(file_finder_overlay(&snapshot, viewport_width, viewport_height, theme))
            } else {
                None
            })
        }
    }
}

fn resolve_dimension(explicit: u16, measured: u16, fallback: u16) -> u16 {
    if explicit > 0 {
        explicit
    } else if measured > 0 {
        measured
    } else {
        fallback
    }
}

fn page_scroll_amount(transcript_height: u16) -> usize {
    transcript_height.saturating_sub(1).max(1) as usize
}

fn message_viewport_height(model: &ChatApp, viewport_width: u16, viewport_height: u16) -> u16 {
    let input_lines = rendered_input_lines(model, viewport_width, &Theme::default());
    let input_height = chat_input_height(input_lines.len());
    viewport_height
        .saturating_sub(HINT_ROW_HEIGHT)
        .saturating_sub(TOKEN_ROW_HEIGHT)
        .saturating_sub(input_height)
        .max(1)
}

fn chat_input_height(line_count: usize) -> u16 {
    line_count.max(1) as u16 + 1
}

fn map_terminal_key_to_msg(key: &KeyEvent, file_finder_active: bool) -> Option<ChatMsg> {
    map_chat_key_to_msg(crossterm_key_event(key), file_finder_active)
}

fn crossterm_key_event(key: &KeyEvent) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent {
        code: key.code,
        modifiers: key.modifiers,
        kind: match key.kind {
            KeyEventKind::Press => crossterm::event::KeyEventKind::Press,
            KeyEventKind::Repeat => crossterm::event::KeyEventKind::Repeat,
            KeyEventKind::Release => crossterm::event::KeyEventKind::Release,
        },
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn dispatch_chat_message(
    model: &mut State<ChatApp>, callbacks: &mut Ref<ChatCallbacks>, scroll_rows: &mut State<usize>, msg: ChatMsg,
) {
    let stay_pinned_to_bottom = scroll_rows.get() == 0;
    let mut next = model.read().clone();
    let action = update_chat_model(&mut next, msg);

    {
        let mut callbacks = callbacks.write();
        if let Some(submission) = next.take_pending_submission() {
            (callbacks.on_submit)(submission);
        }
        if let Some(command) = next.take_pending_command() {
            (callbacks.on_command)(command);
        }
        if action != ScreenAction::None {
            (callbacks.on_action)(action);
        }
    }

    if stay_pinned_to_bottom {
        scroll_rows.set(0);
    }

    model.set(next);
}

fn transcript_lines(model: &ChatApp, width: u16, theme: &Theme, transcript_height: u16) -> Vec<RenderedLine> {
    let mut lines = Vec::new();

    for (idx, message) in model.messages.iter().enumerate() {
        let is_latest = idx + 1 == model.messages.len();
        lines.extend(message_lines(message, width, *theme, &model.streaming_state, is_latest));
        if idx + 1 < model.messages.len() {
            lines.push(RenderedLine::new("─".repeat(width as usize), theme.border_color));
        }
    }

    if lines.is_empty() {
        return lines;
    }

    while lines.len() < transcript_height as usize {
        lines.push(RenderedLine::blank(*theme));
    }

    lines
}

fn visible_transcript_lines(
    transcript: &[RenderedLine], viewport_height: usize, scroll_rows: usize,
) -> Vec<RenderedLine> {
    if transcript.is_empty() {
        return Vec::new();
    }

    let start = transcript
        .len()
        .saturating_sub(viewport_height.saturating_add(scroll_rows));
    transcript.iter().skip(start).take(viewport_height).cloned().collect()
}

fn message_lines(
    message: &ChatMessage, width: u16, theme: Theme, streaming_state: &StreamingState, is_latest: bool,
) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    lines.push(RenderedLine::bold(
        format!("[{} {}]", message.role.label(), timestamp(message.created_at)),
        theme.text_muted,
    ));

    match message.role {
        MessageRole::User => {
            lines.extend(prefixed_lines("❯ ", "  ", &message.content, width, theme.text_primary));
        }
        MessageRole::Assistant => {
            for tool_call in &message.tool_calls {
                lines.extend(tool_call_lines(tool_call, width, theme));
            }

            let reasoning_expanded =
                message.expanded_reasoning || (is_latest && *streaming_state != StreamingState::Idle);
            if reasoning_expanded
                && let Some(reasoning) = message.reasoning_content.as_deref()
                && !reasoning.trim().is_empty()
            {
                lines.push(RenderedLine::bold("↳ REASONING", theme.accent_purple));
                lines.extend(prefixed_lines("  ", "  ", reasoning, width, theme.text_muted));
            }

            if let Some(sections) = &message.sections {
                lines.extend(section_lines(sections, width, theme));
            } else if !message.content.is_empty() {
                lines.extend(prefixed_lines(
                    "  ",
                    "  ",
                    &message.content,
                    width,
                    theme.text_secondary,
                ));
            }

            if is_latest && let Some(label) = streaming_indicator_label(streaming_state) {
                lines.push(RenderedLine::new(" ", theme.text_muted));
                lines.push(RenderedLine::bold(label, theme.accent_yellow));
            }
        }
        MessageRole::Tool => {
            lines.extend(prefixed_lines(
                "tool> ",
                "      ",
                &message.content,
                width,
                theme.text_muted,
            ));
        }
    }

    if lines.len() == 1 {
        lines.push(RenderedLine::blank(theme));
    }

    lines
}

fn tool_call_lines(tool_call: &crate::ToolCallDisplay, width: u16, theme: Theme) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    let (glyph, color) = tool_call_status_glyph(tool_call.status, theme);
    let summary = if tool_call.arguments.trim().is_empty() {
        format!("{glyph} {}", tool_call.name)
    } else {
        format!(
            "{glyph} {}  {}",
            tool_call.name,
            truncate_text(&tool_call.arguments, width.saturating_sub(10) as usize)
        )
    };
    lines.extend(wrap_colored_lines(&summary, width, color, true));

    if tool_call.expanded {
        lines.extend(prefixed_lines(
            "  args: ",
            "        ",
            &tool_call.arguments,
            width,
            theme.text_muted,
        ));
        match tool_call.output.as_deref() {
            Some(output) if !output.trim().is_empty() => {
                lines.push(RenderedLine::bold("  output:", theme.text_muted));
                let output_color =
                    if tool_call.status == ToolCallStatus::Error { theme.accent_red } else { theme.text_secondary };
                lines.extend(prefixed_lines("    ", "    ", output, width, output_color));
            }
            _ if matches!(tool_call.status, ToolCallStatus::Pending | ToolCallStatus::Running) => {
                lines.push(RenderedLine::new("  waiting for tool output", theme.text_muted));
            }
            _ => {}
        }
    }

    lines
}

fn tool_call_status_glyph(status: ToolCallStatus, theme: Theme) -> (&'static str, Color) {
    match status {
        ToolCallStatus::Pending | ToolCallStatus::Running => ("◌", theme.accent_yellow),
        ToolCallStatus::Success => ("✓", theme.accent_green),
        ToolCallStatus::Error => ("✕", theme.accent_red),
    }
}

fn section_lines(sections: &ResponseSections, width: u16, theme: Theme) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    push_section(
        &mut lines,
        "◦",
        "INTENT",
        sections.intent.as_deref(),
        theme.accent_purple,
        width,
        theme.text_secondary,
    );
    push_section(
        &mut lines,
        "•",
        "ACTIONS",
        sections.actions.as_deref(),
        theme.accent_yellow,
        width,
        theme.text_secondary,
    );
    push_section(
        &mut lines,
        "✓",
        "RESULT",
        sections.result.as_deref(),
        theme.accent_green,
        width,
        theme.text_secondary,
    );
    push_section(
        &mut lines,
        "→",
        "NEXT",
        sections.next.as_deref(),
        theme.accent_cyan,
        width,
        theme.text_secondary,
    );
    lines
}

fn push_section(
    lines: &mut Vec<RenderedLine>, icon: &str, title: &str, body: Option<&str>, accent: Color, width: u16,
    body_color: Color,
) {
    let Some(body) = body else {
        return;
    };

    if !lines.is_empty() {
        lines.push(RenderedLine::new(" ", body_color));
    }

    lines.push(RenderedLine::bold(format!("│ {icon} {title}"), accent));
    lines.extend(prefixed_lines("│ ", "│ ", body, width, body_color));
}

fn streaming_indicator_label(state: &StreamingState) -> Option<String> {
    match state {
        StreamingState::Idle => None,
        StreamingState::Streaming => Some("… streaming response".to_string()),
        StreamingState::Thinking => Some("… thinking".to_string()),
        StreamingState::CallingTool(name) => Some(format!("… calling tool: {name}")),
    }
}

fn token_row_content(model: &ChatApp) -> String {
    let pinned_summary = if model.pinned_files().is_empty() {
        "pinned: none".to_string()
    } else {
        let joined = model
            .pinned_files()
            .iter()
            .take(2)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if model.pinned_files().len() > 2 {
            format!(" (+{} more)", model.pinned_files().len() - 2)
        } else {
            String::new()
        };
        format!("pinned: {joined}{suffix}")
    };

    let Some(usage) = model.last_usage else {
        let mut parts = vec!["Awaiting response".to_string(), pinned_summary];
        if let Some(warning) = model.submission_warning() {
            parts.push(warning.to_string());
        }
        return parts.join("  |  ");
    };

    let mut parts = Vec::new();
    if let Some(model_name) = model.last_model.as_deref() {
        parts.push(format!("model: {model_name}"));
    }
    parts.push(format!("prompt: {}", u32_with_grouping(usage.prompt_tokens)));
    parts.push(format!("completion: {}", u32_with_grouping(usage.completion_tokens)));
    parts.push(format!("total: {}", u32_with_grouping(usage.total_tokens)));
    if let Some(model_name) = model.last_model.as_deref()
        && let Some(cost) = estimate_token_cost_usd(model_name, usage.prompt_tokens, usage.completion_tokens)
    {
        parts.push(format!("est: {}", usd(cost)));
    }
    parts.push(pinned_summary);
    if let Some(warning) = model.submission_warning() {
        parts.push(warning.to_string());
    }
    parts.join("  |  ")
}

fn rendered_input_lines(model: &ChatApp, viewport_width: u16, theme: &Theme) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    let input_width = viewport_width.saturating_sub(4).max(1);

    if !model.pinned_files().is_empty() {
        let pinned = format!(
            "@ {}",
            model
                .pinned_files()
                .iter()
                .map(|path| format!("[{}]", path.display()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        lines.extend(wrap_colored_lines(
            &pinned,
            viewport_width.saturating_sub(2),
            theme.accent_green,
            false,
        ));
    }

    let input_with_cursor = input_with_cursor(&model.input_buffer, model.cursor_position);
    let wrapped = prefixed_lines("❯ ", "  ", &input_with_cursor, input_width + 2, theme.text_primary);
    lines.extend(wrapped);

    if lines.is_empty() {
        lines.push(RenderedLine::new("❯ █", theme.text_primary));
    }

    lines
}

fn input_with_cursor(input: &str, cursor_position: usize) -> String {
    let mut output = input.to_string();
    let cursor = cursor_position.min(output.len());
    output.insert(cursor, '\u{2588}');
    output
}

fn hint_tokens(file_finder_active: bool) -> Vec<HintToken> {
    if file_finder_active {
        return vec![
            HintToken::Text("Type to filter, ".to_string()),
            HintToken::Key("Enter".to_string()),
            HintToken::Text(" pin/unpin, ".to_string()),
            HintToken::Key("Esc".to_string()),
            HintToken::Text(" close, ".to_string()),
            HintToken::Key("ctrl+k".to_string()),
            HintToken::Text(" clear".to_string()),
        ];
    }

    vec![
        HintToken::Text("Press ".to_string()),
        HintToken::Key("Enter".to_string()),
        HintToken::Text(" send, ".to_string()),
        HintToken::Key("Shift+Enter".to_string()),
        HintToken::Text("/".to_string()),
        HintToken::Key("ctrl+j".to_string()),
        HintToken::Text(" newline, ".to_string()),
        HintToken::Key("@".to_string()),
        HintToken::Text(" pin files, ".to_string()),
        HintToken::Key("Tab".to_string()),
        HintToken::Text(" toggle tool, ".to_string()),
        HintToken::Key("ctrl+r".to_string()),
        HintToken::Text(" reasoning, ".to_string()),
        HintToken::Key("PageUp/Down".to_string()),
        HintToken::Text(" scroll, ".to_string()),
        HintToken::Key("ctrl+d".to_string()),
        HintToken::Text(" quit".to_string()),
    ]
}

fn file_finder_overlay(
    model: &ChatApp, viewport_width: u16, viewport_height: u16, theme: Theme,
) -> AnyElement<'static> {
    let overlay_width = cmp::min(72, viewport_width.saturating_sub(4)).max(24);
    let overlay_height = cmp::min(FILE_FINDER_HEIGHT, viewport_height.saturating_sub(2)).max(6);
    let left = viewport_width.saturating_sub(overlay_width) / 2;
    let top = viewport_height.saturating_sub(overlay_height) / 2;

    element! {
        View(
            position: Position::Absolute,
            left: left,
            top: top,
            width: overlay_width,
            height: overlay_height,
            border_style: BorderStyle::Round,
            border_color: theme.accent_cyan,
                background_color: theme.bg_terminal,
                flex_direction: FlexDirection::Column,
                padding_left: 1,
                padding_right: 1,
            ) {
                Text(content: "pin file to chat", color: theme.accent_cyan, weight: Weight::Bold)
                Text(
                    content: format!("@{}  (Enter pin/unpin)", model.file_finder_query()),
                    color: theme.text_primary,
                    wrap: TextWrap::NoWrap,
                )
            #(model.file_finder_rows(FILE_FINDER_MAX_ITEMS).into_iter().map(|(selected, pinned, path)| {
                    let prefix = if selected { "> " } else { "  " };
                    let pin = if pinned { "* " } else { "  " };
                    let color = if selected {
                        theme.accent_cyan
                    } else if pinned {
                        theme.accent_green
                    } else {
                        theme.text_secondary
                    };

                    element! {
                        Text(
                            content: format!("{prefix}{pin}{path}"),
                            color: color,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                }))
        }
    }
    .into_any()
}

fn prefixed_lines(
    prefix: &str, continuation_prefix: &str, content: &str, width: u16, color: Color,
) -> Vec<RenderedLine> {
    let prefix_width = prefix.chars().count() as u16;
    let continuation_width = continuation_prefix.chars().count() as u16;
    let first_width = width.saturating_sub(prefix_width).max(1);
    let continuation_width_available = width.saturating_sub(continuation_width).max(1);
    let segments = content.split('\n').collect::<Vec<_>>();
    let mut lines = Vec::new();

    for (segment_idx, segment) in segments.iter().enumerate() {
        let wrapped = wrap_text(
            segment,
            if segment_idx == 0 { first_width } else { continuation_width_available },
        );
        for (idx, part) in wrapped.into_iter().enumerate() {
            let active_prefix = if segment_idx == 0 && idx == 0 { prefix } else { continuation_prefix };
            lines.push(RenderedLine::new(format!("{active_prefix}{part}"), color));
        }
    }

    if lines.is_empty() {
        lines.push(RenderedLine::new(prefix.to_string(), color));
    }

    lines
}

fn wrap_colored_lines(content: &str, width: u16, color: Color, bold: bool) -> Vec<RenderedLine> {
    wrap_text(content, width)
        .into_iter()
        .map(
            |line| {
                if bold { RenderedLine::bold(line, color) } else { RenderedLine::new(line, color) }
            },
        )
        .collect()
}

fn wrap_text(content: &str, width: u16) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    for segment in content.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if segment.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut count = 0usize;
        for ch in segment.chars() {
            current.push(ch);
            count += 1;
            if count >= width as usize {
                lines.push(current);
                current = String::new();
                count = 0;
            }
        }

        if current.is_empty() {
            continue;
        }
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let mut truncated = chars.into_iter().take(keep).collect::<String>();
    truncated.push('…');
    truncated
}

fn timestamp(created_at: DateTime<Utc>) -> String {
    let local_time = created_at.with_timezone(&Local);
    let now = Local::now();
    if local_time.date_naive() == now.date_naive() {
        local_time.format("%H:%M").to_string()
    } else {
        local_time.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn usd(value: f64) -> String {
    if value < 0.0001 { format!("${value:.6}") } else { format!("${value:.4}") }
}

#[cfg(test)]
mod tests {
    use super::{ChatScreen, hint_tokens, visible_transcript_lines};
    use crate::ScreenAction;
    use crate::chat::{ChatApp, ChatMessage, IncomingStreamEvent, TokenUsage};
    use ::iocraft::prelude::*;
    use futures::stream::{self, StreamExt};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Default, Props)]
    struct ChatHarnessProps {
        initial_chat: Option<ChatApp>,
        mode: HarnessMode,
        stream_rx: Option<Arc<Mutex<mpsc::Receiver<IncomingStreamEvent>>>>,
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum HarnessMode {
        #[default]
        Submit,
        Command,
        Quit,
        TimedExit,
    }

    #[component]
    fn ChatHarness(props: &ChatHarnessProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut submitted = hooks.use_state(String::new);
        let mut command = hooks.use_state(String::new);
        let mut quit = hooks.use_state(|| false);
        let mut timed_exit = hooks.use_state(|| false);

        if props.mode == HarnessMode::TimedExit {
            hooks.use_future(async move {
                smol::Timer::after(Duration::from_millis(40)).await;
                timed_exit.set(true);
            });
        }

        let submitted_value = submitted.read().clone();
        let command_value = command.read().clone();
        let quit_value = quit.get();

        if !submitted_value.is_empty() || !command_value.is_empty() || quit_value || timed_exit.get() {
            system.exit();
            let status = if !submitted_value.is_empty() {
                format!("submitted:{submitted_value}")
            } else if !command_value.is_empty() {
                format!("command:{command_value}")
            } else if quit_value {
                "quit".to_string()
            } else {
                "timed".to_string()
            };

            return element! {
                Text(content: status)
            }
            .into_any();
        }

        let mode = props.mode;

        element! {
            ChatScreen(
                initial_chat: props.initial_chat.clone(),
                viewport_width: 72u16,
                viewport_height: 20u16,
                stream_rx: props.stream_rx.clone(),
                on_submit: move |value| {
                    if mode == HarnessMode::Submit {
                        submitted.set(value);
                    }
                },
                on_command: move |value| {
                    if mode == HarnessMode::Command {
                        command.set(value);
                    }
                },
                on_action: move |action| {
                    if mode == HarnessMode::Quit && action == ScreenAction::Quit {
                        quit.set(true);
                    }
                },
            )
        }
        .into_any()
    }

    #[test]
    fn chat_screen_renders_empty_state_and_hints() {
        let actual = element! {
            ChatScreen(viewport_width: 72u16, viewport_height: 20u16)
        }
        .to_string();

        assert!(actual.contains("Start a chat to see the transcript."));
        assert!(actual.contains("Press Enter send"));
    }

    #[test]
    fn chat_screen_submits_typed_input() {
        smol::block_on(async {
            let canvases = element! {
                ChatHarness(mode: HarnessMode::Submit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('h'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('i'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
            ])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("submitted:hi")));
        });
    }

    #[test]
    fn chat_screen_routes_slash_commands_separately() {
        smol::block_on(async {
            let canvases = element! {
                ChatHarness(mode: HarnessMode::Command)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('/'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('c'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('l'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('e'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('r'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
            ])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("command:/clear")));
        });
    }

    #[test]
    fn chat_screen_emits_quit_action() {
        smol::block_on(async {
            let canvases = element! {
                ChatHarness(mode: HarnessMode::Quit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                {
                    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('d'));
                    key.modifiers = KeyModifiers::CONTROL;
                    key
                },
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("quit")));
        });
    }

    #[test]
    fn chat_screen_renders_stream_updates_from_receiver() {
        smol::block_on(async {
            let mut initial_chat = ChatApp::new();
            initial_chat.submit_user_message("Explain this".to_string());

            let (tx, rx) = mpsc::channel();
            tx.send(IncomingStreamEvent::Delta {
                content: Some("Intent\n\nRender the iocraft chat screen.".to_string()),
                reasoning_content: Some("Collecting chat state.".to_string()),
            })
            .expect("delta event should send");
            tx.send(IncomingStreamEvent::Done {
                usage: Some(TokenUsage { prompt_tokens: 1200, completion_tokens: 300, total_tokens: 1500 }),
                model: Some("gpt-5".to_string()),
            })
            .expect("done event should send");
            drop(tx);

            let canvases = element! {
                ChatHarness(
                    mode: HarnessMode::TimedExit,
                    initial_chat: Some(initial_chat),
                    stream_rx: Some(Arc::new(Mutex::new(rx))),
                )
            }
            .mock_terminal_render_loop(MockTerminalConfig::default())
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(
                canvases
                    .iter()
                    .any(|canvas| canvas.contains("Render the iocraft chat screen."))
            );
            assert!(canvases.iter().any(|canvas| canvas.contains("model: gpt-5")));
        });
    }

    #[test]
    fn chat_screen_supports_transcript_scrolling() {
        smol::block_on(async {
            let mut initial_chat = ChatApp::new();
            let messages = (0..18)
                .flat_map(|idx| {
                    [
                        ChatMessage::user(format!("question {idx}")),
                        ChatMessage::assistant(format!("answer {idx}")),
                    ]
                })
                .collect::<Vec<_>>();
            initial_chat.set_messages(messages);

            let canvases = element! {
                ChatHarness(mode: HarnessMode::TimedExit, initial_chat: Some(initial_chat))
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(
                (0..20).map(|_| TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::PageUp))),
            )))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("question 0")));
            assert!(!canvases.last().unwrap_or(&String::new()).contains("question 17"));
        });
    }

    #[test]
    fn chat_screen_opens_file_finder_overlay() {
        smol::block_on(async {
            let canvases = element! {
                ChatHarness(mode: HarnessMode::TimedExit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                KeyEvent::new(KeyEventKind::Press, KeyCode::Char('@')),
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("pin file to chat")));
        });
    }

    #[test]
    fn chat_screen_renders_sections_reasoning_and_tool_output() {
        let mut initial_chat = ChatApp::new();
        let mut assistant = ChatMessage::assistant(
            "Intent\n\nRender the migration review.\n\nActions\n\n- Port the file browser\n- Add tests\n\nResult\n\nPhase 4 is implemented.\n\nNext\n\nWire the entry point later."
                .to_string(),
        );
        assistant.reasoning_content = Some("Checking the remaining migration work.".to_string());
        assistant.expanded_reasoning = true;
        let idx = assistant.add_tool_call("read".to_string(), r#"{"path":"docs/iocraft.md"}"#.to_string());
        assistant.complete_tool_call(idx, "Loaded reference".to_string(), true);

        initial_chat.set_messages(vec![ChatMessage::user("Review phase 3".to_string()), assistant]);

        let actual = element! {
            ChatScreen(
                initial_chat: Some(initial_chat),
                viewport_width: 72u16,
                viewport_height: 20u16,
            )
        }
        .to_string();

        assert!(actual.contains("REASONING"));
        assert!(actual.contains("INTENT"));
        assert!(actual.contains("ACTIONS"));
        assert!(actual.contains("RESULT"));
        assert!(actual.contains("NEXT"));
        assert!(actual.contains("Loaded reference"));
    }

    #[test]
    fn visible_transcript_lines_tracks_scroll_offset() {
        let transcript = vec![
            super::RenderedLine::new("one", Color::White),
            super::RenderedLine::new("two", Color::White),
            super::RenderedLine::new("three", Color::White),
            super::RenderedLine::new("four", Color::White),
        ];

        let visible = visible_transcript_lines(&transcript, 2, 1);
        let texts = visible.into_iter().map(|line| line.text).collect::<Vec<_>>();
        assert_eq!(texts, vec!["two".to_string(), "three".to_string()]);
    }

    #[test]
    fn hint_tokens_switch_for_file_finder_mode() {
        let tokens = hint_tokens(true);
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token, super::HintToken::Key(value) if value == "Esc"))
        );
    }
}
