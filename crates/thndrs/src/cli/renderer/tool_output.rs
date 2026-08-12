//! Semantic projection of tool output into renderer-owned content kinds.

use super::diff::UnifiedDiff;

/// Maximum input accepted by the structured diff projection.
const MAX_DIFF_LINES: usize = 2_000;
const MAX_DIFF_BYTES: usize = 128 * 1024;
const MAX_DIFF_LINE_BYTES: usize = 4 * 1024;

/// Structured content recognized by the transcript renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentKind {
    Plain,
    Code { language: &'static str },
    Diff(UnifiedDiff),
    SearchResults,
}

/// Project a tool result into the narrow set of structures the renderer knows.
pub fn project(tool_name: &str, arguments: &str, output: &[String]) -> ContentKind {
    if let Some(diff) = projected_diff(tool_name, output) {
        return ContentKind::Diff(diff);
    }

    if tool_name.split('#').next() == Some("search_text") {
        return ContentKind::SearchResults;
    }

    super::highlight::tool_output_language(tool_name, arguments)
        .map_or(ContentKind::Plain, |language| ContentKind::Code { language })
}

pub(crate) fn projected_diff(tool_name: &str, output: &[String]) -> Option<UnifiedDiff> {
    if output.len() > MAX_DIFF_LINES
        || output.iter().any(|line| line.len() > MAX_DIFF_LINE_BYTES)
        || output.iter().map(String::len).sum::<usize>() > MAX_DIFF_BYTES
    {
        return None;
    }
    let operation = tool_name.split('#').next().unwrap_or(tool_name);
    let lines = output;
    UnifiedDiff::parse(lines).or_else(|| {
        let (_, diff) = lines
            .split_first()
            .filter(|_| matches!(operation, "create_file" | "replace_range" | "write_patch"))?;
        UnifiedDiff::parse(diff)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_known_source_reads_but_not_shell_output() {
        assert_eq!(
            project("read_file_range", r#"{"path":"src/main.rs"}"#, &[]),
            ContentKind::Code { language: "rs" }
        );
        assert_eq!(
            project("run_shell", r#"{"program":"cargo test"}"#, &[]),
            ContentKind::Plain
        );
    }

    #[test]
    fn projects_search_results_by_tool_contract() {
        assert_eq!(project("search_text", "{}", &[]), ContentKind::SearchResults);
        assert_eq!(project("sawk", "{}", &[]), ContentKind::Plain);
    }

    #[test]
    fn projects_edit_tool_diff_after_its_summary() {
        let output = [
            "edited /workspace/src/lib.rs (from 3 bytes → 3 bytes)".to_string(),
            "--- a/src/lib.rs".to_string(),
            "+++ b/src/lib.rs".to_string(),
            "@@ -1 +1 @@".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ];

        assert!(matches!(project("write_patch", "{}", &output), ContentKind::Diff(_)));
    }

    #[test]
    fn rejects_unusually_large_diff_projection() {
        let mut output = vec![
            "--- a/src/lib.rs".to_string(),
            "+++ b/src/lib.rs".to_string(),
            "@@ -1 +1 @@".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ];
        output.extend((0..MAX_DIFF_LINES).map(|_| "ordinary output".to_string()));

        assert_eq!(project("run_shell", "{}", &output), ContentKind::Plain);
    }

    #[test]
    fn rejects_mixed_and_oversized_line_diff_projection() {
        let mixed = [
            "warning: output may be incomplete".to_string(),
            "--- a/file".to_string(),
            "+++ b/file".to_string(),
            "@@ -1 +1 @@".to_string(),
            "-old".to_string(),
            "+new".to_string(),
        ];
        assert_eq!(project("run_shell", "{}", &mixed), ContentKind::Plain);

        let oversized = [
            "--- a/file".to_string(),
            "+++ b/file".to_string(),
            "@@ -1 +1 @@".to_string(),
            format!("-{}", "a".repeat(MAX_DIFF_LINE_BYTES)),
            "+new".to_string(),
        ];
        assert_eq!(project("run_shell", "{}", &oversized), ContentKind::Plain);
    }
}
