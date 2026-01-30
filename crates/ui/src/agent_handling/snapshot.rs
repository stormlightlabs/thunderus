use crate::app::App;

impl App {
    /// Capture current workspace snapshot state
    ///
    /// Records the current git state for drift comparison later.
    /// Should be called before agent operations to establish a baseline.
    pub fn capture_snapshot_state(&mut self) {
        if let Some(ref sm) = self.snapshot_manager {
            match sm.get_current_state() {
                Ok(state) => {
                    self.last_snapshot_state = Some(state);
                }
                Err(e) => {
                    eprintln!("Failed to capture snapshot state: {}", e);
                }
            }
        }
    }
}
