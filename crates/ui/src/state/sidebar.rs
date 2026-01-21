/// Sidebar section for collapse/expand
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    TokenUsage,
    Events,
    Modified,
    Diffs,
    Integrations,
}

impl SidebarSection {
    pub fn all() -> [SidebarSection; 5] {
        [
            Self::TokenUsage,
            Self::Events,
            Self::Modified,
            Self::Diffs,
            Self::Integrations,
        ]
    }

    pub fn name(&self) -> &str {
        match self {
            SidebarSection::TokenUsage => "Token Usage",
            SidebarSection::Events => "Events",
            SidebarSection::Modified => "Modified",
            SidebarSection::Diffs => "Diffs",
            SidebarSection::Integrations => "Integrations",
        }
    }
}

/// Collapse state for sidebar sections
#[derive(Debug, Clone, Default)]
pub struct SidebarCollapseState {
    token_usage_collapsed: bool,
    events_collapsed: bool,
    modified_collapsed: bool,
    diffs_collapsed: bool,
    integrations_collapsed: bool,
}

impl SidebarCollapseState {
    pub fn is_collapsed(&self, section: SidebarSection) -> bool {
        match section {
            SidebarSection::TokenUsage => self.token_usage_collapsed,
            SidebarSection::Events => self.events_collapsed,
            SidebarSection::Modified => self.modified_collapsed,
            SidebarSection::Diffs => self.diffs_collapsed,
            SidebarSection::Integrations => self.integrations_collapsed,
        }
    }

    pub fn toggle(&mut self, section: SidebarSection) {
        match section {
            SidebarSection::TokenUsage => self.token_usage_collapsed = !self.token_usage_collapsed,
            SidebarSection::Events => self.events_collapsed = !self.events_collapsed,
            SidebarSection::Modified => self.modified_collapsed = !self.modified_collapsed,
            SidebarSection::Diffs => self.diffs_collapsed = !self.diffs_collapsed,
            SidebarSection::Integrations => self.integrations_collapsed = !self.integrations_collapsed,
        }
    }

    pub fn collapse_prev(&mut self) {
        let sections = SidebarSection::all();
        if let Some(pos) = sections.iter().position(|s| !self.is_collapsed(*s)) {
            let new_pos = if pos == 0 { sections.len() - 1 } else { pos - 1 };
            self.toggle(sections[new_pos]);
        }
    }

    pub fn expand_next(&mut self) {
        let sections = SidebarSection::all();
        if let Some(pos) = sections.iter().position(|s| !self.is_collapsed(*s)) {
            let new_pos = (pos + 1) % sections.len();
            self.toggle(sections[new_pos]);
        }
    }
}
