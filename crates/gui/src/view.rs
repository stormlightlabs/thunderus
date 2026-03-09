use super::app::DesktopApp;
use super::model::{ConversationTurn, Message, ModelMessage, SectionKind, ToolAction, ToolActionStatus, TurnState};
use super::model::{color_hex, design_layout_contract};
use iced::alignment::Horizontal;
use iced::border;
use iced::widget::{button, column, container, row, rule, scrollable, text, text_editor};
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

    let workspace_label = app
        .model
        .workspace_root
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "No workspace".to_string());

    let mut header_meta = row![].spacing(8);
    if app.model.streaming {
        header_meta = header_meta.push(render_badge(
            "Streaming",
            color_hex(contract.palette.accent_yellow),
            color_hex("#2e2600"),
        ));
    } else {
        header_meta = header_meta.push(render_badge(
            "Ready",
            color_hex(contract.palette.accent_green),
            color_hex("#0f2612"),
        ));
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

    let history_column = if app.model.turns.is_empty() {
        column![
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
        .spacing(16)
    } else {
        app.model
            .turns
            .iter()
            .fold(column![].spacing(16), |history, turn| history.push(render_turn(turn)))
    };

    let history = scrollable(history_column.width(Length::Fill))
        .height(Length::Fill)
        .anchor_bottom();

    let send_enabled = !app.model.streaming && !app.model.composer_text.trim().is_empty();
    let send_label = if app.model.streaming { "Working…" } else { "Send" };

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
        .padding([16, 18])
        .style(|_theme| iced::widget::container::Style::default().background(color_hex("#0c0c0c")))
        .into()
}

fn render_turn(turn: &ConversationTurn) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let (state_label, state_fg, state_bg) = match turn.state {
        TurnState::Running => (
            "Running",
            color_hex(contract.palette.accent_yellow),
            color_hex("#2e2600"),
        ),
        TurnState::Completed => (
            "Completed",
            color_hex(contract.palette.accent_green),
            color_hex("#0f2612"),
        ),
        TurnState::Failed => ("Failed", color_hex(contract.palette.accent_red), color_hex("#2d1012")),
    };

    let mut content = column![row![
        row![
            text(contract.prompt_symbol).color(color_hex(contract.palette.accent_cyan)),
            text(&turn.prompt)
                .size(15)
                .color(color_hex(contract.palette.text_primary))
        ]
        .spacing(10)
        .width(Length::Fill),
        render_badge(state_label, state_fg, state_bg)
    ]]
    .spacing(12);

    for section in contract.sections {
        content = content.push(render_section(section, turn));
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

fn render_section(section: SectionKind, turn: &ConversationTurn) -> Element<'_, Message> {
    let contract = design_layout_contract();

    match section {
        SectionKind::Intent => render_text_section(
            "◉",
            "Intent",
            &turn.intent,
            color_hex(contract.palette.accent_purple),
            color_hex(contract.palette.text_secondary),
        ),
        SectionKind::Actions => render_actions_section(turn),
        SectionKind::Result => {
            let result = if turn.result.is_empty() { "Awaiting assistant output..." } else { &turn.result };
            render_text_section(
                "✓",
                "Result",
                result,
                color_hex(contract.palette.accent_green),
                color_hex(contract.palette.text_secondary),
            )
        }
        SectionKind::Next => render_text_section(
            "→",
            "Next",
            &turn.next,
            color_hex(contract.palette.accent_cyan),
            color_hex(contract.palette.text_secondary),
        ),
    }
}

fn render_actions_section(turn: &ConversationTurn) -> Element<'_, Message> {
    let contract = design_layout_contract();
    let title = row![
        text("⚡").color(color_hex(contract.palette.accent_yellow)),
        text("Actions")
            .size(13)
            .color(color_hex(contract.palette.accent_yellow))
    ]
    .spacing(8);

    let tool_rows = if turn.actions.is_empty() {
        column![
            text("No tool calls executed.")
                .size(14)
                .color(color_hex(contract.palette.text_secondary))
        ]
    } else {
        turn.actions
            .iter()
            .fold(column![].spacing(8), |col, action| col.push(render_tool_action(action)))
    };

    column![title, tool_rows].spacing(8).into()
}

fn render_tool_action(action: &ToolAction) -> Element<'_, Message> {
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

    let mut detail_lines = column![
        text(format!("args: {}", action.arguments))
            .size(12)
            .color(color_hex(contract.palette.text_secondary))
    ]
    .spacing(5);
    if !action.result.is_empty() {
        detail_lines = detail_lines.push(
            text(format!("result: {}", action.result))
                .size(12)
                .color(color_hex(contract.palette.text_secondary)),
        );
    }

    container(
        column![
            row![
                text(symbol).color(symbol_color),
                text(&action.name)
                    .size(13)
                    .color(color_hex(contract.palette.accent_cyan))
                    .width(Length::Fill),
                render_badge(status_label, symbol_color, status_bg)
            ]
            .spacing(8),
            detail_lines
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
    icon: &'static str, title: &'static str, body: &'a str, title_color: iced::Color, body_color: iced::Color,
) -> Element<'a, Message> {
    container(
        column![
            row![text(icon).color(title_color), text(title).size(13).color(title_color)].spacing(8),
            text(body).size(14).color(body_color)
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
