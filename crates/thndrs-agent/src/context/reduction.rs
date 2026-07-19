//! Independent, deterministic reductions for bounded model projections.
//!
//! Reducers in this module operate on an owned copy of already-bounded lines.
//! They never receive or mutate durable tool evidence. Every proposed or
//! applied change carries the same exact newline-joined byte measurement, a
//! stable reducer name/version, and a preservation check.
//!
//! A failed check returns the original projection and a diagnostic instead of a partial candidate.

use serde::{Deserialize, Serialize};

use crate::accounting::{ContextReductionMode, ContextReductionReceipt};

/// Configuration and schema version for the independent reducer switches.
pub const REDUCTION_CONFIG_VERSION: &str = "context-reduction-config-v1";
/// Version of terminal-control cleanup.
pub const TERMINAL_CONTROL_REDUCER_VERSION: &str = "terminal-control-cleanup-v1";
/// Version of carriage-return progress-redraw cleanup.
pub const PROGRESS_REDRAW_REDUCER_VERSION: &str = "progress-redraw-cleanup-v1";
/// Version of blank-run normalization.
pub const BLANK_RUN_REDUCER_VERSION: &str = "blank-run-normalization-v1";
/// Version of exact repeated-line collapse.
pub const REPEATED_LINE_REDUCER_VERSION: &str = "exact-repeated-line-collapse-v1";

/// Default maximum size of a bounded projection passed to a reducer.
pub const DEFAULT_PROJECTION_MAX_BYTES: usize = 128 * 1024;
/// Maximum configurable blank-run limit.
pub const MAX_BLANK_LINES: usize = 64;

/// The deterministic reducers shipped by this crate.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReducerKind {
    /// Remove ANSI/terminal control sequences while retaining rendered text.
    TerminalControl,
    /// Resolve carriage-return redraws while retaining final status/value text.
    ProgressRedraw,
    /// Limit consecutive blank lines to the configured maximum.
    BlankRun,
    /// Replace consecutive exact non-blank repetitions with one counted line.
    RepeatedLine,
}

impl ReducerKind {
    /// All reducers in their stable application and inspection order.
    pub const ALL: [Self; 4] = [
        Self::TerminalControl,
        Self::ProgressRedraw,
        Self::BlankRun,
        Self::RepeatedLine,
    ];

    /// Stable name used by configuration, receipts, dashboards, and reports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::TerminalControl => "terminal_control_cleanup",
            Self::ProgressRedraw => "progress_redraw_cleanup",
            Self::BlankRun => "blank_run_normalization",
            Self::RepeatedLine => "exact_repeated_line_collapse",
        }
    }

    /// Stable version for this reducer's observable behavior.
    pub const fn version(self) -> &'static str {
        match self {
            Self::TerminalControl => TERMINAL_CONTROL_REDUCER_VERSION,
            Self::ProgressRedraw => PROGRESS_REDRAW_REDUCER_VERSION,
            Self::BlankRun => BLANK_RUN_REDUCER_VERSION,
            Self::RepeatedLine => REPEATED_LINE_REDUCER_VERSION,
        }
    }

    fn enabled(self, config: &ReductionConfig) -> bool {
        match self {
            Self::TerminalControl => config.terminal_control,
            Self::ProgressRedraw => config.progress_redraw,
            Self::BlankRun => config.blank_run,
            Self::RepeatedLine => config.repeated_line,
        }
    }
}

/// Configuration error returned before a reducer can run.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReductionConfigError {
    /// A blank-run limit above the bounded safety cap was requested.
    #[error("max_blank_lines {value} exceeds the maximum {MAX_BLANK_LINES}")]
    BlankRunTooLarge {
        /// Invalid configured limit.
        value: usize,
    },
}

/// Independently configurable model-projection reduction switches.
///
/// `shadow` records what disabled reducers would have done without changing
/// the request. The four reducer switches are deliberately separate; there is
/// no bundled quality, balanced, or economy preset. Applied switches default
/// to `false` so existing provider requests remain unchanged until a user
/// enables a specific mechanism.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReductionConfig {
    /// Record disabled-reducer proposals without changing model requests.
    pub shadow: bool,
    /// Apply terminal-control cleanup to model projections.
    pub terminal_control: bool,
    /// Apply carriage-return progress-redraw cleanup to model projections.
    pub progress_redraw: bool,
    /// Apply blank-run normalization to model projections.
    #[serde(alias = "blank_runs")]
    pub blank_run: bool,
    /// Apply exact repeated-line collapse to model projections.
    #[serde(alias = "repeated_lines")]
    pub repeated_line: bool,
    /// Apply state-identical evidence suppression when an application adapter
    /// supplies a matching tool-specific state fingerprint.
    #[serde(alias = "state_deduplication", alias = "deduplicate_state_identical")]
    pub state_identical: bool,
    /// Maximum consecutive blank lines retained when [`Self::blank_run`] is on.
    pub max_blank_lines: usize,
}

impl Default for ReductionConfig {
    fn default() -> Self {
        Self {
            shadow: true,
            terminal_control: false,
            progress_redraw: false,
            blank_run: false,
            repeated_line: false,
            state_identical: false,
            max_blank_lines: 1,
        }
    }
}

impl ReductionConfig {
    /// Return a configuration with both shadow measurement and applied
    /// reduction disabled.
    pub const fn disabled() -> Self {
        Self {
            shadow: false,
            terminal_control: false,
            progress_redraw: false,
            blank_run: false,
            repeated_line: false,
            state_identical: false,
            max_blank_lines: 1,
        }
    }

    /// Validate user-supplied reducer settings.
    pub fn validate(&self) -> Result<(), ReductionConfigError> {
        if self.max_blank_lines > MAX_BLANK_LINES {
            return Err(ReductionConfigError::BlankRunTooLarge { value: self.max_blank_lines });
        }
        Ok(())
    }

    /// Return enabled reducers in their stable pipeline order.
    pub fn enabled_reducers(&self) -> Vec<ReducerKind> {
        ReducerKind::ALL.into_iter().filter(|kind| kind.enabled(self)).collect()
    }

    /// Whether any configured reduction can change a model projection.
    pub fn has_applied_reducer(&self) -> bool {
        self.state_identical || !self.enabled_reducers().is_empty()
    }
}

/// An owned, bounded model projection supplied to the reducer pipeline.
///
/// The constructor applies the byte cap before any reducer runs. The optional
/// required fragments are semantic preservation gates supplied by the caller;
/// they are copied with the projection and are never treated as durable
/// evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedProjection {
    lines: Vec<String>,
    max_bytes: usize,
    required_fragments: Vec<String>,
}

impl BoundedProjection {
    /// Build a projection capped by exact UTF-8 bytes of newline-joined lines.
    pub fn new(lines: Vec<String>, max_bytes: usize) -> Self {
        Self { lines: bound_lines(lines, max_bytes), max_bytes, required_fragments: Vec::new() }
    }

    /// Build a projection using [`DEFAULT_PROJECTION_MAX_BYTES`].
    pub fn from_lines(lines: Vec<String>) -> Self {
        Self::new(lines, DEFAULT_PROJECTION_MAX_BYTES)
    }

    /// Attach semantic fragments that every accepted candidate must retain.
    pub fn with_required_fragments<I, S>(mut self, fragments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.required_fragments = fragments
            .into_iter()
            .map(Into::into)
            .filter(|fragment| !fragment.is_empty())
            .collect();
        self
    }

    /// Return the bounded lines without exposing durable source content.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Consume the projection and return its bounded lines.
    pub fn into_lines(self) -> Vec<String> {
        self.lines
    }

    /// Exact byte cap applied to this projection.
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Exact UTF-8 bytes of the newline-joined projection.
    pub fn exact_bytes(&self) -> u64 {
        measure_lines(&self.lines)
    }
}

/// One preservation failure emitted when a reducer falls back to baseline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReductionDiagnostic {
    /// Reducer that failed, or `None` for an invalid pipeline configuration.
    pub reducer: Option<ReducerKind>,
    /// Stable diagnostic code.
    pub code: String,
    /// Bounded human-readable explanation.
    pub message: String,
}

impl ReductionDiagnostic {
    fn invalid_config(error: &ReductionConfigError) -> Self {
        Self { reducer: None, code: "invalid_reduction_config".to_string(), message: error.to_string() }
    }

    fn preservation_failed(kind: ReducerKind, message: impl Into<String>) -> Self {
        Self { reducer: Some(kind), code: "reduction_preservation_failed".to_string(), message: message.into() }
    }
}

/// Aggregate measurements used by inspection and the compact model dashboard.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReductionDashboard {
    /// Exact baseline bytes measured before any applied reducer.
    pub before_bytes: u64,
    /// Exact final bytes measured after all applied reducers.
    pub after_bytes: u64,
    /// Baseline line count.
    pub before_lines: usize,
    /// Final line count.
    pub after_lines: usize,
    /// Number of lines omitted from the applied projection.
    pub routine_omissions: usize,
    /// Applied and shadow receipts, in stable reducer order.
    pub receipts: Vec<ContextReductionReceipt>,
}

/// Result of shadowing or applying independently configured reducers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionResult {
    /// Final bounded model projection; baseline when any applied invariant fails.
    pub lines: Vec<String>,
    /// Reduction receipts for shadow, applied, or fallback decisions.
    pub receipts: Vec<ContextReductionReceipt>,
    /// Preservation/configuration diagnostics.
    pub diagnostics: Vec<ReductionDiagnostic>,
    /// Aggregate measurements without per-line placeholder text.
    pub dashboard: ReductionDashboard,
}

impl ReductionResult {
    /// Whether an applied reducer changed the model projection.
    pub fn changed(&self) -> bool {
        self.dashboard.before_bytes != self.dashboard.after_bytes
            || self.dashboard.before_lines != self.dashboard.after_lines
    }

    /// Render the compact aggregate dashboard used by model-visible inspection.
    pub fn render_dashboard(&self) -> String {
        render_reduction_dashboard(&self.dashboard, &self.diagnostics)
    }
}

/// Reduce a bounded projection with independent shadow/applied switches.
pub fn reduce_projection(item_id: &str, projection: &BoundedProjection, config: &ReductionConfig) -> ReductionResult {
    let baseline = projection.lines.clone();
    let mut active = baseline.clone();
    let mut receipts = Vec::new();
    let mut diagnostics = Vec::new();

    if let Err(error) = config.validate() {
        diagnostics.push(ReductionDiagnostic::invalid_config(&error));
        return result(&baseline, baseline.clone(), receipts, diagnostics);
    }

    for kind in ReducerKind::ALL {
        let enabled = kind.enabled(config);
        if !enabled && !config.shadow {
            continue;
        }

        let input = if enabled { active.clone() } else { baseline.clone() };
        let candidate = bound_lines(reduce_kind(kind, &input, config), projection.max_bytes);
        let before_bytes = measure_lines(&input);
        let after_bytes = measure_lines(&candidate);
        let mode = if enabled { ContextReductionMode::Applied } else { ContextReductionMode::Shadow };

        if let Err(message) = validate_candidate(kind, &input, &candidate, projection, config) {
            let diagnostic = ReductionDiagnostic::preservation_failed(kind, message);
            diagnostics.push(diagnostic.clone());
            receipts.push(receipt(
                item_id,
                kind,
                ContextReductionMode::BaselineFallback,
                before_bytes,
                after_bytes,
                Some(diagnostic.message),
            ));
            if enabled {
                for previous in &mut receipts {
                    if previous.mode == ContextReductionMode::Applied {
                        previous.mode = ContextReductionMode::BaselineFallback;
                        previous.diagnostic = Some("later reducer failed; baseline remained active".to_string());
                    }
                }
                return result(&baseline, baseline.clone(), receipts, diagnostics);
            }
            continue;
        }

        receipts.push(receipt(item_id, kind, mode, before_bytes, after_bytes, None));
        if enabled {
            active = candidate;
        }
    }

    result(&baseline, active, receipts, diagnostics)
}

/// Reduce raw bounded lines using [`DEFAULT_PROJECTION_MAX_BYTES`].
pub fn reduce_lines(item_id: &str, lines: Vec<String>, config: &ReductionConfig) -> ReductionResult {
    reduce_projection(item_id, &BoundedProjection::from_lines(lines), config)
}

/// Measure the same newline-joined UTF-8 representation used by shadow and
/// applied receipts.
pub fn measure_lines(lines: &[String]) -> u64 {
    lines
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(lines.len().saturating_sub(1)) as u64
}

/// Render one aggregate reduction dashboard without emitting one placeholder
/// for every routine omission.
pub fn render_reduction_dashboard(dashboard: &ReductionDashboard, diagnostics: &[ReductionDiagnostic]) -> String {
    let mut out = String::new();
    out.push_str("<reduction_dashboard>\n");
    element(&mut out, 2, "config_version", REDUCTION_CONFIG_VERSION);
    element(&mut out, 2, "before_bytes", &dashboard.before_bytes.to_string());
    element(&mut out, 2, "after_bytes", &dashboard.after_bytes.to_string());
    element(&mut out, 2, "before_lines", &dashboard.before_lines.to_string());
    element(&mut out, 2, "after_lines", &dashboard.after_lines.to_string());
    element(
        &mut out,
        2,
        "routine_omissions",
        &dashboard.routine_omissions.to_string(),
    );
    out.push_str("  <reducers>\n");
    for receipt in &dashboard.receipts {
        out.push_str("    <reducer>\n");
        element(&mut out, 6, "name", &receipt.method);
        element(&mut out, 6, "version", &receipt.version);
        element(&mut out, 6, "mode", receipt.mode.label());
        element(&mut out, 6, "before_bytes", &receipt.before_bytes.to_string());
        element(&mut out, 6, "after_bytes", &receipt.after_bytes.to_string());
        out.push_str("    </reducer>\n");
    }
    out.push_str("  </reducers>\n");
    if !diagnostics.is_empty() {
        out.push_str("  <diagnostics>\n");
        for diagnostic in diagnostics {
            element(&mut out, 4, "diagnostic", &format_diagnostic(diagnostic));
        }
        out.push_str("  </diagnostics>\n");
    }
    out.push_str("</reduction_dashboard>");
    out
}

fn result(
    baseline: &[String], active: Vec<String>, receipts: Vec<ContextReductionReceipt>,
    diagnostics: Vec<ReductionDiagnostic>,
) -> ReductionResult {
    let dashboard = ReductionDashboard {
        before_bytes: measure_lines(baseline),
        after_bytes: measure_lines(&active),
        before_lines: baseline.len(),
        after_lines: active.len(),
        routine_omissions: baseline.len().saturating_sub(active.len()),
        receipts: receipts.clone(),
    };
    ReductionResult { lines: active, receipts, diagnostics, dashboard }
}

fn receipt(
    item_id: &str, kind: ReducerKind, mode: ContextReductionMode, before_bytes: u64, after_bytes: u64,
    diagnostic: Option<String>,
) -> ContextReductionReceipt {
    ContextReductionReceipt {
        item_id: item_id.to_string(),
        method: kind.label().to_string(),
        version: kind.version().to_string(),
        before_bytes,
        after_bytes,
        lossy: false,
        mode,
        diagnostic,
    }
}

fn reduce_kind(kind: ReducerKind, lines: &[String], config: &ReductionConfig) -> Vec<String> {
    match kind {
        ReducerKind::TerminalControl => lines.iter().map(|line| strip_terminal_controls(line)).collect(),
        ReducerKind::ProgressRedraw => reduce_progress_redraw(lines),
        ReducerKind::BlankRun => reduce_blank_runs(lines, config.max_blank_lines),
        ReducerKind::RepeatedLine => collapse_repeated_lines(lines),
    }
}

fn validate_candidate(
    kind: ReducerKind, before: &[String], after: &[String], projection: &BoundedProjection, config: &ReductionConfig,
) -> Result<(), String> {
    let rendered = after.join("\n");
    for fragment in &projection.required_fragments {
        if !rendered.contains(fragment) {
            return Err("required projection fragment was not retained".to_string());
        }
    }

    match kind {
        ReducerKind::TerminalControl => {
            let expected = before
                .iter()
                .map(|line| strip_terminal_controls(line))
                .collect::<Vec<_>>();
            if expected != after {
                return Err("terminal-control cleanup changed semantic line content".to_string());
            }
        }
        ReducerKind::ProgressRedraw => {
            if reduce_progress_redraw(before) != after {
                return Err("progress redraw cleanup changed status/value order".to_string());
            }
        }
        ReducerKind::BlankRun => {
            let expected = before.iter().filter(|line| !is_blank(line)).collect::<Vec<_>>();
            let actual = after.iter().filter(|line| !is_blank(line)).collect::<Vec<_>>();
            if expected != actual {
                return Err("blank-run normalization changed a non-blank line or its order".to_string());
            }
            if count_max_blank_run(after) > config.max_blank_lines {
                return Err("blank-run normalization exceeded its configured limit".to_string());
            }
        }
        ReducerKind::RepeatedLine => {
            if collapse_repeated_lines(before) != after {
                return Err("repeated-line collapse changed line order or repetition counts".to_string());
            }
        }
    }
    Ok(())
}

fn reduce_progress_redraw(lines: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    for line in lines {
        if !line.contains('\r') {
            output.push(line.clone());
            continue;
        }
        let frames = progress_frames(line);
        if frames.is_empty() {
            continue;
        }

        let starts_with_redraw = line.starts_with('\r');
        for (index, (_, frame)) in frames.iter().enumerate() {
            if index == 0
                && starts_with_redraw
                && let Some(previous) = output.last_mut()
                && same_redraw_key(previous, frame)
            {
                *previous = frame.clone();
                continue;
            }
            output.push(frame.clone());
        }
    }
    output
}

fn progress_frames(line: &str) -> Vec<(String, String)> {
    let mut frames = Vec::new();
    for segment in line.split('\r').filter(|segment| !segment.is_empty()) {
        let key = redraw_key(segment);
        if !key.is_empty()
            && let Some(index) = frames.iter().position(|(current, _)| current == &key)
        {
            frames.remove(index);
        }
        frames.push((key, segment.to_string()));
    }
    frames
}

fn reduce_blank_runs(lines: &[String], max_blank_lines: usize) -> Vec<String> {
    let mut output = Vec::with_capacity(lines.len());
    let mut blank_count = 0;
    for line in lines {
        if is_blank(line) {
            if blank_count < max_blank_lines {
                output.push(line.clone());
            }
            blank_count = blank_count.saturating_add(1);
        } else {
            blank_count = 0;
            output.push(line.clone());
        }
    }
    output
}

fn collapse_repeated_lines(lines: &[String]) -> Vec<String> {
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        let mut end = index + 1;
        while end < lines.len() && lines[end] == *line && !is_blank(line) {
            end += 1;
        }
        let count = end - index;
        if count > 1 {
            output.push(format!("{line} [repeated {count} times]"));
        } else {
            output.push(line.clone());
        }
        index = end;
    }
    output
}

fn strip_terminal_controls(value: &str) -> String {
    let mut chars = value.chars().peekable();
    let mut output = String::with_capacity(value.len());
    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => skip_escape_sequence(&mut chars),
            '\u{009b}' => skip_csi_sequence(&mut chars),
            '\u{009d}' | '\u{009e}' | '\u{009f}' | '\u{0090}' | '\u{0098}' => skip_control_string(&mut chars),
            '\u{08}' => {
                output.pop();
            }
            '\t' | '\r' => output.push(ch),
            ch if ch.is_control() => {}
            ch => output.push(ch),
        }
    }
    output
}

fn skip_escape_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let Some(kind) = chars.next() else { return };
    match kind {
        '[' => skip_csi_sequence(chars),
        ']' | 'P' | '^' | '_' | 'X' => skip_control_string(chars),
        _ => {}
    }
}

fn skip_csi_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for ch in chars.by_ref() {
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
}

fn skip_control_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(ch) = chars.next() {
        if ch == '\u{07}' {
            break;
        }
        if ch == '\u{1b}' && chars.next_if_eq(&'\\').is_some() {
            break;
        }
    }
}

fn same_redraw_key(left: &str, right: &str) -> bool {
    let left = redraw_key(left);
    let right = redraw_key(right);
    !left.is_empty() && left == right
}

fn redraw_key(value: &str) -> String {
    let value = strip_terminal_controls(value).trim().to_string();
    let mut key = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    let mut whitespace = false;
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            while chars.peek().is_some_and(|next| next.is_ascii_digit() || *next == '.') {
                chars.next();
            }
            key.push('#');
            whitespace = false;
        } else if ch.is_whitespace() {
            whitespace = true;
        } else {
            if whitespace && !key.is_empty() {
                key.push(' ');
            }
            key.push(ch.to_ascii_lowercase());
            whitespace = false;
        }
    }
    key
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn count_max_blank_run(lines: &[String]) -> usize {
    let mut current = 0;
    let mut maximum = 0;
    for line in lines {
        if is_blank(line) {
            current += 1;
            maximum = maximum.max(current);
        } else {
            current = 0;
        }
    }
    maximum
}

fn bound_lines(lines: Vec<String>, max_bytes: usize) -> Vec<String> {
    if max_bytes == 0 {
        return Vec::new();
    }
    let mut output = Vec::new();
    let mut used = 0usize;
    for line in lines {
        let separator = usize::from(!output.is_empty());
        let remaining = max_bytes.saturating_sub(used.saturating_add(separator));
        if remaining == 0 && separator > 0 {
            break;
        }
        let bounded = truncate_utf8(&line, remaining);
        if bounded.is_empty() && !line.is_empty() {
            break;
        }
        used = used.saturating_add(separator).saturating_add(bounded.len());
        output.push(bounded);
        if used >= max_bytes {
            break;
        }
    }
    output
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn format_diagnostic(diagnostic: &ReductionDiagnostic) -> String {
    match diagnostic.reducer {
        Some(reducer) => format!("{}: {}: {}", reducer.label(), diagnostic.code, diagnostic.message),
        None => format!("{}: {}", diagnostic.code, diagnostic.message),
    }
}

fn element(out: &mut String, indent: usize, name: &str, value: &str) {
    let pad = " ".repeat(indent);
    out.push_str(&format!("{pad}<{name}>{}</{name}>\n", escape_xml(value)));
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_for(kind: ReducerKind) -> ReductionConfig {
        let mut config = ReductionConfig::disabled();
        config.shadow = false;
        match kind {
            ReducerKind::TerminalControl => config.terminal_control = true,
            ReducerKind::ProgressRedraw => config.progress_redraw = true,
            ReducerKind::BlankRun => config.blank_run = true,
            ReducerKind::RepeatedLine => config.repeated_line = true,
        }
        config
    }

    fn expand_repeated_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .flat_map(|line| {
                parse_repetition(line).map_or_else(|| vec![line.clone()], |(source, count)| vec![source; count])
            })
            .collect()
    }

    fn parse_repetition(line: &str) -> Option<(String, usize)> {
        let suffix = " times]";
        let end = line.strip_suffix(suffix)?;
        let (source, count) = end.rsplit_once(" [repeated ")?;
        let count = count.parse::<usize>().ok()?;
        (count > 1).then(|| (source.to_string(), count))
    }

    #[test]
    fn reducers_are_named_versioned_and_independent() {
        let config = ReductionConfig::disabled();
        assert_eq!(ReducerKind::ALL.len(), 4);
        assert!(
            ReducerKind::ALL
                .iter()
                .all(|kind| !kind.label().is_empty() && !kind.version().is_empty())
        );
        assert!(config.enabled_reducers().is_empty());
        assert_eq!(
            serde_json::to_value(&config).expect("serialize config")["shadow"],
            false
        );
    }

    #[test]
    fn terminal_control_cleanup_keeps_text_and_removes_ansi() {
        let result = reduce_lines(
            "tool-1",
            vec![
                "\u{1b}[31merror:\u{1b}[0m bad input".to_string(),
                "title\u{009b}2Jlocation".to_string(),
                "name\u{1b}]0;thndrs\u{07}value".to_string(),
                "location: src/lib.rs:4".to_string(),
            ],
            &config_for(ReducerKind::TerminalControl),
        );
        assert_eq!(
            result.lines,
            vec![
                "error: bad input",
                "titlelocation",
                "namevalue",
                "location: src/lib.rs:4"
            ]
        );
        assert_eq!(result.receipts[0].method, "terminal_control_cleanup");
        assert_eq!(result.receipts[0].mode, ContextReductionMode::Applied);
    }

    #[test]
    fn progress_redraw_keeps_final_value_and_changed_status() {
        let result = reduce_lines(
            "tool-1",
            vec![
                "build 10%\rbuild 20%\rbuild 100%".to_string(),
                "build 100%\rbuild failed".to_string(),
            ],
            &config_for(ReducerKind::ProgressRedraw),
        );
        assert_eq!(result.lines, vec!["build 100%", "build 100%", "build failed"]);
        assert!(result.changed());

        let status_change = reduce_lines(
            "tool-1",
            vec!["download 10%\rdownload 20%\rdownload failed".to_string()],
            &config_for(ReducerKind::ProgressRedraw),
        );
        assert_eq!(status_change.lines, vec!["download 20%", "download failed"]);

        let cross_line_redraw = reduce_lines(
            "tool-1",
            vec!["build 10%".to_string(), "\rbuild 100%".to_string()],
            &config_for(ReducerKind::ProgressRedraw),
        );
        assert_eq!(cross_line_redraw.lines, vec!["build 100%"]);
    }

    #[test]
    fn blank_runs_keep_non_blank_order() {
        let result = reduce_lines(
            "tool-1",
            vec![
                "before".to_string(),
                "".to_string(),
                "  ".to_string(),
                "after".to_string(),
            ],
            &config_for(ReducerKind::BlankRun),
        );
        assert_eq!(result.lines, vec!["before", "", "after"]);
    }

    #[test]
    fn repeated_lines_include_count_and_preserve_surrounding_order() {
        let result = reduce_lines(
            "tool-1",
            vec![
                "first".to_string(),
                "same".to_string(),
                "same".to_string(),
                "last".to_string(),
            ],
            &config_for(ReducerKind::RepeatedLine),
        );
        assert_eq!(result.lines, vec!["first", "same [repeated 2 times]", "last"]);
        assert_eq!(
            expand_repeated_lines(&result.lines),
            vec!["first", "same", "same", "last"]
        );
    }

    #[test]
    fn repeated_runs_round_trip_for_multiple_counts_and_literal_suffixes() {
        for count in 2..=16 {
            let lines = vec!["same".to_string(); count];
            let result = reduce_lines("tool-1", lines.clone(), &config_for(ReducerKind::RepeatedLine));
            assert_eq!(expand_repeated_lines(&result.lines), lines);
        }

        let literal = vec!["already [repeated 2 times]".to_string()];
        let result = reduce_lines("tool-1", literal.clone(), &config_for(ReducerKind::RepeatedLine));
        assert_eq!(result.lines, literal);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn shadow_receipts_measure_a_candidate_without_changing_lines() {
        let config = ReductionConfig { repeated_line: false, ..ReductionConfig::default() };
        let baseline = vec!["same".to_string(); 10];
        let result = reduce_lines("tool-1", baseline.clone(), &config);
        assert_eq!(result.lines, baseline);
        assert_eq!(result.receipts.len(), 4);
        assert!(
            result
                .receipts
                .iter()
                .all(|receipt| receipt.mode == ContextReductionMode::Shadow)
        );
        assert!(
            result
                .receipts
                .iter()
                .any(|receipt| receipt.after_bytes < receipt.before_bytes)
        );
    }

    #[test]
    fn shadow_and_applied_receipts_share_exact_measurements() {
        let lines = vec![
            "\u{1b}[32mcompile\u{1b}[0m 10%\r\u{1b}[32mcompile\u{1b}[0m 100%".to_string(),
            "".to_string(),
            "".to_string(),
            "same".to_string(),
            "same".to_string(),
            "same".to_string(),
        ];
        for kind in ReducerKind::ALL {
            let shadow = reduce_lines("tool-1", lines.clone(), &ReductionConfig::default());
            let applied = reduce_lines("tool-1", lines.clone(), &config_for(kind));
            let shadow_receipt = shadow
                .receipts
                .iter()
                .find(|receipt| receipt.method == kind.label())
                .expect("shadow receipt");
            let applied_receipt = applied
                .receipts
                .iter()
                .find(|receipt| receipt.method == kind.label())
                .expect("applied receipt");
            assert_eq!(shadow_receipt.before_bytes, applied_receipt.before_bytes);
            assert_eq!(shadow_receipt.after_bytes, applied_receipt.after_bytes);
        }
    }

    #[test]
    fn protected_fragment_failure_falls_back_to_baseline_and_diagnoses() {
        let projection = BoundedProjection::from_lines(vec!["progress 1%\rprogress 2%".to_string()])
            .with_required_fragments(["progress 1%"]);
        let result = reduce_projection("tool-1", &projection, &config_for(ReducerKind::ProgressRedraw));
        assert_eq!(result.lines, projection.lines());
        assert_eq!(result.diagnostics[0].code, "reduction_preservation_failed");
        assert_eq!(result.receipts[0].mode, ContextReductionMode::BaselineFallback);
    }

    #[test]
    fn later_preservation_failure_restores_prior_applied_reducers() {
        let projection =
            BoundedProjection::from_lines(vec!["before".to_string(), "  ".to_string(), "after".to_string()])
                .with_required_fragments(["  "]);
        let mut config = ReductionConfig::disabled();
        config.terminal_control = true;
        config.blank_run = true;
        config.max_blank_lines = 0;

        let result = reduce_projection("tool-1", &projection, &config);

        assert_eq!(result.lines, projection.lines());
        assert_eq!(result.receipts[0].mode, ContextReductionMode::BaselineFallback);
        assert_eq!(result.receipts[1].mode, ContextReductionMode::BaselineFallback);
        assert!(!result.diagnostics.is_empty());
    }

    #[test]
    fn invalid_blank_limit_falls_back_without_panicking() {
        let mut config = config_for(ReducerKind::BlankRun);
        config.max_blank_lines = MAX_BLANK_LINES + 1;
        let lines = vec!["before".to_string(), "after".to_string()];
        let result = reduce_lines("tool-1", lines.clone(), &config);
        assert_eq!(result.lines, lines);
        assert_eq!(result.diagnostics[0].code, "invalid_reduction_config");
    }

    #[test]
    fn bounded_projection_caps_before_reduction() {
        let projection = BoundedProjection::new(vec!["éé".to_string(), "tail".to_string()], 3);
        assert_eq!(projection.lines(), &["é".to_string()]);
        assert!(projection.exact_bytes() <= 3);
    }

    #[test]
    fn dashboard_aggregates_omissions_without_line_placeholders() {
        let result = reduce_lines(
            "tool-1",
            vec!["same".to_string(), "same".to_string(), "tail".to_string()],
            &config_for(ReducerKind::RepeatedLine),
        );
        let dashboard = result.render_dashboard();
        assert!(dashboard.contains("<routine_omissions>1</routine_omissions>"));
        assert!(!dashboard.contains("placeholder"));
        assert!(dashboard.contains("exact_repeated_line_collapse"));
    }
}
