#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrollState {
    pub offset: usize,
    pub page_size: usize,
    pub total: usize,
}

impl ScrollState {
    pub fn with_viewport(total: usize, page_size: usize) -> Self {
        let mut state = Self { offset: 0, page_size, total };
        state.clamp();
        state
    }

    pub fn set_page_size(&mut self, page_size: usize) {
        self.page_size = page_size;
        self.clamp();
    }

    pub fn set_total(&mut self, total: usize) {
        self.total = total;
        self.clamp();
    }

    pub fn set_viewport(&mut self, total: usize, page_size: usize) {
        self.total = total;
        self.page_size = page_size;
        self.clamp();
    }

    pub fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
        self.clamp();
    }

    pub fn max_offset(&self) -> usize {
        self.total.saturating_sub(self.page_size)
    }

    pub fn clamp(&mut self) {
        self.offset = self.offset.min(self.max_offset());
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.offset = self.offset.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.offset = self.offset.saturating_add(n).min(self.max_offset());
    }

    pub fn page_up(&mut self) {
        let amount = self.page_size.max(1);
        self.scroll_up(amount);
    }

    pub fn page_down(&mut self) {
        let amount = self.page_size.max(1);
        self.scroll_down(amount);
    }

    pub fn ensure_visible(&mut self, index: usize) {
        if self.page_size == 0 {
            self.offset = index.min(self.max_offset());
            return;
        }

        if index < self.offset {
            self.offset = index;
        } else {
            let bottom = self.offset.saturating_add(self.page_size.saturating_sub(1));
            if index > bottom {
                self.offset = index.saturating_add(1).saturating_sub(self.page_size);
            }
        }

        self.clamp();
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollState;

    #[test]
    fn clamp_keeps_offset_within_bounds() {
        let mut state = ScrollState { offset: 20, page_size: 5, total: 12 };
        state.clamp();
        assert_eq!(state.offset, 7);
    }

    #[test]
    fn page_navigation_uses_page_size() {
        let mut state = ScrollState::with_viewport(100, 10);
        state.page_down();
        assert_eq!(state.offset, 10);
        state.page_up();
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn ensure_visible_scrolls_to_include_index() {
        let mut state = ScrollState::with_viewport(100, 5);
        state.ensure_visible(9);
        assert_eq!(state.offset, 5);
        state.ensure_visible(3);
        assert_eq!(state.offset, 3);
    }
}
