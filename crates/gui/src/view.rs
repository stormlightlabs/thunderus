use super::app::DesktopApp;
use super::model::{ConversationTurn, Message, ModelMessage, SectionKind, ToolActionStatus};
use super::model::{color_hex, design_layout_contract};
use iced::alignment::Horizontal;
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
    let mut content = column![
        text(contract.title)
            .size(32)
            .color(color_hex(contract.palette.text_primary)),
        text("Select a workspace folder to begin.")
            .size(16)
            .color(color_hex(contract.palette.text_secondary)),
        button(text("Select Workspace Folder")).on_press(Message::Model(ModelMessage::RequestWorkspacePicker))
    ]
    .spacing(16)
    .align_x(iced::Alignment::Center);

    if let Some(status) = app.model.status_text.as_ref() {
        content = content.push(text(status).size(14).color(color_hex(contract.palette.text_secondary)));
    }
    if let Some(error) = app.model.error_text.as_ref() {
        content = content.push(text(error).size(14).color(color_hex(contract.palette.accent_red)));
    }

    container(content)
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

    let header = row![
        text(contract.title)
            .size(15)
            .width(Length::Fill)
            .align_x(Horizontal::Center),
        text(workspace_label)
            .size(12)
            .color(color_hex(contract.palette.text_secondary))
    ]
    .spacing(8)
    .padding([12, 16]);

    let history_column = if app.model.turns.is_empty() {
        column![
            text("No conversation yet.").color(color_hex("#8d8d8d")),
            text("Type a prompt below to start.").color(color_hex("#8d8d8d"))
        ]
        .spacing(6)
    } else {
        let mut history = column![].spacing(24);
        for turn in &app.model.turns {
            history = history.push(render_turn(turn));
        }
        history
    };

    let mut footer = row![];
    if let Some(status_text) = app.model.status_text.as_ref() {
        footer = footer.push(text(status_text).size(13).color(color_hex("#8d8d8d")));
    }
    if let Some(model) = app.model.last_model.as_ref() {
        footer = footer.push(text(format!("Model: {model}")).size(13).color(color_hex("#8d8d8d")));
    }
    if let Some(error) = app.model.error_text.as_ref() {
        footer = footer.push(text(error).size(13).color(color_hex("#fa4d56")));
    }
    footer = footer.spacing(16);

    let send_enabled = !app.model.streaming && !app.model.composer_text.trim().is_empty();

    let composer = text_editor(&app.model.composer)
        .on_action(|action| Message::Model(ModelMessage::ComposerEdited(action)))
        .height(Length::Fixed(app.model.input_height()))
        .placeholder("Ask Thunderus to inspect, change, or explain code");

    let input = column![
        rule::horizontal(1),
        row![
            text(contract.prompt_symbol)
                .size(22)
                .color(color_hex(contract.palette.accent_cyan)),
            composer,
            button(text("Send")).on_press_maybe(send_enabled.then_some(Message::Model(ModelMessage::SubmitPrompt)))
        ]
        .spacing(12)
        .padding([16, 0]),
        footer
    ]
    .spacing(8)
    .padding(0);

    let body = column![scrollable(history_column).height(Length::Fill), input]
        .spacing(0)
        .padding([16, 20]);

    container(column![header, body])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn render_turn(turn: &ConversationTurn) -> Element<'_, Message> {
    let contract = design_layout_contract();

    let mut content = column![
        row![
            text(contract.prompt_symbol).color(color_hex(contract.palette.accent_cyan)),
            text(&turn.prompt)
                .size(15)
                .color(color_hex(contract.palette.text_primary))
        ]
        .spacing(10)
    ]
    .spacing(14);

    for section in contract.sections {
        content = content.push(render_section(section, turn));
    }

    container(content).into()
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
        column![text("No tool calls executed.").color(color_hex("#8d8d8d")).size(14)]
    } else {
        turn.actions.iter().fold(column![].spacing(8), |col, action| {
            let symbol = match action.status {
                ToolActionStatus::Running => "○",
                ToolActionStatus::Success => "✓",
                ToolActionStatus::Error => "✗",
            };
            let symbol_color = match action.status {
                ToolActionStatus::Running => color_hex(contract.palette.accent_yellow),
                ToolActionStatus::Success => color_hex(contract.palette.accent_green),
                ToolActionStatus::Error => color_hex(contract.palette.accent_red),
            };
            let detail = if action.result.is_empty() { action.arguments.as_str() } else { action.result.as_str() };

            col.push(
                column![
                    row![
                        text(symbol).color(symbol_color),
                        text(&action.name).color(color_hex(contract.palette.accent_cyan)),
                        text(detail)
                            .color(color_hex(contract.palette.text_secondary))
                            .width(Length::Fill)
                    ]
                    .spacing(8)
                ]
                .spacing(4),
            )
        })
    };

    column![title, tool_rows].spacing(8).into()
}

fn render_text_section<'a>(
    icon: &'static str, title: &'static str, body: &'a str, title_color: iced::Color, body_color: iced::Color,
) -> Element<'a, Message> {
    column![
        row![text(icon).color(title_color), text(title).size(13).color(title_color)].spacing(8),
        text(body).size(14).color(body_color)
    ]
    .spacing(6)
    .into()
}
