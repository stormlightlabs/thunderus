use crate::transcript::{Transcript as TranscriptState, TranscriptRenderer};
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
    pub fn new(transcript: &'a TranscriptState) -> Self {
        let renderer = TranscriptRenderer::new(transcript);
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
        let component = Transcript::new(&transcript_state);
        assert_eq!(component.inner().len(), 0);
    }

    #[test]
    fn test_transcript_with_entries() {
        let mut transcript_state = TranscriptState::new();
        transcript_state.add_user_message("Hello");
        transcript_state.add_model_response("Hi there");

        let component = Transcript::new(&transcript_state);
        assert_eq!(component.inner().len(), 2);
    }

    #[test]
    fn test_transcript_inner() {
        let transcript_state = TranscriptState::new();
        let component = Transcript::new(&transcript_state);

        assert_eq!(component.inner(), &transcript_state);
    }
}
