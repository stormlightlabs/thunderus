use super::App;
use crate::event_handler::EventHandler;
use crossterm;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Result;
use std::{panic, time::Duration};

pub async fn run(app: &mut App) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let backend = CrosstermBackend::new(std::io::stdout());
        if let Ok(mut terminal) = Terminal::new(backend) {
            let _ = terminal.show_cursor();
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    terminal.clear()?;
    app.draw(&mut terminal)?;

    while !app.should_exit {
        let tui_poll = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            EventHandler::read()
        };

        tokio::select! {
            maybe_event = tui_poll => {
                if let Some(event) = maybe_event {
                    app.handle_event(event).await;
                    app.draw(&mut terminal)?;
                }
            }
            maybe_drift = async {
                if let Some(ref mut rx) = app.drift_rx {
                    rx.recv().await.ok()
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(drift) = maybe_drift {
                    app.handle_drift_event(drift);
                    app.draw(&mut terminal)?;
                }
            }
            maybe_agent = async {
                if let Some(ref mut rx) = app.agent_event_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                match maybe_agent {
                    Some(event) => {
                        app.handle_agent_event(event);
                        app.draw(&mut terminal)?;
                    }
                    None => {
                        app.agent_event_rx = None;
                        app.state_mut().stop_generation();
                    }
                }
            }
            maybe_request = async {
                if let Some(ref mut approval_rx) = app.approval_request_rx {
                    approval_rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(request) = maybe_request {
                    app.handle_approval_request(request);
                    app.draw(&mut terminal)?;
                }
            }
        }
    }

    app.cancel_token.cancel();
    app.state_mut().stop_generation();

    terminal.show_cursor()?;
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;

    Ok(())
}
