//! Application-owned projections for completed command results.
//!
//! The provider-neutral reducer layer intentionally cannot interpret shell
//! process metadata. This module does so only after a command has completed,
//! retaining operational facts and a recovery reference without
//! changing the command that was executed.

use std::collections::BTreeSet;

use thndrs_agent::{ContextReductionMode, ContextReductionReceipt, context::ReductionConfig, measure_lines};

use super::shell::{ProcessResult, redact_secrets};

/// Stable method name written to request-accounting receipts.
pub const COMMAND_RESULT_PROJECTION_METHOD: &str = "command_result_projection";
/// Stable version for the command-result projection format.
pub const COMMAND_RESULT_PROJECTION_VERSION: &str = "command-result-projection-v1";

const MAX_COMMAND_PROJECTION_BYTES: usize = 32 * 1024;
const MAX_COMMAND_DISPLAY_BYTES: usize = 1024;
const MAX_EVIDENCE_LINE_BYTES: usize = 512;
const MAX_EVIDENCE_LINES_PER_CATEGORY: usize = 12;

/// One applied or shadowed command-result projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandProjection {
    /// Model-facing lines after this command-specific projection.
    pub lines: Vec<String>,
    /// Exact reduction receipt using the shared accounting contract.
    pub receipt: ContextReductionReceipt,
}

/// Build a structured command receipt when the command reducer is enabled or
/// shadowed. The caller keeps `baseline` active for a shadow receipt.
pub fn project(
    item_id: &str, baseline: &[String], result: &ProcessResult, recovery_handle: &str, config: &ReductionConfig,
) -> Option<CommandProjection> {
    if !config.command_result && !config.shadow {
        return None;
    }

    let candidate = render(result, recovery_handle);
    let mode = if config.command_result { ContextReductionMode::Applied } else { ContextReductionMode::Shadow };
    Some(CommandProjection {
        lines: candidate.clone(),
        receipt: ContextReductionReceipt {
            item_id: item_id.to_string(),
            method: COMMAND_RESULT_PROJECTION_METHOD.to_string(),
            version: COMMAND_RESULT_PROJECTION_VERSION.to_string(),
            before_bytes: measure_lines(baseline),
            after_bytes: measure_lines(&candidate),
            lossy: true,
            mode,
            diagnostic: None,
        },
    })
}

fn render(result: &ProcessResult, recovery_handle: &str) -> Vec<String> {
    let command = redact_secrets(&result.command.join(" "));
    let working_directory = redact_secrets(&result.cwd.display().to_string());
    let mut lines = vec![
        format!("command: {}", truncate(&command, MAX_COMMAND_DISPLAY_BYTES)),
        format!("working_directory: {working_directory}"),
        format!("status: {}", result.status.label()),
        format!(
            "exit_code: {}",
            result
                .exit_code
                .map_or_else(|| "unavailable".to_string(), |code| code.to_string())
        ),
        format!("duration_ms: {}", result.elapsed.as_millis()),
        format!("truncated: {}", result.output_truncated),
        format!("recovery: bounded redacted artifact {recovery_handle}"),
    ];

    let output = result
        .stdout
        .iter()
        .chain(&result.stderr)
        .map(|line| truncate(&redact_secrets(line), MAX_EVIDENCE_LINE_BYTES))
        .collect::<Vec<_>>();
    let diagnostics = collect(&output, is_diagnostic);
    let warnings = collect(&output, is_warning);
    let locations = collect(&output, looks_like_location);
    let failed_tests = failed_test_names(&output);
    let summary = final_summary(&output, result);

    push_section(&mut lines, "warnings", &warnings);
    push_section(&mut lines, "errors", &diagnostics);
    push_section(&mut lines, "locations", &locations);
    push_section(&mut lines, "failed_tests", &failed_tests);
    lines.push("final_summary:".to_string());
    lines.push(summary);

    bound_lines(lines)
}

fn collect(lines: &[String], predicate: impl Fn(&str) -> bool) -> Vec<String> {
    let mut unique = BTreeSet::new();
    let matches = lines
        .iter()
        .filter(|line| predicate(line))
        .filter(|line| unique.insert((*line).clone()))
        .cloned()
        .collect::<Vec<_>>();
    sample(&matches)
}

fn failed_test_names(lines: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_failures = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "failures:" {
            in_failures = true;
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("test ")
            .and_then(|line| line.strip_suffix(" ... FAILED"))
        {
            names.push(name.to_string());
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("---- ")
            .and_then(|line| line.strip_suffix(" stdout ----"))
        {
            names.push(name.to_string());
            continue;
        }
        if in_failures {
            if trimmed.is_empty() || trimmed.starts_with("test result:") {
                in_failures = false;
            } else if !trimmed.starts_with("failures:") && !trimmed.starts_with("----") {
                names.push(trimmed.to_string());
            }
        }
    }

    let mut unique = BTreeSet::new();
    sample(
        &names
            .into_iter()
            .filter(|name| unique.insert(name.clone()))
            .collect::<Vec<_>>(),
    )
}

fn final_summary(lines: &[String], result: &ProcessResult) -> String {
    lines
        .iter()
        .rev()
        .find(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("test result:")
                || trimmed.starts_with("error: could not")
                || trimmed.starts_with("Finished ")
                || trimmed.starts_with("Running ")
        })
        .cloned()
        .unwrap_or_else(|| redact_secrets(&result.summary()))
}

fn is_warning(line: &str) -> bool {
    let line = line.trim_start().to_ascii_lowercase();
    line.starts_with("warning")
}

fn is_diagnostic(line: &str) -> bool {
    let line = line.trim_start().to_ascii_lowercase();
    line.starts_with("error")
        || line.contains("error[")
        || line.contains("panic")
        || line.contains("assertion failed")
        || line.ends_with(" failed")
        || line.contains(" failed:")
}

fn looks_like_location(line: &str) -> bool {
    let path = line.trim().trim_start_matches("-->").trim();
    let Some((path, column)) = path.rsplit_once(':') else {
        return false;
    };
    let Some((path, line_number)) = path.rsplit_once(':') else {
        return false;
    };
    !path.trim().is_empty()
        && (path.contains('/') || path.contains('\\'))
        && line_number.trim().chars().all(|character| character.is_ascii_digit())
        && column.trim().chars().all(|character| character.is_ascii_digit())
}

fn push_section(lines: &mut Vec<String>, name: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    lines.push(format!("{name}:"));
    lines.extend(values.iter().map(|value| format!("- {value}")));
}

fn sample(values: &[String]) -> Vec<String> {
    if values.len() <= MAX_EVIDENCE_LINES_PER_CATEGORY {
        return values.to_vec();
    }

    let last = values.len() - 1;
    let mut indexes = BTreeSet::new();
    for index in 0..MAX_EVIDENCE_LINES_PER_CATEGORY {
        indexes.insert(index * last / (MAX_EVIDENCE_LINES_PER_CATEGORY - 1));
    }
    indexes.into_iter().map(|index| values[index].clone()).collect()
}

fn bound_lines(lines: Vec<String>) -> Vec<String> {
    let mut bounded = Vec::new();
    let mut used = 0usize;
    for line in lines {
        let separator = usize::from(!bounded.is_empty());
        let remaining = MAX_COMMAND_PROJECTION_BYTES.saturating_sub(used.saturating_add(separator));
        if remaining == 0 {
            break;
        }
        let line = truncate(&line, remaining);
        if line.is_empty() {
            break;
        }
        used = used.saturating_add(separator).saturating_add(line.len());
        bounded.push(line);
    }
    bounded
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let suffix = "...";
    if max_bytes <= suffix.len() {
        return suffix[..max_bytes].to_string();
    }
    let mut end = max_bytes.saturating_sub(suffix.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], suffix)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use serde::Deserialize;

    use super::*;
    use crate::tools::shell::{ProcessKind, ProcessStatus};

    fn result(stdout: Vec<&str>, stderr: Vec<&str>) -> ProcessResult {
        ProcessResult {
            process_id: None,
            command: vec!["cargo".to_string(), "test".to_string()],
            cwd: PathBuf::from("/repo"),
            status: ProcessStatus::Failed,
            exit_code: Some(101),
            stdout: stdout.into_iter().map(str::to_string).collect(),
            stderr: stderr.into_iter().map(str::to_string).collect(),
            output_truncated: true,
            elapsed: Duration::from_millis(125),
            kind: ProcessKind::OneShot,
        }
    }

    #[test]
    fn projection_retains_command_failure_and_test_evidence_from_every_output_position() {
        let baseline = vec!["raw output".to_string(); 100];
        let result = result(
            vec![
                "noise before",
                "test crate::parser::keeps_location ... FAILED",
                "noise after",
            ],
            vec![
                "warning: unused variable",
                "error[E0308]: mismatched types",
                "  --> crates/app/src/parser/mod.rs:42:9",
                "test result: FAILED. 0 passed; 1 failed",
            ],
        );
        let mut config = ReductionConfig::disabled();
        config.command_result = true;

        let projection = project("tool:1", &baseline, &result, "artifact_v1_test", &config).expect("projection");
        let rendered = projection.lines.join("\n");

        for required in [
            "command: cargo test",
            "working_directory: /repo",
            "status: failed",
            "exit_code: 101",
            "duration_ms: 125",
            "truncated: true",
            "artifact_v1_test",
            "warning: unused variable",
            "error[E0308]",
            "crates/app/src/parser/mod.rs:42:9",
            "crate::parser::keeps_location",
            "test result: FAILED",
        ] {
            assert!(rendered.contains(required), "missing {required}: {rendered}");
        }
        assert_eq!(projection.receipt.mode, ContextReductionMode::Applied);
        assert!(projection.receipt.lossy);
    }

    #[test]
    fn near_duplicate_diagnostics_are_not_collapsed() {
        let result = result(
            Vec::new(),
            vec![
                "error[E0308]: expected usize",
                "error[E0308]: expected u64",
                "test result: FAILED. 0 passed; 1 failed",
            ],
        );
        let mut config = ReductionConfig::disabled();
        config.command_result = true;
        let projection = project("tool:1", &[], &result, "artifact_v1_test", &config).expect("projection");
        let rendered = projection.lines.join("\n");

        assert!(rendered.contains("expected usize"));
        assert!(rendered.contains("expected u64"));
    }

    #[test]
    fn projection_redacts_sensitive_command_and_output_content() {
        let mut result = result(Vec::new(), vec!["api_key=super-secret-value"]);
        result.command = vec!["cargo".to_string(), "api_key=super-secret-value".to_string()];
        let mut config = ReductionConfig::disabled();
        config.command_result = true;

        let projection = project("tool:1", &[], &result, "artifact_v1_test", &config).expect("projection");
        let rendered = projection.lines.join("\n");

        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("super-secret-value"));
    }

    #[test]
    fn truncation_honors_tiny_byte_bounds() {
        assert_eq!(truncate("long value", 0), "");
        assert_eq!(truncate("long value", 1), ".");
        assert_eq!(truncate("long value", 2), "..");
        assert_eq!(truncate("long value", 3), "...");
    }

    #[test]
    fn shadow_receipt_keeps_the_baseline_at_the_call_site() {
        let result = result(vec!["test result: ok. 1 passed"], Vec::new());
        let projection = project(
            "tool:1",
            &["baseline".to_string()],
            &result,
            "artifact_v1_test",
            &ReductionConfig::default(),
        )
        .expect("shadow projection");

        assert_eq!(projection.receipt.mode, ContextReductionMode::Shadow);
        assert!(projection.lines.iter().any(|line| line == "final_summary:"));
    }

    #[derive(Deserialize)]
    struct FrozenFixture {
        id: String,
        command: Vec<String>,
        cwd: PathBuf,
        status: String,
        exit_code: Option<i32>,
        duration_ms: u64,
        truncated: bool,
        stdout: Vec<String>,
        stderr: Vec<String>,
        required: Vec<String>,
    }

    #[test]
    fn frozen_command_family_fixtures_preserve_required_operational_evidence() {
        let fixtures: Vec<FrozenFixture> = serde_json::from_str(include_str!("fixtures/command_projection.json"))
            .expect("valid command projection fixtures");
        let mut config = ReductionConfig::disabled();
        config.command_result = true;

        for fixture in fixtures {
            let status = match fixture.status.as_str() {
                "ok" => ProcessStatus::Ok,
                "failed" => ProcessStatus::Failed,
                other => panic!("unknown fixture status {other}"),
            };
            let result = ProcessResult {
                process_id: None,
                command: fixture.command,
                cwd: fixture.cwd,
                status,
                exit_code: fixture.exit_code,
                stdout: fixture.stdout,
                stderr: fixture.stderr,
                output_truncated: fixture.truncated,
                elapsed: Duration::from_millis(fixture.duration_ms),
                kind: ProcessKind::OneShot,
            };
            let projection = project(
                &fixture.id,
                &result.to_output_lines(),
                &result,
                "artifact_v1_fixture",
                &config,
            )
            .expect("configured projection");
            let rendered = projection.lines.join("\n");
            for evidence in fixture.required {
                assert!(
                    rendered.contains(&evidence),
                    "{} omitted `{evidence}`: {rendered}",
                    fixture.id
                );
            }
            assert!(rendered.contains("artifact_v1_fixture"));
            assert_eq!(projection.receipt.mode, ContextReductionMode::Applied);
        }
    }
}
