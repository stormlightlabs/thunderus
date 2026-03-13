//! Terminal UI for Thunderus.

mod app;
mod card;
mod chat;
mod commands;
mod files;
mod finder;
mod help;
mod help_state;
mod hint_bar;
mod input_field;
mod scroll;
mod settings;
mod settings_state;
mod status_bar;
mod theme;
mod welcome;

use thiserror::Error;

pub(crate) use card::SuggestionCard;
pub use chat::{IncomingStreamEvent, TokenUsage};
pub(crate) use hint_bar::{HintBar, HintToken};
pub(crate) use input_field::InputField;

/// UI Errors
#[derive(Error, Debug)]
pub enum UiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Terminal error: {0}")]
    Terminal(String),
}

pub type Result<T> = std::result::Result<T, UiError>;

/// Run the welcome screen TUI.
pub fn run_welcome_app() -> Result<()> {
    app::run_iocraft_app(None, None)
}

/// Run the welcome screen TUI with streaming callbacks for conversation handling.
pub fn run_welcome_app_with_streaming<S, P>(submit_message: S, poll_event: P) -> Result<()>
where
    S: FnMut(String) -> std::result::Result<(), String> + Send + 'static,
    P: FnMut() -> Option<IncomingStreamEvent> + Send + 'static,
{
    let submitter = Some(app::shared_submitter(submit_message));
    let poller = Some(app::shared_poller(poll_event));
    app::run_iocraft_app(submitter, poller)
}

/// Run the chat screen TUI.
pub fn run_chat_app() -> Result<()> {
    run_welcome_app()
}
