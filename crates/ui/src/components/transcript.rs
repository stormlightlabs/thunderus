use crate::theme::ThemePalette;
use crate::transcript::{RenderOptions, Transcript as TranscriptState, TranscriptRenderer};
use ratatui::{Frame, layout::Rect};

/// Transcript component displaying of conversation
///
/// This component wraps the transcript module and provides a
/// TUI rendering interface used by the app.
pub struct Transcript<'a> {
    transcript: &'a TranscriptState,
    renderer: TranscriptRenderer<'a>,
}

impl<'a> Transcript<'a> {
    pub fn new(transcript: &'a TranscriptState, theme: ThemePalette) -> Self {
        let renderer = TranscriptRenderer::new(transcript, theme);
        Self { transcript, renderer }
    }

    /// Create a new transcript component with vertical scroll offset
    pub fn with_vertical_scroll(
        transcript: &'a TranscriptState, scroll: u16, theme: ThemePalette, options: RenderOptions,
    ) -> Self {
        let renderer = TranscriptRenderer::with_vertical_scroll(transcript, scroll, theme, options);
        Self { transcript, renderer }
    }

    /// Create a new transcript component with streaming ellipsis animation
    pub fn with_streaming_ellipsis(
        transcript: &'a TranscriptState, scroll: u16, ellipsis: &'a str, theme: ThemePalette, options: RenderOptions,
    ) -> Self {
        let renderer = TranscriptRenderer::with_streaming_ellipsis(transcript, scroll, ellipsis, theme, options);
        Self { transcript, renderer }
    }

    /// Render transcript to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        self.renderer.render(frame, area);
    }

    /// Get the underlying transcript
    pub fn inner(&self) -> &TranscriptState {
        self.transcript
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_new() {
        let transcript_state = TranscriptState::new();
        let theme = crate::theme::Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let component = Transcript::new(&transcript_state, theme);
        assert_eq!(component.inner().len(), 0);
    }

    #[test]
    fn test_transcript_with_entries() {
        let mut transcript_state = TranscriptState::new();
        transcript_state.add_user_message("Hello");
        transcript_state.add_model_response("Hi there");

        let theme = crate::theme::Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let component = Transcript::new(&transcript_state, theme);
        assert_eq!(component.inner().len(), 2);
    }

    #[test]
    fn test_transcript_inner() {
        let transcript_state = TranscriptState::new();
        let theme = crate::theme::Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let component = Transcript::new(&transcript_state, theme);

        assert_eq!(component.inner(), &transcript_state);
    }
}
