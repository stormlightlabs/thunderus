/// State for the welcome screen
#[derive(Debug, Clone)]
pub struct WelcomeState {
    /// Current tip index for rotating tips display
    pub current_tip_index: usize,
    /// Recent sessions for quick access
    pub recent_sessions: Vec<RecentSessionInfo>,
}

/// Information about a recent session for the welcome screen
#[derive(Debug, Clone)]
pub struct RecentSessionInfo {
    /// Session identifier
    pub id: String,
    /// Session title (first message or generated summary)
    pub title: Option<String>,
}

/// Tips shown on the welcome screen
pub const WELCOME_TIPS: &[&str] = &[
    "Use /theme to switch between themes",
    "Press Ctrl+S to toggle the sidebar",
    "Use Tab to autocomplete file paths",
    "Press Ctrl+T to change the color theme",
    "Use !cmd to run shell commands directly",
    "Press Ctrl+Shift+G to open external editor",
];

impl WelcomeState {
    pub fn new() -> Self {
        Self { current_tip_index: 0, recent_sessions: Vec::new() }
    }

    /// Get the current tip text
    pub fn current_tip(&self) -> &'static str {
        WELCOME_TIPS[self.current_tip_index % WELCOME_TIPS.len()]
    }

    /// Advance to the next tip
    pub fn next_tip(&mut self) {
        self.current_tip_index = (self.current_tip_index + 1) % WELCOME_TIPS.len();
    }

    /// Add a recent session
    pub fn add_recent_session(&mut self, id: String, title: Option<String>) {
        self.recent_sessions.retain(|s| s.id != id);
        self.recent_sessions.insert(0, RecentSessionInfo { id, title });

        const MAX_RECENT_SESSIONS: usize = 3;
        self.recent_sessions.truncate(MAX_RECENT_SESSIONS);
    }
}

impl Default for WelcomeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welcome_state_new() {
        let state = WelcomeState::new();
        assert_eq!(state.current_tip_index, 0);
        assert!(state.recent_sessions.is_empty());
    }

    #[test]
    fn test_current_tip() {
        let state = WelcomeState::new();
        assert_eq!(state.current_tip(), WELCOME_TIPS[0]);
    }

    #[test]
    fn test_next_tip() {
        let mut state = WelcomeState::new();
        assert_eq!(state.current_tip_index, 0);

        state.next_tip();
        assert_eq!(state.current_tip_index, 1);
        assert_eq!(state.current_tip(), WELCOME_TIPS[1]);
    }

    #[test]
    fn test_tip_wraps_around() {
        let mut state = WelcomeState::new();

        for _ in 0..WELCOME_TIPS.len() {
            state.next_tip();
        }

        assert_eq!(state.current_tip_index, 0);
    }

    #[test]
    fn test_add_recent_session() {
        let mut state = WelcomeState::new();

        state.add_recent_session("session1".to_string(), Some("First session".to_string()));
        assert_eq!(state.recent_sessions.len(), 1);
        assert_eq!(state.recent_sessions[0].id, "session1");

        state.add_recent_session("session2".to_string(), Some("Second session".to_string()));
        assert_eq!(state.recent_sessions.len(), 2);
        assert_eq!(state.recent_sessions[0].id, "session2");
    }

    #[test]
    fn test_add_recent_session_deduplicates() {
        let mut state = WelcomeState::new();

        state.add_recent_session("session1".to_string(), Some("First".to_string()));
        state.add_recent_session("session2".to_string(), Some("Second".to_string()));
        state.add_recent_session("session1".to_string(), Some("Updated First".to_string()));

        assert_eq!(state.recent_sessions.len(), 2);
        assert_eq!(state.recent_sessions[0].id, "session1");
        assert_eq!(state.recent_sessions[0].title, Some("Updated First".to_string()));
    }

    #[test]
    fn test_add_recent_session_max_limit() {
        let mut state = WelcomeState::new();

        for i in 0..5 {
            state.add_recent_session(format!("session{}", i), Some(format!("Session {}", i)));
        }

        assert_eq!(state.recent_sessions.len(), 3);
        assert_eq!(state.recent_sessions[0].id, "session4");
        assert_eq!(state.recent_sessions[1].id, "session3");
        assert_eq!(state.recent_sessions[2].id, "session2");
    }
}
