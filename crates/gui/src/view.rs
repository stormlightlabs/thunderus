use super::app::DesktopApp;
use super::model::{ActivityState, ComposerSuggestion, ConversationTurn, TurnState};
use super::model::{DiffLineKind, DiffPreview};
use super::model::{Message, ModelMessage};
use super::model::{SectionKind, SessionSummary, ToolAction, ToolActionStatus};
use super::model::{color_hex, design_layout_contract, normalize_tool_text};
use iced::alignment::Horizontal;
use iced::border;
use iced::theme::Palette;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, markdown, row, rule, scrollable, text, text_editor};
use iced::{Element, Length};

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
        .spacing(12)
        .height(Length::Fill)
        .width(Length::Fill);

    container(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([16, 18])
        .style(|_theme| iced::widget::container::Style::default().background(color_hex(contract.palette.bg_terminal)))
        .into()
}

fn render_sidebar(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let sessions = render_sessions_list(app);
    let file_tree = render_file_tree(app);

    let panel = column![
        text("Navigator")
            .size(15)
            .color(color_hex(contract.palette.text_primary)),
        sessions,
        rule::horizontal(1),
        file_tree
    ]
    .spacing(10)
    .height(Length::Fill);

    container(panel)
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .padding([12, 12])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#101010"))
                .border(border::rounded(14).color(color_hex(contract.palette.border)).width(1.0))
        })
        .into()
}

fn render_sessions_list(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let heading = row![
        text("Sessions").size(13).color(color_hex(contract.palette.accent_cyan)),
        render_badge(
            app.model.sessions.len().to_string(),
            color_hex(contract.palette.text_secondary),
            color_hex("#1a1a1a"),
        )
    ]
    .spacing(8);

    let list = if app.model.sessions.is_empty() {
        column![
            text("No prompts yet")
                .size(13)
                .color(color_hex(contract.palette.text_secondary))
        ]
    } else {
        app.model.sessions.iter().fold(column![].spacing(6), |col, session| {
            col.push(render_session_item(app, session))
        })
    };

    column![heading, scrollable(list).height(Length::Fixed(210.0))]
        .spacing(8)
        .into()
}

fn render_session_item<'a>(app: &'a DesktopApp, session: &'a SessionSummary) -> Element<'a, Message> {
    let contract = design_layout_contract();
    let selected = app.model.selected_turn == Some(session.turn_index);

    let (prefix, state_color) = match session.state {
        TurnState::Running => (
            app.model.spinner_frame().to_string(),
            color_hex(contract.palette.accent_yellow),
        ),
        TurnState::Completed => ("✓".to_string(), color_hex(contract.palette.accent_green)),
        TurnState::Failed => ("✗".to_string(), color_hex(contract.palette.accent_red)),
    };

    let label = if selected {
        format!("▶ {} {}", prefix, session.title)
    } else {
        format!("{} {}", prefix, session.title)
    };

    button(text(label).size(12).color(state_color))
        .width(Length::Fill)
        .on_press(Message::Model(ModelMessage::SelectTurn(session.turn_index)))
        .into()
}

fn render_file_tree(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let heading = row![
        text("Files").size(13).color(color_hex(contract.palette.accent_purple)),
        render_badge(
            app.model.workspace_files.len().to_string(),
            color_hex(contract.palette.text_secondary),
            color_hex("#1a1a1a"),
        )
    ]
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
            .fold(column![].spacing(4), |col, entry| {
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

    column![heading, scrollable(files).height(Length::Fill)]
        .spacing(8)
        .height(Length::Fill)
        .into()
}

fn render_main_panel(app: &DesktopApp) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let workspace_label = app
        .model
        .workspace_root
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "No workspace".to_string());

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
                    .size(18)
                    .color(color_hex(contract.palette.text_primary)),
                text(workspace_label)
                    .size(12)
                    .color(color_hex(contract.palette.text_secondary))
                    .width(Length::Fill)
                    .align_x(Horizontal::Left)
            ]
            .spacing(6)
            .width(Length::Fill),
            header_meta
        ]
        .spacing(12),
    )
    .padding([14, 16])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#121212"))
            .border(border::rounded(14).color(color_hex(contract.palette.border)).width(1.0))
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

    let input_row = row![
        text(contract.prompt_symbol)
            .size(22)
            .color(color_hex(contract.palette.accent_cyan)),
        composer,
        button(text(send_label)).on_press_maybe(send_enabled.then_some(Message::Model(ModelMessage::SubmitPrompt)))
    ]
    .spacing(12);

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
        .spacing(10),
    )
    .padding([12, 14])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#101010"))
            .border(border::rounded(14).color(color_hex(contract.palette.border)).width(1.0))
    });

    let body = column![history, input_panel].spacing(12).height(Length::Fill);

    container(column![header, body].spacing(12))
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
        .fold(row![].spacing(8), |row, suggestion| {
            row.push(render_suggestion_chip(suggestion.clone()))
        });

    container(
        column![
            text("Suggestions")
                .size(12)
                .color(color_hex(contract.palette.text_secondary)),
            chips
        ]
        .spacing(6),
    )
    .padding([8, 10])
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
            .wrapping(Wrapping::WordOrGlyph),
    )
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
                        .size(16)
                        .color(color_hex(contract.palette.text_primary)),
                    text("Type a prompt below to start your first run.")
                        .size(14)
                        .color(color_hex(contract.palette.text_secondary))
                ]
                .spacing(8),
            )
            .padding([18, 20])
            .style(|_theme| {
                iced::widget::container::Style::default()
                    .background(color_hex("#111418"))
                    .border(border::rounded(12).color(color_hex(contract.palette.border)).width(1.0))
            })
        ]
        .spacing(16);
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
        .fold(column![].spacing(16), |history, (index, turn)| {
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

    let mut content = column![row![
        row![
            text(design_layout_contract().prompt_symbol).color(color_hex(contract.palette.accent_cyan)),
            text(&turn.prompt)
                .size(15)
                .color(color_hex(contract.palette.text_primary))
                .wrapping(Wrapping::WordOrGlyph)
        ]
        .spacing(10)
        .width(Length::Fill),
        render_badge(state_label, state_fg, state_bg)
    ]]
    .spacing(12);

    for section in contract.sections {
        content = content.push(render_section(app, turn_index, section, turn));
    }

    container(content)
        .padding([14, 16])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#111418"))
                .border(border::rounded(12).color(color_hex(contract.palette.border)).width(1.0))
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
        text("Result").size(13).color(color_hex(contract.palette.accent_green))
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
        .padding([8, 10])
        .style(|_theme| {
            iced::widget::container::Style::default()
                .background(color_hex("#141414"))
                .border(border::rounded(10).color(color_hex("#262626")).width(1.0))
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
            .size(13)
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
            .padding([8, 10])
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
            .spacing(8),
            details
        ]
        .spacing(6),
    )
    .padding([10, 12])
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
        .spacing(4),
    )
    .padding([8, 10])
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
        .spacing(4),
    )
    .padding([8, 10])
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
    container(
        column![
            row![text(icon).color(title_color), text(title).size(13).color(title_color)].spacing(8),
            text(body).size(14).color(body_color).wrapping(Wrapping::WordOrGlyph)
        ]
        .spacing(6),
    )
    .padding([8, 10])
    .style(|_theme| {
        iced::widget::container::Style::default()
            .background(color_hex("#141414"))
            .border(border::rounded(10).color(color_hex("#262626")).width(1.0))
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
