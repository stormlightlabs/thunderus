#![allow(dead_code)]

//! Credential storage for provider API keys.
//!
//! Credentials are stored in simple `.env`-format files, **not** in TOML config.
//! This keeps secrets out of version control, logs, sessions, and diagnostics.
//!
//! ## Storage paths
//!
//! - Global: `~/.thndrs/credentials.env`
//! - Project: `<workspace>/.thndrs/credentials.env`
//!
//! ## Precedence
//!
//! Provider key resolution follows this order (first wins):
//! 1. Process environment variables
//! 2. Global credential store (`~/.thndrs/credentials.env`)
//! 3. Project credential store (`.thndrs/credentials.env`)
//! 4. Workspace `.env` file (legacy compatibility)

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::utils;

/// Environment variable name for the Umans API key.
pub const UMANS_API_KEY_ENV: &str = "UMANS_API_KEY";

/// Environment variable name for the OpenCode Go API key.
pub const OPENCODE_GO_KEY_ENV: &str = "OPENCODE_GO_KEY";

/// Known provider API key variable names.
pub const KNOWN_API_KEY_VARS: &[&str] = &[UMANS_API_KEY_ENV, OPENCODE_GO_KEY_ENV];

/// Describes where a credential value was found, without leaking the value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    /// Found in a process environment variable.
    Environment,
    /// Found in the global credential store at `~/.thndrs/credentials.env`.
    GlobalStore,
    /// Found in the project credential store at `<workspace>/.thndrs/credentials.env`.
    ProjectStore,
    /// Found in the workspace `.env` file (legacy fallback).
    DotEnvLegacy,
}

impl CredentialSource {
    /// Human-readable label for the source.
    pub fn label(self) -> &'static str {
        match self {
            CredentialSource::Environment => "environment",
            CredentialSource::GlobalStore => "global credentials",
            CredentialSource::ProjectStore => "project credentials",
            CredentialSource::DotEnvLegacy => ".env",
        }
    }
}

/// Errors produced by credential store operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Failed to read a credential file.
    #[error("failed to read credentials {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Failed to write a credential file.
    #[error("failed to write credentials {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Malformed line in a credential file.
    #[error("malformed credential file {path}: {message}")]
    Malformed { path: PathBuf, message: String },
    /// Home directory is not available.
    #[error("home directory not found; set $HOME or $USERPROFILE")]
    NoHomeDirectory,
    /// Failed to update `.git/info/exclude`.
    #[error("git exclude failed: {0}")]
    GitExclude(String),
}

/// Global credential store path: `~/.thndrs/credentials.env`.
pub fn global_credentials_path() -> Result<PathBuf, AuthError> {
    let home = utils::home_dir().ok_or(AuthError::NoHomeDirectory)?;
    Ok(home.join(".thndrs").join("credentials.env"))
}

/// Project credential store path: `<workspace>/.thndrs/credentials.env`.
pub fn project_credentials_path(workspace: &Path) -> PathBuf {
    workspace.join(".thndrs").join("credentials.env")
}

/// Read all credential key-value pairs from a `.env`-format file.
///
/// Blank lines, comment lines (starting with `#`), and `export`-prefixed lines
/// are skipped. The first assignment for each key wins (duplicate keys are
/// ignored).
pub fn read_credentials(path: &Path) -> Result<BTreeMap<String, String>, AuthError> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let file = fs::File::open(path).map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
    let reader = std::io::BufReader::new(file);
    let mut credentials = BTreeMap::new();

    for (i, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
        if let Some((key, value)) = parse_credential_line(&line) {
            credentials.entry(key).or_insert(value);
        } else if !line.trim().is_empty() && !line.trim().starts_with('#') {
            return Err(AuthError::Malformed {
                path: path.to_path_buf(),
                message: format!("line {}: invalid env format", i + 1),
            });
        }
    }

    Ok(credentials)
}

/// Parse a single `.env`-format assignment line.
///
/// Supports `KEY=value`, `export KEY=value`, and quoted values
/// (`KEY="value"`, `KEY='value'`).
fn parse_credential_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, raw_value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let raw_value = raw_value.trim();
    let value = raw_value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| raw_value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(raw_value);
    if value.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

/// Write a single credential key-value pair to the credential file,
/// preserving all unrelated entries.
///
/// If the file does not exist, it is created. If the key already exists in the
/// file, its line is replaced. Otherwise the new assignment is appended.
///
/// The write is atomic: content is written to a temporary file in the same
/// directory, then renamed over the target path.
///
/// On Unix, the file is created with mode `0600`.
pub fn set_credential(path: &Path, key: &str, value: &str) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    }

    let existing = if path.is_file() { read_lines(path)? } else { Vec::new() };

    let new_line = format!("{key}={value}");

    let mut replaced = false;
    let mut lines = existing;

    for line in &mut lines {
        if let Some((line_key, _)) = parse_credential_line(line) {
            if line_key == key && !replaced {
                *line = new_line.clone();
                replaced = true;
            }
        }
    }

    if !replaced {
        lines.push(new_line);
    }

    write_lines_atomic(path, &lines)
}

/// Remove a single credential key from the credential file, preserving all
/// other entries.
///
/// If the key appears multiple times, all matching lines are removed.
pub fn remove_credential(path: &Path, key: &str) -> Result<(), AuthError> {
    if !path.is_file() {
        return Ok(());
    }

    let lines = read_lines(path)?;
    let filtered: Vec<String> = lines
        .into_iter()
        .filter(
            |line| {
                if let Some((line_key, _)) = parse_credential_line(line) { line_key != key } else { true }
            },
        )
        .collect();

    write_lines_atomic(path, &filtered)
}

/// Read all lines from a text file.
fn read_lines(path: &Path) -> Result<Vec<String>, AuthError> {
    let file = fs::File::open(path).map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
    let reader = std::io::BufReader::new(file);
    reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| AuthError::Read { path: path.to_path_buf(), source })
}

/// Write lines to a temporary file in the same directory, then atomically
/// rename over the target path.
///
/// On Unix, sets file mode `0600`.
fn write_lines_atomic(path: &Path, lines: &[String]) -> Result<(), AuthError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;

    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;

    for line in lines {
        writeln!(tmp, "{line}").map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    }
    tmp.flush()
        .map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;

    set_unix_permissions(tmp.path());

    tmp.persist(path)
        .map(|_| ())
        .map_err(|e| AuthError::Write { path: path.to_path_buf(), source: e.error })
}

/// Redact a credential value for display/debug output.
///
/// This returns a fixed sentinel string so callers cannot accidentally leak
/// value prefixes, hashes, suffixes, or lengths.
pub fn redact_value(_value: &str) -> String {
    String::from("[redacted]")
}

/// Resolve a credential by checking all sources in precedence order.
///
/// Order: process environment → global credential store
/// (`~/.thndrs/credentials.env`) → project credential store
/// (`<workspace>/.thndrs/credentials.env`) → workspace `.env` (legacy).
///
/// Returns `None` when the key is not found in any source.
pub fn resolve_credential(key: &str, workspace: &Path) -> Option<(String, CredentialSource)> {
    if let Ok(value) = std::env::var(key) {
        if !value.is_empty() {
            return Some((value, CredentialSource::Environment));
        }
    }

    if let Ok(global_path) = global_credentials_path() {
        if let Ok(creds) = read_credentials(&global_path) {
            if let Some(value) = creds.get(key) {
                return Some((value.clone(), CredentialSource::GlobalStore));
            }
        }
    }

    let project_path = project_credentials_path(workspace);
    if let Ok(creds) = read_credentials(&project_path) {
        if let Some(value) = creds.get(key) {
            return Some((value.clone(), CredentialSource::ProjectStore));
        }
    }

    let dotenv_path = workspace.join(".env");
    if let Ok(creds) = read_credentials(&dotenv_path) {
        if let Some(value) = creds.get(key) {
            return Some((value.clone(), CredentialSource::DotEnvLegacy));
        }
    }

    None
}

/// Return the [`CredentialSource`] for a key without revealing its value.
pub fn credential_source(key: &str, workspace: &Path) -> Option<CredentialSource> {
    resolve_credential(key, workspace).map(|(_, source)| source)
}

/// Ensure `.thndrs/credentials.env` is listed in `.git/info/exclude` so it
/// cannot be accidentally committed.
///
/// If the workspace is not inside a git repository, this is a no-op.
/// Repeated calls are idempotent: the exclude entry is never duplicated.
pub fn ensure_git_exclude(workspace: &Path) -> Result<(), AuthError> {
    let git_dir = workspace.join(".git");
    let exclude_path = git_dir.join("info").join("exclude");
    if !exclude_path.is_file() {
        return Ok(());
    }

    let exclude_entry = ".thndrs/credentials.env";
    let content = fs::read_to_string(&exclude_path)
        .map_err(|source| AuthError::GitExclude(format!("failed to read {}: {source}", exclude_path.display())))?;

    let already_excluded = content.lines().any(|line| line.trim() == exclude_entry);
    if already_excluded {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&exclude_path)
        .map_err(|source| AuthError::GitExclude(format!("failed to open {}: {source}", exclude_path.display())))?;

    writeln!(file, "{exclude_entry}")
        .map_err(|source| AuthError::GitExclude(format!("failed to write {}: {source}", exclude_path.display())))?;

    Ok(())
}

/// Set Unix file mode `0600` (owner read/write only) when supported.
#[cfg(unix)]
fn set_unix_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_unix_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cred_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("credentials.env");
        (dir, path)
    }

    fn write_cred_file(path: &Path, content: &str) {
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn parses_simple_assignment() {
        assert_eq!(
            parse_credential_line("UMANS_API_KEY=sk-abc123"),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_export_prefix() {
        assert_eq!(
            parse_credential_line("export UMANS_API_KEY=sk-abc123"),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_double_quoted_value() {
        assert_eq!(
            parse_credential_line(r#"UMANS_API_KEY="sk-abc123""#),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_single_quoted_value() {
        assert_eq!(
            parse_credential_line("UMANS_API_KEY='sk-abc123'"),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_export_quoted_value() {
        assert_eq!(
            parse_credential_line(r#"export UMANS_API_KEY="sk-abc123""#),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn skips_blank_lines() {
        assert_eq!(parse_credential_line(""), None);
        assert_eq!(parse_credential_line("   "), None);
    }

    #[test]
    fn skips_comment_lines() {
        assert_eq!(parse_credential_line("# this is a comment"), None);
        assert_eq!(parse_credential_line("  # indented comment"), None);
    }

    #[test]
    fn skips_empty_values() {
        assert_eq!(parse_credential_line("UMANS_API_KEY="), None);
        assert_eq!(parse_credential_line("UMANS_API_KEY=\"\""), None);
    }

    #[test]
    fn rejects_lines_without_equals() {
        assert_eq!(parse_credential_line("just a string"), None);
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(
            parse_credential_line("  UMANS_API_KEY = sk-abc123  "),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn value_can_contain_equals() {
        assert_eq!(
            parse_credential_line(r#"UMANS_API_KEY="sk-abc=xyz""#),
            Some(("UMANS_API_KEY".to_string(), "sk-abc=xyz".to_string()))
        );
    }

    #[test]
    fn reads_empty_file_as_empty_map() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "");
        let creds = read_credentials(&path).unwrap();
        assert!(creds.is_empty());
    }

    #[test]
    fn reads_multiple_credentials() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans-1\nOPENCODE_GO_KEY=sk-opencode-1\n");
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-umans-1");
        assert_eq!(creds.get("OPENCODE_GO_KEY").unwrap(), "sk-opencode-1");
        assert_eq!(creds.len(), 2);
    }

    #[test]
    fn first_key_wins_on_duplicate() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=first\nUMANS_API_KEY=second\n");
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "first");
    }

    #[test]
    fn reads_with_comments_and_blanks() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "# Umans key\nUMANS_API_KEY=sk-umans-1\n\n# OpenCode key\nOPENCODE_GO_KEY=sk-opencode-1\n",
        );
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.len(), 2);
    }

    #[test]
    fn missing_file_returns_empty() {
        let (_dir, path) = temp_cred_path();
        let creds = read_credentials(&path).unwrap();
        assert!(creds.is_empty());
    }

    #[test]
    fn malformed_file_is_rejected() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-valid\nthis is not valid\n");
        let err = read_credentials(&path).unwrap_err();
        assert!(matches!(err, AuthError::Malformed { .. }));
    }

    #[test]
    fn malformed_error_does_not_print_value() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "totally invalid line\n");
        let err = read_credentials(&path).unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("sk-"), "error should not contain secret-like values");
    }

    #[test]
    fn creates_new_file() {
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-fresh").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-fresh");
    }

    #[test]
    fn appends_new_key_to_existing_file() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans-1\n");
        set_credential(&path, "OPENCODE_GO_KEY", "sk-opencode-1").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-umans-1");
        assert_eq!(creds.get("OPENCODE_GO_KEY").unwrap(), "sk-opencode-1");
    }

    #[test]
    fn replaces_existing_key_in_place() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-old\n");
        set_credential(&path, "UMANS_API_KEY", "sk-new").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-new");
    }

    #[test]
    fn preserves_unrelated_entries() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "UMANS_API_KEY=sk-umans\nOTHER_VAR=keep-me\nOPENCODE_GO_KEY=sk-opencode\n",
        );
        set_credential(&path, "OPENCODE_GO_KEY", "sk-opencode-new").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("OTHER_VAR=keep-me"),
            "unrelated entries must be preserved"
        );
        assert!(
            content.contains("UMANS_API_KEY=sk-umans"),
            "other credentials must be preserved"
        );
    }

    #[test]
    fn preserves_comments_and_blanks() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "# Umans key\nUMANS_API_KEY=sk-umans\n\n# OpenCode key\nOPENCODE_GO_KEY=sk-opencode\n",
        );
        set_credential(&path, "OPENCODE_GO_KEY", "sk-opencode-new").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Umans key"));
        assert!(content.contains("# OpenCode key"));
    }

    #[test]
    fn set_is_idempotent() {
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-val").unwrap();
        set_credential(&path, "UMANS_API_KEY", "sk-val").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-val");
        assert_eq!(creds.len(), 1);
    }

    #[test]
    fn removes_existing_key() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans\nOPENCODE_GO_KEY=sk-opencode\n");
        remove_credential(&path, "UMANS_API_KEY").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert!(creds.get("UMANS_API_KEY").is_none());
        assert_eq!(creds.get("OPENCODE_GO_KEY").unwrap(), "sk-opencode");
    }

    #[test]
    fn removing_missing_key_is_noop() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans\n");
        remove_credential(&path, "OPENCODE_GO_KEY").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-umans");
    }

    #[test]
    fn removing_from_missing_file_is_noop() {
        let (_dir, path) = temp_cred_path();
        remove_credential(&path, "UMANS_API_KEY").unwrap();
    }

    #[test]
    fn remove_preserves_unrelated_entries() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "UMANS_API_KEY=sk-umans\nOTHER_VAR=keep-me\nOPENCODE_GO_KEY=sk-opencode\n",
        );
        remove_credential(&path, "OPENCODE_GO_KEY").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("OTHER_VAR=keep-me"));
        assert!(content.contains("UMANS_API_KEY=sk-umans"));
        assert!(!content.contains("OPENCODE_GO_KEY"));
    }

    #[test]
    fn redact_returns_fixed_string() {
        assert_eq!(redact_value(""), "[redacted]");
        assert_eq!(redact_value("sk-abc123"), "[redacted]");
        assert_eq!(redact_value("my-secret-key-12345"), "[redacted]");
    }

    #[test]
    fn global_path_uses_home_thndrs() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &home) };
        let path = global_credentials_path().unwrap();
        assert_eq!(path, home.join(".thndrs").join("credentials.env"));
        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn global_path_fails_without_home() {
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::remove_var("HOME") };
        let old_profile = std::env::var_os("USERPROFILE");
        unsafe { std::env::remove_var("USERPROFILE") };
        let err = global_credentials_path().unwrap_err();
        assert!(matches!(err, AuthError::NoHomeDirectory));
        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            }
            if let Some(p) = old_profile {
                std::env::set_var("USERPROFILE", p);
            }
        }
    }

    #[test]
    fn project_path_uses_workspace_thndrs() {
        let path = project_credentials_path(Path::new("/repo"));
        assert_eq!(path, PathBuf::from("/repo/.thndrs/credentials.env"));
    }

    #[test]
    fn git_exclude_creates_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git").join("info");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("exclude"), "# git exclude\n").unwrap();

        ensure_git_exclude(tmp.path()).unwrap();
        let content = fs::read_to_string(git_dir.join("exclude")).unwrap();
        assert!(content.contains(".thndrs/credentials.env"));
    }

    #[test]
    fn git_exclude_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git").join("info");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("exclude"), "# git exclude\n").unwrap();

        ensure_git_exclude(tmp.path()).unwrap();
        ensure_git_exclude(tmp.path()).unwrap();
        ensure_git_exclude(tmp.path()).unwrap();

        let content = fs::read_to_string(git_dir.join("exclude")).unwrap();
        let count = content
            .lines()
            .filter(|l| l.trim() == ".thndrs/credentials.env")
            .count();
        assert_eq!(count, 1, "entry should appear exactly once");
    }

    #[test]
    fn git_exclude_noop_without_git_dir() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_git_exclude(tmp.path()).unwrap();
    }

    #[test]
    fn git_exclude_preserves_existing_content() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git").join("info");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("exclude"), "*.log\n.env\n").unwrap();

        ensure_git_exclude(tmp.path()).unwrap();
        let content = fs::read_to_string(git_dir.join("exclude")).unwrap();
        assert!(content.contains("*.log"));
        assert!(content.contains(".env"));
        assert!(content.contains(".thndrs/credentials.env"));
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("dir").join("credentials.env");
        set_credential(&path, "UMANS_API_KEY", "sk-test").unwrap();
        assert!(path.is_file());
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-test");
    }

    #[test]
    fn read_write_round_trip() {
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-round-trip").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-round-trip");
    }

    #[test]
    fn source_labels_are_readable() {
        assert_eq!(CredentialSource::Environment.label(), "environment");
        assert_eq!(CredentialSource::GlobalStore.label(), "global credentials");
        assert_eq!(CredentialSource::ProjectStore.label(), "project credentials");
        assert_eq!(CredentialSource::DotEnvLegacy.label(), ".env");
    }

    #[test]
    fn debug_does_not_leak_values() {
        let label = format!("{:?}", CredentialSource::Environment);
        assert!(!label.contains("[redacted]"));
        assert!(label.contains("Environment"));
    }

    #[test]
    fn resolve_picks_env_var_first() {
        let key = "RESOLVE_TEST_ENV_FIRST";
        unsafe { std::env::set_var(key, "from-env") };
        let dir = tempfile::tempdir().unwrap();
        let (value, source) = resolve_credential(key, dir.path()).unwrap();
        assert_eq!(value, "from-env");
        assert_eq!(source, CredentialSource::Environment);
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn resolve_falls_through_to_global_store() {
        let key = "RESOLVE_TEST_GLOBAL";
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");

        let global_path = home.join(".thndrs").join("credentials.env");
        fs::create_dir_all(global_path.parent().unwrap()).unwrap();
        fs::write(&global_path, format!("{key}=from-global\n")).unwrap();

        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &home) };

        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-global");
        assert_eq!(source, CredentialSource::GlobalStore);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_falls_through_to_project_store() {
        let key = "RESOLVE_TEST_PROJECT";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        let project_path = workspace.join(".thndrs").join("credentials.env");
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(&project_path, format!("{key}=from-project\n")).unwrap();

        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-project");
        assert_eq!(source, CredentialSource::ProjectStore);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_falls_through_to_dotenv_legacy() {
        let key = "RESOLVE_TEST_DOTENV";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        fs::write(workspace.join(".env"), format!("{key}=from-dotenv\n")).unwrap();

        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-dotenv");
        assert_eq!(source, CredentialSource::DotEnvLegacy);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_returns_none_for_missing_key() {
        let key = "RESOLVE_TEST_NONEXISTENT";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        assert!(resolve_credential(key, &workspace).is_none());

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_precedence_env_overrides_all() {
        let key = "RESOLVE_TEST_PRECEDENCE";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        let global_path = test_home.join(".thndrs").join("credentials.env");
        fs::create_dir_all(global_path.parent().unwrap()).unwrap();
        fs::write(&global_path, format!("{key}=from-global\n")).unwrap();

        let project_path = workspace.join(".thndrs").join("credentials.env");
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(&project_path, format!("{key}=from-project\n")).unwrap();

        fs::write(workspace.join(".env"), format!("{key}=from-dotenv\n")).unwrap();

        unsafe { std::env::set_var(key, "from-env") };
        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-env");
        assert_eq!(source, CredentialSource::Environment);
        unsafe { std::env::remove_var(key) };

        unsafe { std::env::set_var("HOME", &test_home) };
        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-global");
        assert_eq!(source, CredentialSource::GlobalStore);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_empty_env_var_is_skipped() {
        let key = "RESOLVE_TEST_EMPTY_ENV";
        unsafe { std::env::set_var(key, "") };
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve_credential(key, dir.path()).is_none());
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn credential_source_returns_label_without_value() {
        let key = "RESOLVE_SRC_LABEL";
        unsafe { std::env::set_var(key, "some-value") };
        let dir = tempfile::tempdir().unwrap();
        let source = credential_source(key, dir.path());
        assert_eq!(source, Some(CredentialSource::Environment));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn credential_source_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(credential_source("THIS_KEY_DOES_NOT_EXIST", dir.path()), None);
    }

    #[test]
    fn known_api_key_vars_are_complete() {
        assert!(KNOWN_API_KEY_VARS.contains(&UMANS_API_KEY_ENV));
        assert!(KNOWN_API_KEY_VARS.contains(&OPENCODE_GO_KEY_ENV));
    }

    #[test]
    fn remove_one_preserves_others() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "UMANS_API_KEY=sk-umans\nOPENCODE_GO_KEY=sk-opencode\nOTHER_KEY=other-val\n",
        );
        remove_credential(&path, "UMANS_API_KEY").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.len(), 2);
        assert!(creds.contains_key("OPENCODE_GO_KEY"));
        assert!(creds.contains_key("OTHER_KEY"));
    }

    #[test]
    #[cfg(unix)]
    fn file_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-perm").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "file permissions should be 0600");
    }
}
