//! Append-only JSONL session persistence.
//!
//! Records prompt metadata and transcript entries for audit and resume without
//! storing full raw provider payloads by default. Raw payloads can contain
//! prompt text, repository content, and secrets — only structured metadata is
//! persisted.
//!
//! ## Record format
//!
//! Each session is a single append-only JSONL file. Every line is a
//! [`SessionRecord`] tagged with `schema_version`, `seq`, `time`, and `type`.
//! The `seq` field is a monotonic sequence number within the session. Records
//! are never rewritten — appends are the only mutation.
//!
//! ## Record types
//!
//! - `session_meta`: id, cwd, title, provider, model, websearch, app version.
//! - `context`: loaded AGENTS.md source metadata (path, scope, hash, truncation).
//! - `user`: prompt text and turn id.
//! - `assistant_finished`: final replayable assistant text.
//! - `reasoning_finished`: final replayable reasoning text.
//! - `tool_started`: tool call id, name, input.
//! - `tool_finished`: tool call id, status, output.
//! - `file_write`: file write audit (op, path, before/after hash+bytes, status).
//! - `cancelled`: turn id and reason.
//! - `failed`: turn id and error message.
//! - `session_renamed`: new title (latest wins).

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::{Entry, ToolStatus};
use crate::context::ContextSource;
use crate::datetime;
use crate::prompt::{EnvironmentMetadata, HistoryReuse, PromptBundle};
use crate::tools::WriteOp;

/// Current JSONL schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// A single line in a session JSONL file.
///
/// Every record carries `schema_version`, `seq` (monotonic within the session),
/// `time` (ISO 8601 UTC), and a `type` tag. Records are never rewritten;
/// appends are the only mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionRecord {
    /// First line of a session: identity and environment.
    #[serde(rename = "session_meta")]
    SessionMeta {
        schema_version: u32,
        seq: u64,
        time: String,
        session_id: String,
        cwd: String,
        title: String,
        provider: String,
        model: String,
        websearch: String,
        app_version: String,
    },
    /// Loaded context source metadata (AGENTS.md etc.).
    #[serde(rename = "context")]
    Context {
        schema_version: u32,
        seq: u64,
        time: String,
        sources: Vec<ContextSourceMeta>,
    },
    /// A user-submitted prompt.
    #[serde(rename = "user")]
    User {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        text: String,
    },
    /// Final replayable assistant text.
    #[serde(rename = "assistant_finished")]
    AssistantFinished {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        text: String,
    },
    /// Final replayable reasoning text.
    #[serde(rename = "reasoning_finished")]
    ReasoningFinished {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        text: String,
    },
    /// A tool call started.
    #[serde(rename = "tool_started")]
    ToolStarted {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    /// A tool call finished.
    #[serde(rename = "tool_finished")]
    ToolFinished {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        call_id: String,
        status: ToolStatus,
        output: Vec<String>,
    },
    /// The agent run was cancelled.
    #[serde(rename = "cancelled")]
    Cancelled {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        reason: String,
    },
    /// The agent run failed.
    #[serde(rename = "failed")]
    Failed {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        error: String,
    },
    /// Session title was renamed (latest wins).
    #[serde(rename = "session_renamed")]
    SessionRenamed {
        schema_version: u32,
        seq: u64,
        time: String,
        title: String,
    },
    /// A file write operation completed.
    ///
    /// Records the operation type, target path, and before/after metadata
    /// for session audit. Only hashes and byte counts are persisted but
    /// not file content.
    #[serde(rename = "file_write")]
    FileWrite {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        op: WriteOp,
        path: String,
        before_hash: Option<u64>,
        before_bytes: Option<usize>,
        after_hash: u64,
        after_bytes: usize,
        status: ToolStatus,
    },
}

impl SessionRecord {
    /// The sequence number of this record.
    pub fn seq(&self) -> u64 {
        match self {
            SessionRecord::SessionMeta { seq, .. }
            | SessionRecord::Context { seq, .. }
            | SessionRecord::User { seq, .. }
            | SessionRecord::AssistantFinished { seq, .. }
            | SessionRecord::ReasoningFinished { seq, .. }
            | SessionRecord::ToolStarted { seq, .. }
            | SessionRecord::ToolFinished { seq, .. }
            | SessionRecord::Cancelled { seq, .. }
            | SessionRecord::Failed { seq, .. }
            | SessionRecord::SessionRenamed { seq, .. }
            | SessionRecord::FileWrite { seq, .. } => *seq,
        }
    }

    /// Serialize to a JSON string for JSONL append.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Convert a transcript [`Entry`] into a [`SessionRecord`].
    ///
    /// Only finalized entries are converted — streaming entries (assistant/
    /// reasoning still streaming, tools still running) are skipped because
    /// they represent incomplete live state.
    pub fn from_entry(entry: &Entry, seq: u64, time: &str, turn_id: &str) -> Option<SessionRecord> {
        let sv = SCHEMA_VERSION;
        match entry {
            Entry::User { text } => Some(SessionRecord::User {
                schema_version: sv,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                text: text.clone(),
            }),
            Entry::Assistant { text, streaming: false } => Some(SessionRecord::AssistantFinished {
                schema_version: sv,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                text: text.clone(),
            }),
            Entry::Reasoning { text, streaming: false } => Some(SessionRecord::ReasoningFinished {
                schema_version: sv,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                text: text.clone(),
            }),
            Entry::Tool { name, arguments, status, output } if *status != ToolStatus::Running => {
                let (tool_name, call_id) = split_tool_name_id(name);
                Some(SessionRecord::ToolFinished {
                    schema_version: sv,
                    seq,
                    time: time.to_string(),
                    turn_id: turn_id.to_string(),
                    call_id,
                    status: *status,
                    output: output.clone(),
                })
                .map(|r| (r, tool_name))
                .map(|(r, _)| r)
            }
            _ => None,
        }
    }

    /// Reconstruct a transcript [`Entry`] from this record.
    ///
    /// Returns `None` for records that don't map to a transcript row
    /// (session_meta, context, session_renamed).
    pub fn to_entry(&self) -> Option<Entry> {
        match self {
            SessionRecord::User { text, .. } => Some(Entry::User { text: text.clone() }),
            SessionRecord::AssistantFinished { text, .. } => {
                Some(Entry::Assistant { text: text.clone(), streaming: false })
            }
            SessionRecord::ReasoningFinished { text, .. } => {
                Some(Entry::Reasoning { text: text.clone(), streaming: false })
            }

            SessionRecord::ToolFinished { call_id, status, output, .. } => Some(Entry::Tool {
                name: format!("#{call_id}"),
                arguments: String::new(),
                status: *status,
                output: output.clone(),
            }),
            SessionRecord::Cancelled { reason, .. } => Some(Entry::Status { text: reason.clone() }),
            SessionRecord::Failed { error, .. } => Some(Entry::Error { text: error.clone() }),
            SessionRecord::FileWrite { op, path, status, .. } => {
                Some(Entry::Status { text: format!("{} {}: {path}", status.icon(), op.label()) })
            }
            _ => None,
        }
    }
}

/// Split a tool entry name like `"search_text#0"` into `("search_text", "0")`.
///
/// If there is no `#`, the whole string is the name and the id defaults to `"?"`.
fn split_tool_name_id(name: &str) -> (String, String) {
    match name.rsplit_once('#') {
        Some((n, id)) => (n.to_string(), id.to_string()),
        None => (name.to_string(), "?".to_string()),
    }
}

/// Metadata for a single prompt turn, suitable for append-only JSONL storage.
///
/// This is the audit record: enough to reconstruct *what* was sent without
/// storing *the content itself*. Full raw provider payloads are deliberately
/// excluded because they can contain prompt text, repo content, and secrets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptMetadata {
    /// Selected model name.
    pub model: String,
    /// Web search mode label.
    pub search_mode: String,
    /// Rounded date (YYYY-MM-DD) used for cache stability.
    pub date: String,
    /// Workspace root path.
    pub cwd: String,
    /// Metadata for each loaded AGENTS.md source (no content).
    pub context_sources: Vec<ContextSourceMeta>,
    /// Number of tools in the catalog sent this turn.
    pub tool_catalog_size: usize,
    /// Whether history reuse was active for this turn.
    pub history_reuse: bool,
    /// Content hash of the root AGENTS.md from the previous turn, if any.
    pub prev_context_hash: Option<u64>,
    /// Number of transcript entries included in the projected tail.
    pub transcript_tail_size: usize,
    /// Whether the user turn was non-empty.
    pub has_user_turn: bool,
}

/// Metadata for a loaded context source, without the content itself.
///
/// Records the path, scope, content hash, and truncation state so the
/// session can audit which AGENTS.md was loaded and whether it was capped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextSourceMeta {
    /// Absolute path to the source file.
    pub path: String,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// Stable hash of the full original content (before truncation).
    pub content_hash: u64,
    /// Whether the content was truncated to fit the size cap.
    pub truncated: bool,
    /// Original byte count of the file (before truncation).
    pub byte_count: usize,
}

impl PromptMetadata {
    /// Extract prompt metadata from a [`PromptBundle`] for session storage.
    ///
    /// This captures the structural metadata of the turn — model, search mode,
    /// context sources (hashes and truncation, not content), tool count, and
    /// transcript tail size. It does not store prompt text, AGENTS.md content,
    /// or provider request/response bodies.
    pub fn from_bundle(bundle: &PromptBundle) -> Self {
        let environment: &EnvironmentMetadata = &bundle.environment;
        PromptMetadata {
            model: environment.model.clone(),
            search_mode: environment.search_mode.clone(),
            date: environment.date.clone(),
            cwd: environment.cwd.clone(),
            context_sources: bundle
                .project_context
                .iter()
                .map(ContextSourceMeta::from_source)
                .collect(),
            tool_catalog_size: bundle.tool_catalog.len(),
            history_reuse: bundle.history_reuse == HistoryReuse::Available,
            prev_context_hash: bundle.prev_context_hash,
            transcript_tail_size: bundle.transcript_tail.len(),
            has_user_turn: !bundle.user_turn.is_empty(),
        }
    }

    /// Serialize to a JSON string for JSONL append.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from a JSON string (for resume/replay).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl ContextSourceMeta {
    /// Extract metadata from a [`ContextSource`], omitting the content.
    pub fn from_source(source: &ContextSource) -> Self {
        ContextSourceMeta {
            path: source.path.display().to_string(),
            scope: source.scope.clone(),
            content_hash: source.content_hash,
            truncated: source.truncated,
            byte_count: source.byte_count,
        }
    }
}

/// Append-only JSONL session writer.
///
/// Each session is a single `.jsonl` file. Records are appended one per line
/// and never rewritten. The writer tracks a monotonic `seq` counter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionWriter {
    path: PathBuf,
    seq: u64,
    session_id: String,
}

impl SessionWriter {
    /// Create a new session file in `dir` with the given session id.
    ///
    /// Writes the initial `session_meta` record as the first line.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        dir: &Path, session_id: &str, cwd: &str, title: &str, provider: &str, model: &str, websearch: &str,
        app_version: &str,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{session_id}.jsonl"));

        let record = SessionRecord::SessionMeta {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            title: title.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            websearch: websearch.to_string(),
            app_version: app_version.to_string(),
        };

        std::fs::write(&path, format!("{}\n", record.to_json().map_err(io_err)?))?;

        Ok(SessionWriter { path, seq: 1, session_id: session_id.to_string() })
    }

    /// Append a record to the session file.
    pub fn append(&mut self, mut record: SessionRecord) -> std::io::Result<()> {
        let seq = self.seq;
        self.seq += 1;
        set_seq(&mut record, seq);

        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a transcript entry as a record, if it maps to one.
    ///
    /// Streaming/live entries are skipped — only finalized entries are
    /// persisted for replay.
    pub fn append_entry(&mut self, entry: &Entry, turn_id: &str) -> std::io::Result<()> {
        if let Some(record) = SessionRecord::from_entry(entry, self.seq, &datetime::now_iso8601(), turn_id) {
            self.seq += 1;
            let line = record.to_json().map_err(io_err)?;
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    /// Append a context record with loaded AGENTS.md source metadata.
    pub fn append_context(&mut self, sources: &[ContextSource]) -> std::io::Result<()> {
        let metas: Vec<ContextSourceMeta> = sources.iter().map(ContextSourceMeta::from_source).collect();
        let record = SessionRecord::Context {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            sources: metas,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a file-write audit record.
    ///
    /// Records the operation type, path, before/after hashes and byte counts,
    /// and status. File content is never stored — only hashes and byte counts,
    /// so secrets and large files are not persisted.
    pub fn append_file_write(
        &mut self, turn_id: &str, result: &crate::tools::WriteResult, status: ToolStatus,
    ) -> std::io::Result<()> {
        let record = SessionRecord::FileWrite {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            op: result.op,
            path: result.path.display().to_string(),
            before_hash: result.before_hash,
            before_bytes: result.before_bytes,
            after_hash: result.after_hash,
            after_bytes: result.after_bytes,
            status,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// The session file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The session id.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Set the seq field on a record (used by `SessionWriter::append`).
fn set_seq(record: &mut SessionRecord, seq: u64) {
    match record {
        SessionRecord::SessionMeta { seq: s, .. }
        | SessionRecord::Context { seq: s, .. }
        | SessionRecord::User { seq: s, .. }
        | SessionRecord::AssistantFinished { seq: s, .. }
        | SessionRecord::ReasoningFinished { seq: s, .. }
        | SessionRecord::ToolStarted { seq: s, .. }
        | SessionRecord::ToolFinished { seq: s, .. }
        | SessionRecord::Cancelled { seq: s, .. }
        | SessionRecord::Failed { seq: s, .. }
        | SessionRecord::SessionRenamed { seq: s, .. }
        | SessionRecord::FileWrite { seq: s, .. } => *s = seq,
    }
}

/// Reads a session JSONL file and reconstructs transcript entries.
///
/// Corrupt lines are skipped silently — the rest of the file is still
/// readable. This makes resume resilient to partial writes.
pub struct SessionReader;

impl SessionReader {
    /// Read a session file and return all records, in order.
    ///
    /// Corrupt lines are skipped. Returns an empty vec if the file does
    /// not exist.
    pub fn read_records(path: &Path) -> Vec<SessionRecord> {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        content
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| SessionRecord::from_json(line).ok())
            .collect()
    }

    /// Read a session file and reconstruct the transcript.
    ///
    /// Only records that map to [`Entry`] values are included. Metadata
    /// records (session_meta, context, session_renamed) are skipped.
    pub fn read_transcript(path: &Path) -> Vec<Entry> {
        Self::read_records(path)
            .into_iter()
            .filter_map(|r| r.to_entry())
            .collect()
    }

    /// Read the session title from a session file.
    ///
    /// Returns the latest `session_renamed` title, or the initial
    /// `session_meta` title, or the file stem as a fallback.
    pub fn read_title(path: &Path) -> String {
        let records = Self::read_records(path);
        for r in records.iter().rev() {
            if let SessionRecord::SessionRenamed { title, .. } = r {
                return title.clone();
            }
        }

        for r in &records {
            if let SessionRecord::SessionMeta { title, .. } = r {
                return title.clone();
            }
        }

        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session")
            .to_string()
    }
}

/// The sessions directory under a workspace root: `{root}/.thndrs/sessions/`.
pub fn sessions_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".thndrs").join("sessions")
}

/// List session files in a directory, sorted newest-first by modification time.
pub fn list_session_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") { Some(path) } else { None }
        })
        .collect();

    files.sort_by(|a, b| {
        let mtime_a = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let mtime_b = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        mtime_b.cmp(&mtime_a)
    });
    files
}

/// List session titles from a directory, newest-first.
///
/// Each title is read from the session file (latest rename or session_meta).
/// Falls back to the file stem if the file cannot be parsed.
pub fn list_session_titles(dir: &Path) -> Vec<String> {
    list_session_files(dir)
        .into_iter()
        .map(|p| SessionReader::read_title(&p))
        .collect()
}

/// Find the most recently modified session file in a directory.
///
/// Returns `None` if the directory does not exist or has no `.jsonl` files.
pub fn latest_session_file(dir: &Path) -> Option<PathBuf> {
    list_session_files(dir).into_iter().next()
}

/// Generate a session id from a timestamp (format: `session-YYYYMMDD-HHMMSS`).
pub fn generate_session_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let remainder = secs % 86_400;
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;
    let date = datetime::date_from_days(days);
    let date_compact = date.replace('-', "");
    format!("session-{date_compact}-{hour:02}{minute:02}{second:02}")
}

/// Convert a serde_json error into an io::Error.
fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}
