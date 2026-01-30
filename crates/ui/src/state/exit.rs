/// Exit detection state
#[derive(Debug, Clone, Default)]
pub struct ExitState {
    /// Consecutive CTRL+C press count for exit detection
    ctrl_c_press_count: u8,
    /// Last CTRL+C press timestamp (for reset)
    last_ctrl_c_time: Option<std::time::Instant>,
}

impl ExitState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a CTRL+C press and return whether it should trigger exit
    pub fn record_ctrl_c_press(&mut self) -> bool {
        let now = std::time::Instant::now();
        const RESET_DURATION_MS: u64 = 2000;

        if let Some(last_time) = self.last_ctrl_c_time
            && now.duration_since(last_time).as_millis() > RESET_DURATION_MS as u128
        {
            self.ctrl_c_press_count = 0;
        }

        self.ctrl_c_press_count += 1;
        self.last_ctrl_c_time = Some(now);

        self.ctrl_c_press_count >= 2
    }

    /// Reset CTRL+C press count
    pub fn reset_ctrl_c_count(&mut self) {
        self.ctrl_c_press_count = 0;
        self.last_ctrl_c_time = None;
    }
}
