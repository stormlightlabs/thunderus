use std::time::Duration;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self { max_attempts: 3, initial_delay_ms: 1000, max_delay_ms: 30000, backoff_multiplier: 2.0 }
    }
}

impl RetryConfig {
    /// Create from provider options
    pub fn from_options(retry_count: u32, retry_delay_ms: u64, timeout_ms: u64) -> Self {
        Self {
            max_attempts: retry_count.max(1),
            initial_delay_ms: retry_delay_ms,
            max_delay_ms: timeout_ms.saturating_sub(5000),
            backoff_multiplier: 2.0,
        }
    }

    /// Calculate delay for the given attempt (0-indexed)
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = if attempt == 0 {
            0
        } else {
            let delay = self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32 - 1);
            delay.min(self.max_delay_ms as f64) as u64
        };

        Duration::from_millis(delay_ms)
    }

    /// Check if we should retry given the attempt number
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }
}

/// Check if an error is retryable
pub fn is_retryable_error(error: &thunderus_core::Error) -> bool {
    match error {
        thunderus_core::Error::Provider(msg) => {
            let msg_lower = msg.to_lowercase();
            msg_lower.contains("timeout")
                || msg_lower.contains("network")
                || msg_lower.contains("connection")
                || msg_lower.contains("429")
                || msg_lower.contains("rate limit")
                || msg_lower.contains("temporary")
        }
        thunderus_core::Error::Io(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 1000);
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = RetryConfig::default();
        let delay0 = config.delay_for_attempt(0);
        let delay1 = config.delay_for_attempt(1);
        let delay2 = config.delay_for_attempt(2);

        assert_eq!(delay0.as_millis(), 0);
        assert_eq!(delay1.as_millis(), 1000);
        assert_eq!(delay2.as_millis(), 2000);
    }

    #[test]
    fn test_retry_delay_with_backoff() {
        let config = RetryConfig { initial_delay_ms: 500, backoff_multiplier: 3.0, ..Default::default() };

        assert_eq!(config.delay_for_attempt(1).as_millis(), 500);
        assert_eq!(config.delay_for_attempt(2).as_millis(), 1500);
        assert_eq!(config.delay_for_attempt(3).as_millis(), 4500);
    }

    #[test]
    fn test_retry_delay_with_max() {
        let config =
            RetryConfig { initial_delay_ms: 1000, backoff_multiplier: 10.0, max_delay_ms: 5000, ..Default::default() };

        assert_eq!(config.delay_for_attempt(1).as_millis(), 1000);
        assert_eq!(config.delay_for_attempt(2).as_millis(), 5000);
        assert_eq!(config.delay_for_attempt(3).as_millis(), 5000);
    }

    #[test]
    fn test_should_retry() {
        let config = RetryConfig { max_attempts: 3, ..Default::default() };

        assert!(config.should_retry(0));
        assert!(config.should_retry(1));
        assert!(config.should_retry(2));
        assert!(!config.should_retry(3));
        assert!(!config.should_retry(4));
    }

    #[test]
    fn test_is_retryable_error() {
        let network_err = thunderus_core::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "connection failed",
        ));
        assert!(is_retryable_error(&network_err));

        let timeout_err = thunderus_core::Error::Provider("Request timeout".to_string());
        assert!(is_retryable_error(&timeout_err));

        let rate_limit_err = thunderus_core::Error::Provider("Rate limit exceeded (429)".to_string());
        assert!(is_retryable_error(&rate_limit_err));

        let auth_err = thunderus_core::Error::Config("Invalid API key".to_string());
        assert!(!is_retryable_error(&auth_err));
    }
}
