//! ACP filesystem callbacks.

use std::path::{Path, PathBuf};

use crate::app::ToolStatus;
use crate::tools::{self, WriteOp, WriteResult, hash_content};

/// Successful ACP read callback output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpReadResult {
    /// File content returned to the ACP agent.
    pub content: String,
    /// Transcript lines describing the request.
    pub output: Vec<String>,
}

/// Successful ACP write callback output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpWriteResult {
    /// Transcript lines describing the request.
    pub output: Vec<String>,
    /// File-write audit metadata.
    pub write_result: WriteResult,
}

/// Read a workspace-contained UTF-8 text file for an ACP agent.
pub fn read_text_file(
    path: &Path, root: &Path, line: Option<u32>, limit: Option<u32>,
) -> Result<AcpReadResult, String> {
    let resolved = resolve_existing_file(path, root)?;
    let bytes = std::fs::read(&resolved).map_err(|err| format!("read failed: {err}"))?;
    if bytes.len() > tools::MAX_OUTPUT_BYTES {
        return Err(format!(
            "read denied: file is {} bytes, exceeding {} byte cap",
            bytes.len(),
            tools::MAX_OUTPUT_BYTES
        ));
    }
    let content = String::from_utf8(bytes).map_err(|_| "read denied: file is not valid UTF-8".to_string())?;
    let content = select_lines(&content, line, limit);
    let output = vec![format!("read {} ({} bytes)", resolved.display(), content.len())];
    Ok(AcpReadResult { content, output })
}

/// Write a workspace-contained UTF-8 text file for an ACP agent.
pub fn write_text_file(path: &Path, root: &Path, content: &str) -> Result<AcpWriteResult, String> {
    let resolved = tools::resolve_workspace_path(root, path).map_err(|err| err.to_string())?;
    if let Ok(metadata) = std::fs::metadata(&resolved)
        && metadata.is_dir()
    {
        return Err(format!("write denied: {} is a directory", resolved.display()));
    }

    let before = match std::fs::read_to_string(&resolved) {
        Ok(existing) => Some(existing),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("write denied: existing file is not readable UTF-8: {err}")),
    };

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("failed to create parent directories: {err}"))?;
    }
    std::fs::write(&resolved, content).map_err(|err| format!("write failed: {err}"))?;

    let result = WriteResult {
        op: if before.is_some() { WriteOp::Replace } else { WriteOp::Create },
        path: resolved,
        before_hash: before.as_ref().map(|text| hash_content(text)),
        before_bytes: before.as_ref().map(|text| text.len()),
        after_hash: hash_content(content),
        after_bytes: content.len(),
    };
    Ok(AcpWriteResult { output: vec![result.summary()], write_result: result })
}

/// Build a failed tool event payload for ACP filesystem callbacks.
pub fn failed_output(message: &str) -> (Vec<String>, ToolStatus) {
    (vec![message.to_string()], ToolStatus::Failed)
}

fn resolve_existing_file(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let resolved = tools::resolve_workspace_path(root, path).map_err(|err| err.to_string())?;
    let metadata = std::fs::metadata(&resolved).map_err(|err| format!("read denied: {err}"))?;
    if metadata.is_dir() {
        return Err(format!("read denied: {} is a directory", resolved.display()));
    }
    Ok(resolved)
}

fn select_lines(content: &str, line: Option<u32>, limit: Option<u32>) -> String {
    let Some(line) = line else {
        return content.to_string();
    };
    let start = line.max(1) as usize;
    let limit = limit.unwrap_or(u32::MAX).max(1) as usize;
    content
        .lines()
        .skip(start.saturating_sub(1))
        .take(limit)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_text_file_ok() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").expect("write");

        let result = read_text_file(&dir.path().join("a.txt"), dir.path(), Some(2), Some(1)).expect("read");

        assert_eq!(result.content, "two");
    }

    #[test]
    fn write_text_file_ok() {
        let dir = tempfile::tempdir().expect("temp dir");

        let result = write_text_file(&dir.path().join("a.txt"), dir.path(), "hello").expect("write");

        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).expect("read"),
            "hello"
        );
        assert_eq!(result.write_result.op, WriteOp::Create);
    }

    #[test]
    fn traversal_denied() {
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = dir.path().parent().expect("parent").join("outside.txt");

        let err = read_text_file(&outside, dir.path(), None, None).unwrap_err();

        assert!(err.contains("escapes workspace root"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_denied() {
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret.txt"), "secret").expect("write");
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), dir.path().join("link.txt")).expect("symlink");

        let err = read_text_file(&dir.path().join("link.txt"), dir.path(), None, None).unwrap_err();

        assert!(err.contains("escapes workspace root"));
    }

    #[test]
    fn oversized_read_denied() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("big.txt"), "x".repeat(tools::MAX_OUTPUT_BYTES + 1)).expect("write");

        let err = read_text_file(&dir.path().join("big.txt"), dir.path(), None, None).unwrap_err();

        assert!(err.contains("exceeding"));
    }

    #[test]
    fn non_utf8_read_denied() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("bin.dat"), [0xff, 0xfe]).expect("write");

        let err = read_text_file(&dir.path().join("bin.dat"), dir.path(), None, None).unwrap_err();

        assert!(err.contains("UTF-8"));
    }
}
