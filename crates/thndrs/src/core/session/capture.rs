//! Durable, fail-closed policy for optional context content capture.

use std::io;

use serde::{Deserialize, Serialize};
use thndrs_agent::{MODEL_PROJECTION_MAX_BYTES, ModelProjectionMessage, ProviderRequestAccounting};

use crate::artifacts::{DEFAULT_MAX_ARTIFACT_BYTES, redact_artifact_content};

/// Version of the reviewed capture, redaction, and lifecycle rules.
pub const CONTEXT_CAPTURE_POLICY_VERSION: &str = "context-capture-v1";
/// Maximum number of normalized request bytes persisted for one attempt.
pub const MAX_CAPTURED_REQUEST_BYTES: usize = MODEL_PROJECTION_MAX_BYTES;
/// Number of days captured artifacts remain eligible for local recovery.
pub const CAPTURE_RETENTION_DAYS: u32 = 30;

/// Whether content may be retained for this session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextCaptureMode {
    /// Persist accounting and transformations, but no request or artifact bodies.
    #[default]
    MetadataOnly,
    /// Persist sanitized, bounded provider-neutral request and artifact content.
    RetainedContent,
}

/// Durable rules used by every context inspection and export path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextCapturePolicy {
    pub version: String,
    pub mode: ContextCaptureMode,
    pub access: String,
    pub redaction: String,
    pub retention_days: u32,
    pub deletion_scope: String,
    pub max_request_bytes: usize,
    pub max_artifact_bytes: usize,
}

impl Default for ContextCapturePolicy {
    fn default() -> Self {
        Self::metadata_only()
    }
}

impl ContextCapturePolicy {
    /// Build the default policy, which never persists content.
    pub fn metadata_only() -> Self {
        Self::new(ContextCaptureMode::MetadataOnly)
    }

    /// Build the explicit per-run content opt-in policy.
    pub fn retained_content() -> Self {
        Self::new(ContextCaptureMode::RetainedContent)
    }

    fn new(mode: ContextCaptureMode) -> Self {
        Self {
            version: CONTEXT_CAPTURE_POLICY_VERSION.to_string(),
            mode,
            access: "local_session_owner".to_string(),
            redaction: "thndrs_secret_redaction_v1".to_string(),
            retention_days: CAPTURE_RETENTION_DAYS,
            deletion_scope: "session_and_owned_artifacts".to_string(),
            max_request_bytes: MAX_CAPTURED_REQUEST_BYTES,
            max_artifact_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
        }
    }

    /// Whether sanitized request and artifact content may be retained or exported.
    pub fn permits_content(&self) -> bool {
        self.mode == ContextCaptureMode::RetainedContent
            && self.version == CONTEXT_CAPTURE_POLICY_VERSION
            && self.max_request_bytes <= MAX_CAPTURED_REQUEST_BYTES
            && self.max_artifact_bytes <= DEFAULT_MAX_ARTIFACT_BYTES
    }

    /// Sanitize a provider-neutral request projection, rejecting incomplete or oversized input.
    pub fn capture_request(
        &self, accounting: &ProviderRequestAccounting,
    ) -> io::Result<Option<CapturedRequestContent>> {
        if !self.permits_content() {
            return Ok(None);
        }
        if accounting.model_projection_truncated {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "normalized request exceeded the capture limit",
            ));
        }
        let messages = accounting
            .model_projection
            .iter()
            .map(|message| ModelProjectionMessage {
                role: redact_artifact_content(&message.role),
                content: redact_artifact_content(&message.content),
            })
            .collect::<Vec<_>>();
        let capture = CapturedRequestContent {
            request_id: accounting.request_id.clone(),
            turn_id: accounting.turn_id.clone(),
            attempt: accounting.attempt,
            messages,
        };
        let bytes = serde_json::to_vec(&capture).map_err(io::Error::other)?.len();
        if bytes > self.max_request_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sanitized request exceeded the capture limit",
            ));
        }
        Ok(Some(capture))
    }
}

/// Sanitized provider-neutral request content for one attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapturedRequestContent {
    pub request_id: String,
    pub turn_id: String,
    pub attempt: u32,
    pub messages: Vec<ModelProjectionMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounting(content: &str) -> ProviderRequestAccounting {
        ProviderRequestAccounting::from_serialized_request("turn", "request", 1, "provider", "model", b"{}", vec![])
            .with_model_projection(vec![ModelProjectionMessage {
                role: "user".to_string(),
                content: content.to_string(),
            }])
    }

    #[test]
    fn metadata_only_never_captures_content() {
        assert_eq!(
            ContextCapturePolicy::metadata_only()
                .capture_request(&accounting("hello"))
                .unwrap(),
            None
        );
    }

    #[test]
    fn retained_capture_is_sanitized() {
        let capture = ContextCapturePolicy::retained_content()
            .capture_request(&accounting("Authorization: Bearer secret-value"))
            .unwrap()
            .unwrap();
        assert!(!capture.messages[0].content.contains("secret-value"));
    }

    #[test]
    fn truncated_projection_fails_closed() {
        let oversized = "x".repeat(MAX_CAPTURED_REQUEST_BYTES + 1);
        let error = ContextCapturePolicy::retained_content()
            .capture_request(&accounting(&oversized))
            .expect_err("truncated request must not be captured");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
