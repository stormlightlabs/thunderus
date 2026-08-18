//! Session readers and session-directory lookup helpers.

use super::*;

/// Reads a session JSONL file and reconstructs transcript entries.
///
/// Corrupt lines are skipped silently — the rest of the file is still
/// readable. This makes resume resilient to partial writes.
pub struct SessionReader;

impl SessionReader {
    /// Read every record while validating session identity and ordering.
    ///
    /// Unlike the recovery-oriented readers, this rejects malformed lines and
    /// is used before a session is opened for continued writing.
    pub fn read_validated_records(path: &Path, session_id: &str) -> std::io::Result<Vec<SessionRecord>> {
        let content = std::fs::read_to_string(path)?;
        let mut records = Vec::new();
        let mut previous_sequence = None;

        for (index, line) in content.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let record = SessionRecord::from_json(line).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("session record {} is corrupt: {error}", index + 1),
                )
            })?;
            if previous_sequence.is_some_and(|sequence| record.seq() <= sequence) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("session record {} has an invalid sequence", index + 1),
                ));
            }
            previous_sequence = Some(record.seq());
            records.push(record);
        }

        match records.first() {
            Some(SessionRecord::SessionMeta { session_id: stored_id, .. }) if stored_id == session_id => {}
            Some(SessionRecord::SessionMeta { session_id: stored_id, .. }) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("session identity mismatch: expected `{session_id}`, found `{stored_id}`"),
                ));
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "session metadata is missing",
                ));
            }
        }

        Ok(records)
    }

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

    /// Read only the trailing portion of a session file.
    ///
    /// If the read starts in the middle of a JSONL record, that first
    /// partial record is discarded. This is suitable for input recall,
    /// where recent turns matter more than a full historical reconstruction.
    pub fn read_records_from_tail(path: &Path, max_bytes: usize) -> Vec<SessionRecord> {
        use std::io::{Read, Seek};

        if max_bytes == 0 {
            return Vec::new();
        }

        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(file_len) = file.metadata().map(|metadata| metadata.len()) else {
            return Vec::new();
        };
        let start = file_len.saturating_sub(max_bytes as u64);
        let read_start = start.saturating_sub(1);
        if file.seek(std::io::SeekFrom::Start(read_start)).is_err() {
            return Vec::new();
        }

        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        let content = String::from_utf8_lossy(&bytes);
        let complete_records = if start == 0 {
            content.as_ref()
        } else {
            content.split_once('\n').map_or("", |(_, records)| records)
        };

        complete_records
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

    /// Replay semantic session records into stable transcript blocks.
    ///
    /// Matching tool and permission lifecycle records update their original
    /// block so resume restores the same action, target, and final state.
    pub fn read_transcript_blocks(path: &Path) -> TranscriptBlocks {
        let records = Self::read_records(path);
        let context_history = ContextHistory::from_records(&records);
        let mut transcript = TranscriptBlocks::new();
        let mut previous_failure: Option<(String, String)> = None;
        for record in records {
            let duplicate_failure = match &record {
                SessionRecord::Failed { turn_id, error, .. } => previous_failure
                    .as_ref()
                    .is_some_and(|(previous_turn, previous_error)| previous_turn == turn_id && previous_error == error),
                _ => false,
            };
            previous_failure = match &record {
                SessionRecord::Failed { turn_id, error, .. } => Some((turn_id.clone(), error.clone())),
                _ => None,
            };
            if duplicate_failure {
                continue;
            }
            if let Some((id, text)) = context_history.transcript_event(&record) {
                transcript.push_context_event(id, text);
                continue;
            }
            match &record {
                SessionRecord::ToolStarted { call_id, name, arguments, .. } => {
                    if transcript.queue_tool(call_id, name, arguments).is_ok() {
                        let _ = transcript.start_tool(call_id);
                    }
                }
                SessionRecord::ToolFinished { call_id, status, output, artifact, .. } => {
                    if transcript
                        .finish_tool(
                            call_id,
                            *status,
                            output.clone(),
                            artifact.as_ref().is_some_and(|artifact| artifact.truncated),
                        )
                        .is_err()
                        && let Some(entry) = record.to_entry()
                    {
                        transcript.push(entry);
                    }
                }
                SessionRecord::AcpPermissionRequest { tool_call_id, title, .. } => {
                    transcript.push_permission(
                        tool_call_id,
                        format!("acp permission requested: {title} ({tool_call_id})"),
                    );
                }
                SessionRecord::AcpPermissionOutcome { tool_call_id, outcome, .. } => {
                    let text = format!("acp permission {tool_call_id}: {outcome}");
                    if !transcript.resolve_permission(tool_call_id, text.clone()) {
                        transcript.push_permission(tool_call_id, text);
                    }
                }
                SessionRecord::ShellExec {
                    process_id: Some(process_id), command, process_status, elapsed_ms, ..
                } => {
                    transcript.push_child_activity(
                        *process_id,
                        format!("shell [{process_id}] {process_status}: {command} ({elapsed_ms}ms)"),
                    );
                }
                _ => {
                    if let Some(entry) = record.to_entry() {
                        transcript.push(entry);
                    }
                }
            }
        }
        transcript
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

    /// Read compact sidebar metadata from a session file.
    pub fn read_summary(path: &Path) -> SessionSummary {
        let records = Self::read_records(path);
        let mut title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session")
            .to_string();
        let mut model = String::from("unknown");
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut accounted_input_tokens = 0u64;
        let mut accounted_output_tokens = 0u64;
        let mut has_request_accounting = false;

        for record in records {
            match record {
                SessionRecord::SessionMeta { title: t, model: m, .. } => {
                    title = t;
                    model = m;
                }
                SessionRecord::SessionRenamed { title: t, .. } => title = t,
                SessionRecord::Usage { input_tokens: i, output_tokens: o, .. } => {
                    input_tokens = input_tokens.saturating_add(i);
                    output_tokens = output_tokens.saturating_add(o);
                }
                SessionRecord::RequestAccounting { accounting, .. } => {
                    has_request_accounting = true;
                    if let Some(usage) = accounting.provider_usage {
                        accounted_input_tokens =
                            accounted_input_tokens.saturating_add(usage.components.input_tokens.unwrap_or(0));
                        accounted_output_tokens =
                            accounted_output_tokens.saturating_add(usage.components.output_tokens.unwrap_or(0));
                    }
                }
                _ => {}
            }
        }

        if has_request_accounting {
            input_tokens = accounted_input_tokens;
            output_tokens = accounted_output_tokens;
        }

        SessionSummary { title, model, input_tokens, output_tokens }
    }

    /// Read a renderer-independent, redacted JSON projection of every valid
    /// record in sequence order. Malformed lines remain omitted just as they
    /// are for transcript recovery.
    pub fn read_redacted_records(path: &Path) -> Vec<serde_json::Value> {
        Self::read_records(path)
            .into_iter()
            .filter_map(|record| serde_json::to_value(record).ok())
            .map(redact_json_value)
            .collect()
    }
}

/// Compact session metadata for sidebar display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub title: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl SessionSummary {
    pub fn sidebar_label(&self) -> String {
        format!("{}\nin {} out {}", self.model, self.input_tokens, self.output_tokens)
    }
}

/// An error resolving a user-supplied local session identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionLookupError {
    /// No session has the supplied exact id or prefix.
    NotFound { query: String },
    /// More than one session has the supplied prefix.
    Ambiguous { query: String, matches: Vec<String> },
}

impl std::fmt::Display for SessionLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { query } => write!(formatter, "session `{query}` is not found"),
            Self::Ambiguous { query, matches } => {
                write!(
                    formatter,
                    "session prefix `{query}` is ambiguous; matches: {}",
                    matches.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for SessionLookupError {}

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
        mtime_b.cmp(&mtime_a).then_with(|| b.cmp(a))
    });
    files
}

/// Resolve an exact session id or a unique id prefix.
///
/// Matching only considers `.jsonl` files in `dir`; a missing or corrupt
/// session file therefore cannot prevent other valid files from being found.
pub fn resolve_session_file(dir: &Path, query: &str) -> Result<PathBuf, SessionLookupError> {
    let files = list_session_files(dir);
    if let Some(path) = files.iter().find(|path| session_id_from_path(path) == Some(query)) {
        return Ok(path.clone());
    }

    let matches: Vec<PathBuf> = files
        .into_iter()
        .filter(|path| session_id_from_path(path).is_some_and(|id| id.starts_with(query)))
        .collect();
    match matches.as_slice() {
        [] => Err(SessionLookupError::NotFound { query: query.to_string() }),
        [path] => Ok(path.clone()),
        _ => Err(SessionLookupError::Ambiguous {
            query: query.to_string(),
            matches: matches
                .iter()
                .filter_map(|path| session_id_from_path(path).map(ToString::to_string))
                .collect(),
        }),
    }
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

/// List session sidebar summaries, newest-first.
pub fn list_session_summaries(dir: &Path) -> Vec<SessionSummary> {
    list_session_files(dir)
        .into_iter()
        .map(|p| SessionReader::read_summary(&p))
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

/// Create an independently resumable session from a settled parent turn.
///
/// The fork copies only the replayable semantic prefix through the requested
/// terminal turn record. Runtime-owned state and incomplete lifecycle pairs
/// stay with the parent session.
pub fn fork_session(dir: &Path, parent_path: &Path, parent_session_id: &str, turn_id: &str) -> std::io::Result<String> {
    use std::collections::HashSet;

    let records = SessionReader::read_validated_records(parent_path, parent_session_id)?;
    let boundary = records
        .iter()
        .rposition(|record| {
            matches!(
                record,
                SessionRecord::AssistantFinished { turn_id: record_turn, .. }
                    | SessionRecord::Cancelled { turn_id: record_turn, .. }
                    | SessionRecord::Failed { turn_id: record_turn, .. }
                    if record_turn == turn_id
            )
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("turn `{turn_id}` is not a settled replayable boundary"),
            )
        })?;
    if !records[..=boundary]
        .iter()
        .any(|record| matches!(record, SessionRecord::User { turn_id: record_turn, .. } if record_turn == turn_id))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("turn `{turn_id}` has no replayable user message"),
        ));
    }

    let (cwd, provider, model, websearch, app_version, config) = match &records[0] {
        SessionRecord::SessionMeta { cwd, provider, model, websearch, app_version, config, .. } => (
            cwd.clone(),
            provider.clone(),
            model.clone(),
            websearch.clone(),
            app_version.clone(),
            config.clone(),
        ),
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "validated session does not begin with metadata",
            ));
        }
    };
    let title = SessionReader::read_title(parent_path);
    let completed_tools: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::ToolFinished { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();
    let completed_permissions: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::AcpPermissionOutcome { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect();
    let started_tools: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::ToolStarted { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();
    let requested_permissions: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::AcpPermissionRequest { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect();

    let mut lineage = records[..=boundary]
        .iter()
        .find_map(|record| match record {
            SessionRecord::SessionFork { lineage, .. } => Some(lineage.clone()),
            _ => None,
        })
        .unwrap_or_default();
    lineage.push(SessionLineageEntry { session_id: parent_session_id.to_string(), turn_id: turn_id.to_string() });

    let base_id = generate_session_id();
    let mut suffix = 1_u64;
    let session_id = loop {
        let candidate = if suffix == 1 { base_id.clone() } else { format!("{base_id}-{suffix}") };
        if !dir.join(format!("{candidate}.jsonl")).exists() {
            break candidate;
        }
        suffix = suffix.saturating_add(1);
    };
    let mut writer = SessionWriter::create(
        dir,
        &session_id,
        &cwd,
        &title,
        &provider,
        &model,
        &websearch,
        &app_version,
        config,
    )?;
    writer.append(SessionRecord::SessionFork {
        schema_version: SCHEMA_VERSION,
        seq: 0,
        time: datetime::now_iso8601(),
        parent_session_id: parent_session_id.to_string(),
        parent_turn_id: turn_id.to_string(),
        lineage,
    })?;
    for record in records.into_iter().take(boundary + 1) {
        let copy = match &record {
            SessionRecord::SessionMeta { .. }
            | SessionRecord::SessionFork { .. }
            | SessionRecord::AcpSession { .. }
            | SessionRecord::QueuedInput { .. } => false,
            SessionRecord::ToolStarted { call_id, .. } => completed_tools.contains(call_id.as_str()),
            SessionRecord::ToolFinished { call_id, .. } => started_tools.contains(call_id.as_str()),
            SessionRecord::AcpPermissionRequest { tool_call_id, .. } => {
                completed_permissions.contains(tool_call_id.as_str())
            }
            SessionRecord::AcpPermissionOutcome { tool_call_id, .. } => {
                requested_permissions.contains(tool_call_id.as_str())
            }
            SessionRecord::CompactionReview { review: CompactionReviewResult::Pending, .. } => false,
            SessionRecord::ShellExec { process_status, .. } => process_status != "running",
            _ => true,
        };
        if copy {
            writer.append(record)?;
        }
    }
    Ok(session_id)
}

fn session_id_from_path(path: &Path) -> Option<&str> {
    path.file_stem().and_then(|stem| stem.to_str())
}

fn redact_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(shell::redact_secrets(&text)),
        serde_json::Value::Array(items) => serde_json::Value::Array(items.into_iter().map(redact_json_value).collect()),
        serde_json::Value::Object(items) => serde_json::Value::Object(
            items
                .into_iter()
                .map(|(key, value)| (key, redact_json_value(value)))
                .collect(),
        ),
        value => value,
    }
}

/// Read the last `max_lines` of a text log, redacting values and bounding the
/// returned payload. Missing files produce an empty result.
pub fn read_redacted_log_tail(path: &Path, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut lines = VecDeque::with_capacity(max_lines);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if lines.len() == max_lines {
            lines.pop_front();
        }
        lines.push_back(shell::redact_secrets(&line));
    }
    let mut bytes = 0usize;
    let mut output = Vec::new();
    for line in lines.into_iter().rev() {
        let line_bytes = line.len().saturating_add(1);
        if bytes.saturating_add(line_bytes) > MAX_LOG_OUTPUT_BYTES {
            break;
        }
        bytes = bytes.saturating_add(line_bytes);
        output.push(line);
    }
    output.reverse();
    output
}
