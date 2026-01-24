use crate::MemoryDoc;
use crate::error::{Error, Result};
use crate::session::events::{Event, LoggedEvent};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

/// A node in the trajectory graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryNode {
    /// The original logged event
    pub event: LoggedEvent,
    /// derived impact score or label
    pub impact: String,
    /// IDs of downstream events (causal links)
    pub causal_links: Vec<u64>,
}

/// Utility to walk backwards from a memory document to its source events
pub struct TrajectoryWalker {
    /// Directory containing session logs
    session_dir: PathBuf,
}

impl TrajectoryWalker {
    pub fn new(session_dir: PathBuf) -> Self {
        Self { session_dir }
    }

    /// Walk the trajectory for a given memory document
    pub async fn walk(&self, doc: &MemoryDoc) -> Result<Vec<TrajectoryNode>> {
        let event_ids = &doc.frontmatter.provenance.events;
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }

        let events_path = self.session_dir.join("events.jsonl");
        if !events_path.exists() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Session events log not found at {:?}. Trajectory cannot be reconstructed without events.jsonl.",
                    events_path
                ),
            )));
        }

        let requested_seqs: Vec<u64> = event_ids
            .iter()
            .filter_map(|id| id.strip_prefix("evt_"))
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();

        let mut trajectory = Vec::new();

        let file = File::open(&events_path).await.map_err(Error::Io)?;
        let mut reader = BufReader::new(file).lines();

        while let Some(line) = reader.next_line().await.map_err(Error::Io)? {
            let logged: LoggedEvent =
                serde_json::from_str(&line).map_err(|e| Error::Parse(format!("Failed to parse event: {}", e)))?;

            if requested_seqs.contains(&logged.seq) {
                let impact = self.derive_impact(&logged.event);
                let causal_links = self.resolve_causal_links(&logged, &events_path).await?;

                trajectory.push(TrajectoryNode { event: logged, impact, causal_links });
            }
        }

        trajectory.sort_by_key(|n| n.event.seq);

        Ok(trajectory)
    }

    fn derive_impact(&self, event: &Event) -> String {
        match event {
            Event::UserMessage { .. } => "Input".to_string(),
            Event::ToolResult { success: true, .. } => "Evidence".to_string(),
            Event::ToolResult { success: false, .. } => "Failure".to_string(),
            Event::Patch { .. } => "Change".to_string(),
            Event::Approval { approved: true, .. } => "Verification".to_string(),
            Event::Approval { approved: false, .. } => "Correction".to_string(),
            Event::ViewEdit { .. } => "Human Intervention".to_string(),
            _ => "Context".to_string(),
        }
    }

    /// Resolve causal links for an event by scanning for related events
    async fn resolve_causal_links(&self, node: &LoggedEvent, events_path: &std::path::Path) -> Result<Vec<u64>> {
        let mut links = Vec::new();

        match &node.event {
            Event::ToolCall { .. } => {
                let file = File::open(events_path).await.map_err(Error::Io)?;
                let mut reader = BufReader::new(file).lines();
                while let Some(line) = reader.next_line().await.map_err(Error::Io)? {
                    let logged: LoggedEvent = serde_json::from_str(&line)
                        .map_err(|e| Error::Parse(format!("Failed to parse event: {}", e)))?;

                    if logged.seq > node.seq
                        && let Event::ToolResult { tool, .. } = &logged.event
                        && let Event::ToolCall { tool: call_tool, .. } = &node.event
                        && tool == call_tool
                    {
                        links.push(logged.seq);
                        break;
                    }
                }
            }
            Event::Patch { .. } => {
                let file = File::open(events_path).await.map_err(Error::Io)?;
                let mut reader = BufReader::new(file).lines();
                let mut last_tool_call_seq = None;

                while let Some(line) = reader.next_line().await.map_err(Error::Io)? {
                    let logged: LoggedEvent = serde_json::from_str(&line)
                        .map_err(|e| Error::Parse(format!("Failed to parse event: {}", e)))?;

                    if logged.seq < node.seq {
                        if let Event::ToolCall { .. } = &logged.event {
                            last_tool_call_seq = Some(logged.seq);
                        }
                    } else {
                        break;
                    }
                }
                if let Some(seq) = last_tool_call_seq {
                    links.push(seq);
                }
            }
            _ => {}
        }

        Ok(links)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryKind;
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_trajectory_walk_basic() -> Result<()> {
        let dir = tempdir().map_err(Error::Io)?;
        let events_path = dir.path().join("events.jsonl");

        let mut file = File::create(&events_path).await.map_err(Error::Io)?;

        let events = vec![
            LoggedEvent {
                seq: 0,
                session_id: "test-session".to_string(),
                timestamp: "2026-01-23T22:20:00Z".to_string(),
                event: Event::UserMessage { content: "Hello".to_string() },
            },
            LoggedEvent {
                seq: 1,
                session_id: "test-session".to_string(),
                timestamp: "2026-01-23T22:21:00Z".to_string(),
                event: Event::ToolResult {
                    tool: "ls".to_string(),
                    result: serde_json::json!({"files": []}),
                    success: true,
                    error: None,
                },
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            file.write_all(json.as_bytes()).await.map_err(Error::Io)?;
            file.write_all(b"\n").await.map_err(Error::Io)?;
        }
        file.flush().await.map_err(Error::Io)?;

        let mut doc = MemoryDoc::new(
            "fact.test",
            "Test Fact",
            MemoryKind::Fact,
            vec!["test".to_string()],
            "Body",
        );
        doc.add_provenance_event("evt_0");
        doc.add_provenance_event("evt_1");

        let walker = TrajectoryWalker::new(dir.path().to_path_buf());
        let trajectory = walker.walk(&doc).await?;

        assert_eq!(trajectory.len(), 2);
        assert_eq!(trajectory[0].event.seq, 0);
        assert_eq!(trajectory[0].impact, "Input");
        assert_eq!(trajectory[1].event.seq, 1);
        assert_eq!(trajectory[1].impact, "Evidence");

        Ok(())
    }

    #[tokio::test]
    async fn test_trajectory_causal_links() -> Result<()> {
        let dir = tempdir().map_err(Error::Io)?;
        let events_path = dir.path().join("events.jsonl");

        let mut file = File::create(&events_path).await.map_err(Error::Io)?;

        let events = vec![
            LoggedEvent {
                seq: 0,
                session_id: "test".to_string(),
                timestamp: "T1".to_string(),
                event: Event::ToolCall { tool: "edit".to_string(), arguments: serde_json::json!({}) },
            },
            LoggedEvent {
                seq: 1,
                session_id: "test".to_string(),
                timestamp: "T2".to_string(),
                event: Event::Patch {
                    name: "test".to_string(),
                    status: crate::session::events::PatchStatus::Applied,
                    files: vec!["f1.rs".to_string()],
                    diff: "".to_string(),
                },
            },
            LoggedEvent {
                seq: 2,
                session_id: "test".to_string(),
                timestamp: "T3".to_string(),
                event: Event::ToolResult {
                    tool: "edit".to_string(),
                    result: serde_json::json!({}),
                    success: true,
                    error: None,
                },
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            file.write_all(json.as_bytes()).await.map_err(Error::Io)?;
            file.write_all(b"\n").await.map_err(Error::Io)?;
        }
        file.flush().await.map_err(Error::Io)?;

        let mut doc = MemoryDoc::new("f.t", "T", crate::MemoryKind::Fact, vec![], "B");
        doc.add_provenance_event("evt_0");
        doc.add_provenance_event("evt_1");
        doc.add_provenance_event("evt_2");

        let walker = TrajectoryWalker::new(dir.path().to_path_buf());
        let trajectory = walker.walk(&doc).await?;

        assert_eq!(trajectory[0].event.seq, 0);
        assert_eq!(trajectory[0].causal_links, vec![2]);

        assert_eq!(trajectory[1].event.seq, 1);
        assert_eq!(trajectory[1].causal_links, vec![0]);

        Ok(())
    }
}
