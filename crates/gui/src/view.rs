use super::app::DesktopApp;
use super::model::{ActivityState, ComposerSuggestion, ConversationTurn, TurnState};
use super::model::{DiffLineKind, DiffPreview};
use super::model::{Message, ModelMessage};
use super::model::{SectionKind, SessionAgeGroup, SessionSummary, ToolAction, ToolActionStatus};
use super::model::{color_hex, design_layout_contract, normalize_tool_text};
use iced::alignment::Horizontal;
use iced::border;
use iced::theme::Palette;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, markdown, pick_list, row, rule, scrollable, text, text_editor, text_input,
};
use iced::{Element, Length};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

const APP_GAP: f32 = 14.0;
const CARD_GAP: f32 = 10.0;
const SECTION_GAP: f32 = 8.0;
const PANEL_RADIUS: f32 = 14.0;
const CARD_RADIUS: f32 = 12.0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceOption {
    path: PathBuf,
    label: String,
}

impl Display for WorkspaceOption {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

pub fn view(app: &DesktopApp) -> Element<'_, Message> {
    if app.model.workspace_root.is_none() {
        return render_welcome(app);
    }

    render_chat(app)
}

fn render_welcome(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let mut panel = column![
        text(contract.title)
            .size(34)
            .color(color_hex(contract.palette.text_primary)),
        text("Desktop AI coding with tools and workspace context.")
            .size(16)
            .color(color_hex(contract.palette.text_secondary)),
        button(text("Select Workspace Folder")).on_press(Message::Model(ModelMessage::RequestWorkspacePicker))
    ]
    .spacing(18)
    .align_x(iced::Alignment::Center);

    if let Some(status) = app.model.status_text.as_ref() {
        panel = panel.push(text(status).size(14).color(color_hex(contract.palette.text_secondary)));
    }
    if let Some(error) = app.model.error_text.as_ref() {
        panel = panel.push(text(error).size(14).color(color_hex(contract.palette.accent_red)));
    }

    let panel = container(panel)
        .width(Length::Fill)
        .max_width(680)
        .padding([30, 28])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#121212"))
                .border(border::rounded(16).color(color_hex(contract.palette.border)).width(1.0))
        });

    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn render_chat(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let layout = row![render_sidebar(app), render_main_panel(app)]
        .spacing(APP_GAP)
        .height(Length::Fill)
        .width(Length::Fill);

    container(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 20])
        .style(|_theme| iced::widget::container::Style::default().background(color_hex(contract.palette.bg_terminal)))
        .into()
}

fn workspace_dropdown_options(app: &DesktopApp) -> Vec<WorkspaceOption> {
    let Some(current_workspace) = app.model.workspace_root.as_ref() else {
        return Vec::new();
    };

    app.model
        .recent_workspaces
        .iter()
        .filter(|path| path != &current_workspace)
        .filter(|path| path.is_dir())
        .map(|path| WorkspaceOption { path: path.clone(), label: workspace_option_label(path) })
        .collect()
}

fn workspace_option_label(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|segment| segment.to_str())
        .unwrap_or("workspace");
    format!("{name}  ·  {}", path.display())
}

fn render_sidebar(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let workspace_section = render_workspace_section(app);
    let sessions = render_sessions_list(app);
    let file_tree = render_file_tree(app);

    let panel = column![workspace_section, sessions, file_tree]
        .spacing(CARD_GAP)
        .height(Length::Fill);

    container(panel)
        .width(Length::Fixed(332.0))
        .height(Length::Fill)
        .padding([12, 12])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#101010"))
                .border(
                    border::rounded(PANEL_RADIUS)
                        .color(color_hex(contract.palette.border))
                        .width(1.0),
                )
        })
        .into()
}

fn render_workspace_section(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();
    let workspace_path = app
        .model
        .workspace_root
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "No workspace selected".to_string());
    let options = workspace_dropdown_options(app);

    let switcher: Element<'_, Message> = if options.is_empty() {
        container(
            text("No other saved projects")
                .size(12)
                .color(color_hex(contract.palette.text_secondary)),
        )
        .padding([8, 10])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#141414"))
                .border(border::rounded(8).color(color_hex(contract.palette.border)).width(1.0))
        })
        .into()
    } else {
        pick_list(options, None::<WorkspaceOption>, |option| {
            Message::Model(ModelMessage::SwitchWorkspace(option.path))
        })
        .placeholder("Switch project")
        .width(Length::Fill)
        .into()
    };

    container(
        column![
            row![
                button(text("⌂ Home")).on_press(Message::Model(ModelMessage::GoHome)),
                render_badge(
                    "Workspace",
                    color_hex(contract.palette.accent_cyan),
                    color_hex("#10222f"),
                )
            ]
            .align_y(iced::Alignment::Center)
            .spacing(8),
            text(workspace_path)
                .size(12)
                .color(color_hex(contract.palette.text_secondary))
                .wrapping(Wrapping::WordOrGlyph),
            text("Other projects")
                .size(12)
                .color(color_hex(contract.palette.text_secondary)),
            switcher
        ]
        .spacing(SECTION_GAP),
    )
    .padding([12, 12])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#121212"))
            .border(
                border::rounded(CARD_RADIUS)
                    .color(color_hex(contract.palette.border))
                    .width(1.0),
            )
    })
    .into()
}

fn render_sessions_list(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let heading = row![
        text("Sessions").size(14).color(color_hex(contract.palette.accent_cyan)),
        render_badge(
            app.model.sessions.len().to_string(),
            color_hex(contract.palette.text_secondary),
            color_hex("#1a1a1a"),
        )
    ]
    .align_y(iced::Alignment::Center)
    .spacing(8);
    let search = text_input("Search sessions", &app.model.session_search_query)
        .on_input(|value| Message::Model(ModelMessage::SessionSearchEdited(value)))
        .size(12)
        .padding([6, 8]);

    let list = if app.model.sessions.is_empty() {
        column![
            text("No prompts yet")
                .size(13)
                .color(color_hex(contract.palette.text_secondary))
        ]
    } else {
        let mut grouped = column![].spacing(CARD_GAP);
        for group in [
            SessionAgeGroup::Today,
            SessionAgeGroup::Yesterday,
            SessionAgeGroup::Older,
        ] {
            if let Some(section) = render_session_group(app, group) {
                grouped = grouped.push(section);
            }
        }
        grouped
    };

    container(column![heading, search, scrollable(list).height(Length::Fill)].spacing(SECTION_GAP))
        .padding([12, 12])
        .height(Length::FillPortion(2))
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#121212"))
                .border(
                    border::rounded(CARD_RADIUS)
                        .color(color_hex(contract.palette.border))
                        .width(1.0),
                )
        })
        .into()
}

fn render_session_group(app: &DesktopApp, group: SessionAgeGroup) -> Option<Element<'_, Message>> {
    let contract = design_layout_contract();

    let mut items = column![].spacing(SECTION_GAP);
    let mut count = 0;
    for session in app.model.sessions.iter().filter(|session| session.age_group == group) {
        items = items.push(render_session_item(session));
        count += 1;
    }

    if count == 0 {
        return None;
    }

    Some(
        column![
            text(group.label())
                .size(11)
                .color(color_hex(contract.palette.text_secondary)),
            items
        ]
        .spacing(6)
        .into(),
    )
}

fn render_session_item(session: &SessionSummary) -> Element<'_, Message> {
    let contract = design_layout_contract();
    let title_color = if session.is_active {
        color_hex(contract.palette.accent_cyan)
    } else {
        color_hex(contract.palette.text_primary)
    };
    let meta_color = color_hex(contract.palette.text_secondary);
    let open_message = Message::Model(ModelMessage::ResumeSession(session.id.clone()));
    let export_message = Message::Model(ModelMessage::ExportSession(session.id.clone()));
    let delete_message = Message::Model(ModelMessage::DeleteSession(session.id.clone()));
    let mut metadata_row = row![
        text(format!("{} messages", session.message_count))
            .size(11)
            .color(meta_color),
        text(session.updated_at_label.clone()).size(11).color(meta_color)
    ]
    .spacing(10);

    if let Some(model) = session.model.as_ref() {
        metadata_row = metadata_row.push(text(format!("model: {model}")).size(11).color(meta_color));
    }
    if let Some(tokens) = session.total_tokens {
        metadata_row = metadata_row.push(
            text(format!("tokens: {}", format_token_count(tokens)))
                .size(11)
                .color(meta_color),
        );
    }
    if let Some(workspace) = session.workspace.as_ref() {
        metadata_row = metadata_row.push(text(format!("workspace: {workspace}")).size(11).color(meta_color));
    }

    container(
        column![
            row![
                button(
                    text(&session.title)
                        .size(13)
                        .color(title_color)
                        .width(Length::Fill)
                        .align_x(Horizontal::Left),
                )
                .padding([6, 8])
                .width(Length::Fill)
                .on_press(open_message),
                button(text("Export").size(11)).padding([4, 8]).on_press(export_message),
                button(text("Delete").size(11)).padding([4, 8]).on_press(delete_message)
            ]
            .align_y(iced::Alignment::Center)
            .spacing(6),
            metadata_row
        ]
        .spacing(4),
    )
    .padding([6, 6])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#141414"))
            .border(border::rounded(8).color(color_hex(contract.palette.border)).width(1.0))
    })
    .into()
}

fn format_token_count(value: u32) -> String {
    let raw = value.to_string();
    if raw.len() <= 3 {
        return raw;
    }

    let mut grouped = String::new();
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }

    grouped.chars().rev().collect()
}

fn render_file_tree(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let heading = row![
        text("Files").size(14).color(color_hex(contract.palette.accent_purple)),
        render_badge(
            app.model.workspace_files.len().to_string(),
            color_hex(contract.palette.text_secondary),
            color_hex("#1a1a1a"),
        )
    ]
    .align_y(iced::Alignment::Center)
    .spacing(8);

    let files = if app.model.workspace_files.is_empty() {
        column![
            text("No indexed files yet")
                .size(12)
                .color(color_hex(contract.palette.text_secondary))
        ]
    } else {
        app.model
            .workspace_files
            .iter()
            .fold(column![].spacing(6), |col, entry| {
                let indent = "  ".repeat(entry.depth.min(6));
                let icon = if entry.is_dir { "▸" } else { "·" };
                let label = format!("{indent}{icon} {}", entry.relative_path);
                col.push(
                    text(label)
                        .size(12)
                        .color(color_hex(contract.palette.text_secondary))
                        .wrapping(Wrapping::WordOrGlyph),
                )
            })
    };

    container(column![heading, scrollable(files).height(Length::Fill)].spacing(SECTION_GAP))
        .padding([12, 12])
        .height(Length::FillPortion(3))
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#121212"))
                .border(
                    border::rounded(CARD_RADIUS)
                        .color(color_hex(contract.palette.border))
                        .width(1.0),
                )
        })
        .into()
}

fn render_main_panel(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let animated = app.model.activity != ActivityState::Idle;
    let animated_bg = if animated && app.model.animation_tick.is_multiple_of(2) {
        color_hex("#203018")
    } else {
        color_hex("#0f2612")
    };

    let mut header_meta = row![].spacing(8);
    match app.model.activity {
        ActivityState::Idle => {
            header_meta = header_meta.push(render_badge(
                "Ready",
                color_hex(contract.palette.accent_green),
                color_hex("#0f2612"),
            ));
        }
        ActivityState::Dispatching
        | ActivityState::Thinking
        | ActivityState::RunningTools
        | ActivityState::Streaming => {
            header_meta = header_meta.push(render_badge(
                format!("{} Working", app.model.spinner_frame()),
                color_hex(contract.palette.accent_green),
                animated_bg,
            ));
        }
        ActivityState::Failed => {
            header_meta = header_meta.push(render_badge(
                "Failed",
                color_hex(contract.palette.accent_red),
                color_hex("#2d1012"),
            ));
        }
    }

    if let Some(model) = app.model.last_model.as_ref() {
        header_meta = header_meta.push(render_badge(
            format!("Model: {model}"),
            color_hex(contract.palette.text_secondary),
            color_hex("#1d1d1d"),
        ));
    }

    let header = container(
        row![
            column![
                text(contract.title)
                    .size(20)
                    .color(color_hex(contract.palette.text_primary)),
                text("Desktop")
                    .size(13)
                    .color(color_hex(contract.palette.text_secondary))
            ]
            .spacing(6)
            .width(Length::Fill),
            header_meta
        ]
        .align_y(iced::Alignment::Center)
        .spacing(CARD_GAP),
    )
    .padding([16, 18])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#121212"))
            .border(
                border::rounded(PANEL_RADIUS)
                    .color(color_hex(contract.palette.border))
                    .width(1.0),
            )
    });

    let history_column = build_history_column(app);
    let history = scrollable(history_column.width(Length::Fill))
        .height(Length::Fill)
        .anchor_bottom();

    let send_enabled = !app.model.streaming && !app.model.composer_text.trim().is_empty();
    let send_label = if app.model.streaming {
        format!("{} Working", app.model.spinner_frame())
    } else {
        "Send".to_string()
    };

    let composer = text_editor(&app.model.composer)
        .on_action(|action| Message::Model(ModelMessage::ComposerEdited(action)))
        .height(Length::Fixed(app.model.input_height()))
        .placeholder("Ask Thunderus to inspect, edit, or explain code in this workspace");

    let send_button = button(text(send_label).width(Length::Fixed(74.0)).align_x(Horizontal::Center))
        .on_press_maybe(send_enabled.then_some(Message::Model(ModelMessage::SubmitPrompt)));

    let input_row = row![
        text(contract.prompt_symbol)
            .size(22)
            .color(color_hex(contract.palette.accent_cyan)),
        composer,
        send_button
    ]
    .align_y(iced::Alignment::Center)
    .spacing(CARD_GAP);

    let suggestions_panel = render_suggestions(app);

    let mut footer = row![].spacing(8);
    if let Some(status_text) = app.model.status_text.as_ref() {
        footer = footer.push(render_badge(
            status_text.clone(),
            color_hex(contract.palette.text_secondary),
            color_hex("#1a1a1a"),
        ));
    }
    if let Some(error) = app.model.error_text.as_ref() {
        footer = footer.push(render_badge(
            error.clone(),
            color_hex(contract.palette.accent_red),
            color_hex("#2d1012"),
        ));
    }

    let input_panel = container(
        column![
            input_row,
            suggestions_panel,
            rule::horizontal(1),
            text("Enter adds a new line. Use Send to dispatch.")
                .size(12)
                .color(color_hex(contract.palette.text_secondary)),
            footer
        ]
        .spacing(CARD_GAP),
    )
    .padding([14, 16])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#101010"))
            .border(
                border::rounded(PANEL_RADIUS)
                    .color(color_hex(contract.palette.border))
                    .width(1.0),
            )
    });

    let body = column![history, input_panel].spacing(APP_GAP).height(Length::Fill);

    container(column![header, body].spacing(APP_GAP))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn render_suggestions(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();
    if app.model.composer_suggestions.is_empty() {
        return container(column![]).into();
    }

    let chips = app
        .model
        .composer_suggestions
        .iter()
        .take(5)
        .fold(column![].spacing(6), |col, suggestion| {
            col.push(render_suggestion_chip(suggestion.clone()))
        });

    container(
        column![
            text("Suggestions")
                .size(12)
                .color(color_hex(contract.palette.text_secondary)),
            chips
        ]
        .spacing(SECTION_GAP),
    )
    .padding([10, 12])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#131313"))
            .border(border::rounded(10).color(color_hex(contract.palette.border)).width(1.0))
    })
    .into()
}

fn render_suggestion_chip(suggestion: ComposerSuggestion) -> Element<'static, Message> {
    button(
        text(format!("{} · {}", suggestion.label, suggestion.detail))
            .size(12)
            .width(Length::Fill)
            .align_x(Horizontal::Left)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding([6, 8])
    .width(Length::Fill)
    .on_press(Message::Model(ModelMessage::ApplyComposerSuggestion(suggestion)))
    .into()
}

fn build_history_column(app: &DesktopApp) -> iced::widget::Column<'_, Message> {
    let contract = design_layout_contract();

    if app.model.turns.is_empty() {
        return column![
            container(
                column![
                    text("No conversation yet.")
                        .size(18)
                        .color(color_hex(contract.palette.text_primary)),
                    text("Type a prompt below to start your first run.")
                        .size(14)
                        .color(color_hex(contract.palette.text_secondary))
                ]
                .spacing(SECTION_GAP),
            )
            .padding([22, 24])
            .style(|_theme| {
                iced::widget::container::Style::default()
                    .background(color_hex("#111418"))
                    .border(
                        border::rounded(CARD_RADIUS)
                            .color(color_hex(contract.palette.border))
                            .width(1.0),
                    )
            })
        ]
        .spacing(18);
    }

    if let Some(selected) = app.model.selected_turn
        && let Some(turn) = app.model.turns.get(selected)
    {
        return column![render_turn(app, selected, turn)].spacing(16);
    }

    app.model
        .turns
        .iter()
        .enumerate()
        .fold(column![].spacing(18), |history, (index, turn)| {
            history.push(render_turn(app, index, turn))
        })
}

fn render_turn<'a>(app: &'a DesktopApp, turn_index: usize, turn: &'a ConversationTurn) -> Element<'a, Message> {
    let contract = design_layout_contract();

    let (state_label, state_fg, state_bg) = match turn.state {
        TurnState::Running => (
            format!("{} Running", app.model.spinner_frame()),
            color_hex(contract.palette.accent_yellow),
            if app.model.animation_tick.is_multiple_of(2) { color_hex("#2e2600") } else { color_hex("#3a2f00") },
        ),
        TurnState::Completed => (
            "Completed".to_string(),
            color_hex(contract.palette.accent_green),
            color_hex("#0f2612"),
        ),
        TurnState::Failed => (
            "Failed".to_string(),
            color_hex(contract.palette.accent_red),
            color_hex("#2d1012"),
        ),
    };

    let mut content = column![
        row![
            row![
                text(design_layout_contract().prompt_symbol).color(color_hex(contract.palette.accent_cyan)),
                text(&turn.prompt)
                    .size(16)
                    .color(color_hex(contract.palette.text_primary))
                    .wrapping(Wrapping::WordOrGlyph)
            ]
            .spacing(CARD_GAP)
            .width(Length::Fill),
            render_badge(state_label, state_fg, state_bg)
        ]
        .align_y(iced::Alignment::Center)
    ]
    .spacing(CARD_GAP);

    for section in contract.sections {
        content = content.push(render_section(app, turn_index, section, turn));
    }

    container(content)
        .padding([16, 18])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#111418"))
                .border(
                    border::rounded(CARD_RADIUS)
                        .color(color_hex(contract.palette.border))
                        .width(1.0),
                )
        })
        .into()
}

fn render_section<'a>(
    app: &'a DesktopApp, turn_index: usize, section: SectionKind, turn: &'a ConversationTurn,
) -> Element<'a, Message> {
    let contract = design_layout_contract();

    match section {
        SectionKind::Intent => render_text_section(
            "◉",
            "Intent",
            turn.intent.clone(),
            color_hex(contract.palette.accent_purple),
            color_hex(contract.palette.text_secondary),
        ),
        SectionKind::Actions => render_actions_section(app, turn_index, turn),
        SectionKind::Result => render_result_section(app, turn),
        SectionKind::Next => {
            let guidance = if turn.state == TurnState::Running {
                format!("{} Agent is still working…", app.model.spinner_frame())
            } else {
                turn.next.clone()
            };
            render_text_section(
                "→",
                "Next",
                guidance,
                color_hex(contract.palette.accent_cyan),
                color_hex(contract.palette.text_secondary),
            )
        }
    }
}

fn render_result_section<'a>(app: &'a DesktopApp, turn: &'a ConversationTurn) -> Element<'a, Message> {
    let contract = design_layout_contract();
    let title = row![
        text("✓").color(color_hex(contract.palette.accent_green)),
        text("Result").size(14).color(color_hex(contract.palette.accent_green))
    ]
    .spacing(8);

    let mut body = column![title].spacing(8);

    if !turn.result_markdown.is_empty() {
        let markdown_view = markdown::view(turn.result_markdown.iter(), markdown_settings())
            .map(|uri| Message::Model(ModelMessage::OpenMarkdownLink(uri)));
        body = body.push(markdown_view);
    } else if turn.state == TurnState::Failed {
        let error_text = turn
            .error
            .as_deref()
            .unwrap_or("No assistant output was produced before failure.");
        body = body.push(
            text(error_text)
                .size(14)
                .color(color_hex(contract.palette.accent_red))
                .wrapping(Wrapping::WordOrGlyph),
        );
    } else if turn.state == TurnState::Running {
        body = body.push(
            text(format!("{} Awaiting assistant output…", app.model.spinner_frame()))
                .size(14)
                .color(color_hex(contract.palette.text_secondary)),
        );
    } else {
        body = body.push(
            text("No assistant output.")
                .size(14)
                .color(color_hex(contract.palette.text_secondary)),
        );
    }

    container(body)
        .padding([10, 12])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#141414"))
                .border(border::rounded(10).color(color_hex(contract.palette.border)).width(1.0))
        })
        .into()
}

fn render_actions_section<'a>(
    app: &'a DesktopApp, turn_index: usize, turn: &'a ConversationTurn,
) -> Element<'a, Message> {
    let contract = design_layout_contract();
    let title = row![
        text("⚡").color(color_hex(contract.palette.accent_yellow)),
        text("Actions")
            .size(14)
            .color(color_hex(contract.palette.accent_yellow))
    ]
    .spacing(8);

    let mut section = column![title].spacing(8);

    if !turn.thinking.is_empty() {
        let latest_thinking = turn.thinking.last().cloned().unwrap_or_default();
        section = section.push(
            container(
                text(format!("{} {}", app.model.spinner_frame(), latest_thinking))
                    .size(12)
                    .color(color_hex(contract.palette.text_secondary))
                    .wrapping(Wrapping::WordOrGlyph),
            )
            .padding([10, 12])
            .style(|_theme| {
                iced::widget::container::Style::default()
                    .background(color_hex("#161616"))
                    .border(border::rounded(8).color(color_hex(contract.palette.border)).width(1.0))
            }),
        );
    }

    if turn.actions.is_empty() {
        section = section.push(
            text("No tool calls executed.")
                .size(14)
                .color(color_hex(contract.palette.text_secondary)),
        );
    } else {
        for (action_index, action) in turn.actions.iter().enumerate() {
            section = section.push(render_tool_action(turn_index, action_index, action));
        }
    }

    section.into()
}

fn render_tool_action(turn_index: usize, action_index: usize, action: &ToolAction) -> Element<'_, Message> {
    let contract = design_layout_contract();
    let (symbol, status_label, symbol_color, status_bg) = match action.status {
        ToolActionStatus::Running => (
            "○",
            "running",
            color_hex(contract.palette.accent_yellow),
            color_hex("#2e2600"),
        ),
        ToolActionStatus::Success => (
            "✓",
            "success",
            color_hex(contract.palette.accent_green),
            color_hex("#0f2612"),
        ),
        ToolActionStatus::Error => (
            "✗",
            "error",
            color_hex(contract.palette.accent_red),
            color_hex("#2d1012"),
        ),
    };

    let toggle_label = if action.expanded { "Hide" } else { "Details" };

    let mut details = column![].spacing(8);
    if action.expanded {
        let args_text = normalize_tool_text(&action.arguments);
        let result_text = normalize_tool_text(&action.result);
        details = details
            .push(
                text(format!("id: {}", action.id))
                    .size(11)
                    .color(color_hex(contract.palette.text_secondary)),
            )
            .push(render_code_like_block("args", args_text))
            .push(render_code_like_block("result", result_text));

        if let Some(preview) = action.diff_preview.as_ref() {
            details = details.push(render_diff_preview(preview));
        }
    }

    container(
        column![
            row![
                text(symbol).color(symbol_color),
                text(&action.name)
                    .size(13)
                    .color(color_hex(contract.palette.accent_cyan))
                    .width(Length::Fill),
                render_badge(status_label, symbol_color, status_bg),
                button(text(toggle_label)).on_press(Message::Model(ModelMessage::ToggleToolAction {
                    turn_index,
                    action_index
                }))
            ]
            .align_y(iced::Alignment::Center)
            .spacing(SECTION_GAP),
            details
        ]
        .spacing(SECTION_GAP),
    )
    .padding([12, 14])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#151515"))
            .border(border::rounded(10).color(color_hex(contract.palette.border)).width(1.0))
    })
    .into()
}

fn render_code_like_block(label: &'static str, content: String) -> Element<'static, Message> {
    let contract = design_layout_contract();

    container(
        column![
            text(label).size(11).color(color_hex(contract.palette.accent_purple)),
            text(content)
                .size(12)
                .color(color_hex(contract.palette.text_secondary))
                .wrapping(Wrapping::WordOrGlyph)
        ]
        .spacing(6),
    )
    .padding([10, 12])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#101316"))
            .border(border::rounded(8).color(color_hex(contract.palette.border)).width(1.0))
    })
    .into()
}

fn render_diff_preview(preview: &DiffPreview) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let lines = preview.lines.iter().fold(column![].spacing(2), |col, line| {
        let color = match line.kind {
            DiffLineKind::Added => color_hex(contract.palette.accent_green),
            DiffLineKind::Removed => color_hex(contract.palette.accent_red),
            DiffLineKind::Context => color_hex(contract.palette.text_secondary),
        };

        col.push(text(&line.text).size(12).color(color).wrapping(Wrapping::WordOrGlyph))
    });

    container(
        column![
            text(&preview.title)
                .size(11)
                .color(color_hex(contract.palette.accent_cyan)),
            lines
        ]
        .spacing(6),
    )
    .padding([10, 12])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#121920"))
            .border(border::rounded(8).color(color_hex(contract.palette.border)).width(1.0))
    })
    .into()
}

fn render_badge<'a>(label: impl Into<String>, fg: iced::Color, bg: iced::Color) -> Element<'a, Message> {
    let text_value = label.into();
    container(text(text_value).size(12).color(fg))
        .padding([4, 8])
        .style(move |_theme| {
            iced::widget::container::Style::default()
                .background(bg)
                .border(border::rounded(8).color(bg).width(1.0))
        })
        .into()
}

fn render_text_section<'a>(
    icon: &'static str, title: &'static str, body: String, title_color: iced::Color, body_color: iced::Color,
) -> Element<'a, Message> {
    let contract = design_layout_contract();
    container(
        column![
            row![text(icon).color(title_color), text(title).size(14).color(title_color)].spacing(8),
            text(body).size(13).color(body_color).wrapping(Wrapping::WordOrGlyph)
        ]
        .spacing(SECTION_GAP),
    )
    .padding([10, 12])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#141414"))
            .border(border::rounded(10).color(color_hex(contract.palette.border)).width(1.0))
    })
    .into()
}

fn markdown_settings() -> markdown::Settings {
    markdown::Settings::with_text_size(
        14,
        markdown::Style::from_palette(Palette {
            background: color_hex("#111418"),
            text: color_hex("#f4f4f4"),
            primary: color_hex("#33b1ff"),
            success: color_hex("#42be65"),
            warning: color_hex("#f1c21b"),
            danger: color_hex("#fa4d56"),
        }),
    )
}
