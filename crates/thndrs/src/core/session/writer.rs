//! Append-only session writer.

use super::*;

/// Action variants supported by [`SessionWriter::append_context_action`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextActionKind {
    Pin,
    Drop,
    Recovery,
}

/// Append-only JSONL session writer.
///
/// Each session is a single `.jsonl` file. Records are appended one per line
/// and never rewritten. The writer tracks a monotonic `seq` counter.
#[derive(Debug)]
pub struct SessionWriter {
    path: PathBuf,
    seq: u64,
    session_id: String,
    lock_path: PathBuf,
    _lock: File,
}

impl SessionWriter {
    /// Create a new session file in `dir` with the given session id.
    ///
    /// Writes the initial `session_meta` record as the first line.
    #[expect(
        clippy::too_many_arguments,
        reason = "session metadata is written as a flat JSONL record"
    )]
    pub fn create(
        dir: &Path, session_id: &str, cwd: &str, title: &str, provider: &str, model: &str, _historical_websearch: &str,
        app_version: &str, config: Option<SessionConfigMeta>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{session_id}.jsonl"));
        let (lock_path, lock) = acquire_writer_lock(&path)?;

        let record = SessionRecord::SessionMeta {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            title: title.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            websearch: String::new(),
            app_version: app_version.to_string(),
            config,
        };

        let write_result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
            writeln!(file, "{}", record.to_json().map_err(io_err)?)
        })();
        if let Err(error) = write_result {
            let _ = std::fs::remove_file(&lock_path);
            return Err(error);
        }

        Ok(SessionWriter { path, seq: 1, session_id: session_id.to_string(), lock_path, _lock: lock })
    }

    /// Sequence number that will be assigned to the next appended record.
    pub fn next_sequence(&self) -> u64 {
        self.seq
    }

    /// Reopen a validated existing session file for append-only continuation.
    pub fn resume(path: &Path, session_id: &str) -> std::io::Result<Self> {
        let (lock_path, lock) = acquire_writer_lock(path)?;
        let records = match SessionReader::read_validated_records(path, session_id) {
            Ok(records) => records,
            Err(error) => {
                let _ = std::fs::remove_file(&lock_path);
                return Err(error);
            }
        };
        let seq = records
            .iter()
            .map(SessionRecord::seq)
            .max()
            .map_or(0, |max_seq| max_seq.saturating_add(1));

        if let Err(error) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = std::fs::remove_file(&lock_path);
            return Err(error);
        }

        Ok(SessionWriter { path: path.to_path_buf(), seq, session_id: session_id.to_string(), lock_path, _lock: lock })
    }

    /// Append a validated display-name change without rewriting history.
    pub fn append_rename(&mut self, name: &str) -> std::io::Result<()> {
        let name = validate_session_name(name)?;
        self.append(SessionRecord::SessionRenamed {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            title: name.to_string(),
        })
    }

    /// Append a record to the session file.
    pub fn append(&mut self, mut record: SessionRecord) -> std::io::Result<()> {
        let seq = self.seq;
        record.set_seq(seq);

        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        self.seq += 1;
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

    /// Append a finalized transcript entry with bounded artifact metadata.
    pub fn append_entry_with_artifact(
        &mut self, entry: &Entry, turn_id: &str, artifact: Option<ArtifactMetadata>,
    ) -> std::io::Result<()> {
        if let Some(record) =
            SessionRecord::from_entry_with_artifact(entry, self.seq, &datetime::now_iso8601(), turn_id, artifact)
        {
            self.seq += 1;
            let line = record.to_json().map_err(io_err)?;
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    /// Append a `tool_started` record for a tool call that has begun.
    ///
    /// This records the command start: tool name, call id, and arguments.
    /// The matching `tool_finished` (via [`Self::append_entry`]) records the
    /// output, status, and summary. For `run_shell`, an additional
    /// [`Self::append_shell_exec`] record captures exit code, elapsed time, and
    /// process kind.
    pub fn append_tool_started(&mut self, turn_id: &str, call_id: &str, name: &str, args: &str) -> std::io::Result<()> {
        let record = SessionRecord::ToolStarted {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments: args.to_string(),
            mcp: mcp_tool_session_meta(name),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
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

    /// Append a content-free context ledger snapshot for a prompt turn.
    pub fn append_context_ledger(&mut self, turn_id: &str, ledger: &ContextLedger) -> std::io::Result<()> {
        self.append(SessionRecord::ContextLedger {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            ledger: ContextLedgerMeta::from(ledger),
        })
    }

    /// Append a versioned context snapshot for one request attempt.
    pub fn append_context_snapshot(&mut self, snapshot: ContextSnapshot) -> std::io::Result<()> {
        self.append(SessionRecord::ContextSnapshot {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            snapshot: Box::new(snapshot),
        })
    }

    /// Append the durable capture policy used by this session.
    pub fn append_context_capture_policy(&mut self, policy: &ContextCapturePolicy) -> std::io::Result<()> {
        self.append(SessionRecord::ContextCapturePolicy {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            policy: policy.clone(),
        })
    }

    /// Append sanitized normalized request content after policy validation.
    pub fn append_captured_request(&mut self, capture: CapturedRequestContent) -> std::io::Result<()> {
        self.append(SessionRecord::RequestContentCaptured {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            capture,
        })
    }

    /// Append a content-free user pin action.
    pub fn append_context_pin(&mut self, item: &ContextItem, reason: &str) -> std::io::Result<()> {
        self.append_context_action(item, reason, ContextActionKind::Pin)
    }

    /// Append a content-free user drop action.
    pub fn append_context_drop(&mut self, item: &ContextItem, reason: &str) -> std::io::Result<()> {
        self.append_context_action(item, reason, ContextActionKind::Drop)
    }

    /// Append a content-free user recovery action.
    pub fn append_context_recovery(&mut self, item: &ContextItem, reason: &str) -> std::io::Result<()> {
        self.append_context_action(item, reason, ContextActionKind::Recovery)
    }

    /// Append an explicit lifecycle transition with content-free post-state
    /// metadata. Callers should apply the returned pure transition only after
    /// this append succeeds.
    pub fn append_context_lifecycle(
        &mut self, item: &ContextItem, action: thndrs_agent::context::ContextLifecycleAction, reason: &str,
    ) -> std::io::Result<()> {
        self.append(SessionRecord::ContextLifecycle {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            audit: ContextLifecycleAudit {
                action,
                item: ContextItemMeta::from(item),
                reason: tools::shell::redact_secrets(reason),
            },
        })
    }

    /// Append a compaction audit record without source payloads.
    pub fn append_compaction(&mut self, audit: &CompactionAudit) -> std::io::Result<()> {
        self.append(SessionRecord::Compaction {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            audit: audit.redacted(),
        })
    }

    /// Append the review decision for a previously pending compaction.
    pub fn append_compaction_review(
        &mut self, recovery_handle: &str, review: CompactionReviewResult,
    ) -> std::io::Result<()> {
        self.append(SessionRecord::CompactionReview {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            recovery_handle: tools::shell::redact_secrets(recovery_handle),
            review,
        })
    }

    /// Append one typed context action without exposing content fields.
    fn append_context_action(
        &mut self, item: &ContextItem, reason: &str, action: ContextActionKind,
    ) -> std::io::Result<()> {
        let item = ContextItemMeta::from(item);
        let reason = tools::shell::redact_secrets(reason);
        let record = match action {
            ContextActionKind::Pin => SessionRecord::ContextPin {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: datetime::now_iso8601(),
                item,
                reason,
            },
            ContextActionKind::Drop => SessionRecord::ContextDrop {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: datetime::now_iso8601(),
                item,
                reason,
            },
            ContextActionKind::Recovery => SessionRecord::ContextRecovery {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: datetime::now_iso8601(),
                item,
                reason,
            },
        };
        self.append(record)
    }

    /// Append prompt assembly provenance for a user turn.
    pub fn append_prompt_metadata(&mut self, turn_id: &str, metadata: &PromptMetadata) -> std::io::Result<()> {
        let record = SessionRecord::PromptMetadata {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            metadata: metadata.clone(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append provider token usage for the session.
    pub fn append_usage(&mut self, input_tokens: u64, output_tokens: u64) -> std::io::Result<()> {
        if input_tokens == 0 && output_tokens == 0 {
            return Ok(());
        }

        let record = SessionRecord::Usage {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            input_tokens,
            output_tokens,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append accounting for one successful provider request.
    pub fn append_request_accounting(
        &mut self, turn_id: &str, accounting: &ProviderRequestAccounting,
    ) -> std::io::Result<()> {
        self.append(SessionRecord::RequestAccounting {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            accounting: Box::new(accounting.clone()),
        })
    }

    /// Append a file-write audit record.
    ///
    /// Records the operation type, path, before/after hashes and byte counts,
    /// and status. File content is never stored — only hashes and byte counts,
    /// so secrets and large files are not persisted.
    pub fn append_file_write(
        &mut self, turn_id: &str, result: &tools::WriteResult, status: ToolStatus,
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

    /// Append an audit record when MCP config file hashes change mid-session.
    pub fn append_mcp_config_changed(
        &mut self, turn_id: &str, previous_files: Vec<SessionConfigFile>, current_files: Vec<SessionConfigFile>,
        diagnostics: Vec<String>,
    ) -> std::io::Result<()> {
        let record = SessionRecord::McpConfigChanged {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            previous_files,
            current_files,
            diagnostics,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a shell-execution audit record.
    ///
    /// Records the command argv, working directory, registry id, lifecycle
    /// status, exit code, elapsed time, and process kind. stdout/stderr are
    /// not stored here — they are captured in redacted and capped output
    /// records.
    pub fn append_shell_exec(&mut self, turn_id: &str, result: &shell::ProcessResult) -> std::io::Result<()> {
        let record = SessionRecord::ShellExec {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            process_id: result.process_id,
            command: shell::redact_secrets(&result.command.join(" ")),
            cwd: result.cwd.display().to_string(),
            process_status: result.status.label().to_string(),
            exit_code: result.exit_code,
            elapsed_ms: result.elapsed.as_millis() as u64,
            kind: result.kind.label().to_string(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a skill activation metadata record.
    pub fn append_skill_activation(&mut self, activation: &SkillActivation) -> std::io::Result<()> {
        let record = SessionRecord::SkillActivated {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            name: activation.name.clone(),
            path: activation.path.display().to_string(),
            content_hash: activation.content_hash,
            byte_count: activation.byte_count,
            rendered_content_hash: activation.rendered_content_hash,
            rendered_byte_count: activation.rendered_byte_count,
            loaded_references: activation
                .loaded_references
                .iter()
                .map(SkillReferenceRecord::from)
                .collect(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a queued input audit record.
    pub fn append_queued(&mut self, queue_id: u64, kind: &str, action: &str, text: &str) -> std::io::Result<()> {
        let record = SessionRecord::QueuedInput {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            queue_id,
            kind: kind.to_string(),
            action: action.to_string(),
            text: text.to_string(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append ACP permission request metadata.
    pub fn append_acp_permission_request(&mut self, turn_id: &str, request: &PendingPermission) -> std::io::Result<()> {
        let record = SessionRecord::AcpPermissionRequest {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            tool_call_id: request.tool_call_id.clone(),
            title: request.title.clone(),
            options: request
                .options
                .iter()
                .map(|option| AcpPermissionOptionRecord {
                    id: option.id.clone(),
                    name: option.name.clone(),
                    kind: option.kind.label().to_string(),
                })
                .collect(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append ACP session metadata.
    pub fn append_acp_session(&mut self, metadata: &AcpSessionMetadata) -> std::io::Result<()> {
        let record = SessionRecord::AcpSession {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            local_session_id: self.session_id.clone(),
            agent_name: metadata.agent_name.clone(),
            acp_session_id: metadata.acp_session_id.clone(),
            command: metadata.command.clone(),
            protocol_version: metadata.protocol_version.clone(),
            agent_info_name: metadata.agent_info_name.clone(),
            agent_info_version: metadata.agent_info_version.clone(),
            client_info_name: metadata.client_info_name.clone(),
            client_info_version: metadata.client_info_version.clone(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append ACP permission outcome metadata.
    pub fn append_acp_permission_outcome(
        &mut self, turn_id: &str, tool_call_id: &str, outcome: &str,
    ) -> std::io::Result<()> {
        let record = SessionRecord::AcpPermissionOutcome {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            tool_call_id: tool_call_id.to_string(),
            outcome: outcome.to_string(),
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

impl Drop for SessionWriter {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

fn validate_session_name(name: &str) -> std::io::Result<&str> {
    if name.chars().any(char::is_control) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session names cannot contain control characters",
        ));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session names cannot be empty",
        ));
    }
    if name.chars().count() > MAX_SESSION_NAME_CHARS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("session names cannot exceed {MAX_SESSION_NAME_CHARS} characters"),
        ));
    }
    Ok(name)
}

fn acquire_writer_lock(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.jsonl");
    let lock_path = path.with_file_name(format!("{file_name}.lock"));
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    format!("session `{}` already has an active writer", path.display()),
                )
            } else {
                error
            }
        })?;
    Ok((lock_path, lock))
}
