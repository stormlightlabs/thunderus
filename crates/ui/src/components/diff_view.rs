use crate::state::AppState;
use crate::theme::Theme;
use ratatui::{
    Frame,
    layout::Alignment,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use thunderus_core::{Patch, PatchStatus};

/// Diff view component for displaying patches with hunk navigation
///
/// This component renders:
/// - Summary view: List of patches with status and file counts
/// - Detailed view: Individual hunks with approve/reject indicators
pub struct DiffView<'a> {
    state: &'a AppState,
    patches: &'a [Patch],
}

impl<'a> DiffView<'a> {
    pub fn new(state: &'a AppState, patches: &'a [Patch]) -> Self {
        Self { state, patches }
    }

    /// Render the diff view
    pub fn render(&self, frame: &mut Frame<'_>, area: ratatui::layout::Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        if self.patches.is_empty() {
            self.render_empty(frame, area, theme);
            return;
        }

        let nav = self.state.diff_navigation();

        if !nav.show_hunk_details {
            self.render_summary(frame, area, theme);
        } else {
            self.render_detailed(frame, area, theme);
        }
    }

    /// Render empty state when no patches
    fn render_empty(&self, frame: &mut Frame<'_>, area: ratatui::layout::Rect, theme: crate::theme::ThemePalette) {
        let paragraph = Paragraph::new("No patches in queue")
            .block(
                Block::default()
                    .title(Span::styled("Patches", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    /// Render summary view of patches
    fn render_summary(&self, frame: &mut Frame<'_>, area: ratatui::layout::Rect, theme: crate::theme::ThemePalette) {
        let mut lines = Vec::new();

        let nav = self.state.diff_navigation();

        for (idx, patch) in self.patches.iter().enumerate() {
            let is_selected = nav.selected_patch_index == Some(idx);
            let status_color = self.status_color(&patch.status);
            let status_text = self.status_text(&patch.status);

            let base_style =
                if is_selected { Style::default().bg(theme.blue).fg(theme.black) } else { Style::default() };

            lines.push(Line::from(vec![
                Span::styled(format!("{} ", if is_selected { ">" } else { " " }), base_style),
                Span::styled(format!("#{} ", idx + 1), Style::default().fg(theme.muted)),
                Span::styled(status_text, status_color),
                Span::styled(format!(" {} ({})", patch.name, patch.files.len()), base_style),
            ]));
        }

        let help_text = Line::from(vec![
            Span::styled("N", Style::default().fg(theme.blue)),
            Span::raw("/"),
            Span::styled("P", Style::default().fg(theme.blue)),
            Span::raw(": prev/next patch | "),
            Span::styled("Enter", Style::default().fg(theme.blue)),
            Span::raw(": view details"),
        ]);

        lines.push(help_text);

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled("Patches", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    /// Render detailed view of a patch with hunks
    fn render_detailed(&self, frame: &mut Frame<'_>, area: ratatui::layout::Rect, theme: crate::theme::ThemePalette) {
        let nav = self.state.diff_navigation();

        let Some(patch_idx) = nav.selected_patch_index else {
            return self.render_summary(frame, area, theme);
        };

        let Some(patch) = self.patches.get(patch_idx) else {
            return self.render_summary(frame, area, theme);
        };

        let mut lines = vec![Line::from(vec![Span::styled(
            format!("Patch #{}: {}", patch_idx + 1, patch.name),
            Style::default().fg(theme.blue).bold(),
        )])];

        lines.push(Line::from(vec![Span::styled(
            format!(
                "Status: {}, Files: {}",
                self.status_text(&patch.status),
                patch.files.len()
            ),
            Style::default().fg(theme.muted),
        )]));

        lines.push(Line::from(""));

        for file_path in &patch.files {
            let Some(hunks) = patch.hunks.get(file_path) else {
                continue;
            };

            let is_selected_file = nav.selected_file_path.as_deref() == Some(file_path.to_str().unwrap_or(""));

            lines.push(Line::from(vec![
                Span::styled(
                    if is_selected_file { "> " } else { "  " },
                    Style::default().fg(theme.blue),
                ),
                Span::styled(
                    file_path.to_str().unwrap_or("<invalid>"),
                    if is_selected_file { Style::default().bold() } else { Style::default() },
                ),
            ]));

            if is_selected_file {
                for (hunk_idx, hunk) in hunks.iter().enumerate() {
                    let is_selected_hunk = nav.selected_hunk_index == Some(hunk_idx);
                    let hunk_style = if is_selected_hunk { Style::default().bg(theme.blue) } else { Style::default() };

                    lines.push(Line::from(vec![
                        Span::styled("    ", hunk_style),
                        Span::styled(hunk.header(), Style::default().fg(theme.cyan)),
                        Span::styled(
                            if hunk.approved { " [APPROVED]" } else { " [PENDING]" },
                            if hunk.approved {
                                Style::default().fg(theme.green)
                            } else {
                                Style::default().fg(theme.yellow)
                            },
                        ),
                    ]));

                    if let Some(ref intent) = hunk.intent {
                        lines.push(Line::from(vec![
                            Span::styled("      ", hunk_style),
                            Span::styled(format!("Intent: {}", intent), Style::default().fg(theme.muted)),
                        ]));
                    }

                    if is_selected_hunk {
                        for hunk_line in hunk.content.lines().take(3) {
                            let line_style = if hunk_line.starts_with('-') {
                                Style::default().fg(theme.red)
                            } else if hunk_line.starts_with('+') {
                                Style::default().fg(theme.green)
                            } else {
                                Style::default().fg(theme.fg)
                            };

                            lines.push(Line::from(vec![
                                Span::styled("        ", hunk_style),
                                Span::styled(hunk_line, line_style),
                            ]));
                        }

                        if hunk.content.lines().count() > 3 {
                            lines.push(Line::from(vec![
                                Span::styled("        ", hunk_style),
                                Span::styled(
                                    format!("(+ {} more lines)", hunk.content.lines().count() - 3),
                                    Style::default().fg(theme.muted),
                                ),
                            ]));
                        }
                    }
                }
            }
        }

        lines.push(Line::from(""));

        let help_text = Line::from(vec![
            Span::styled("Esc", Style::default().fg(theme.blue)),
            Span::raw(": back | "),
            Span::styled("n", Style::default().fg(theme.blue)),
            Span::raw("/"),
            Span::styled("p", Style::default().fg(theme.blue)),
            Span::raw(": hunk nav | "),
            Span::styled("a", Style::default().fg(theme.green)),
            Span::raw("/"),
            Span::styled("r", Style::default().fg(theme.red)),
            Span::raw(": approve/reject"),
        ]);

        lines.push(help_text);

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled("Patch Details", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    /// Get color for patch status
    fn status_color(&self, status: &PatchStatus) -> Style {
        let theme = Theme::palette(self.state.theme_variant());
        match status {
            PatchStatus::Proposed => Style::default().fg(theme.yellow),
            PatchStatus::Approved => Style::default().fg(theme.green),
            PatchStatus::Applied => Style::default().fg(theme.green),
            PatchStatus::Rejected => Style::default().fg(theme.red),
            PatchStatus::Failed => Style::default().fg(theme.red).bold(),
        }
    }

    /// Get text for patch status
    fn status_text(&self, status: &PatchStatus) -> &'static str {
        match status {
            PatchStatus::Proposed => "PROPOSED",
            PatchStatus::Approved => "APPROVED",
            PatchStatus::Applied => "APPLIED",
            PatchStatus::Rejected => "REJECTED",
            PatchStatus::Failed => "FAILED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, PatchId, ProviderConfig, SandboxMode, SessionId};

    fn create_test_state() -> AppState {
        AppState::new(
            PathBuf::from("."),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
        )
    }

    fn create_test_patch(id: &str, name: &str) -> Patch {
        let diff = "diff --git a/test.rs b/test.rs\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        Patch::new(
            PatchId::new(id),
            name.to_string(),
            "HEAD".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap()
    }

    #[test]
    fn test_diff_view_new() {
        let state = create_test_state();
        let patches = vec![];
        let diff_view = DiffView::new(&state, &patches);

        assert_eq!(diff_view.patches.len(), 0);
    }

    #[test]
    fn test_diff_view_with_patches() {
        let state = create_test_state();
        let patches = vec![create_test_patch("patch1", "Test patch")];
        let diff_view = DiffView::new(&state, &patches);

        assert_eq!(diff_view.patches.len(), 1);
    }

    #[test]
    fn test_status_color() {
        let state = create_test_state();
        let diff_view = DiffView::new(&state, &[]);
        let theme = Theme::palette(state.theme_variant());

        assert_eq!(
            diff_view.status_color(&PatchStatus::Proposed),
            Style::default().fg(theme.yellow)
        );
        assert_eq!(
            diff_view.status_color(&PatchStatus::Approved),
            Style::default().fg(theme.green)
        );
        assert_eq!(
            diff_view.status_color(&PatchStatus::Applied),
            Style::default().fg(theme.green)
        );
        assert_eq!(
            diff_view.status_color(&PatchStatus::Rejected),
            Style::default().fg(theme.red)
        );
        assert_eq!(
            diff_view.status_color(&PatchStatus::Failed),
            Style::default().fg(theme.red).bold()
        );
    }

    #[test]
    fn test_status_text() {
        let state = create_test_state();
        let diff_view = DiffView::new(&state, &[]);

        assert_eq!(diff_view.status_text(&PatchStatus::Proposed), "PROPOSED");
        assert_eq!(diff_view.status_text(&PatchStatus::Approved), "APPROVED");
        assert_eq!(diff_view.status_text(&PatchStatus::Applied), "APPLIED");
        assert_eq!(diff_view.status_text(&PatchStatus::Rejected), "REJECTED");
        assert_eq!(diff_view.status_text(&PatchStatus::Failed), "FAILED");
    }

    #[test]
    fn test_diff_view_with_multiple_patches() {
        let state = create_test_state();
        let patches = vec![
            create_test_patch("patch1", "First patch"),
            create_test_patch("patch2", "Second patch"),
            create_test_patch("patch3", "Third patch"),
        ];
        let diff_view = DiffView::new(&state, &patches);

        assert_eq!(diff_view.patches.len(), 3);
    }

    #[test]
    fn test_diff_view_with_navigation_state() {
        let mut state = create_test_state();
        let patches = vec![
            create_test_patch("patch1", "First patch"),
            create_test_patch("patch2", "Second patch"),
        ];

        state.ui.diff_navigation.selected_patch_index = Some(1);
        let diff_view = DiffView::new(&state, &patches);

        assert_eq!(diff_view.patches.len(), 2);
        assert_eq!(state.ui.diff_navigation.selected_patch_index, Some(1));
    }

    #[test]
    fn test_diff_view_with_hunk_selection() {
        let mut state = create_test_state();
        let diff = "diff --git a/test.rs b/test.rs\n@@ -1,2 +1,2 @@\n line1\n-old\n+new";
        let session_id = SessionId::new();
        let patch = Patch::new(
            PatchId::new("patch1"),
            "Test patch".to_string(),
            "HEAD".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        let patches = vec![patch];
        state.ui.diff_navigation.selected_patch_index = Some(0);
        state.ui.diff_navigation.selected_file_path = Some("test.rs".to_string());
        state.ui.diff_navigation.selected_hunk_index = Some(0);
        state.ui.diff_navigation.show_hunk_details = true;

        let diff_view = DiffView::new(&state, &patches);
        assert_eq!(diff_view.patches.len(), 1);
    }

    #[test]
    fn test_diff_view_hunks_have_approval_status() {
        let diff = "diff --git a/test.rs b/test.rs\n@@ -1,2 +1,2 @@\n line1\n-old\n+new";
        let session_id = SessionId::new();
        let mut patch = Patch::new(
            PatchId::new("patch1"),
            "Test patch".to_string(),
            "HEAD".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        let hunk = &patch.hunks[&std::path::PathBuf::from("test.rs")][0];
        assert!(!hunk.approved);

        patch.approve_hunk(&std::path::PathBuf::from("test.rs"), 0).unwrap();
        let hunk = &patch.hunks[&std::path::PathBuf::from("test.rs")][0];
        assert!(hunk.approved);
    }

    #[test]
    fn test_diff_view_intent_labeling() {
        let diff = "diff --git a/test.rs b/test.rs\n@@ -1,2 +1,2 @@\n line1\n-old\n+new";
        let session_id = SessionId::new();
        let mut patch = Patch::new(
            PatchId::new("patch1"),
            "Test patch".to_string(),
            "HEAD".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        patch
            .set_hunk_intent(&std::path::PathBuf::from("test.rs"), 0, "Fix bug".to_string())
            .unwrap();

        let hunk = &patch.hunks[&std::path::PathBuf::from("test.rs")][0];
        assert_eq!(hunk.intent, Some("Fix bug".to_string()));
    }

    #[test]
    fn test_diff_view_empty_patch_list() {
        let state = create_test_state();
        let patches: Vec<Patch> = vec![];
        let diff_view = DiffView::new(&state, &patches);

        assert_eq!(diff_view.patches.len(), 0);
    }

    #[test]
    fn test_diff_view_applied_vs_proposed_status() {
        let state = create_test_state();
        let diff = "diff --git a/test.rs b/test.rs\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        let mut proposed_patch = Patch::new(
            PatchId::new("patch1"),
            "Proposed patch".to_string(),
            "HEAD".to_string(),
            diff.to_string(),
            session_id.clone(),
            0,
        )
        .unwrap();

        let mut applied_patch = Patch::new(
            PatchId::new("patch2"),
            "Applied patch".to_string(),
            "HEAD".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        proposed_patch.status = PatchStatus::Proposed;
        applied_patch.status = PatchStatus::Applied;

        let patches = vec![proposed_patch, applied_patch];
        let diff_view = DiffView::new(&state, &patches);
        assert_eq!(diff_view.patches[0].status, PatchStatus::Proposed);
        assert_eq!(diff_view.patches[1].status, PatchStatus::Applied);

        let proposed_color = diff_view.status_color(&PatchStatus::Proposed);
        let applied_color = diff_view.status_color(&PatchStatus::Applied);
        let theme = Theme::palette(state.theme_variant());
        assert_eq!(proposed_color, Style::default().fg(theme.yellow));
        assert_eq!(applied_color, Style::default().fg(theme.green));
    }
}
