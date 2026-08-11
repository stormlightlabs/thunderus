//! Structured, read-only review workflow and shared finding contract.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path};
use std::process::Command as ProcessCommand;

use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::cli::commands::review::ReviewCommand;
use crate::context::discover_workspace_root;
use crate::tools::ToolAuthority;

const MAX_TARGET_BYTES: usize = 128 * 1024;
const MAX_EVIDENCE_BYTES: usize = 2 * 1024;
const MAX_LOCATION_LINES: u32 = 20;
const REVIEW_SCHEMA_VERSION: u8 = 1;

/// The resolved source of the change set reviewed by a provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewTarget {
    /// Staged, unstaged, and untracked working-tree changes.
    WorkingTree,
    /// One verified Git revision.
    Revision { revision: String },
    /// A verified Git range.
    Range { base: String, head: String },
    /// One resolved local session record.
    Session { session_id: String },
}

/// Review finding severity, ordered from most to least urgent.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl ReviewSeverity {
    const fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
        }
    }
}

/// Whether a structured review found actionable defects.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    Clean,
    Findings,
}

/// Stable discriminator for review records emitted to machine consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRecordType {
    ReviewResult,
}

/// Tight source location attached to a review finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingLocation {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// One actionable, evidenced review finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewFinding {
    pub severity: ReviewSeverity,
    pub title: String,
    pub evidence: String,
    pub location: FindingLocation,
}

/// Provider-neutral review result used by human, JSONL, and ACP-backed runs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReviewResult {
    #[serde(rename = "type")]
    pub record_type: ReviewRecordType,
    pub schema_version: u8,
    pub target: ReviewTarget,
    pub paths: Vec<String>,
    pub input_bytes: usize,
    pub input_truncated: bool,
    pub outcome: ReviewOutcome,
    pub summary: String,
    pub findings: Vec<ReviewFinding>,
    pub verification: Vec<String>,
    pub failures: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderReview {
    outcome: ReviewOutcome,
    summary: String,
    #[serde(default)]
    findings: Vec<ReviewFinding>,
    #[serde(default)]
    verification: Vec<String>,
    #[serde(default)]
    failures: Vec<String>,
}

struct ResolvedTarget {
    target: ReviewTarget,
    paths: Vec<String>,
    content: String,
    truncated: bool,
}

/// Resolve, run, validate, and print one structured review.
pub fn run_command(cli: &Cli, command: &ReviewCommand) -> io::Result<()> {
    let root = discover_workspace_root(&cli.cwd);
    let resolved = resolve_target(cli, command, &root)?;
    let prompt = review_prompt(&resolved);
    let mut review_cli = cli.clone();
    review_cli.authority = ToolAuthority::ReadOnly;
    review_cli.websearch = crate::cli::WebSearchMode::None;
    let response = crate::headless::run_prompt_capture(&review_cli, &prompt)?;
    let provider = parse_provider_review(&response)?;
    let result = validate_result(provider, resolved, &root)?;

    let stdout = io::stdout();
    let mut writer = stdout.lock();
    if command.jsonl {
        serde_json::to_writer(&mut writer, &result).map_err(io::Error::other)?;
        writeln!(writer)
    } else {
        write_human(&mut writer, &result)
    }
}

fn resolve_target(cli: &Cli, command: &ReviewCommand, root: &Path) -> io::Result<ResolvedTarget> {
    if command.working_tree {
        return resolve_working_tree(root);
    }
    if let Some(revision) = &command.revision {
        verify_revision(root, revision)?;
        let diff = git(
            root,
            &["show", "--format=", "--no-ext-diff", "--unified=3", revision, "--"],
        )?;
        return bounded_target(ReviewTarget::Revision { revision: revision.clone() }, diff);
    }
    if let Some(range) = &command.range {
        let (base, head) = range
            .split_once("..")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "--range must use BASE..HEAD"))?;
        if base.is_empty() || head.is_empty() || head.contains("..") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--range must use BASE..HEAD",
            ));
        }
        verify_revision(root, base)?;
        verify_revision(root, head)?;
        let spec = format!("{base}..{head}");
        let diff = git(root, &["diff", "--no-ext-diff", "--unified=3", &spec, "--"])?;
        return bounded_target(
            ReviewTarget::Range { base: base.to_string(), head: head.to_string() },
            diff,
        );
    }
    if let Some(session_id) = &command.session {
        let dir = cli
            .session_dir
            .clone()
            .unwrap_or_else(|| crate::session::sessions_dir(root));
        let path = crate::session::resolve_session_file(&dir, session_id).map_err(io::Error::other)?;
        let records = crate::session::SessionReader::read_redacted_records(&path);
        let content = serde_json::to_string_pretty(&records).map_err(io::Error::other)?;
        let paths = collect_session_paths(&records);
        let (content, truncated) = truncate_bytes(content, MAX_TARGET_BYTES);
        return Ok(ResolvedTarget {
            target: ReviewTarget::Session { session_id: session_id.clone() },
            paths,
            content,
            truncated,
        });
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "exactly one review target is required",
    ))
}

fn resolve_working_tree(root: &Path) -> io::Result<ResolvedTarget> {
    let mut content = git(root, &["diff", "HEAD", "--no-ext-diff", "--unified=3", "--"])?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "--"])?;
    for path in untracked.lines().filter(|path| valid_relative_path(path)) {
        let Ok(bytes) = fs::read(root.join(path)) else { continue };
        let excerpt = String::from_utf8_lossy(&bytes);
        content.push_str(&format!(
            "\n--- /dev/null\n+++ b/{path}\n@@ untracked file @@\n{excerpt}\n"
        ));
        if content.len() > MAX_TARGET_BYTES {
            break;
        }
    }
    bounded_target(ReviewTarget::WorkingTree, content)
}

fn bounded_target(target: ReviewTarget, content: String) -> io::Result<ResolvedTarget> {
    if content.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "review target contains no changes",
        ));
    }
    let paths = diff_paths(&content);
    let (content, truncated) = truncate_bytes(content, MAX_TARGET_BYTES);
    Ok(ResolvedTarget { target, paths, content, truncated })
}

fn git(root: &Path, args: &[&str]) -> io::Result<String> {
    let output = ProcessCommand::new("git").arg("-C").arg(root).args(args).output()?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "failed to resolve review target: {}",
            error.trim()
        )));
    }
    String::from_utf8(output.stdout).map_err(|_| io::Error::other("Git output was not valid UTF-8"))
}

fn verify_revision(root: &Path, revision: &str) -> io::Result<()> {
    if revision.trim().is_empty() || revision.starts_with('-') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "revision must be a non-empty Git object name",
        ));
    }
    git(root, &["rev-parse", "--verify", &format!("{revision}^{{commit}}")]).map(|_| ())
}

fn review_prompt(target: &ResolvedTarget) -> String {
    format!(
        "Review the resolved change target below. Use only read-only tools. Report only actionable correctness, security, or maintainability defects introduced by the target. Every finding needs direct evidence and a tight location of at most {MAX_LOCATION_LINES} lines. Return JSON only with this shape: {{\"outcome\":\"clean|findings\",\"summary\":\"...\",\"findings\":[{{\"severity\":\"critical|high|medium|low\",\"title\":\"...\",\"evidence\":\"...\",\"location\":{{\"path\":\"relative/path\",\"start_line\":1,\"end_line\":1}}}}],\"verification\":[\"...\"],\"failures\":[\"...\"]}}. Use outcome clean with an empty findings array when no actionable defects remain. Target metadata: {}. Input truncated: {}.\n\n{}",
        serde_json::to_string(&target.target).unwrap_or_default(),
        target.truncated,
        target.content
    )
}

fn parse_provider_review(response: &str) -> io::Result<ProviderReview> {
    let trimmed = response.trim();
    let json = if trimmed.starts_with("```") {
        let after_open = trimmed.split_once('\n').map_or("", |(_, rest)| rest);
        after_open
            .rsplit_once("```")
            .map_or(after_open, |(body, _)| body)
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str(json)
        .map_err(|error| io::Error::other(format!("provider returned invalid review JSON: {error}")))
}

fn validate_result(provider: ProviderReview, target: ResolvedTarget, root: &Path) -> io::Result<ReviewResult> {
    if provider.summary.trim().is_empty() {
        return Err(io::Error::other("provider review summary is required"));
    }
    if (provider.outcome == ReviewOutcome::Clean) != provider.findings.is_empty() {
        return Err(io::Error::other(
            "clean reviews must have no findings; findings reviews must have at least one",
        ));
    }
    let allowed: BTreeSet<_> = target.paths.iter().map(String::as_str).collect();
    for finding in &provider.findings {
        validate_finding(finding, &allowed, root)?;
    }
    let mut findings = provider.findings;
    findings.sort_by(|a, b| {
        (a.severity.rank(), &a.location.path, a.location.start_line, &a.title).cmp(&(
            b.severity.rank(),
            &b.location.path,
            b.location.start_line,
            &b.title,
        ))
    });
    let input_bytes = target.content.len();
    Ok(ReviewResult {
        record_type: ReviewRecordType::ReviewResult,
        schema_version: REVIEW_SCHEMA_VERSION,
        target: target.target,
        paths: target.paths,
        input_bytes,
        input_truncated: target.truncated,
        outcome: provider.outcome,
        summary: provider.summary,
        findings,
        verification: provider.verification,
        failures: provider.failures,
    })
}

fn validate_finding(finding: &ReviewFinding, allowed: &BTreeSet<&str>, root: &Path) -> io::Result<()> {
    if finding.title.trim().is_empty() || finding.title.len() > 200 {
        return Err(io::Error::other("finding title must contain 1..=200 bytes"));
    }
    if finding.evidence.trim().is_empty() || finding.evidence.len() > MAX_EVIDENCE_BYTES {
        return Err(io::Error::other(format!(
            "finding evidence must contain 1..={MAX_EVIDENCE_BYTES} bytes"
        )));
    }
    let location = &finding.location;
    if !valid_relative_path(&location.path) || !allowed.contains(location.path.as_str()) {
        return Err(io::Error::other(format!(
            "finding path is outside the resolved target: {}",
            location.path
        )));
    }
    if location.start_line == 0
        || location.end_line < location.start_line
        || location.end_line - location.start_line + 1 > MAX_LOCATION_LINES
    {
        return Err(io::Error::other(format!(
            "finding location is invalid or wider than {MAX_LOCATION_LINES} lines"
        )));
    }
    if let Ok(source) = fs::read_to_string(root.join(&location.path)) {
        let line_count = u32::try_from(source.lines().count()).unwrap_or(u32::MAX).max(1);
        if location.end_line > line_count {
            return Err(io::Error::other(format!(
                "finding location exceeds {} ({} lines)",
                location.path, line_count
            )));
        }
    }
    Ok(())
}

fn diff_paths(diff: &str) -> Vec<String> {
    diff.lines()
        .filter_map(|line| line.strip_prefix("+++ b/").or_else(|| line.strip_prefix("--- a/")))
        .filter(|path| *path != "/dev/null" && valid_relative_path(path))
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collect_session_paths(records: &[serde_json::Value]) -> Vec<String> {
    fn visit(value: &serde_json::Value, paths: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(path) = object.get("path").and_then(serde_json::Value::as_str)
                    && valid_relative_path(path)
                {
                    paths.insert(path.to_string());
                }
                object.values().for_each(|value| visit(value, paths));
            }
            serde_json::Value::Array(values) => values.iter().for_each(|value| visit(value, paths)),
            _ => {}
        }
    }
    let mut paths = BTreeSet::new();
    records.iter().for_each(|value| visit(value, &mut paths));
    paths.into_iter().collect()
}

fn valid_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn truncate_bytes(mut value: String, max: usize) -> (String, bool) {
    if value.len() <= max {
        return (value, false);
    }
    let mut boundary = max;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    (value, true)
}

fn write_human(writer: &mut impl Write, result: &ReviewResult) -> io::Result<()> {
    writeln!(writer, "Review: {:?}", result.outcome)?;
    writeln!(
        writer,
        "Paths: {}",
        if result.paths.is_empty() { "(none)".to_string() } else { result.paths.join(", ") }
    )?;
    writeln!(
        writer,
        "Input: {} bytes{}",
        result.input_bytes,
        if result.input_truncated { " (truncated)" } else { "" }
    )?;
    writeln!(writer, "{}", result.summary)?;
    for finding in &result.findings {
        writeln!(
            writer,
            "- {:?} {}:{}-{} — {}\n  {}",
            finding.severity,
            finding.location.path,
            finding.location.start_line,
            finding.location.end_line,
            finding.title,
            finding.evidence
        )?;
    }
    if !result.verification.is_empty() {
        writeln!(writer, "Verification: {}", result.verification.join("; "))?;
    }
    if !result.failures.is_empty() {
        writeln!(writer, "Failures: {}", result.failures.join("; "))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok(root: &Path, args: &[&str]) {
        let status = ProcessCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn target() -> ResolvedTarget {
        ResolvedTarget {
            target: ReviewTarget::WorkingTree,
            paths: vec!["src/lib.rs".to_string()],
            content: String::new(),
            truncated: false,
        }
    }

    #[test]
    fn rejects_clean_result_with_findings() {
        let provider = ProviderReview {
            outcome: ReviewOutcome::Clean,
            summary: "looks good".to_string(),
            findings: vec![ReviewFinding {
                severity: ReviewSeverity::High,
                title: "bug".to_string(),
                evidence: "evidence".to_string(),
                location: FindingLocation { path: "src/lib.rs".to_string(), start_line: 1, end_line: 1 },
            }],
            verification: Vec::new(),
            failures: Vec::new(),
        };
        assert!(validate_result(provider, target(), Path::new(".")).is_err());
    }

    #[test]
    fn findings_are_sorted_deterministically() {
        let finding = |severity, line| ReviewFinding {
            severity,
            title: format!("line {line}"),
            evidence: "direct evidence".to_string(),
            location: FindingLocation { path: "src/lib.rs".to_string(), start_line: line, end_line: line },
        };
        let provider = ProviderReview {
            outcome: ReviewOutcome::Findings,
            summary: "two defects".to_string(),
            findings: vec![finding(ReviewSeverity::Low, 2), finding(ReviewSeverity::Critical, 1)],
            verification: vec!["cargo test".to_string()],
            failures: Vec::new(),
        };
        let result = validate_result(provider, target(), Path::new(".")).unwrap();
        assert_eq!(result.findings[0].severity, ReviewSeverity::Critical);
    }

    #[test]
    fn rejects_wide_or_unrelated_locations() {
        let finding = ReviewFinding {
            severity: ReviewSeverity::Medium,
            title: "bug".to_string(),
            evidence: "direct evidence".to_string(),
            location: FindingLocation { path: "other.rs".to_string(), start_line: 1, end_line: 30 },
        };
        let allowed = BTreeSet::from(["src/lib.rs"]);
        assert!(validate_finding(&finding, &allowed, Path::new(".")).is_err());
    }

    #[test]
    fn parses_json_with_or_without_a_fence() {
        let json = r#"{"outcome":"clean","summary":"clean","findings":[],"verification":[],"failures":[]}"#;
        assert_eq!(parse_provider_review(json).unwrap().outcome, ReviewOutcome::Clean);
        assert_eq!(
            parse_provider_review(&format!("```json\n{json}\n```\n"))
                .unwrap()
                .outcome,
            ReviewOutcome::Clean
        );
        assert!(parse_provider_review("not json").is_err());
    }

    #[test]
    fn working_tree_target_is_resolved_and_bounded_before_review() {
        let dir = tempfile::tempdir().expect("temp repo");
        git_ok(dir.path(), &["init", "--quiet"]);
        fs::write(dir.path().join("lib.rs"), "fn before() {}\n").expect("write fixture");
        git_ok(dir.path(), &["add", "lib.rs"]);
        git_ok(
            dir.path(),
            &[
                "-c",
                "user.name=thndrs",
                "-c",
                "user.email=thndrs@example.test",
                "commit",
                "--quiet",
                "-m",
                "base",
            ],
        );
        fs::write(dir.path().join("lib.rs"), "fn after() {}\n").expect("change fixture");

        let resolved = resolve_working_tree(dir.path()).expect("resolve working tree");

        assert_eq!(resolved.target, ReviewTarget::WorkingTree);
        assert_eq!(resolved.paths, ["lib.rs"]);
        assert!(resolved.content.contains("fn after()"));
        assert!(resolved.content.len() <= MAX_TARGET_BYTES);
    }

    #[test]
    fn json_result_has_a_stable_record_discriminator() {
        let provider = ProviderReview {
            outcome: ReviewOutcome::Clean,
            summary: "No actionable findings.".to_string(),
            findings: Vec::new(),
            verification: vec!["cargo test".to_string()],
            failures: Vec::new(),
        };
        let result = validate_result(provider, target(), Path::new(".")).expect("valid result");
        let json = serde_json::to_value(result).expect("serialize result");

        assert_eq!(json["type"], "review_result");
        assert_eq!(json["schema_version"], REVIEW_SCHEMA_VERSION);
        assert_eq!(json["input_bytes"], 0);
        assert_eq!(json["input_truncated"], false);
    }

    #[test]
    fn clean_and_finding_surfaces_are_stable() {
        let clean = validate_result(
            ProviderReview {
                outcome: ReviewOutcome::Clean,
                summary: "No actionable findings.".to_string(),
                findings: Vec::new(),
                verification: vec!["cargo test".to_string()],
                failures: Vec::new(),
            },
            target(),
            Path::new("."),
        )
        .expect("valid clean result");
        let mut output = Vec::new();
        write_human(&mut output, &clean).expect("render clean result");
        insta::assert_snapshot!(String::from_utf8(output).unwrap(), @r###"
        Review: Clean
        Paths: src/lib.rs
        Input: 0 bytes
        No actionable findings.
        Verification: cargo test
        "###);

        let finding = validate_result(
            ProviderReview {
                outcome: ReviewOutcome::Findings,
                summary: "One actionable defect.".to_string(),
                findings: vec![ReviewFinding {
                    severity: ReviewSeverity::High,
                    title: "State is discarded".to_string(),
                    evidence: "The error branch returns before persisting state.".to_string(),
                    location: FindingLocation { path: "src/lib.rs".to_string(), start_line: 4, end_line: 6 },
                }],
                verification: Vec::new(),
                failures: vec!["integration test unavailable".to_string()],
            },
            target(),
            Path::new("."),
        )
        .expect("valid finding result");
        let mut output = Vec::new();
        write_human(&mut output, &finding).expect("render finding result");
        insta::assert_snapshot!(String::from_utf8(output).unwrap(), @r###"
        Review: Findings
        Paths: src/lib.rs
        Input: 0 bytes
        One actionable defect.
        - High src/lib.rs:4-6 — State is discarded
          The error branch returns before persisting state.
        Failures: integration test unavailable
        "###);
    }
}
