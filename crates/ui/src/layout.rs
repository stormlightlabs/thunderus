//! Shared layout primitives for responsive, predictable screen geometry.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Reusable area computation contract for components and composed elements.
pub trait AreaSpec {
    fn area(&self, parent: Rect) -> Rect {
        parent
    }
}

/// Constraint-driven layout contract.
pub trait ConstraintSpec: AreaSpec {
    fn direction(&self) -> Direction;
    fn constraints(&self, area: Rect) -> Vec<Constraint>;

    fn split(&self, parent: Rect) -> Vec<Rect> {
        let area = self.area(parent);
        split(area, self.direction(), self.constraints(area))
    }
}

/// Constraint-driven layout contract that depends on runtime context.
pub trait DynamicConstraintSpec: AreaSpec {
    type Context;

    fn direction(&self) -> Direction;
    fn constraints(&self, area: Rect, context: &Self::Context) -> Vec<Constraint>;

    fn split(&self, parent: Rect, context: &Self::Context) -> Vec<Rect> {
        let area = self.area(parent);
        split(area, self.direction(), self.constraints(area, context))
    }
}

/// Applies a ratatui split and returns a plain vector for ergonomic indexing.
pub fn split(area: Rect, direction: Direction, constraints: Vec<Constraint>) -> Vec<Rect> {
    if constraints.is_empty() {
        return Vec::new();
    }

    Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area)
        .iter()
        .copied()
        .collect()
}
