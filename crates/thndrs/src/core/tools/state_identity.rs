//! Application-owned state fingerprints for eligible tool projections.
//!
//! The generic reducer in `thndrs-agent` only compares opaque source and
//! fingerprint values. This module defines what those values mean for local
//! tools, keeping repository and process details out of provider-neutral APIs.

use std::path::Path;

use thndrs_agent::context::StateProjectionIdentity;

use super::{ToolOutput, ToolUseRequest, hash_content, sorted_json_value};

/// Return a conservative state identity for a tool result when its adapter can
/// prove the relevant observed state.
///
/// File reads use the requested path/range plus a hash of the untruncated
/// range content. Workspace searches use normalized query arguments, workspace
/// identity, and the complete raw `rg` output hash. Shell commands carry a
/// monotonic freshness epoch because arbitrary commands may mutate workspace
/// or environment state outside the typed write-tool boundary.
pub fn identity_for(
    request: &ToolUseRequest, output: &ToolOutput, root: &Path, freshness_epoch: u64,
) -> Option<StateProjectionIdentity> {
    match request.name.as_str() {
        "read_file_range" => file_read_identity(request, output),
        "search_text" => workspace_search_identity(request, output, root),
        "run_shell" => command_identity(request, output, root, freshness_epoch),
        _ => None,
    }
}

fn file_read_identity(request: &ToolUseRequest, output: &ToolOutput) -> Option<StateProjectionIdentity> {
    let arguments = parse_object(&request.arguments)?;
    let path = arguments.get("path")?.as_str()?.trim();
    let start = arguments.get("start_line")?.as_u64()?;
    let end = arguments.get("end_line").and_then(serde_json::Value::as_u64);
    let range = end.map_or_else(|| format!("{start}:default"), |end| format!("{start}:{end}"));
    let fingerprint = output.evidence.content_hash.as_deref()?;
    StateProjectionIdentity::new(format!("file_read:{path}:{range}"), fingerprint)
}

fn workspace_search_identity(
    request: &ToolUseRequest, output: &ToolOutput, root: &Path,
) -> Option<StateProjectionIdentity> {
    let arguments = normalized_arguments(&request.arguments)?;
    let fingerprint = output.evidence.content_hash.as_deref()?;
    StateProjectionIdentity::new(
        format!("workspace_search:{}:{arguments}", workspace_label(root)),
        fingerprint,
    )
}

fn command_identity(
    request: &ToolUseRequest, output: &ToolOutput, root: &Path, freshness_epoch: u64,
) -> Option<StateProjectionIdentity> {
    let arguments = normalized_arguments(&request.arguments)?;
    let content_hash = output
        .evidence
        .content_hash
        .clone()
        .unwrap_or_else(|| format!("{:016x}", hash_content(&output.model_lines().join("\n"))));
    StateProjectionIdentity::new(
        format!("command:{}:{arguments}", workspace_label(root)),
        format!("epoch:{freshness_epoch}:result:{content_hash}"),
    )
}

fn parse_object(arguments: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .as_object()
        .cloned()
}

fn normalized_arguments(arguments: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    serde_json::to_string(&sorted_json_value(&value)).ok()
}

fn workspace_label(root: &Path) -> String {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolOutput;

    fn request(name: &str, arguments: &serde_json::Value) -> ToolUseRequest {
        ToolUseRequest::new(name, arguments.to_string(), "call")
    }

    #[test]
    fn file_reads_require_path_range_and_content_hash() {
        let output = ToolOutput::ok("read_file_range", vec!["1: use std::fmt;".to_string()])
            .with_evidence_content_hash("content-a");
        let identity = identity_for(
            &request(
                "read_file_range",
                &serde_json::json!({"path":"src/lib.rs","start_line":1,"end_line":1}),
            ),
            &output,
            Path::new("/workspace"),
            0,
        )
        .expect("identity");

        assert_eq!(identity.source(), "file_read:src/lib.rs:1:1");
        assert_eq!(identity.fingerprint(), "content-a");
    }

    #[test]
    fn searches_bind_normalized_arguments_to_workspace_result_state() {
        let output = ToolOutput::ok("search_text", vec!["src/lib.rs:1:needle".to_string()])
            .with_evidence_content_hash("rg-output-a");
        let root = Path::new(".");
        let first = identity_for(
            &request(
                "search_text",
                &serde_json::json!({"glob":"src/**/*.rs","pattern":"needle"}),
            ),
            &output,
            root,
            0,
        )
        .expect("identity");
        let reordered = identity_for(
            &request(
                "search_text",
                &serde_json::json!({"pattern":"needle","glob":"src/**/*.rs"}),
            ),
            &output,
            root,
            0,
        )
        .expect("identity");

        assert_eq!(first, reordered);
    }

    #[test]
    fn commands_are_fresh_after_a_state_epoch_change() {
        let output = ToolOutput::ok("run_shell", vec!["ok".to_string()]);
        let request = request("run_shell", &serde_json::json!({"argv":["cargo","test"]}));
        let before = identity_for(&request, &output, Path::new("."), 3).expect("identity");
        let after = identity_for(&request, &output, Path::new("."), 4).expect("identity");

        assert_eq!(before.source(), after.source());
        assert_ne!(before.fingerprint(), after.fingerprint());
    }
}
