//! Deterministic session fixtures for terminal capture scenarios.
//!
//! A capture set is comparable to the one before it only when the transcript
//! behind it is identical, so every capture scenario past `startup` resumes a
//! session written here instead of sending a prompt.
//!
//! Fixtures are built by constructing [`SessionRecord`] values and serializing
//! them, never by emitting JSONL text. A change to the record enum breaks this
//! module at compile time rather than breaking a capture run with no warning
//! first. The content is invented, so nothing written here needs redaction.
//!
//! Generation writes into scratch space that `.gitignore` covers and no
//! `.jsonl` file is committed. [`default_fixture_dir`] names that location.
//!
//! A fixture is consumed once. Resuming a session appends to it, and a run
//! writes its own session into the same directory, so a capture pass
//! regenerates before it starts rather than reusing what the last one left.

use clap::ValueEnum;

use super::*;
use crate::cli::Theme;

/// Workspace root recorded in every fixture session.
const FIXTURE_CWD: &str = "/workspace/thndrs-fixture";

/// Provider name recorded in every fixture session. No such provider exists,
/// which is what keeps a capture run from reaching one.
const FIXTURE_PROVIDER: &str = "fixture";

/// Model name recorded in every fixture session.
const FIXTURE_MODEL: &str = "fixture/deterministic-1";

/// Application version recorded in every fixture session. A real version would
/// change the fixture bytes on every release.
const FIXTURE_APP_VERSION: &str = "0.0.0-fixture";

/// Unix time of the first record in every fixture session: 2026-01-05T09:00:00Z.
const FIXTURE_START_UNIX: u64 = 1_767_603_600;

/// Seconds between one fixture record and the next.
const FIXTURE_STEP_SECONDS: u64 = 7;

/// Scratch directory for generated fixtures, relative to the workspace root.
const DEFAULT_FIXTURE_DIR: [&str; 3] = ["target", "tui-fixtures", "sessions"];

/// One capture scenario that resumes a fixture session.
///
/// `startup` is the only scenario without a fixture: it captures the first
/// frame before any input, so it has no transcript to load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureScenario {
    /// The session picker over a populated transcript.
    PickerOpen,
    /// Assistant text arriving while a tool still runs.
    StreamingMidTool,
    /// A tool group whose output passed its budget.
    ToolOutputTruncated,
    /// A tool waiting on approval.
    PermissionPrompt,
    /// A failed turn and the recovery it offers.
    Error,
    /// The populated transcript at 60 columns.
    NarrowSixtyColumns,
    /// The populated transcript at 16 rows.
    ShortSixteenRows,
    /// The populated transcript under `NO_COLOR`.
    NoColor,
    /// The populated transcript under one built-in theme.
    ThemeVariant(Theme),
}

impl FixtureScenario {
    /// Every scenario that loads a fixture, in capture order.
    ///
    /// Theme scenarios come from [`Theme::value_variants`], so a theme added to
    /// that enum adds a fixture without this list being edited.
    pub fn all() -> Vec<Self> {
        let named = [
            Self::PickerOpen,
            Self::StreamingMidTool,
            Self::ToolOutputTruncated,
            Self::PermissionPrompt,
            Self::Error,
            Self::NarrowSixtyColumns,
            Self::ShortSixteenRows,
            Self::NoColor,
        ];
        named
            .into_iter()
            .chain(Theme::value_variants().iter().copied().map(Self::ThemeVariant))
            .collect()
    }

    /// The scenario name a capture file is stored under.
    pub fn name(self) -> String {
        match self {
            Self::PickerOpen => String::from("picker-open"),
            Self::StreamingMidTool => String::from("streaming-mid-tool"),
            Self::ToolOutputTruncated => String::from("tool-output-truncated"),
            Self::PermissionPrompt => String::from("permission-prompt"),
            Self::Error => String::from("error"),
            Self::NarrowSixtyColumns => String::from("narrow-60-cols"),
            Self::ShortSixteenRows => String::from("short-16-rows"),
            Self::NoColor => String::from("no-color"),
            Self::ThemeVariant(theme) => format!("theme-{}", theme_slug(theme)),
        }
    }

    /// The session id `/resume` takes for this scenario. One session per
    /// scenario keeps the resume argument and the capture name the same word.
    pub fn session_id(self) -> String {
        self.name()
    }

    /// The display title the session picker shows for this scenario.
    pub fn title(self) -> String {
        match self {
            Self::PickerOpen => String::from("Picker over a populated transcript"),
            Self::StreamingMidTool => String::from("Assistant text while a tool runs"),
            Self::ToolOutputTruncated => String::from("Tool output past its budget"),
            Self::PermissionPrompt => String::from("Tool waiting on approval"),
            Self::Error => String::from("Failed turn and recovery"),
            Self::NarrowSixtyColumns => String::from("Populated transcript at 60 columns"),
            Self::ShortSixteenRows => String::from("Populated transcript at 16 rows"),
            Self::NoColor => String::from("Populated transcript without color"),
            Self::ThemeVariant(theme) => format!("Populated transcript in {}", theme_slug(theme)),
        }
    }

    /// The records this scenario's session file holds, in sequence order.
    pub fn records(self) -> Vec<SessionRecord> {
        let mut builder = FixtureBuilder::new(&self.session_id(), &self.title());
        populated_transcript(&mut builder);
        match self {
            Self::PickerOpen
            | Self::NarrowSixtyColumns
            | Self::ShortSixteenRows
            | Self::NoColor
            | Self::ThemeVariant(_) => {}
            Self::StreamingMidTool => streaming_mid_tool(&mut builder),
            Self::ToolOutputTruncated => tool_output_truncated(&mut builder),
            Self::PermissionPrompt => permission_prompt(&mut builder),
            Self::Error => failed_turn(&mut builder),
        }
        builder.finish()
    }
}

/// One fixture session written to disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFixture {
    /// The scenario this session was written for.
    pub scenario: FixtureScenario,
    /// The id `/resume` takes, which is also the file stem.
    pub session_id: String,
    /// The session file that was written.
    pub path: PathBuf,
    /// How many records the file holds.
    pub record_count: usize,
}

/// Accumulates one session's records with a monotonic sequence and a fixed
/// clock, so two generation runs produce the same bytes.
struct FixtureBuilder {
    records: Vec<SessionRecord>,
    seq: u64,
    clock: u64,
}

impl FixtureBuilder {
    /// Start a session with its `session_meta` record.
    fn new(session_id: &str, title: &str) -> Self {
        let meta = SessionRecord::SessionMeta {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::from_unix_seconds(FIXTURE_START_UNIX),
            session_id: session_id.to_string(),
            cwd: String::from(FIXTURE_CWD),
            title: title.to_string(),
            provider: String::from(FIXTURE_PROVIDER),
            model: String::from(FIXTURE_MODEL),
            websearch: String::new(),
            app_version: String::from(FIXTURE_APP_VERSION),
            config: None,
        };
        Self { records: vec![meta], seq: 1, clock: FIXTURE_START_UNIX }
    }

    /// The sequence number and timestamp for the next record.
    fn stamp(&mut self) -> (u64, String) {
        let seq = self.seq;
        self.seq = self.seq.saturating_add(1);
        self.clock = self.clock.saturating_add(FIXTURE_STEP_SECONDS);
        (seq, datetime::from_unix_seconds(self.clock))
    }

    fn user(&mut self, turn_id: &str, text: &str) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::User {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            text: text.to_string(),
        });
        self
    }

    fn reasoning(&mut self, turn_id: &str, text: &str) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::ReasoningFinished {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            text: text.to_string(),
        });
        self
    }

    fn assistant(&mut self, turn_id: &str, text: &str) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::AssistantFinished {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            text: text.to_string(),
        });
        self
    }

    fn tool_started(&mut self, turn_id: &str, call_id: &str, name: &str, arguments: &str) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::ToolStarted {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
            mcp: None,
        });
        self
    }

    fn tool_finished(
        &mut self, turn_id: &str, call_id: &str, status: ToolStatus, output: &[&str],
        artifact: Option<ArtifactMetadata>,
    ) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::ToolFinished {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            call_id: call_id.to_string(),
            status,
            output: output.iter().map(|line| (*line).to_string()).collect(),
            artifact,
            mcp: None,
        });
        self
    }

    fn permission_request(&mut self, turn_id: &str, call_id: &str, title: &str) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::AcpPermissionRequest {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            tool_call_id: call_id.to_string(),
            title: title.to_string(),
            options: vec![
                AcpPermissionOptionRecord {
                    id: String::from("allow-once"),
                    name: String::from("Allow once"),
                    kind: String::from("allow_once"),
                },
                AcpPermissionOptionRecord {
                    id: String::from("allow-always"),
                    name: String::from("Allow for this session"),
                    kind: String::from("allow_always"),
                },
                AcpPermissionOptionRecord {
                    id: String::from("reject-once"),
                    name: String::from("Reject"),
                    kind: String::from("reject_once"),
                },
            ],
        });
        self
    }

    fn failed(&mut self, turn_id: &str, error: &str) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::Failed {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            turn_id: turn_id.to_string(),
            error: error.to_string(),
        });
        self
    }

    fn usage(&mut self, input_tokens: u64, output_tokens: u64) -> &mut Self {
        let (seq, time) = self.stamp();
        self.records.push(SessionRecord::Usage {
            schema_version: SCHEMA_VERSION,
            seq,
            time,
            input_tokens,
            output_tokens,
        });
        self
    }

    fn finish(self) -> Vec<SessionRecord> {
        self.records
    }
}

/// The scratch directory generation writes to when none is named.
///
/// The path is relative to the workspace root and sits under `target/`, which
/// `.gitignore` already covers, so a generation run leaves the working tree
/// clean.
pub fn default_fixture_dir() -> PathBuf {
    DEFAULT_FIXTURE_DIR.iter().collect()
}

/// Write one session file per capture scenario past `startup` into `dir`.
///
/// Existing fixture files are replaced, so repeated runs converge on the same
/// bytes. Sessions outside [`FixtureScenario::all`] are left alone.
///
/// Generation assumes no live process is writing these sessions: a writer lock
/// left behind by an interrupted capture is removed with the session it guards.
pub fn generate(dir: &Path) -> std::io::Result<Vec<GeneratedFixture>> {
    std::fs::create_dir_all(dir)?;
    let mut generated = Vec::new();
    for scenario in FixtureScenario::all() {
        let session_id = scenario.session_id();
        let records = scenario.records();
        let path = dir.join(format!("{session_id}.jsonl"));
        let _ = std::fs::remove_file(writer_lock_path(&path));
        write_session(&path, &records)?;
        generated.push(GeneratedFixture { scenario, session_id, path, record_count: records.len() });
    }
    Ok(generated)
}

/// The three-turn transcript every fixture session opens with.
fn populated_transcript(builder: &mut FixtureBuilder) {
    builder
        .user("turn-1", "Where does the renderer decide how much tool output to show?")
        .reasoning(
            "turn-1",
            "The budget is applied where a tool group is finished, so start from the transcript blocks.",
        )
        .tool_started(
            "turn-1",
            "call-1",
            "search_text",
            r#"{"pattern":"output_budget","glob":"crates/**/*.rs"}"#,
        )
        .tool_finished(
            "turn-1",
            "call-1",
            ToolStatus::Ok,
            &[
                "renderer/transcript.rs:118: let budget = output_budget(height);",
                "renderer/transcript.rs:204: if lines.len() > budget {",
                "app/transcript_blocks.rs:61: pub fn output_budget(rows: u16) -> usize {",
            ],
            None,
        )
        .assistant(
            "turn-1",
            "`output_budget` in `app/transcript_blocks.rs` returns the line allowance, and the renderer \
             applies it in `renderer/transcript.rs` when a tool group settles. Nothing else trims output.",
        )
        .usage(1_284, 216);

    builder
        .user("turn-2", "Show me the allowance itself.")
        .tool_started(
            "turn-2",
            "call-2",
            "read_file_range",
            r#"{"path":"crates/thndrs/src/cli/app/transcript_blocks.rs","start_line":58,"end_line":66}"#,
        )
        .tool_finished(
            "turn-2",
            "call-2",
            ToolStatus::Ok,
            &[
                "pub fn output_budget(rows: u16) -> usize {",
                "    let visible = usize::from(rows).saturating_sub(HEADER_ROWS);",
                "    visible.clamp(MIN_TOOL_LINES, MAX_TOOL_LINES)",
                "}",
            ],
            None,
        )
        .assistant(
            "turn-2",
            "The allowance follows the viewport: visible rows less the header, clamped between \
             `MIN_TOOL_LINES` and `MAX_TOOL_LINES`. A short terminal shows fewer lines rather than a \
             different layout.",
        )
        .usage(1_902, 184);

    builder
        .user("turn-3", "Does a failed tool keep the same allowance?")
        .tool_started(
            "turn-3",
            "call-3",
            "run_shell",
            r#"{"argv":["cargo","test","-p","thndrs","budget"]}"#,
        )
        .tool_finished(
            "turn-3",
            "call-3",
            ToolStatus::Ok,
            &[
                "running 3 tests",
                "test budget::clamps_to_minimum ... ok",
                "test result: ok. 3 passed; 0 failed",
            ],
            None,
        )
        .assistant(
            "turn-3",
            "Yes. Status changes the marker and the color, not the allowance, and the tests above cover \
             the clamp at both ends.",
        )
        .usage(2_470, 142);
}

/// A tool still running while the assistant text for its turn has settled.
fn streaming_mid_tool(builder: &mut FixtureBuilder) {
    builder
        .user("turn-4", "Run the renderer suite and summarize what it covers.")
        .tool_started(
            "turn-4",
            "call-4",
            "run_shell",
            r#"{"argv":["cargo","test","-p","thndrs","renderer"]}"#,
        )
        .assistant(
            "turn-4",
            "The suite is running. It covers wrapping, tool group collapse, and the inline viewport \
             handoff, which is the part that decides what leaves the pane.",
        );
}

/// A finished tool whose output passed its budget.
fn tool_output_truncated(builder: &mut FixtureBuilder) {
    builder
        .user("turn-4", "List every renderer source file.")
        .tool_started(
            "turn-4",
            "call-4",
            "list_searchable_files",
            r#"{"glob":"crates/thndrs/src/cli/renderer/**/*.rs"}"#,
        )
        .tool_finished(
            "turn-4",
            "call-4",
            ToolStatus::Ok,
            &[
                "crates/thndrs/src/cli/renderer/mod.rs",
                "crates/thndrs/src/cli/renderer/ratatui.rs",
                "crates/thndrs/src/cli/renderer/transcript.rs",
                "crates/thndrs/src/cli/renderer/markdown.rs",
                "crates/thndrs/src/cli/renderer/highlight.rs",
                "crates/thndrs/src/cli/renderer/status.rs",
                "crates/thndrs/src/cli/renderer/input.rs",
                "crates/thndrs/src/cli/renderer/picker.rs",
                "crates/thndrs/src/cli/renderer/theme.rs",
                "crates/thndrs/src/cli/renderer/layout.rs",
            ],
            Some(truncated_artifact()),
        )
        .assistant(
            "turn-4",
            "The listing is longer than the allowance, so the group is collapsed to its head and the \
             rest stays recoverable through the artifact handle.",
        );
}

/// A tool waiting on approval, with no outcome recorded.
fn permission_prompt(builder: &mut FixtureBuilder) {
    builder
        .user("turn-4", "Apply the clamp fix to the transcript module.")
        .permission_request(
            "turn-4",
            "call-4",
            "write_patch: crates/thndrs/src/cli/app/transcript_blocks.rs",
        );
}

/// A turn that failed, with the recovery the next assistant message offers.
fn failed_turn(builder: &mut FixtureBuilder) {
    builder
        .user("turn-4", "Re-run the renderer suite against the release profile.")
        .tool_started(
            "turn-4",
            "call-4",
            "run_shell",
            r#"{"argv":["cargo","test","--release","-p","thndrs","renderer"]}"#,
        )
        .tool_finished(
            "turn-4",
            "call-4",
            ToolStatus::Failed,
            &[
                "error: could not compile `thndrs` (test `renderer`)",
                "error: linking with `cc` failed",
            ],
            None,
        )
        .failed("turn-4", "the turn stopped: `run_shell` exited 101");
}

/// Metadata for a tool result whose body was bounded before it was persisted.
fn truncated_artifact() -> ArtifactMetadata {
    ArtifactMetadata {
        schema_version: SCHEMA_VERSION,
        identity: String::from("fixture/tool-output-truncated/call-4"),
        kind: artifacts::ArtifactKind::ToolEvidence,
        handle: String::from("artifact-fixture-0004"),
        content_hash: "0".repeat(64),
        original_byte_count: 8_192,
        bounded_byte_count: 512,
        truncated: true,
        redacted: false,
        created_at: datetime::from_unix_seconds(FIXTURE_START_UNIX),
        created_at_unix: FIXTURE_START_UNIX,
        expires_at: None,
        expires_at_unix: None,
        retention: artifacts::ArtifactRetentionState::Active,
    }
}

/// The `--theme` value for one built-in theme.
///
/// A match rather than a lookup: a theme added to [`Theme`] fails to compile
/// here instead of producing a fixture with a made-up name.
fn theme_slug(theme: Theme) -> &'static str {
    match theme {
        Theme::EldritchMinimal => "eldritch-minimal",
        Theme::IcebergDark => "iceberg-dark",
        Theme::CatppuccinMocha => "catppuccin-mocha",
    }
}

/// The writer lock guarding a session file, as [`SessionWriter`] names it.
fn writer_lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.jsonl");
    path.with_file_name(format!("{file_name}.lock"))
}

/// Serialize records to JSONL and replace the file at `path`.
fn write_session(path: &Path, records: &[SessionRecord]) -> std::io::Result<()> {
    let mut body = String::new();
    for record in records {
        let line = record
            .to_json()
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        body.push_str(&line);
        body.push('\n');
    }
    std::fs::write(path, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn generated_in(dir: &Path) -> Vec<GeneratedFixture> {
        generate(dir).expect("generate fixtures")
    }

    #[test]
    fn every_generated_session_passes_a_validated_read() {
        let dir = tempdir().expect("temp fixture dir");
        for fixture in generated_in(dir.path()) {
            let records = SessionReader::read_validated_records(&fixture.path, &fixture.session_id)
                .unwrap_or_else(|error| panic!("{} is not readable: {error}", fixture.session_id));

            assert_eq!(
                records.len(),
                fixture.record_count,
                "{} record count",
                fixture.session_id
            );
            assert!(
                matches!(records.first(), Some(SessionRecord::SessionMeta { .. })),
                "{} does not open with metadata",
                fixture.session_id
            );
            assert!(records.len() > 1, "{} has no transcript", fixture.session_id);
        }
    }

    #[test]
    fn every_generated_session_replays_as_a_populated_transcript() {
        let dir = tempdir().expect("temp fixture dir");
        for fixture in generated_in(dir.path()) {
            let transcript = SessionReader::read_transcript_blocks(&fixture.path);
            assert!(
                transcript.blocks().len() > 1,
                "{} replays to one block or none",
                fixture.session_id
            );
        }
    }

    #[test]
    fn scenarios_cover_the_capture_set_past_startup() {
        let names: Vec<String> = FixtureScenario::all().into_iter().map(FixtureScenario::name).collect();

        assert_eq!(
            names,
            vec![
                "picker-open",
                "streaming-mid-tool",
                "tool-output-truncated",
                "permission-prompt",
                "error",
                "narrow-60-cols",
                "short-16-rows",
                "no-color",
                "theme-eldritch-minimal",
                "theme-iceberg-dark",
                "theme-catppuccin-mocha",
            ]
        );
        assert!(!names.iter().any(|name| name == "startup"), "startup loads no fixture");
    }

    #[test]
    fn theme_scenarios_track_the_theme_enum() {
        let names: Vec<String> = FixtureScenario::all().into_iter().map(FixtureScenario::name).collect();
        let themes = Theme::value_variants();

        assert_eq!(
            names.iter().filter(|name| name.starts_with("theme-")).count(),
            themes.len(),
            "one capture per built-in theme"
        );
        for theme in themes {
            let value = theme.to_possible_value().expect("theme is a clap value");
            assert_eq!(theme_slug(*theme), value.get_name(), "slug matches the --theme value");
            assert!(
                names.contains(&format!("theme-{}", value.get_name())),
                "{value:?} has a fixture"
            );
        }
    }

    #[test]
    fn session_ids_are_unique_and_match_their_file_stems() {
        let dir = tempdir().expect("temp fixture dir");
        let fixtures = generated_in(dir.path());
        let mut ids: Vec<String> = fixtures.iter().map(|fixture| fixture.session_id.clone()).collect();
        let count = ids.len();
        ids.sort();
        ids.dedup();

        assert_eq!(ids.len(), count, "session ids collide");
        for fixture in fixtures {
            assert_eq!(
                fixture.path.file_stem().and_then(|stem| stem.to_str()),
                Some(fixture.session_id.as_str())
            );
            assert_eq!(resolve_session_file(dir.path(), &fixture.session_id), Ok(fixture.path));
        }
    }

    #[test]
    fn generation_repeats_byte_for_byte() {
        let first = tempdir().expect("temp fixture dir");
        let second = tempdir().expect("temp fixture dir");
        let fixtures = generated_in(first.path());
        generated_in(second.path());
        generated_in(first.path());

        for fixture in fixtures {
            let repeated = std::fs::read(&fixture.path).expect("re-read fixture");
            let elsewhere = std::fs::read(second.path().join(format!("{}.jsonl", fixture.session_id)))
                .expect("read second fixture");
            assert_eq!(repeated, elsewhere, "{} is not deterministic", fixture.session_id);
        }
    }

    #[test]
    fn timestamps_are_fixed_and_iso_8601() {
        for scenario in FixtureScenario::all() {
            let records = scenario.records();
            let SessionRecord::SessionMeta { time, .. } = &records[0] else {
                panic!("{} does not open with metadata", scenario.name());
            };
            assert_eq!(time, "2026-01-05T09:00:00Z");
            for record in &records {
                let time = serde_json::to_value(record)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("time")
                            .and_then(|time| time.as_str())
                            .map(ToString::to_string)
                    })
                    .expect("record carries a time");
                assert!(time.contains('T') && time.ends_with('Z'), "`{time}` is not ISO 8601");
            }
        }
    }

    #[test]
    fn sequences_start_at_zero_and_increase_by_one() {
        for scenario in FixtureScenario::all() {
            let sequences: Vec<u64> = scenario.records().iter().map(SessionRecord::seq).collect();
            let expected: Vec<u64> = (0..sequences.len() as u64).collect();
            assert_eq!(sequences, expected, "{} has a gap in its sequence", scenario.name());
        }
    }

    #[test]
    fn the_streaming_scenario_leaves_a_tool_running() {
        let dir = tempdir().expect("temp fixture dir");
        let path = dir.path().join("streaming-mid-tool.jsonl");
        generated_in(dir.path());
        let transcript = SessionReader::read_transcript_blocks(&path);

        assert!(
            matches!(
                transcript.tool_entry("call-4"),
                Some(Entry::Tool { status: ToolStatus::Running, .. })
            ),
            "the last tool settled"
        );
    }

    #[test]
    fn the_truncated_scenario_marks_its_artifact() {
        let truncated = FixtureScenario::ToolOutputTruncated.records().into_iter().any(|record| {
            matches!(record, SessionRecord::ToolFinished { artifact: Some(artifact), .. } if artifact.truncated)
        });

        assert!(truncated, "no tool result is marked truncated");
    }

    #[test]
    fn the_permission_scenario_records_no_outcome() {
        let records = FixtureScenario::PermissionPrompt.records();

        assert!(
            records
                .iter()
                .any(|record| matches!(record, SessionRecord::AcpPermissionRequest { .. })),
            "no permission was requested"
        );
        assert!(
            !records
                .iter()
                .any(|record| matches!(record, SessionRecord::AcpPermissionOutcome { .. })),
            "the prompt is already answered"
        );
    }

    #[test]
    fn the_error_scenario_ends_in_a_failed_turn() {
        let records = FixtureScenario::Error.records();

        assert!(
            matches!(records.last(), Some(SessionRecord::Failed { .. })),
            "the turn did not fail"
        );
    }

    #[test]
    fn generation_replaces_a_stale_writer_lock() {
        let dir = tempdir().expect("temp fixture dir");
        generated_in(dir.path());
        let lock = dir.path().join("picker-open.jsonl.lock");
        std::fs::write(&lock, "").expect("write stale lock");

        generated_in(dir.path());

        assert!(!lock.exists(), "a stale lock survived generation");
    }

    #[test]
    fn generation_leaves_unrelated_sessions_alone() {
        let dir = tempdir().expect("temp fixture dir");
        let unrelated = dir.path().join("session-20260105-090000.jsonl");
        std::fs::create_dir_all(dir.path()).expect("create fixture dir");
        std::fs::write(&unrelated, "{}\n").expect("write unrelated session");

        generated_in(dir.path());

        assert_eq!(std::fs::read_to_string(&unrelated).expect("re-read"), "{}\n");
    }

    #[test]
    fn the_default_directory_is_ignored_scratch() {
        let dir = default_fixture_dir();

        assert_eq!(dir, PathBuf::from("target").join("tui-fixtures").join("sessions"));
        assert!(dir.is_relative(), "the default directory is workspace-relative");
    }
}
