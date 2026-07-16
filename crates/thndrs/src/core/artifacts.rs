//! Bounded, redacted local artifacts for recoverable tool evidence.
//!
//! The artifact store is an application boundary: it owns filesystem effects,
//! redaction, retention, and recovery diagnostics. Handles and metadata are
//! safe to place in provider-neutral tool and context contracts; artifact
//! bodies are never stored in those contracts.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::tools::shell::redact_secrets;
use crate::utils::datetime;

/// Default maximum number of bytes retained for one artifact body.
pub const DEFAULT_MAX_ARTIFACT_BYTES: usize = 64 * 1024;

/// Default artifact retention period.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

const ARTIFACT_SCHEMA_VERSION: u32 = 1;
const TRUNCATION_MARKER: &str = "...[truncated]";

/// Coarse classification of an artifact body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Bounded output from an application-owned tool execution.
    ToolEvidence,
}

/// Retention state recorded with artifact metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRetentionState {
    /// Body is present and within its retention period.
    Active,
    /// Body is no longer eligible for recovery.
    Expired,
    /// Metadata exists but the body is missing or failed integrity checks.
    Corrupt,
}

/// Durable metadata for one bounded redacted artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    /// Schema version for this metadata file.
    pub schema_version: u32,
    /// Application-defined evidence identity.
    pub identity: String,
    /// Artifact classification.
    pub kind: ArtifactKind,
    /// Stable opaque handle used by tool results and context items.
    pub handle: String,
    /// SHA-256 hash of the bounded redacted body.
    pub content_hash: String,
    /// Bytes supplied before redaction and bounding.
    pub original_byte_count: usize,
    /// Bytes persisted after redaction and bounding.
    pub bounded_byte_count: usize,
    /// Whether the byte cap changed the body.
    pub truncated: bool,
    /// Whether redaction changed one or more source bytes.
    pub redacted: bool,
    /// ISO-8601 UTC creation time.
    pub created_at: String,
    /// Unix creation time used for deterministic expiry checks.
    pub created_at_unix: u64,
    /// ISO-8601 UTC expiry time, when retention is enabled.
    pub expires_at: Option<String>,
    /// Unix expiry time used for recovery checks.
    pub expires_at_unix: Option<u64>,
    /// Current retention state as last evaluated by the store.
    pub retention: ArtifactRetentionState,
}

/// A successful artifact write, including the safe projection used by the UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactWrite {
    /// Persisted metadata.
    pub metadata: ArtifactMetadata,
    /// Bounded redacted lines suitable for display or compatibility records.
    pub bounded_lines: Vec<String>,
}

/// A recovery result that keeps metadata usable when the body is unavailable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRecovery {
    /// Metadata recovered from the artifact sidecar.
    pub metadata: ArtifactMetadata,
    /// Bounded redacted body, when it is present and valid.
    pub content: Option<String>,
    /// Explicit diagnostic for missing, expired, or corrupt evidence.
    pub diagnostic: Option<ArtifactDiagnostic>,
}

/// Safe diagnostic returned when an artifact cannot be recovered.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactDiagnostic {
    /// Stable machine-readable diagnostic code.
    pub code: String,
    /// Secret-free human-readable explanation.
    pub message: String,
}

/// Application-owned bounded artifact store.
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    root: PathBuf,
    max_bytes: usize,
    retention: Option<Duration>,
}

impl ArtifactStore {
    /// Create a store using the default body cap and retention period.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_limits(root, DEFAULT_MAX_ARTIFACT_BYTES, Some(DEFAULT_RETENTION))
    }

    /// Create a store with explicit body and retention limits.
    ///
    /// `retention = None` is an explicit application choice for no expiry; it
    /// is not used by thndrs' default construction.
    pub fn with_limits(root: impl Into<PathBuf>, max_bytes: usize, retention: Option<Duration>) -> Self {
        Self { root: root.into(), max_bytes, retention }
    }

    /// Persist one tool result after redaction and byte bounding.
    pub fn create_tool_evidence(&self, identity: &str, lines: &[String]) -> std::io::Result<ArtifactWrite> {
        self.create(identity, ArtifactKind::ToolEvidence, &lines.join("\n"))
    }

    /// Persist one body after redaction and byte bounding.
    pub fn create(&self, identity: &str, kind: ArtifactKind, original: &str) -> std::io::Result<ArtifactWrite> {
        let redacted = redact_artifact_content(original);
        let (bounded, capped) = cap_bytes(&redacted, self.max_bytes);
        let content_hash = sha256_hex(bounded.as_bytes());
        let handle = format!(
            "artifact_v{ARTIFACT_SCHEMA_VERSION}_{}",
            stable_handle_hash(identity, &content_hash)
        );
        let created_at_unix = unix_now();
        let expires_at_unix = self
            .retention
            .map(|duration| created_at_unix.saturating_add(duration.as_secs()));
        let expires_at = expires_at_unix.map(datetime::from_unix_seconds);
        let metadata = ArtifactMetadata {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            identity: redact_secrets(identity),
            kind,
            handle: handle.clone(),
            content_hash,
            original_byte_count: original.len(),
            bounded_byte_count: bounded.len(),
            truncated: capped,
            redacted: redacted != original,
            created_at: datetime::from_unix_seconds(created_at_unix),
            created_at_unix,
            expires_at,
            expires_at_unix,
            retention: ArtifactRetentionState::Active,
        };

        fs::create_dir_all(&self.root)?;
        write_atomic(&self.body_path(&handle), bounded.as_bytes())?;
        let metadata_json = serde_json::to_vec(&metadata)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        write_atomic(&self.metadata_path(&handle), &metadata_json)?;

        Ok(ArtifactWrite { metadata, bounded_lines: split_lines(&bounded) })
    }

    /// Read artifact metadata without attempting body recovery.
    pub fn metadata(&self, handle: &str) -> std::io::Result<ArtifactMetadata> {
        let path = self.metadata_path(handle);
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    /// Recover a bounded redacted body and preserve metadata on failure.
    pub fn recover(&self, handle: &str) -> std::io::Result<ArtifactRecovery> {
        let mut metadata = self.metadata(handle)?;
        if Self::is_expired(&metadata) {
            metadata.retention = ArtifactRetentionState::Expired;
            return Ok(ArtifactRecovery {
                metadata,
                content: None,
                diagnostic: Some(ArtifactDiagnostic {
                    code: "artifact_expired".to_string(),
                    message: format!("artifact `{handle}` is outside its retention period"),
                }),
            });
        }

        let body = match fs::read(self.body_path(handle)) {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                metadata.retention = ArtifactRetentionState::Corrupt;
                return Ok(ArtifactRecovery {
                    metadata,
                    content: None,
                    diagnostic: Some(ArtifactDiagnostic {
                        code: "artifact_missing".to_string(),
                        message: format!("artifact `{handle}` body is unavailable"),
                    }),
                });
            }
            Err(error) => return Err(error),
        };
        let hash = sha256_hex(&body);
        if hash != metadata.content_hash || body.len() != metadata.bounded_byte_count {
            metadata.retention = ArtifactRetentionState::Corrupt;
            return Ok(ArtifactRecovery {
                metadata,
                content: None,
                diagnostic: Some(ArtifactDiagnostic {
                    code: "artifact_corrupt".to_string(),
                    message: format!("artifact `{handle}` failed integrity verification"),
                }),
            });
        }

        let content = String::from_utf8(body)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "artifact body is not valid UTF-8"))?;
        Ok(ArtifactRecovery { metadata, content: Some(content), diagnostic: None })
    }

    /// Return the artifact body path for controlled corruption/retention tests.
    pub fn body_path_for_test(&self, handle: &str) -> PathBuf {
        self.body_path(handle)
    }

    fn is_expired(metadata: &ArtifactMetadata) -> bool {
        metadata
            .expires_at_unix
            .is_some_and(|expires_at| unix_now() >= expires_at)
    }

    fn body_path(&self, handle: &str) -> PathBuf {
        self.root.join(format!("{handle}.body"))
    }

    fn metadata_path(&self, handle: &str) -> PathBuf {
        self.root.join(format!("{handle}.json"))
    }
}

/// Redact and cap tool lines before placing them in a compatibility/session
/// projection. This function never returns the original body by default.
pub fn bounded_redacted_lines(lines: &[String], max_bytes: usize) -> Vec<String> {
    let original = lines.join("\n");
    let redacted = redact_artifact_content(&original);
    let (bounded, _) = cap_bytes(&redacted, max_bytes);
    split_lines(&bounded)
}

fn redact_artifact_content(value: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(json) => redact_json_value(json).to_string(),
        Err(_) => {
            let redacted = redact_secrets(value);
            let token_re = regex_lite::Regex::new(r"(?i)(token|auth_token)\s*[:=]\s*\S{4,}")
                .expect("valid artifact redaction regex");
            token_re.replace_all(&redacted, "$1=[REDACTED]").to_string()
        }
    }
}

fn redact_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(redact_json_value).collect())
        }
        serde_json::Value::Object(entries) => serde_json::Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let value = if is_sensitive_key(&key) {
                        serde_json::Value::String("[REDACTED]".to_string())
                    } else {
                        redact_json_value(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        serde_json::Value::String(text) => serde_json::Value::String(redact_secrets(&text)),
        value => value,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "password" | "passwd" | "api_key" | "apikey" | "access_token" | "secret" | "token"
    )
}

fn cap_bytes(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    if max_bytes <= TRUNCATION_MARKER.len() {
        let mut end = max_bytes;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        return (value[..end].to_string(), true);
    }
    let mut end = max_bytes - TRUNCATION_MARKER.len();
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = value[..end].to_string();
    bounded.push_str(TRUNCATION_MARKER);
    (bounded, true)
}

fn split_lines(value: &str) -> Vec<String> {
    if value.is_empty() { Vec::new() } else { value.split('\n').map(str::to_string).collect() }
}

fn stable_handle_hash(identity: &str, content_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(identity.as_bytes());
    hasher.update([0]);
    hasher.update(content_hash.as_bytes());
    hex_digest(hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_bounded_redacted_artifact_with_stable_metadata() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ArtifactStore::with_limits(root.path(), 32, None);
        let write = store
            .create_tool_evidence(
                "tool:call-1",
                &[
                    "api_key=sk-supersecretvalue".to_string(),
                    "abcdefghijklmnopqrstuvwxyz".to_string(),
                ],
            )
            .expect("artifact");

        assert!(write.metadata.handle.starts_with("artifact_v1_"));
        assert_eq!(write.metadata.bounded_byte_count, 32);
        assert!(write.metadata.truncated);
        assert!(!write.bounded_lines.join("\n").contains("supersecretvalue"));
        let recovered = store.recover(&write.metadata.handle).expect("recover");
        assert_eq!(recovered.diagnostic, None);
        let expected = write.bounded_lines.join("\n");
        assert_eq!(recovered.content.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn missing_and_corrupt_bodies_keep_metadata_and_return_diagnostics() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ArtifactStore::new(root.path());
        let write = store
            .create("identity", ArtifactKind::ToolEvidence, "safe")
            .expect("artifact");
        fs::remove_file(store.body_path_for_test(&write.metadata.handle)).expect("remove body");
        let recovery = store.recover(&write.metadata.handle).expect("diagnostic");
        assert_eq!(recovery.metadata.handle, write.metadata.handle);
        assert_eq!(
            recovery.diagnostic.as_ref().map(|d| d.code.as_str()),
            Some("artifact_missing")
        );
    }

    #[test]
    fn json_secrets_are_redacted_before_the_size_cap() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ArtifactStore::with_limits(root.path(), 256, None);
        let write = store
            .create(
                "identity",
                ArtifactKind::ToolEvidence,
                r#"{"token":"do-not-store","message":"ok"}"#,
            )
            .expect("artifact");
        let body = write.bounded_lines.join("\n");
        assert!(!body.contains("do-not-store"));
        assert!(body.contains("[REDACTED]"));
    }

    #[test]
    fn expiry_is_explicit_and_metadata_survives_recovery_failure() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ArtifactStore::with_limits(root.path(), 128, Some(Duration::ZERO));
        let write = store
            .create("identity", ArtifactKind::ToolEvidence, "safe")
            .expect("artifact");
        let recovery = store.recover(&write.metadata.handle).expect("expired diagnostic");
        assert_eq!(recovery.metadata.handle, write.metadata.handle);
        assert_eq!(recovery.metadata.retention, ArtifactRetentionState::Expired);
        assert_eq!(
            recovery.diagnostic.as_ref().map(|d| d.code.as_str()),
            Some("artifact_expired")
        );
    }
}
