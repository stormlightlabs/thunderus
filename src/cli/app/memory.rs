//! `/memory` slash command handling.
//!
//! `/memory recall` is read-only and allowed while the agent is working.

use crate::memory::{RecallRequest, recall};

use super::{App, Entry};

/// Handle a `/memory` slash command.
///
/// Returns `Some(command)` when the command was recognized and handled (so the
/// caller clears input), or `None` when the command is unknown and should fall
/// through to the default unknown-command behavior.
///
/// Currently handles:
/// - `/memory recall <query>`: lexical recall over metadata + FTS5/BM25.
pub fn run_memory_command(app: &mut App, command: &str) -> Option<&'static str> {
    if let Some(query) = command.strip_prefix("memory recall ") {
        run_memory_recall(app, query.trim());
        return Some("memory recall");
    }
    if command == "memory recall" {
        app.transcript
            .push(Entry::Error { text: String::from("usage: /memory recall <query>") });
        return Some("memory recall");
    }
    None
}

/// Run `/memory recall <query>` and render the outcome to the transcript.
///
/// Searches both user and project memory roots, ordering core memory before
/// archival memory, and surfaces a useful diagnostic when nothing matches.
fn run_memory_recall(app: &mut App, query: &str) {
    if query.is_empty() {
        app.transcript
            .push(Entry::Error { text: String::from("usage: /memory recall <query>") });
        return;
    }

    let roots = &app.memory_roots;
    let cache_dir = roots
        .user
        .as_ref()
        .and_then(|root| root.parent())
        .map(|thndrs_dir| thndrs_dir.join("cache").join("memory"));
    let request = RecallRequest::new(query);
    let outcome = recall(roots, Some(&app.cwd), cache_dir.as_deref(), &request);
    let text = match &outcome.diagnostic {
        Some(diagnostic) => format!("memory recall  {diagnostic}"),
        None => {
            let mut lines = vec![format!(
                "memory recall  {} result(s) for {query:?}",
                outcome.results.len()
            )];
            for result in &outcome.results {
                lines.push(format!("  {}", result.summary()));
            }
            for warning in &outcome.warnings {
                lines.push(format!("  warning: {warning}"));
            }
            lines.join("\n")
        }
    };

    app.transcript.push(Entry::Status { text });
}
