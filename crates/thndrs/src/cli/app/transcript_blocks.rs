//! Stable semantic transcript blocks and validated tool lifecycles.
//!
//! This application module owns transcript identity, kinds, and mutable
//! lifecycle state. It does not decide how blocks look in a terminal; renderer
//! modules consume this state and own its visual projection.

use std::fmt;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Entry, ToolStatus};
use crate::tools::shell::redact_secrets;

const MAX_TARGET_CHARS: usize = 240;

/// Stable identity for one semantic transcript block.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TranscriptBlockId(String);

impl TranscriptBlockId {
    /// Return the identifier as text for projections and persistence adapters.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TranscriptBlockId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Semantic family owned by a transcript block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptBlockKind {
    UserPrompt,
    AssistantResponse,
    ReasoningSummary,
    ToolCall,
    Edit,
    Diff,
    Permission,
    ContextEvent,
    Status,
    Error,
    ChildActivity,
}

/// Valid lifecycle states for a tool block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecycleState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ToolLifecycleState {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running | Self::Cancelled)
                | (Self::Running, Self::Succeeded | Self::Failed | Self::Cancelled)
        )
    }

    fn from_status(status: ToolStatus) -> Self {
        match status {
            ToolStatus::Running => Self::Running,
            ToolStatus::Ok => Self::Succeeded,
            ToolStatus::Failed => Self::Failed,
            ToolStatus::Cancelled => Self::Cancelled,
        }
    }
}

/// Whether a bounded projection has semantic content to show.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockContentState {
    Unknown,
    Empty,
    Unchanged,
    Present,
    Truncated,
}

/// Rejected duplicate or out-of-order tool lifecycle update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLifecycleError {
    pub call_id: String,
    pub current: Option<ToolLifecycleState>,
    pub requested: ToolLifecycleState,
}

impl fmt::Display for ToolLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.current {
            Some(current) => write!(
                formatter,
                "invalid tool lifecycle transition for {}: {current:?} -> {:?}",
                self.call_id, self.requested
            ),
            None => write!(formatter, "unknown tool call {}", self.call_id),
        }
    }
}

impl std::error::Error for ToolLifecycleError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ToolBlockState {
    call_id: String,
    action: String,
    target: BlockContentState,
    target_text: Option<String>,
    state: ToolLifecycleState,
    result: BlockContentState,
}

/// Metadata and content for one transcript block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptBlock<'a> {
    pub id: &'a TranscriptBlockId,
    pub kind: TranscriptBlockKind,
    pub entry: &'a Entry,
    tool: Option<&'a ToolBlockState>,
}

impl TranscriptBlock<'_> {
    /// Tool action without provider-call identity suffixes.
    pub fn action(&self) -> Option<&str> {
        self.tool.map(|tool| tool.action.as_str())
    }

    /// Concise target extracted from structured tool arguments.
    pub fn target(&self) -> Option<&str> {
        self.tool.and_then(|tool| tool.target_text.as_deref())
    }

    pub fn target_state(&self) -> Option<BlockContentState> {
        self.tool.map(|tool| tool.target)
    }

    pub fn lifecycle(&self) -> Option<ToolLifecycleState> {
        self.tool.map(|tool| tool.state)
    }

    pub fn result_state(&self) -> Option<BlockContentState> {
        self.tool.map(|tool| tool.result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct BlockMetadata {
    id: TranscriptBlockId,
    kind: TranscriptBlockKind,
    tool: Option<ToolBlockState>,
}

/// Ordered semantic transcript storage.
///
/// Entry content remains available as a slice while block metadata owns stable
/// identity, explicit kind, and lifecycle invariants.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TranscriptBlocks {
    entries: Vec<Entry>,
    metadata: Vec<BlockMetadata>,
    next_id: u64,
}

impl TranscriptBlocks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: Entry) {
        let kind = kind_for_entry(&entry);
        let tool = tool_state_for_entry(&entry);
        self.push_metadata(entry, kind, tool, None);
    }

    pub fn pop(&mut self) -> Option<Entry> {
        self.metadata.pop()?;
        self.entries.pop()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.metadata.clear();
    }

    pub fn blocks(&self) -> impl DoubleEndedIterator<Item = TranscriptBlock<'_>> + ExactSizeIterator {
        self.entries
            .iter()
            .zip(&self.metadata)
            .map(|(entry, metadata)| TranscriptBlock {
                id: &metadata.id,
                kind: metadata.kind,
                entry,
                tool: metadata.tool.as_ref(),
            })
    }

    pub fn block(&self, index: usize) -> Option<TranscriptBlock<'_>> {
        let entry = self.entries.get(index)?;
        let metadata = self.metadata.get(index)?;
        Some(TranscriptBlock { id: &metadata.id, kind: metadata.kind, entry, tool: metadata.tool.as_ref() })
    }

    /// Return the entry owned by a tool call without relying on row position.
    pub fn tool_entry(&self, call_id: &str) -> Option<&Entry> {
        let index = self
            .metadata
            .iter()
            .position(|metadata| metadata.tool.as_ref().is_some_and(|tool| tool.call_id == call_id))?;
        self.entries.get(index)
    }

    /// Create a queued tool block. A repeated call id is rejected.
    pub fn queue_tool(&mut self, call_id: &str, action: &str, arguments: &str) -> Result<(), ToolLifecycleError> {
        if let Some(existing) = self.tool_metadata(call_id) {
            return Err(ToolLifecycleError {
                call_id: call_id.to_string(),
                current: Some(existing.state),
                requested: ToolLifecycleState::Queued,
            });
        }
        let (target, target_text) = extract_target(arguments);
        let entry = Entry::Tool {
            name: format!("{action}#{call_id}"),
            arguments: arguments.to_string(),
            status: ToolStatus::Running,
            output: Vec::new(),
        };
        let tool = ToolBlockState {
            call_id: call_id.to_string(),
            action: action.to_string(),
            target,
            target_text,
            state: ToolLifecycleState::Queued,
            result: BlockContentState::Unknown,
        };
        self.push_metadata(
            entry,
            TranscriptBlockKind::ToolCall,
            Some(tool),
            Some(TranscriptBlockId(format!("tool:{call_id}"))),
        );
        Ok(())
    }

    pub fn start_tool(&mut self, call_id: &str) -> Result<(), ToolLifecycleError> {
        self.transition_tool(call_id, ToolLifecycleState::Running, None)
    }

    pub fn finish_tool(
        &mut self, call_id: &str, status: ToolStatus, output: Vec<String>, truncated: bool,
    ) -> Result<(), ToolLifecycleError> {
        let requested = ToolLifecycleState::from_status(status);
        if requested == ToolLifecycleState::Running {
            return Err(ToolLifecycleError {
                call_id: call_id.to_string(),
                current: self.tool_metadata(call_id).map(|tool| tool.state),
                requested,
            });
        }
        self.transition_tool(call_id, requested, Some((status, output, truncated)))
    }

    pub fn cancel_running_tools(&mut self) {
        let call_ids = self
            .metadata
            .iter()
            .filter_map(|metadata| {
                let tool = metadata.tool.as_ref()?;
                matches!(tool.state, ToolLifecycleState::Queued | ToolLifecycleState::Running)
                    .then(|| tool.call_id.clone())
            })
            .collect::<Vec<_>>();
        for call_id in call_ids {
            let _ = self.transition_tool(
                &call_id,
                ToolLifecycleState::Cancelled,
                Some((ToolStatus::Cancelled, Vec::new(), false)),
            );
        }
    }

    pub fn push_permission(&mut self, call_id: &str, text: String) {
        let id = TranscriptBlockId(format!("permission:{call_id}"));
        if let Some(index) = self.metadata.iter().position(|metadata| metadata.id == id) {
            self.entries[index] = Entry::Status { text };
            return;
        }
        self.push_metadata(Entry::Status { text }, TranscriptBlockKind::Permission, None, Some(id));
    }

    pub fn resolve_permission(&mut self, call_id: &str, text: String) -> bool {
        let id = format!("permission:{call_id}");
        let Some(index) = self.metadata.iter().position(|metadata| metadata.id.as_str() == id) else {
            return false;
        };
        self.entries[index] = Entry::Status { text };
        true
    }

    pub fn push_child_activity(&mut self, process_id: u64, text: String) {
        let id = TranscriptBlockId(format!("child:{process_id}"));
        if let Some(index) = self.metadata.iter().position(|metadata| metadata.id == id) {
            self.entries[index] = Entry::Status { text };
            return;
        }
        self.push_metadata(
            Entry::Status { text },
            TranscriptBlockKind::ChildActivity,
            None,
            Some(id),
        );
    }

    /// Add a dimmed semantic context event with durable identity.
    pub fn push_context_event(&mut self, id: String, text: String) {
        self.push_metadata(
            Entry::Status { text },
            TranscriptBlockKind::ContextEvent,
            None,
            Some(TranscriptBlockId(id)),
        );
    }

    fn transition_tool(
        &mut self, call_id: &str, requested: ToolLifecycleState, completion: Option<(ToolStatus, Vec<String>, bool)>,
    ) -> Result<(), ToolLifecycleError> {
        let Some(index) = self
            .metadata
            .iter()
            .position(|metadata| metadata.tool.as_ref().is_some_and(|tool| tool.call_id == call_id))
        else {
            return Err(ToolLifecycleError { call_id: call_id.to_string(), current: None, requested });
        };
        let Some(tool) = self.metadata[index].tool.as_mut() else {
            return Err(ToolLifecycleError { call_id: call_id.to_string(), current: None, requested });
        };
        if !tool.state.can_transition_to(requested) {
            return Err(ToolLifecycleError { call_id: call_id.to_string(), current: Some(tool.state), requested });
        }
        tool.state = requested;
        if let Some((status, output, truncated)) = completion {
            tool.result = result_state(&output, truncated);
            if let Entry::Tool { status: entry_status, output: entry_output, .. } = &mut self.entries[index] {
                *entry_status = status;
                *entry_output = output;
            }
            self.metadata[index].kind = completed_tool_kind(&tool.action, &self.entries[index]);
        }
        Ok(())
    }

    fn tool_metadata(&self, call_id: &str) -> Option<&ToolBlockState> {
        self.metadata
            .iter()
            .find_map(|metadata| metadata.tool.as_ref().filter(|tool| tool.call_id == call_id))
    }

    fn push_metadata(
        &mut self, entry: Entry, kind: TranscriptBlockKind, tool: Option<ToolBlockState>, id: Option<TranscriptBlockId>,
    ) {
        let id = id.unwrap_or_else(|| {
            self.next_id = self.next_id.saturating_add(1);
            TranscriptBlockId(format!("block:{}", self.next_id))
        });
        self.entries.push(entry);
        self.metadata.push(BlockMetadata { id, kind, tool });
    }
}

impl From<Vec<Entry>> for TranscriptBlocks {
    fn from(entries: Vec<Entry>) -> Self {
        let mut blocks = Self::new();
        for entry in entries {
            blocks.push(entry);
        }
        blocks
    }
}

impl FromIterator<Entry> for TranscriptBlocks {
    fn from_iter<T: IntoIterator<Item = Entry>>(iter: T) -> Self {
        Self::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl PartialEq<Vec<Entry>> for TranscriptBlocks {
    fn eq(&self, other: &Vec<Entry>) -> bool {
        self.entries == *other
    }
}

impl Deref for TranscriptBlocks {
    type Target = [Entry];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for TranscriptBlocks {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

impl<'a> IntoIterator for &'a TranscriptBlocks {
    type Item = &'a Entry;
    type IntoIter = std::slice::Iter<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<'a> IntoIterator for &'a mut TranscriptBlocks {
    type Item = &'a mut Entry;
    type IntoIter = std::slice::IterMut<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter_mut()
    }
}

fn kind_for_entry(entry: &Entry) -> TranscriptBlockKind {
    match entry {
        Entry::User { .. } => TranscriptBlockKind::UserPrompt,
        Entry::Agent { .. } => TranscriptBlockKind::AssistantResponse,
        Entry::Reasoning { .. } => TranscriptBlockKind::ReasoningSummary,
        Entry::Tool { name, .. } => completed_tool_kind(name, entry),
        Entry::Status { .. } => TranscriptBlockKind::Status,
        Entry::Error { .. } => TranscriptBlockKind::Error,
    }
}

fn completed_tool_kind(action: &str, entry: &Entry) -> TranscriptBlockKind {
    let Entry::Tool { output, .. } = entry else {
        return TranscriptBlockKind::ToolCall;
    };
    if output
        .iter()
        .any(|line| line.starts_with("diff --git") || line.starts_with("@@"))
    {
        TranscriptBlockKind::Diff
    } else if ["create_file", "replace_range", "write_patch"]
        .iter()
        .any(|name| action.starts_with(name))
    {
        TranscriptBlockKind::Edit
    } else {
        TranscriptBlockKind::ToolCall
    }
}

fn tool_state_for_entry(entry: &Entry) -> Option<ToolBlockState> {
    let Entry::Tool { name, arguments, status, output } = entry else {
        return None;
    };
    let (action, call_id) = name.rsplit_once('#').unwrap_or((name, name));
    let (target, target_text) = extract_target(arguments);
    Some(ToolBlockState {
        call_id: call_id.to_string(),
        action: action.to_string(),
        target,
        target_text,
        state: ToolLifecycleState::from_status(*status),
        result: if *status == ToolStatus::Running {
            BlockContentState::Unknown
        } else {
            result_state(output, false)
        },
    })
}

fn extract_target(arguments: &str) -> (BlockContentState, Option<String>) {
    if arguments.trim().is_empty() {
        return (BlockContentState::Empty, None);
    }
    let Ok(Value::Object(arguments)) = serde_json::from_str::<Value>(arguments) else {
        return (BlockContentState::Unknown, None);
    };
    for key in ["path", "file_path", "query", "pattern", "url", "command"] {
        if let Some(value) = arguments.get(key) {
            let text = value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
            return if text.is_empty() {
                (BlockContentState::Empty, None)
            } else {
                (BlockContentState::Present, Some(bounded_target(&text)))
            };
        }
    }
    (BlockContentState::Unknown, None)
}

fn bounded_target(target: &str) -> String {
    let redacted = redact_secrets(target);
    let mut chars = redacted.chars();
    let bounded = chars.by_ref().take(MAX_TARGET_CHARS).collect::<String>();
    if chars.next().is_some() { format!("{bounded}…") } else { bounded }
}

fn result_state(output: &[String], truncated: bool) -> BlockContentState {
    if truncated {
        return BlockContentState::Truncated;
    }
    if output.is_empty() || output.iter().all(|line| line.is_empty()) {
        return BlockContentState::Empty;
    }
    if output.iter().all(|line| {
        let line = line.to_ascii_lowercase();
        line.contains("unchanged") || line.contains("no changes") || line.contains("already up to date")
    }) {
        return BlockContentState::Unchanged;
    }
    BlockContentState::Present
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_tool_lifecycle_and_rejects_duplicates() {
        let mut transcript = TranscriptBlocks::new();
        transcript
            .queue_tool("call-1", "search", r#"{"query":"needle"}"#)
            .unwrap();
        assert_eq!(
            transcript.block(0).unwrap().lifecycle(),
            Some(ToolLifecycleState::Queued)
        );
        transcript.start_tool("call-1").unwrap();
        transcript
            .finish_tool("call-1", ToolStatus::Ok, vec!["one result".to_string()], false)
            .unwrap();

        let duplicate = transcript
            .finish_tool("call-1", ToolStatus::Ok, vec!["one result".to_string()], false)
            .unwrap_err();
        assert_eq!(duplicate.current, Some(ToolLifecycleState::Succeeded));
        assert_eq!(transcript.block(0).unwrap().target(), Some("needle"));
        assert_eq!(
            transcript.block(0).unwrap().result_state(),
            Some(BlockContentState::Present)
        );
    }

    #[test]
    fn distinguishes_unknown_empty_unchanged_and_truncated_content() {
        let mut transcript = TranscriptBlocks::new();
        transcript.queue_tool("unknown", "custom", "not-json").unwrap();
        assert_eq!(
            transcript.block(0).unwrap().target_state(),
            Some(BlockContentState::Unknown)
        );
        let out_of_order = transcript
            .finish_tool("unknown", ToolStatus::Ok, Vec::new(), false)
            .unwrap_err();
        assert_eq!(out_of_order.current, Some(ToolLifecycleState::Queued));
        transcript.start_tool("unknown").unwrap();
        transcript
            .finish_tool("unknown", ToolStatus::Ok, Vec::new(), false)
            .unwrap();
        assert_eq!(
            transcript.block(0).unwrap().result_state(),
            Some(BlockContentState::Empty)
        );

        transcript.queue_tool("same", "read", "{}").unwrap();
        transcript.start_tool("same").unwrap();
        transcript
            .finish_tool("same", ToolStatus::Ok, vec!["unchanged".to_string()], false)
            .unwrap();
        assert_eq!(
            transcript.block(1).unwrap().result_state(),
            Some(BlockContentState::Unchanged)
        );

        transcript.queue_tool("long", "compiler", "{}").unwrap();
        transcript.start_tool("long").unwrap();
        transcript
            .finish_tool("long", ToolStatus::Failed, vec!["error".to_string()], true)
            .unwrap();
        assert_eq!(
            transcript.block(2).unwrap().result_state(),
            Some(BlockContentState::Truncated)
        );
    }

    #[test]
    fn block_metadata_serializes_round_trip() {
        let mut transcript = TranscriptBlocks::new();
        transcript.push(Entry::User { text: "hello".to_string() });
        transcript
            .queue_tool("call-1", "read", r#"{"path":"src/lib.rs"}"#)
            .unwrap();
        let json = serde_json::to_string(&transcript).unwrap();
        let restored: TranscriptBlocks = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, transcript);
    }

    #[test]
    fn child_activity_replaces_the_stable_process_block() {
        let mut transcript = TranscriptBlocks::new();
        transcript.push_child_activity(7, "running".to_string());
        transcript.push_child_activity(7, "succeeded".to_string());

        assert_eq!(transcript.len(), 1);
        assert_eq!(transcript.block(0).unwrap().id.as_str(), "child:7");
        assert_eq!(transcript[0], Entry::Status { text: "succeeded".to_string() });
    }

    #[test]
    fn target_projection_is_bounded_and_redacted() {
        let mut transcript = TranscriptBlocks::new();
        let target = format!("password=very-secret-value {}", "x".repeat(MAX_TARGET_CHARS));
        transcript
            .queue_tool("safe", "shell", &serde_json::json!({ "command": target }).to_string())
            .unwrap();
        let target = transcript.block(0).unwrap().target().unwrap().to_string();

        assert!(target.contains("password=[REDACTED]"));
        assert!(target.ends_with('…'));
        assert!(target.chars().count() <= MAX_TARGET_CHARS + 1);
    }
}
