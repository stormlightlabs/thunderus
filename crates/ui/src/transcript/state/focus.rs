use super::Transcript;
use crate::transcript::entry::CardDetailLevel;

impl Transcript {
    /// Get the index of the currently focused action card
    pub fn focused_card_index(&self) -> Option<usize> {
        self.focused_card_index
    }

    /// Get all action card indices in the transcript
    fn get_action_card_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_action_card())
            .map(|(i, _)| i)
            .collect()
    }

    /// Focus the first action card
    pub fn focus_first_card(&mut self) -> bool {
        let card_indices = self.get_action_card_indices();
        if let Some(&first) = card_indices.first() {
            self.focused_card_index = Some(first);
            true
        } else {
            false
        }
    }

    /// Focus the last action card
    pub fn focus_last_card(&mut self) -> bool {
        let card_indices = self.get_action_card_indices();
        if let Some(&last) = card_indices.last() {
            self.focused_card_index = Some(last);
            true
        } else {
            false
        }
    }

    /// Focus the next action card
    pub fn focus_next_card(&mut self) -> bool {
        let card_indices = self.get_action_card_indices();
        if card_indices.is_empty() {
            return false;
        }

        match self.focused_card_index {
            Some(current) => {
                if let Some(pos) = card_indices.iter().position(|&i| i == current)
                    && pos + 1 < card_indices.len()
                {
                    self.focused_card_index = Some(card_indices[pos + 1]);
                    return true;
                }
            }
            None => {
                if let Some(&first) = card_indices.first() {
                    self.focused_card_index = Some(first);
                    return true;
                }
            }
        }
        false
    }

    /// Focus the previous action card
    pub fn focus_prev_card(&mut self) -> bool {
        let card_indices = self.get_action_card_indices();
        if card_indices.is_empty() {
            return false;
        }

        match self.focused_card_index {
            Some(current) => {
                if let Some(pos) = card_indices.iter().position(|&i| i == current)
                    && pos > 0
                {
                    self.focused_card_index = Some(card_indices[pos - 1]);
                    return true;
                }
            }
            None => {
                if let Some(&last) = card_indices.last() {
                    self.focused_card_index = Some(last);
                    return true;
                }
            }
        }
        false
    }

    /// Toggle detail level of the currently focused card
    pub fn toggle_focused_card_detail_level(&mut self) -> bool {
        if let Some(idx) = self.focused_card_index
            && let Some(entry) = self.entries.get_mut(idx)
        {
            entry.toggle_detail_level();
            return true;
        }
        false
    }

    /// Set detail level of the currently focused card
    pub fn set_focused_card_detail_level(&mut self, level: CardDetailLevel) -> bool {
        if let Some(idx) = self.focused_card_index
            && let Some(entry) = self.entries.get_mut(idx)
        {
            entry.set_detail_level(level);
            return true;
        }
        false
    }

    /// Clear card focus
    pub fn clear_card_focus(&mut self) {
        self.focused_card_index = None;
    }

    /// Check if a specific entry is focused
    pub fn is_entry_focused(&self, index: usize) -> bool {
        self.focused_card_index.map(|i| i == index).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_focus_initial() {
        let transcript = Transcript::new();
        assert_eq!(transcript.focused_card_index(), None);
    }

    #[test]
    fn test_card_focus_no_cards() {
        let mut transcript = Transcript::new();
        assert!(!transcript.focus_first_card());
        assert!(!transcript.focus_last_card());
        assert!(!transcript.focus_next_card());
        assert!(!transcript.focus_prev_card());
        assert_eq!(transcript.focused_card_index(), None);
    }

    #[test]
    fn test_card_focus_single_card() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("test", "{}", "safe");
        assert!(transcript.focus_first_card());
        assert_eq!(transcript.focused_card_index(), Some(0));

        transcript.clear_card_focus();
        assert!(transcript.focus_last_card());
        assert_eq!(transcript.focused_card_index(), Some(0));
    }

    #[test]
    fn test_card_focus_multiple_cards() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Message 1");
        transcript.add_tool_call("tool1", "{}", "safe");
        transcript.add_model_response("Response 1");
        transcript.add_tool_call("tool2", "{}", "safe");
        transcript.add_tool_result("tool1", "result", true);
        transcript.add_user_message("Message 2");

        assert!(transcript.focus_first_card());
        assert_eq!(transcript.focused_card_index(), Some(1));

        assert!(transcript.focus_next_card());
        assert_eq!(transcript.focused_card_index(), Some(3));

        assert!(transcript.focus_next_card());
        assert_eq!(transcript.focused_card_index(), Some(4));

        assert!(!transcript.focus_next_card());
        assert_eq!(transcript.focused_card_index(), Some(4));
    }

    #[test]
    fn test_card_focus_prev() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("tool1", "{}", "safe");
        transcript.add_tool_call("tool2", "{}", "safe");
        transcript.add_tool_call("tool3", "{}", "safe");

        transcript.focus_last_card();
        assert_eq!(transcript.focused_card_index(), Some(2));

        assert!(transcript.focus_prev_card());
        assert_eq!(transcript.focused_card_index(), Some(1));

        assert!(transcript.focus_prev_card());
        assert_eq!(transcript.focused_card_index(), Some(0));

        assert!(!transcript.focus_prev_card());
        assert_eq!(transcript.focused_card_index(), Some(0));
    }

    #[test]
    fn test_card_focus_clear() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("test", "{}", "safe");
        transcript.focus_first_card();
        assert_eq!(transcript.focused_card_index(), Some(0));

        transcript.clear_card_focus();
        assert_eq!(transcript.focused_card_index(), None);
    }

    #[test]
    fn test_toggle_card_detail_level() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("test", "{}", "safe");
        transcript.focus_first_card();

        assert_eq!(
            transcript.entries().front().unwrap().detail_level(),
            CardDetailLevel::Brief
        );

        assert!(transcript.toggle_focused_card_detail_level());
        assert_eq!(
            transcript.entries().front().unwrap().detail_level(),
            CardDetailLevel::Detailed
        );

        assert!(transcript.toggle_focused_card_detail_level());
        assert_eq!(
            transcript.entries().front().unwrap().detail_level(),
            CardDetailLevel::Verbose
        );
    }

    #[test]
    fn test_toggle_card_detail_level_no_focus() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("test", "{}", "safe");
        assert!(!transcript.toggle_focused_card_detail_level());
    }

    #[test]
    fn test_set_card_detail_level() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("test", "{}", "safe");
        transcript.focus_first_card();
        assert!(transcript.set_focused_card_detail_level(CardDetailLevel::Verbose));
        assert_eq!(
            transcript.entries().front().unwrap().detail_level(),
            CardDetailLevel::Verbose
        );
    }

    #[test]
    fn test_is_entry_focused() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Message");
        transcript.add_tool_call("tool", "{}", "safe");
        transcript.add_model_response("Response");
        transcript.focus_first_card();

        assert!(!transcript.is_entry_focused(0));
        assert!(transcript.is_entry_focused(1));
        assert!(!transcript.is_entry_focused(2));
    }

    #[test]
    fn test_clear_clears_focus() {
        let mut transcript = Transcript::new();
        transcript.add_tool_call("test", "{}", "safe");
        transcript.focus_first_card();
        assert_eq!(transcript.focused_card_index(), Some(0));

        transcript.clear();
        assert_eq!(transcript.focused_card_index(), None);
    }
}
