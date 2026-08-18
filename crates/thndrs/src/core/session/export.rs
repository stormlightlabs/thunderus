//! Deterministic, provider-neutral session exports.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use minijinja::{AutoEscape, Environment};
use serde::Serialize;
use serde_json::Value;

use super::{SCHEMA_VERSION, SessionReader};

const MAX_EXPORT_RECORDS: usize = 4096;
const SESSION_HTML_TEMPLATE: &str = include_str!("session.html.j2");

/// A redacted semantic session projection used by human-readable exporters.
#[derive(Clone, Debug, Serialize)]
pub struct SessionExport {
    id: String,
    title: String,
    started: String,
    provider: String,
    model: String,
    status: String,
    turn_count: usize,
    lineage: Vec<LineageItem>,
    messages: Vec<MessageItem>,
    activities: Vec<ActivityItem>,
    artifacts: Vec<ArtifactItem>,
    findings: Vec<String>,
    requests: Vec<RequestItem>,
    transformations: Vec<TransformationItem>,
    truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
struct LineageItem {
    session_id: String,
    turn_id: String,
}

#[derive(Clone, Debug, Serialize)]
struct MessageItem {
    role: String,
    turn_id: String,
    time: String,
    text: String,
}

#[derive(Clone, Debug, Serialize)]
struct ActivityItem {
    time: String,
    kind: String,
    name: String,
    detail: String,
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct ArtifactItem {
    kind: String,
    label: String,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
struct RequestItem {
    time: String,
    request_id: String,
    detail: String,
    attempt: u64,
}

#[derive(Clone, Debug, Serialize)]
struct TransformationItem {
    time: String,
    kind: String,
    detail: String,
}

/// Build a redacted export projection from persisted session data.
pub fn export_session(path: &Path, session_id: &str) -> io::Result<SessionExport> {
    let all_records = SessionReader::read_redacted_records(path);
    let truncated = all_records.len() > MAX_EXPORT_RECORDS;
    let records: Vec<_> = all_records.into_iter().take(MAX_EXPORT_RECORDS).collect();
    let metadata = records
        .first()
        .filter(|record| string(record, "type") == "session_meta")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "session metadata is missing"))?;

    let mut export = SessionExport {
        id: session_id.to_string(),
        title: string(metadata, "title").to_string(),
        started: string(metadata, "time").to_string(),
        provider: string(metadata, "provider").to_string(),
        model: string(metadata, "model").to_string(),
        status: "in progress".to_string(),
        turn_count: 0,
        lineage: Vec::new(),
        messages: Vec::new(),
        activities: Vec::new(),
        artifacts: Vec::new(),
        findings: Vec::new(),
        requests: Vec::new(),
        transformations: Vec::new(),
        truncated,
    };
    let mut tool_names = HashMap::new();
    let mut exported_transformations = HashSet::new();
    let mut turns = Vec::new();

    for record in &records {
        let record_type = string(record, "type");
        let time = display_time(string(record, "time"));
        match record_type {
            "session_fork" => {
                export.lineage = record
                    .get("lineage")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|item| LineageItem {
                        session_id: string(item, "session_id").to_string(),
                        turn_id: string(item, "turn_id").to_string(),
                    })
                    .collect();
            }
            "session_renamed" => export.title = string(record, "title").to_string(),
            "user" | "assistant_finished" | "reasoning_finished" => {
                if record_type == "user" {
                    export.status = "in progress".to_string();
                } else if record_type == "assistant_finished" {
                    export.status = "complete".to_string();
                }
                let turn_id = string(record, "turn_id").to_string();
                if !turns.contains(&turn_id) {
                    turns.push(turn_id.clone());
                }
                let role = match record_type {
                    "user" => "human",
                    "assistant_finished" => "thndrs",
                    _ => "reasoning",
                };
                export.messages.push(MessageItem {
                    role: role.to_string(),
                    turn_id,
                    time: time.clone(),
                    text: string(record, "text").to_string(),
                });
            }
            "cancelled" | "failed" => {
                export.status = record_type.to_string();
                export.messages.push(MessageItem {
                    role: "status".to_string(),
                    turn_id: string(record, "turn_id").to_string(),
                    time,
                    text: if record_type == "failed" {
                        string(record, "error").to_string()
                    } else {
                        string(record, "reason").to_string()
                    },
                });
            }
            "tool_started" => {
                tool_names.insert(
                    string(record, "call_id").to_string(),
                    string(record, "name").to_string(),
                );
            }
            "tool_finished" => {
                let call_id = string(record, "call_id");
                let status = string(record, "status").to_string();
                let detail = if status == "failed" {
                    record
                        .get("output")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    "result omitted from review copy".to_string()
                };
                export.activities.push(ActivityItem {
                    time,
                    kind: "tool".to_string(),
                    name: tool_names.get(call_id).cloned().unwrap_or_else(|| call_id.to_string()),
                    detail,
                    status,
                });
                if let Some(artifact) = record.get("artifact") {
                    export.artifacts.push(ArtifactItem {
                        kind: "tool evidence".to_string(),
                        label: call_id.to_string(),
                        detail: compact_json(artifact),
                    });
                }
            }
            "file_write" => {
                let path = string(record, "path").to_string();
                let before = record.get("before_bytes").and_then(Value::as_u64);
                let after = record.get("after_bytes").and_then(Value::as_u64).unwrap_or_default();
                let detail =
                    before.map_or_else(|| format!("{after} bytes"), |value| format!("{value} → {after} bytes"));
                export.activities.push(ActivityItem {
                    time,
                    kind: "write".to_string(),
                    name: string(record, "op").to_string(),
                    detail: format!("{path} · {detail}"),
                    status: string(record, "status").to_string(),
                });
                export
                    .artifacts
                    .push(ArtifactItem { kind: "file".to_string(), label: path, detail });
            }
            "shell_exec" => export.activities.push(ActivityItem {
                time,
                kind: "shell".to_string(),
                name: string(record, "kind").to_string(),
                detail: string(record, "command").to_string(),
                status: string(record, "process_status").to_string(),
            }),
            "acp_permission_outcome" => export.activities.push(ActivityItem {
                time,
                kind: "permission".to_string(),
                name: string(record, "tool_call_id").to_string(),
                detail: "ACP permission decision".to_string(),
                status: string(record, "outcome").to_string(),
            }),
            "request_accounting" => {
                let accounting = record.get("accounting").unwrap_or(&Value::Null);
                let estimated = accounting
                    .pointer("/estimated_input_tokens/value")
                    .and_then(Value::as_u64)
                    .map_or_else(|| "unknown".to_string(), |value| value.to_string());
                let provider = string(accounting, "provider");
                let model = string(accounting, "model");
                export.requests.push(RequestItem {
                    time: time.clone(),
                    request_id: string(accounting, "request_id").to_string(),
                    detail: format!("{provider}/{model} · projected {estimated} input tokens"),
                    attempt: accounting.get("attempt").and_then(Value::as_u64).unwrap_or_default(),
                });
                for (kind, field) in [
                    ("applied reduction", "applied_receipts"),
                    ("fallback reduction", "fallback_receipts"),
                ] {
                    if let Some(receipts) = accounting.get(field).and_then(Value::as_array) {
                        export.transformations.extend(receipts.iter().filter_map(|receipt| {
                            let key = transformation_key(accounting, receipt);
                            exported_transformations.insert(key).then(|| TransformationItem {
                                time: time.clone(),
                                kind: kind.to_string(),
                                detail: compact_json(receipt),
                            })
                        }));
                    }
                }
            }
            "context_snapshot" => {
                let snapshot = record.get("snapshot").unwrap_or(&Value::Null);
                if string(snapshot, "state") != "dispatched"
                    && let Some(receipts) = snapshot.get("transformations").and_then(Value::as_array)
                {
                    export.transformations.extend(receipts.iter().filter_map(|receipt| {
                        if string(receipt, "mode") != "applied" {
                            return None;
                        }
                        let key = transformation_key(snapshot, receipt);
                        exported_transformations.insert(key).then(|| TransformationItem {
                            time: time.clone(),
                            kind: "applied reduction".to_string(),
                            detail: compact_json(receipt),
                        })
                    }));
                }
            }
            "compaction" => {
                let audit = record.get("audit").unwrap_or(&Value::Null);
                export.transformations.push(TransformationItem {
                    time,
                    kind: "compaction".to_string(),
                    detail: compact_json(audit),
                });
                if let Some(findings) = audit.pointer("/typed_summary/findings").and_then(Value::as_array) {
                    export
                        .findings
                        .extend(findings.iter().filter_map(Value::as_str).map(ToString::to_string));
                }
            }
            "context_pin" | "context_drop" | "context_recovery" | "context_lifecycle" | "compaction_review" => {
                export.transformations.push(TransformationItem {
                    time,
                    kind: record_type.to_string(),
                    detail: compact_json(record),
                });
            }
            _ => {}
        }
    }
    export.turn_count = turns.len();
    Ok(export)
}

fn transformation_key(request: &Value, receipt: &Value) -> String {
    format!(
        "{}#{}:{}",
        string(request, "request_id"),
        request.get("attempt").and_then(Value::as_u64).unwrap_or_default(),
        compact_json(receipt)
    )
}

impl SessionExport {
    /// Render a stable Markdown review copy.
    pub fn to_markdown(&self) -> String {
        let mut out = format!(
            "# {}\n\nSession: `{}`  \nStarted: {}  \nRuntime: `{}/{}`  \nStatus: {} · {} turns\n\n",
            self.title, self.id, self.started, self.provider, self.model, self.status, self.turn_count
        );
        out.push_str("## Lineage\n\n");
        if self.lineage.is_empty() {
            out.push_str("Root session.\n\n");
        } else {
            for item in &self.lineage {
                out.push_str(&format!("- `{}` at `{}`\n", item.session_id, item.turn_id));
            }
            out.push('\n');
        }
        out.push_str("## Conversation\n\n");
        for message in &self.messages {
            out.push_str(&format!(
                "### {} · `{}` · {}\n\n{}\n\n",
                message.role, message.turn_id, message.time, message.text
            ));
        }
        append_rows(
            &mut out,
            "Activity",
            self.activities.iter().map(|item| {
                format!(
                    "{} · {} · **{}** · {} · {}",
                    item.time, item.kind, item.name, item.detail, item.status
                )
            }),
        );
        append_rows(
            &mut out,
            "Artifacts and findings",
            self.artifacts
                .iter()
                .map(|item| format!("{} · `{}` · {}", item.kind, item.label, item.detail))
                .chain(self.findings.iter().map(|finding| format!("finding · {finding}"))),
        );
        append_rows(
            &mut out,
            "Request references",
            self.requests.iter().map(|item| {
                format!(
                    "{} · `{}` · {} · attempt {}",
                    item.time, item.request_id, item.detail, item.attempt
                )
            }),
        );
        append_rows(
            &mut out,
            "Context transformations",
            self.transformations
                .iter()
                .map(|item| format!("{} · {} · {}", item.time, item.kind, item.detail)),
        );
        if self.truncated {
            out.push_str("\n> Export item limit reached; later records were omitted.\n");
        }
        out
    }

    /// Render a self-contained HTML review copy from the bundled template.
    pub fn to_html(&self) -> io::Result<String> {
        let mut environment = Environment::new();
        environment.set_auto_escape_callback(|_| AutoEscape::Html);
        environment
            .add_template("session.html", SESSION_HTML_TEMPLATE)
            .map_err(io::Error::other)?;
        environment
            .get_template("session.html")
            .and_then(|template| template.render(minijinja::context! { session => self, schema => SCHEMA_VERSION }))
            .map_err(io::Error::other)
    }
}

fn append_rows(out: &mut String, heading: &str, rows: impl Iterator<Item = String>) {
    out.push_str(&format!("## {heading}\n\n"));
    let mut empty = true;
    for row in rows {
        empty = false;
        out.push_str(&format!("- {row}\n"));
    }
    if empty {
        out.push_str("None recorded.\n");
    }
    out.push('\n');
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn display_time(value: &str) -> String {
    value
        .split_once('T')
        .map_or(value, |(_, time)| time)
        .chars()
        .take(8)
        .collect()
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
