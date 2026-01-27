use crate::Provider;
use async_trait::async_trait;
use std::time::Duration;
use thunderus_core::Result;

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

impl HealthCheckResult {
    pub fn healthy(latency_ms: u64) -> Self {
        Self { healthy: true, latency_ms, error: None }
    }

    pub fn unhealthy(error: String) -> Self {
        Self { healthy: false, latency_ms: 0, error: Some(error) }
    }
}

/// Health check trait for providers
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check_health(&self) -> Result<HealthCheckResult>;
}

/// Health checker for any provider
pub struct ProviderHealthChecker {
    provider: std::sync::Arc<dyn Provider>,
    timeout: Duration,
}

impl ProviderHealthChecker {
    pub fn new(provider: std::sync::Arc<dyn Provider>, timeout: Duration) -> Self {
        Self { provider, timeout }
    }

    /// Perform a quick health check using a minimal request
    pub async fn check(&self) -> Result<HealthCheckResult> {
        let start = std::time::Instant::now();

        let check = tokio::time::timeout(self.timeout, async {
            use crate::ChatMessage;
            use crate::ChatRequest;

            let request = ChatRequest::builder()
                .add_message(ChatMessage::user("health check"))
                .max_tokens(10)
                .build();

            let cancel_token = crate::CancelToken::new();
            let mut stream = self.provider.stream_chat(request, cancel_token).await?;

            let mut token_count = 0;
            let mut done = false;

            while !done {
                match tokio::time::timeout(Duration::from_secs(5), tokio_stream::StreamExt::next(&mut stream)).await {
                    Ok(Some(event)) => match event {
                        crate::StreamEvent::Token(_) => {
                            token_count += 1;
                            if token_count >= 1 {
                                done = true;
                            }
                        }
                        crate::StreamEvent::Done => {
                            done = true;
                        }
                        crate::StreamEvent::Error(_) => {
                            done = true;
                        }
                        _ => {}
                    },
                    Ok(None) => {
                        done = true;
                    }
                    Err(_) => {
                        return Err(thunderus_core::Error::Provider("Health check timeout".to_string()));
                    }
                }
            }

            Ok(())
        });

        let latency = start.elapsed().as_millis() as u64;

        match check.await {
            Ok(_) => Ok(HealthCheckResult::healthy(latency)),
            Err(e) => {
                let error_msg = format!("Health check failed: {}", e);
                Ok(HealthCheckResult::unhealthy(error_msg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_result_healthy() {
        let result = HealthCheckResult::healthy(100);
        assert!(result.healthy);
        assert_eq!(result.latency_ms, 100);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_health_check_result_unhealthy() {
        let result = HealthCheckResult::unhealthy("Connection failed".to_string());
        assert!(!result.healthy);
        assert_eq!(result.error, Some("Connection failed".to_string()));
    }
}
