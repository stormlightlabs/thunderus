//! Small pure helpers for context-control calculations.

pub fn ratio_of(value: u64, ratio: f64) -> u64 {
    if value == 0 { 0 } else { ((value as f64) * ratio).floor() as u64 }
}

pub fn scope_depth(scope: &str) -> usize {
    if scope == "." || scope.is_empty() { 0 } else { scope.matches('/').count() + 1 }
}
