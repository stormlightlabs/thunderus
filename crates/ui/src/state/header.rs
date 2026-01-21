/// State for the session header display
///
/// Tracks task title derived from first user message and token/cost statistics.
#[derive(Debug, Clone, Default)]
pub struct HeaderState {
    /// Task title derived from first user message
    pub task_title: Option<String>,
    /// Total tokens used in current session
    pub tokens_used: usize,
    /// Context window limit
    pub context_limit: usize,
    /// Estimated cost in dollars
    pub estimated_cost: f64,
}

impl HeaderState {
    pub fn new() -> Self {
        Self { task_title: None, tokens_used: 0, context_limit: 128_000, estimated_cost: 0.0 }
    }

    /// Calculate context usage percentage
    pub fn context_percentage(&self) -> u8 {
        if self.context_limit == 0 {
            return 0;
        }
        ((self.tokens_used as f64 / self.context_limit as f64) * 100.0).min(100.0) as u8
    }

    /// Set task title from the first user message
    ///
    /// Takes first 50 chars, truncates at word boundary if needed.
    pub fn set_task_title_from_message(&mut self, message: &str) {
        if self.task_title.is_some() {
            return;
        }

        let title = derive_task_title(message);
        self.task_title = Some(title);
    }

    /// Format tokens for display (e.g., "14,295" or "14.3k")
    pub fn tokens_display(&self) -> String {
        if self.tokens_used >= 1000 {
            format!("{:.1}k", self.tokens_used as f64 / 1000.0)
        } else {
            self.tokens_used.to_string()
        }
    }

    /// Format cost for display (e.g., "$0.00")
    pub fn cost_display(&self) -> String {
        format!("${:.2}", self.estimated_cost)
    }

    /// Update token count
    pub fn update_tokens(&mut self, tokens: usize) {
        self.tokens_used = tokens;
    }

    /// Update estimated cost
    pub fn update_cost(&mut self, cost: f64) {
        self.estimated_cost = cost;
    }
}

/// Derive a task title from the first user message
///
/// Takes first line, truncates to ~50 chars at word boundary.
fn derive_task_title(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or(message).trim();

    if first_line.is_empty() {
        return "New Session".to_string();
    }

    if first_line.len() <= 50 {
        return first_line.to_string();
    }

    let truncated = &first_line[..50];
    if let Some(last_space) = truncated.rfind(' ')
        && last_space > 30
    {
        return format!("{}...", &truncated[..last_space]);
    }

    format!("{}...", &first_line[..47])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_header_state_new() {
        let state = HeaderState::new();
        assert!(state.task_title.is_none());
        assert_eq!(state.tokens_used, 0);
        assert_eq!(state.context_limit, 128_000);
        assert_eq!(state.estimated_cost, 0.0);
    }

    #[test]
    fn test_context_percentage() {
        let mut state = HeaderState::new();
        state.tokens_used = 12800;
        assert_eq!(state.context_percentage(), 10);

        state.tokens_used = 64000;
        assert_eq!(state.context_percentage(), 50);

        state.tokens_used = 128000;
        assert_eq!(state.context_percentage(), 100);
    }

    #[test]
    fn test_set_task_title_from_message_short() {
        let mut state = HeaderState::new();
        state.set_task_title_from_message("Fix the login bug");
        assert_eq!(state.task_title, Some("Fix the login bug".to_string()));
    }

    #[test]
    fn test_set_task_title_from_message_long() {
        let mut state = HeaderState::new();
        state.set_task_title_from_message(
            "This is a very long message that should be truncated because it exceeds the limit",
        );
        let title = state.task_title.unwrap();
        assert!(title.len() <= 53);
        assert!(title.ends_with("..."));
    }

    #[test]
    fn test_set_task_title_only_once() {
        let mut state = HeaderState::new();
        state.set_task_title_from_message("First message");
        state.set_task_title_from_message("Second message");
        assert_eq!(state.task_title, Some("First message".to_string()));
    }

    #[test]
    fn test_tokens_display() {
        let mut state = HeaderState::new();
        state.tokens_used = 500;
        assert_eq!(state.tokens_display(), "500");

        state.tokens_used = 1500;
        assert_eq!(state.tokens_display(), "1.5k");

        state.tokens_used = 14295;
        assert_eq!(state.tokens_display(), "14.3k");
    }

    #[test]
    fn test_cost_display() {
        let mut state = HeaderState::new();
        assert_eq!(state.cost_display(), "$0.00");

        state.estimated_cost = 0.05;
        assert_eq!(state.cost_display(), "$0.05");

        state.estimated_cost = 1.234;
        assert_eq!(state.cost_display(), "$1.23");
    }

    #[test]
    fn test_derive_task_title_empty() {
        assert_eq!(derive_task_title(""), "New Session");
        assert_eq!(derive_task_title("   "), "New Session");
    }

    #[test]
    fn test_derive_task_title_multiline() {
        let result = derive_task_title("First line\nSecond line\nThird line");
        assert_eq!(result, "First line");
    }
}
