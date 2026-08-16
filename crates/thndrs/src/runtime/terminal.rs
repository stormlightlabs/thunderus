//! Ratatui terminal surface used by the interactive runtime.

use super::*;

pub(crate) trait InteractiveSurface {
    fn draw(&mut self, app: &mut App, full_repaint: bool) -> io::Result<()>;
    fn resize(&mut self, width: u16, height: u16) -> io::Result<()>;
    fn clear(&mut self) -> io::Result<()>;
    fn suspend(&mut self) -> io::Result<()>;
    fn handle_navigation(&mut self, app: &mut App, action: &Action) -> bool;
}

pub(crate) struct RatatuiSurface<W: io::Write> {
    terminal: Terminal<CrosstermBackend<W>>,
    viewport: AlternateViewport,
    pub(crate) terminal_session: AlternateScreenSession,
}

impl<W: io::Write> RatatuiSurface<W> {
    pub(crate) fn new(terminal: Terminal<CrosstermBackend<W>>, terminal_session: AlternateScreenSession) -> Self {
        Self { terminal, viewport: AlternateViewport::default(), terminal_session }
    }
}

impl<W: io::Write> InteractiveSurface for RatatuiSurface<W> {
    fn draw(&mut self, app: &mut App, full_repaint: bool) -> io::Result<()> {
        if full_repaint {
            self.terminal.clear()?;
        }
        let projection_started = Instant::now();
        renderer::style::set_theme(app.runtime.theme);
        let area = self.terminal.size()?;
        let logical = self
            .viewport
            .build_frame(app, area.width as usize, area.height as usize);
        let projection_elapsed = projection_started.elapsed();
        let draw_started = Instant::now();
        self.terminal.draw(|frame| render_logical_frame(frame, &logical))?;
        tracing::trace!(
            projection_us = projection_elapsed.as_micros(),
            draw_us = draw_started.elapsed().as_micros(),
            width = area.width,
            height = area.height,
            "ratatui frame timing"
        );
        Ok(())
    }

    fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.terminal.resize(Rect::new(0, 0, width, height))?;
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.viewport.reset();
        self.terminal.clear()
    }

    fn suspend(&mut self) -> io::Result<()> {
        self.terminal_session.suspend()?;
        let status = std::process::Command::new("kill")
            .args(["-TSTP", &std::process::id().to_string()])
            .status()?;
        self.terminal_session.resume()?;
        if status.success() { Ok(()) } else { Err(io::Error::other("failed to suspend process")) }
    }

    fn handle_navigation(&mut self, app: &mut App, action: &Action) -> bool {
        if matches!(action, Action::CopyTranscriptSelection) {
            let result = self
                .viewport
                .selected_text()
                .ok_or_else(|| io::Error::other("no transcript text is selected"))
                .and_then(|text| {
                    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
                        return Err(io::Error::other("terminal clipboard is unavailable"));
                    }
                    let encoded = encode_base64(text.as_bytes());
                    Backend::flush(self.terminal.backend_mut())?;
                    let mut stdout = io::stdout();
                    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
                    stdout.flush()
                });
            match result {
                Ok(()) => app.show_status_toast("Copied transcript selection", app::StatusToastKind::Success),
                Err(error) => app.show_status_toast(
                    format!("Could not copy transcript selection: {error}"),
                    app::StatusToastKind::Error,
                ),
            }
            return true;
        }
        self.viewport.handle_navigation(app, action)
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        encoded.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 { ALPHABET[((value >> 6) & 63) as usize] as char } else { '=' });
        encoded.push(if chunk.len() > 2 { ALPHABET[(value & 63) as usize] as char } else { '=' });
    }
    encoded
}
