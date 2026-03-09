use crate::ScreenMode;
use crossterm::event::KeyEvent;
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAction {
    None,
    Quit,
    SwitchTo(ScreenMode),
    ReturnToPrevious,
}

pub trait Screen {
    fn handle_input(&mut self, key: KeyEvent) -> ScreenAction;
    fn draw(&self, frame: &mut Frame);
}
