//! Higher-level layout elements composed from atomic components.

use super::{
    SUGGESTIONS,
    components::{CardItem, single_line_top_bordered_row_height},
    layout::{AreaSpec, ConstraintSpec},
};
use ratatui::layout::{Constraint, Direction, Rect};
use thndrs_ui_macros::AreaSpec;

#[derive(AreaSpec)]
pub struct WelcomeShell;

impl ConstraintSpec for WelcomeShell {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(single_line_top_bordered_row_height()),
        ]
    }
}

pub struct WelcomeMainColumn;

impl AreaSpec for WelcomeMainColumn {
    fn area(&self, parent: Rect) -> Rect {
        let content_width = 60u16.min(parent.width);
        let content_x = parent.x + (parent.width.saturating_sub(content_width)) / 2;
        Rect::new(content_x, parent.y, content_width, parent.height)
    }
}

#[derive(AreaSpec)]
pub struct WelcomeContent;

impl ConstraintSpec for WelcomeContent {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![
            Constraint::Min(1),
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(9),
            Constraint::Min(1),
        ]
    }
}

#[derive(AreaSpec)]
pub struct Suggestions;

impl ConstraintSpec for Suggestions {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, area: Rect) -> Vec<Constraint> {
        let mut constraints = Vec::with_capacity(SUGGESTIONS.len() + 2);
        constraints.push(Constraint::Length(1));
        for label in SUGGESTIONS {
            constraints.push(Constraint::Length(CardItem::required_height(label, area.width)));
        }
        constraints.push(Constraint::Min(0));
        constraints
    }
}

impl Suggestions {
    pub fn card_areas(self, area: Rect) -> Vec<Rect> {
        let layout = self.split(area);
        SUGGESTIONS
            .iter()
            .enumerate()
            .filter_map(|(idx, _)| layout.get(idx + 1).copied())
            .collect()
    }
}

#[derive(AreaSpec)]
pub struct ChatShell;

impl ConstraintSpec for ChatShell {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(single_line_top_bordered_row_height()),
            Constraint::Length(single_line_top_bordered_row_height()),
        ]
    }
}
