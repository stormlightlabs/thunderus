use super::*;
use crate::context::ContextSource;
use prompt::PromptBundle;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn durable_exit_message_includes_session_id_and_resume_command() {
    let dir = tempfile::tempdir().expect("temp workspace");
    let app = App::from_cli(&Cli { cwd: dir.path().to_path_buf(), ..Cli::default() });

    assert_eq!(
        session_resume_message(&app),
        Some(format!(
            "Session ID: {}\nResume with: thndrs session resume {}",
            app.session.id, app.session.id
        ))
    );
}

#[test]
fn ephemeral_exit_has_no_resume_message() {
    let dir = tempfile::tempdir().expect("temp workspace");
    let app = App::from_cli(&Cli { cwd: dir.path().to_path_buf(), ephemeral: true, ..Cli::default() });

    assert_eq!(session_resume_message(&app), None);
}

fn test_agent_slot(
    receiver: mpsc::Receiver<app::AgentEvent>, cancel: CancelToken, steering: mpsc::Sender<String>,
) -> AgentSlot {
    let handle = harness::HarnessHandle::from_test_receiver(receiver, cancel);
    AgentSlot { request: test_request(), receiver: handle.events, cancel: handle.cancel, steering }
}

fn test_request() -> EffectRequest {
    EffectRequest { session_id: "test-session".to_string(), turn: 1 }
}

fn snapshot_bundle() -> PromptBundle {
    let source = ContextSource {
        path: PathBuf::from("/repo/AGENTS.md"),
        scope: ".".to_string(),
        content: "# Project\n\nBuild with cargo. Run tests with cargo test.\n".to_string(),
        content_hash: 12345,
        truncated: false,
        byte_count: 50,
    };
    PromptBundle {
        fragments: prompt::default_fragments(),
        environment: prompt::EnvironmentMetadata {
            cwd: "/repo".to_string(),
            model: "opencode/big-pickle".to_string(),
            date: "2026-06-29".to_string(),
        },
        project_context: vec![source],
        tool_catalog: tools::tool_definitions(),
        available_skills: Vec::new(),
        transcript_tail: Vec::new(),
        user_turn: "explain this repo".to_string(),
        history_reuse: prompt::HistoryReuse::Unavailable,
        prev_context_hash: None,
        context_ledger: None,
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|err| panic!("git {args:?} failed to start: {err}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn with_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = crate::test_env::lock();
    let old_home = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", home) };
    let result = f();
    unsafe {
        if let Some(old_home) = old_home {
            std::env::set_var("HOME", old_home);
        } else {
            std::env::remove_var("HOME");
        }
    }
    result
}

// FIXME: should be a include_str!
fn fake_agent_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fake_acp_agent.py")
}

#[derive(Default)]
struct TestSurface {
    events: Vec<&'static str>,
    clears: usize,
    draws: usize,
    full_repaints: usize,
    suspends: usize,
    size: (u16, u16),
}

impl InteractiveSurface for TestSurface {
    fn draw(&mut self, _app: &mut App, full_repaint: bool) -> io::Result<()> {
        self.events.push("draw");
        self.draws += 1;
        self.full_repaints += usize::from(full_repaint);
        Ok(())
    }

    fn resize(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.size = (width, height);
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.clears += 1;
        Ok(())
    }

    fn suspend(&mut self) -> io::Result<()> {
        self.events.push("suspend");
        self.suspends += 1;
        Ok(())
    }

    fn handle_navigation(&mut self, _app: &mut App, _action: &Action) -> bool {
        false
    }
}

fn presented_scheduler(now: Instant, frame_interval: Duration) -> PresentationScheduler {
    let mut scheduler = PresentationScheduler::new(frame_interval);
    scheduler.request_immediate();
    scheduler.mark_presented(now);
    scheduler
}

#[test]
fn scheduler_presents_background_update_when_the_frame_interval_has_elapsed() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let now = start + interval;
    let mut scheduler = presented_scheduler(start, interval);

    scheduler.request_throttled(now);

    assert!(scheduler.should_present(now));
    assert_eq!(scheduler.next_deadline(), Some(now));
}

#[test]
fn scheduler_coalesces_background_updates_at_the_earliest_legal_deadline() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let deadline = start + interval;
    let mut scheduler = presented_scheduler(start, interval);

    scheduler.request_throttled(start + Duration::from_millis(10));
    scheduler.request_throttled(start + Duration::from_millis(40));

    assert_eq!(scheduler.next_deadline(), Some(deadline));
    assert!(!scheduler.should_present(start + Duration::from_millis(99)));
    assert!(scheduler.should_present(deadline));
}

#[test]
fn scheduler_repeated_background_requests_do_not_delay_a_scheduled_frame() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let mut scheduler = presented_scheduler(start, interval);
    scheduler.request_throttled(start + Duration::from_millis(10));
    let first_deadline = scheduler.next_deadline();

    scheduler.request_throttled(start + Duration::from_millis(90));

    assert_eq!(scheduler.next_deadline(), first_deadline);
}

#[test]
fn scheduler_immediate_request_bypasses_background_throttling() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let now = start + Duration::from_millis(10);
    let mut scheduler = presented_scheduler(start, interval);
    scheduler.request_throttled(now);

    scheduler.request_immediate();

    assert!(scheduler.should_present(now));
    assert_eq!(scheduler.next_deadline(), None);
}

#[test]
fn scheduler_full_repaint_stays_sticky_until_a_frame_is_presented() {
    let now = Instant::now();
    let mut scheduler = PresentationScheduler::new(Duration::from_millis(100));

    scheduler.request_full_repaint();
    scheduler.request_throttled(now);

    assert!(scheduler.full_repaint_required());
    scheduler.mark_presented(now);
    assert!(!scheduler.full_repaint_required());
}

#[test]
fn scheduler_presenting_clears_dirty_and_deadline_state() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let deadline = start + interval;
    let mut scheduler = presented_scheduler(start, interval);
    scheduler.request_throttled(start + Duration::from_millis(10));

    scheduler.mark_presented(deadline);

    assert!(!scheduler.should_present(deadline));
    assert_eq!(scheduler.next_deadline(), None);
}

#[test]
fn scheduler_background_update_after_a_frame_schedules_the_next_frame() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);
    let first_frame = start + interval;
    let mut scheduler = presented_scheduler(start, interval);
    scheduler.request_throttled(start + Duration::from_millis(10));
    scheduler.mark_presented(first_frame);

    scheduler.request_throttled(first_frame + Duration::from_millis(20));

    assert_eq!(scheduler.next_deadline(), Some(first_frame + interval));
}

#[test]
fn presentation_boundary_draws_coalesced_full_repaint_once() {
    let now = Instant::now();
    let mut scheduler = PresentationScheduler::new(Duration::from_millis(100));
    scheduler.request_throttled(now);
    scheduler.request_full_repaint();
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    let mut surface = TestSurface::default();

    present_if_due(&mut surface, &mut app, &mut scheduler, now).expect("present frame");
    present_if_due(&mut surface, &mut app, &mut scheduler, now).expect("skip clean frame");

    assert_eq!(surface.draws, 1);
    assert_eq!(surface.full_repaints, 1);
}

#[test]
fn ephemeral_runs_do_not_create_per_session_logs() {
    let temp = tempfile::tempdir().expect("create workspace");

    let observability = init_tracing(temp.path(), "ephemeral", app::RunPersistence::Ephemeral);

    assert!(observability.is_none());
    assert!(!temp.path().join(".thndrs/logs/sessions/thndrs-ephemeral.log").exists());
}

fn acp_fixture_cli(cwd: &Path, script: &str) -> Cli {
    let mut agents = config::AcpAgentsConfig::new();
    agents.insert(
        "local".to_string(),
        config::AcpAgentConfig {
            command: "python3".to_string(),
            args: vec![fake_agent_fixture().display().to_string(), script.to_string()],
            timeout_secs: 2,
            ..config::AcpAgentConfig::default()
        },
    );
    Cli { cwd: cwd.to_path_buf(), acp_agents: agents, ephemeral: true, ..Cli::default() }
}

fn write_registry_fixture(path: &Path, version: &str) {
    std::fs::write(
        path,
        format!(
            r#"{{
                    "version": "1.0.0",
                    "agents": [{{
                        "id": "codex-acp",
                        "name": "Codex",
                        "version": "{version}",
                        "description": "ACP adapter",
                        "distribution": {{
                            "npx": {{
                                "package": "@agentclientprotocol/codex-acp@{version}",
                                "args": ["--acp"]
                            }}
                        }}
                    }}]
                }}"#
        ),
    )
    .expect("write registry");
}

fn write_fake_mcp_server(dir: &Path) -> PathBuf {
    let path = dir.join("fake_mcp_cli.py");
    std::fs::write(
        &path,
        r#"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "fake", "version": "0.1.0"}
            }
        }), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "Echo text",
                    "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}
                }]
            }
        }), flush=True)
    elif method == "tools/call":
        args = msg.get("params", {}).get("arguments", {})
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {"content": [{"type": "text", "text": args.get("text", "")}], "isError": False}
        }), flush=True)
    elif method == "resources/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {"resources": [{"uri": "memo://status", "name": "status", "mimeType": "text/plain", "size": 5}]}
        }), flush=True)
    elif method == "resources/read":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {"contents": [{"uri": msg["params"]["uri"], "mimeType": "text/plain", "text": "ready"}]}
        }), flush=True)
"#,
    )
    .expect("write fake mcp server");
    path
}

#[test]
fn render_print_prompt_snapshot() {
    let bundle = snapshot_bundle();
    let output = render_print_prompt(&bundle);
    insta::assert_snapshot!(output);
}

#[test]
fn render_print_prompt_redacts_secrets() {
    let mut bundle = snapshot_bundle();
    bundle.user_turn = "my key is sk-test123".to_string();
    let output = render_print_prompt(&bundle);
    assert!(
        !output.contains("sk-test123"),
        "secrets should be redacted in print-prompt output"
    );
    assert!(output.contains("sk-[REDACTED]"), "redacted marker should appear");
}

#[test]
fn render_print_prompt_includes_all_sections() {
    let bundle = snapshot_bundle();
    let output = render_print_prompt(&bundle);
    assert!(
        output.contains("=== System Prompt ==="),
        "should have system prompt section"
    );
    assert!(output.contains("=== Tool Catalog"), "should have tool catalog section");
    assert!(
        output.contains("=== Lowered Provider Messages"),
        "should have messages section"
    );
    assert!(
        output.contains("=== Environment ==="),
        "should have environment section"
    );
}

#[test]
fn render_print_prompt_config_includes_effective_config_metadata() {
    let mut origins = std::collections::BTreeMap::new();
    origins.insert(
        "model".to_string(),
        config::ConfigOrigin { source: config::ConfigSource::CliFlag, detail: "--model".to_string() },
    );
    let cli = Cli {
        model: "opencode/gpt-5.6-luna".to_string(),
        session_dir: Some(PathBuf::from("/repo/custom-sessions")),
        config_layers: vec![config::LoadedConfigLayer {
            source: config::ConfigSource::ProjectFile,
            config: config::Config::default(),
            path: None,
            display_path: Some(".thndrs/config.toml".to_string()),
            hash: Some("abc123".to_string()),
        }],
        config_origins: origins,
        config_diagnostics: vec!["diagnostic with sk-secret".to_string()],
        ..Cli::default()
    };

    let output = render_print_prompt_config(&cli, Path::new("/repo"));

    assert!(output.contains("=== Effective Config ==="));
    assert!(output.contains("provider: opencode-zen"));
    assert!(output.contains("model: opencode/gpt-5.6-luna"));
    assert!(!output.contains("search:"));
    assert!(output.contains("workspace: /repo"));
    assert!(output.contains("session_dir: /repo/custom-sessions"));
    assert!(output.contains("project .thndrs/config.toml abc123"));
    assert!(output.contains("model: cli:--model"));
    assert!(output.contains("sk-[REDACTED]secret"));

    insta::assert_snapshot!(output, @r###"


=== Effective Config ===
  provider: opencode-zen
  model: opencode/gpt-5.6-luna
  workspace: /repo
  session_dir: /repo/custom-sessions
  files:
    project .thndrs/config.toml abc123
  origins:
    model: cli:--model
  diagnostics:
    diagnostic with sk-[REDACTED]secret
"###);
}

#[test]
fn render_print_prompt_date_is_redacted() {
    let bundle = snapshot_bundle();
    let output = render_print_prompt(&bundle);
    let env_section = output.split("=== Environment ===").nth(1).unwrap_or("");
    assert!(
        env_section.contains("date: [date]"),
        "date in env section should be redacted to [date] for snapshot stability"
    );
}

#[test]
fn redact_secret_replaces_sk_prefix() {
    let result = redact_secret("token: sk-abc123 rest");
    assert_eq!(result, "token: sk-[REDACTED]abc123 rest");
}

#[test]
fn acp_list_renders_enabled_and_disabled_agents() {
    let mut agents = config::AcpAgentsConfig::new();
    agents.insert(
        "disabled".to_string(),
        config::AcpAgentConfig {
            command: "agent-off".to_string(),
            enabled: false,
            ..config::AcpAgentConfig::default()
        },
    );
    agents.insert(
        "local".to_string(),
        config::AcpAgentConfig {
            command: "agent".to_string(),
            args: vec!["--acp".to_string()],
            ..config::AcpAgentConfig::default()
        },
    );
    let cli = Cli { acp_agents: agents, ..Cli::default() };

    let output = render_acp_list(&cli);

    assert!(output.contains("disabled\tdisabled\tagent-off"));
    assert!(output.contains("local\tenabled\tagent --acp"));
}

#[test]
fn acp_inspect_renders_redacted_config_details() {
    let mut agents = config::AcpAgentsConfig::new();
    agents.insert(
        "local".to_string(),
        config::AcpAgentConfig {
            command: "agent".to_string(),
            args: vec!["--acp".to_string()],
            env: std::collections::BTreeMap::from([
                ("TOKEN".to_string(), "secret-value".to_string()),
                ("PLAIN".to_string(), "visible-value".to_string()),
            ]),
            timeout_secs: 12,
            ..config::AcpAgentConfig::default()
        },
    );
    let mut origins = std::collections::BTreeMap::new();
    origins.insert(
        "acp_agents".to_string(),
        config::ConfigOrigin { source: config::ConfigSource::ProjectFile, detail: ".thndrs/config.toml".to_string() },
    );
    let cli = Cli { acp_agents: agents, config_origins: origins, ..Cli::default() };

    let output = render_acp_inspect(&cli, "local").expect("inspect");

    assert!(output.contains("name: local"));
    assert!(output.contains("status: enabled"));
    assert!(output.contains("command: agent --acp"));
    assert!(output.contains("args: --acp"));
    assert!(output.contains("env_keys: PLAIN, TOKEN"));
    assert!(output.contains("timeout_secs: 12"));
    assert!(output.contains("source: project:.thndrs/config.toml"));
    assert!(!output.contains("secret-value"));
    assert!(!output.contains("visible-value"));
}

#[test]
fn acp_smoke_runs_fake_agent_and_prints_events() {
    let temp = tempfile::tempdir().expect("temp dir");
    let mut agents = config::AcpAgentsConfig::new();
    agents.insert(
        "local".to_string(),
        config::AcpAgentConfig {
            command: "python3".to_string(),
            args: vec![fake_agent_fixture().display().to_string(), "lifecycle".to_string()],
            timeout_secs: 2,
            ..config::AcpAgentConfig::default()
        },
    );
    let cli = Cli { cwd: temp.path().to_path_buf(), acp_agents: agents, ..Cli::default() };
    let mut output = Vec::new();

    run_acp_smoke(&cli, "local", "ping", &mut output).expect("smoke run");
    let output = String::from_utf8(output).expect("utf8");

    assert!(output.contains("started"));
    assert!(output.contains("status: acp: connected to fake-acp-agent 0.0.0"));
    assert!(output.contains("acp_session: local fake-session-1"));
    assert!(output.contains("pong from fake ACP agent"));
    assert!(output.contains("finished"));
}

#[test]
fn acp_logout_runs_fake_agent_and_prints_result() {
    let mut agents = config::AcpAgentsConfig::new();
    agents.insert(
        "local".to_string(),
        config::AcpAgentConfig {
            command: "python3".to_string(),
            args: vec![fake_agent_fixture().display().to_string(), "auth-success".to_string()],
            timeout_secs: 2,
            ..config::AcpAgentConfig::default()
        },
    );
    let cli = Cli { acp_agents: agents, ..Cli::default() };
    let mut output = Vec::new();

    run_acp_logout(&cli, "local", &mut output).expect("logout run");
    let output = String::from_utf8(output).expect("utf8");

    assert!(output.contains("acp: logged out `local`"));
}

#[test]
fn acp_list_sessions_runs_fake_agent_and_prints_sessions() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cli = acp_fixture_cli(temp.path(), "sessions");
    let mut output = Vec::new();

    run_acp_list_sessions(&cli, "local", &mut output).expect("list sessions");
    let output = String::from_utf8(output).expect("utf8");
    assert!(output.contains("external-session-1"));
    assert!(output.contains("Fixture Session"));
    assert!(output.contains("2026-07-04T00:00:00Z"));
}

#[test]
fn acp_load_session_runs_fake_agent_and_prints_replay() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cli = acp_fixture_cli(temp.path(), "sessions");
    let mut output = Vec::new();

    run_acp_load_session(&cli, "local", "external-session-1", &mut output).expect("load session");
    let output = String::from_utf8(output).expect("utf8");
    assert!(output.contains("replayed external-session-1"));
    assert!(output.contains("loaded: local external-session-1"));
}

#[test]
fn acp_resume_and_close_session_run_fake_agent() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cli = acp_fixture_cli(temp.path(), "sessions");
    let mut resume_output = Vec::new();
    let mut close_output = Vec::new();

    run_acp_resume_session(&cli, "local", "external-session-1", &mut resume_output).expect("resume session");
    run_acp_close_session(&cli, "local", "external-session-1", &mut close_output).expect("close session");
    let resume_output = String::from_utf8(resume_output).expect("utf8");
    let close_output = String::from_utf8(close_output).expect("utf8");
    assert!(resume_output.contains("acp_session: local external-session-1"));
    assert!(resume_output.contains("resumed: local external-session-1"));
    assert!(close_output.contains("acp: closed `local` session external-session-1"));
}

#[test]
fn session_list_and_latest_print_local_session_summaries() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    let mut writer = session::SessionWriter::create(
        &session_dir,
        "session-new",
        "/repo",
        "Latest Work",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    writer.append_usage(7, 11).expect("append usage");

    let mut list_output = Vec::new();
    let mut progress = Vec::new();
    let mut latest_output = Vec::new();
    let cancellation = CancelToken::new();

    run_session_list(
        &session_dir,
        temp.path(),
        &mut list_output,
        &mut progress,
        &cancellation,
    )
    .expect("list sessions");
    run_session_latest(&session_dir, &mut latest_output).expect("latest session");
    let list_output = String::from_utf8(list_output).expect("utf8");
    let latest_output = String::from_utf8(latest_output).expect("utf8");

    assert!(list_output.contains("session-new"));
    assert!(list_output.contains("Latest Work"));
    assert!(list_output.contains("opencode/big-pickle"));
    assert_eq!(String::from_utf8(progress).expect("utf8"), "Scanning sessions...\n");
    assert!(list_output.contains("activity "));
    assert!(list_output.contains("source root"));
    assert!(list_output.contains("locked"));
    assert!(list_output.contains("in 7 out 11"));
    assert!(latest_output.contains("id: session-new"));
    assert!(latest_output.contains("title: Latest Work"));
    assert!(latest_output.contains("tokens: in 7 out 11"));
}

#[test]
fn scan_heavy_session_commands_write_progress_separately_from_results() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    let cli = Cli { cwd: temp.path().to_path_buf(), ..Cli::default() };
    let cancellation = CancelToken::new();

    let mut prune_output = Vec::new();
    let mut prune_progress = Vec::new();
    run_session_prune(
        &cli,
        &session_dir,
        temp.path(),
        SessionPruneRequest {
            overrides: session::PruneOverrides { older_than_days: None, keep_count: Some(50) },
            dry_run: true,
            format: SessionReportFormat::Human,
        },
        &mut prune_output,
        &mut prune_progress,
        &cancellation,
    )
    .expect("prune preview");
    assert!(
        String::from_utf8(prune_progress)
            .expect("utf8")
            .starts_with("Scanning sessions for pruning...\n")
    );
    assert!(
        String::from_utf8(prune_output)
            .expect("utf8")
            .contains("would move 0 session(s)")
    );

    let mut storage_output = Vec::new();
    let mut storage_progress = Vec::new();
    run_session_storage(
        &cli,
        &session_dir,
        temp.path(),
        SessionReportFormat::Human,
        &mut storage_output,
        &mut storage_progress,
        &cancellation,
    )
    .expect("storage report");
    assert_eq!(
        String::from_utf8(storage_progress).expect("utf8"),
        "Scanning session storage...\n"
    );
    assert!(String::from_utf8(storage_output).expect("utf8").contains("live 0"));

    let mut purge_output = Vec::new();
    let mut purge_progress = Vec::new();
    run_session_purge(
        &session_dir,
        temp.path(),
        SessionPurgeRequest { confirmed: false, allow_pinned: false, format: SessionReportFormat::Human },
        &mut purge_output,
        &mut purge_progress,
        &cancellation,
    )
    .expect("purge preview");
    assert_eq!(
        String::from_utf8(purge_progress).expect("utf8"),
        "Scanning sessions for purge...\n"
    );
    assert!(
        String::from_utf8(purge_output)
            .expect("utf8")
            .contains("purge would remove 0 session(s)")
    );
}

#[test]
fn session_prune_reports_live_move_progress_and_a_concise_human_result() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    for session_id in ["session-oldest", "session-older"] {
        drop(
            session::SessionWriter::create(
                &session_dir,
                session_id,
                temp.path().to_str().expect("workspace path"),
                session_id,
                "provider",
                "model",
                "none",
                "1",
                None,
            )
            .expect("session writer"),
        );
    }
    let mut cli = Cli { cwd: temp.path().to_path_buf(), ..Cli::default() };
    cli.session_retention.min_age_days = 0;
    let cancellation = CancelToken::new();
    let mut output = Vec::new();
    let mut progress = Vec::new();

    run_session_prune(
        &cli,
        &session_dir,
        temp.path(),
        SessionPruneRequest {
            overrides: session::PruneOverrides { older_than_days: None, keep_count: Some(0) },
            dry_run: false,
            format: SessionReportFormat::Human,
        },
        &mut output,
        &mut progress,
        &cancellation,
    )
    .expect("prune sessions");

    let progress = String::from_utf8(progress).expect("utf8");
    assert!(progress.contains("Moving 2 session(s) to trash..."));
    assert!(progress.contains("Prune progress: 2/2 processed (2 moved, 0 failed)"));
    let output = String::from_utf8(output).expect("utf8");
    assert!(output.contains("moved 2 of 2 selected session(s)"));
    assert!(output.contains("selected by live-session keep limit: 2"));
    assert!(!output.contains("session-oldest"));
    assert!(!output.contains("LiveCount"));
}

#[test]
fn session_titles_prints_titles_newest_first() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    session::SessionWriter::create(
        &session_dir,
        "session-old",
        "/repo",
        "Old",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create old session");
    std::thread::sleep(Duration::from_millis(5));
    session::SessionWriter::create(
        &session_dir,
        "session-new",
        "/repo",
        "New",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create new session");

    let mut output = Vec::new();

    run_session_titles(&session_dir, &mut output).expect("titles");
    let output = String::from_utf8(output).expect("utf8");

    assert_eq!(output.lines().collect::<Vec<_>>(), vec!["New", "Old"]);
}

#[test]
fn session_show_prints_replayable_transcript() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    let mut writer = session::SessionWriter::create(
        &session_dir,
        "session-show",
        "/repo",
        "Show",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    writer
        .append_entry(&app::Entry::User { text: "hello".to_string() }, "turn_1")
        .expect("append user");
    writer
        .append_entry(
            &app::Entry::Agent { text: "hi".to_string(), streaming: false },
            "turn_1",
        )
        .expect("append assistant");
    let mut output = Vec::new();

    run_session_show(&session_dir, "session-show", &mut output).expect("show session");
    let output = String::from_utf8(output).expect("utf8");

    assert!(output.contains("user: hello"));
    assert!(output.contains("assistant: hi"));
}

#[test]
fn session_rename_projects_the_name_without_changing_the_id() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    let mut writer = session::SessionWriter::create(
        &session_dir,
        "session-named",
        "/repo",
        "Original",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    writer
        .append_entry(&app::Entry::User { text: "hello".to_string() }, "turn_1")
        .expect("append user");
    drop(writer);

    let mut rename = Vec::new();
    run_session_rename(&session_dir, "session-nam", "Named work", &mut rename).expect("rename session");

    let mut list = Vec::new();
    let mut progress = Vec::new();
    let mut show = Vec::new();
    let mut inspect = Vec::new();
    let mut export = Vec::new();
    run_session_list(&session_dir, temp.path(), &mut list, &mut progress, &CancelToken::new()).expect("list sessions");
    run_session_show(&session_dir, "session-named", &mut show).expect("show session");
    run_session_inspect(&session_dir, "session-named", SessionDataFormat::Json, &mut inspect).expect("inspect");
    run_session_export(&session_dir, "session-named", SessionDataFormat::Jsonl, &mut export).expect("export");

    for output in [&rename, &list, &show, &inspect, &export] {
        assert!(String::from_utf8_lossy(output).contains("Named work"));
    }
    assert_eq!(
        session::resolve_session_file(&session_dir, "session-named")
            .expect("same session id")
            .file_stem()
            .and_then(|stem| stem.to_str()),
        Some("session-named")
    );
}

#[test]
fn session_inspect_and_export_are_redacted_and_sequence_ordered() {
    let temp = tempfile::tempdir().expect("temp dir");
    let session_dir = temp.path().join("sessions");
    let mut writer = session::SessionWriter::create(
        &session_dir,
        "session-inspect",
        "/repo",
        "Inspect",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    writer
        .append_entry(
            &app::Entry::User { text: "api_key=sk-secretvalue123".to_string() },
            "turn_1",
        )
        .expect("append user");
    writer.append_usage(2, 3).expect("append usage");
    drop(writer);

    let mut inspect = Vec::new();
    let mut export = Vec::new();
    run_session_inspect(&session_dir, "session-ins", SessionDataFormat::Json, &mut inspect).expect("inspect");
    run_session_export(&session_dir, "session-inspect", SessionDataFormat::Jsonl, &mut export).expect("export");
    let inspect = String::from_utf8(inspect).expect("utf8");
    let export = String::from_utf8(export).expect("utf8");

    assert!(inspect.contains("\"input_tokens\": 2"));
    assert!(inspect.contains("api_key=[REDACTED]"));
    assert!(!inspect.contains("sk-secretvalue123"));
    let lines = export.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("session_meta"));
    assert!(lines[1].contains("user"));
    assert!(lines[2].contains("usage"));
}

#[test]
fn debug_session_log_reads_a_bounded_redacted_tail() {
    let temp = tempfile::tempdir().expect("temp dir");
    let sessions_dir = session::sessions_dir(temp.path());
    let writer = session::SessionWriter::create(
        &sessions_dir,
        "session-log",
        "/repo",
        "Log",
        "opencode-zen",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    drop(writer);
    let log_dir = temp.path().join(".thndrs").join("logs").join("sessions");
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    std::fs::write(
        log_dir.join("thndrs-session-log.log"),
        "first\napi_key=sk-secretvalue123\nlast\n",
    )
    .expect("write log");

    let mut output = Vec::new();
    run_debug_session_log(temp.path(), "session-l", 2, &mut output).expect("read log");
    let output = String::from_utf8(output).expect("utf8");

    assert!(!output.contains("first"));
    assert!(output.contains("api_key=[REDACTED]"));
    assert!(!output.contains("sk-secretvalue123"));
    assert!(output.contains("last"));
}

#[test]
fn acp_registry_reads_file_and_prints_review_gate() {
    let temp = tempfile::tempdir().expect("temp dir");
    let registry_path = temp.path().join("registry.json");
    std::fs::write(
        &registry_path,
        r#"{
                "version": "1.0.0",
                "agents": [{
                    "id": "codex-acp",
                    "name": "Codex",
                    "version": "1.1.0",
                    "description": "ACP adapter for OpenAI's coding assistant",
                    "repository": "https://github.com/agentclientprotocol/codex-acp",
                    "distribution": {
                        "npx": {
                            "package": "@agentclientprotocol/codex-acp@1.1.0",
                            "env": {"OPENAI_API_KEY": "sk-secret"}
                        }
                    }
                }]
            }"#,
    )
    .expect("write registry");
    let mut output = Vec::new();

    run_acp_registry(Some(&registry_path), &mut output).expect("registry");
    let output = String::from_utf8(output).expect("utf8");

    assert!(output.contains("ACP registry v1.0.0"));
    assert!(output.contains("codex-acp\tCodex\t1.1.0\tnpx:@agentclientprotocol/codex-acp@1.1.0"));
    assert!(output.contains("install/update: use `thndrs acp install"));
    assert!(!output.contains("OPENAI_API_KEY"));
    assert!(!output.contains("sk-secret"));
}

#[test]
fn acp_install_and_update_registry_agent() {
    let temp = tempfile::tempdir().expect("temp dir");
    let registry_path = temp.path().join("registry.json");
    write_registry_fixture(&registry_path, "1.1.0");
    let cli = Cli { cwd: temp.path().to_path_buf(), ..Cli::default() };
    let mut install_output = Vec::new();

    run_acp_install(
        &cli,
        "codex-acp",
        Some("codex".to_string()),
        Some(&registry_path),
        true,
        &mut install_output,
    )
    .expect("install");
    let install_output = String::from_utf8(install_output).expect("utf8");

    assert!(install_output.contains("installed: codex codex-acp 1.1.0"));
    assert!(install_output.contains("model: acp:codex"));

    write_registry_fixture(&registry_path, "1.2.0");
    let mut update_output = Vec::new();
    run_acp_update(&cli, "codex", Some(&registry_path), true, &mut update_output).expect("update");
    let update_output = String::from_utf8(update_output).expect("utf8");

    assert!(update_output.contains("updated: codex codex-acp 1.2.0"));
    let config = std::fs::read_to_string(temp.path().join(".thndrs/config.toml")).expect("config");
    assert!(config.contains("@agentclientprotocol/codex-acp@1.2.0"));
}

#[test]
fn mcp_list_tools_and_call_use_fake_server() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(workspace.join(".thndrs")).expect("create config dir");
    let script = write_fake_mcp_server(tmp.path());
    std::fs::write(
        workspace.join(".thndrs").join("mcp.toml"),
        format!(
            r#"
                [servers.docs]
                command = "python3"
                args = [{:?}]
                timeout_secs = 5
                "#,
            script.display().to_string()
        ),
    )
    .expect("write mcp config");
    let cli = Cli { cwd: workspace, ..Cli::default() };

    with_home(&home, || {
        let mut blocked_output = Vec::new();
        run_mcp_list(&cli, &mut blocked_output).expect("list blocked mcp");
        let blocked = String::from_utf8(blocked_output).expect("utf8");
        assert!(blocked.contains("blocked by trust"));

        let mut trust_output = Vec::new();
        run_mcp_trust(&cli, &mut trust_output).expect("trust mcp");

        let mut list_output = Vec::new();
        run_mcp_list(&cli, &mut list_output).expect("list mcp");
        let list = String::from_utf8(list_output).expect("utf8");
        assert!(list.contains("docs"));
        assert!(list.contains("source=project"));
        assert!(list.contains("execution=local-process\tpermissions=thndrs-process"));

        let mut tools_output = Vec::new();
        run_mcp_tools(&cli, "docs", &mut tools_output).expect("tools mcp");
        let tools = String::from_utf8(tools_output).expect("utf8");
        assert!(tools.contains("mcp__docs__echo"));

        let mut call_output = Vec::new();
        run_mcp_call(&cli, "docs", "echo", r#"{"text":"hello"}"#, &mut call_output).expect("call mcp");
        let call = String::from_utf8(call_output).expect("utf8");
        assert!(call.contains("hello"));

        let mut resources_output = Vec::new();
        run_mcp_resources(&cli, "docs", &mut resources_output).expect("list resources");
        let resources = String::from_utf8(resources_output).expect("utf8");
        assert!(resources.contains("mcp__docs__resource"));
        assert!(resources.contains("uri=memo://status"));

        let mut resource_output = Vec::new();
        run_mcp_resource(&cli, "docs", "memo://status", &mut resource_output).expect("read resource");
        let resource: serde_json::Value = serde_json::from_slice(&resource_output).expect("resource JSON");
        assert_eq!(resource["contents"][0]["kind"], "text");
        assert_eq!(resource["contents"][0]["data"], "ready");
        assert_eq!(resource["contents"][0]["mime_type"], "text/plain");

        let mut revoke_output = Vec::new();
        run_mcp_revoke(&cli, &mut revoke_output).expect("revoke mcp");
        let error = run_mcp_tools(&cli, "docs", &mut Vec::new()).expect_err("revoked server blocked");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    });
}

#[test]
fn mcp_add_and_remove_write_scoped_configuration_without_trusting_or_starting_servers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(workspace.join(".thndrs")).expect("create workspace config");
    let project_path = workspace.join(".thndrs/mcp.toml");
    std::fs::write(
        &project_path,
        "# Keep this comment.\n[servers.keep]\ncommand = \"keep-mcp\"\n\n[servers.docs]\ncommand = \"old-mcp\"\n",
    )
    .expect("write project config");
    let cli = Cli { cwd: workspace.clone(), ..Cli::default() };

    with_home(&home, || {
        let mut project_output = Vec::new();
        run_mcp_add(
            &cli,
            "docs",
            crate::cli::commands::mcp::McpConfigScope::Project,
            Some("npx"),
            &["-y".to_string(), "@vendor/docs".to_string()],
            None,
            &mut project_output,
        )
        .expect("add project stdio server");
        let project_output = String::from_utf8(project_output).expect("utf8");
        assert!(project_output.contains("thndrs mcp status"));
        assert!(project_output.contains("thndrs mcp trust"));
        let project = std::fs::read_to_string(&project_path).expect("read project config");
        assert!(project.contains("# Keep this comment."));
        assert!(project.contains("[servers.keep]"));
        assert!(project.contains("command = \"npx\""));
        assert!(project.contains("args = [\"-y\", \"@vendor/docs\"]"));
        let effective = mcp::config::load_effective_mcp(&workspace, &[]).expect("load project config");
        assert!(!effective.config.servers.contains_key("keep"));
        assert!(effective.blocked_project_servers.contains_key("docs"));
        assert!(effective.blocked_project_servers.contains_key("keep"));

        let mut global_output = Vec::new();
        run_mcp_add(
            &cli,
            "search",
            crate::cli::commands::mcp::McpConfigScope::Global,
            None,
            &[],
            Some("https://mcp.example.test/mcp"),
            &mut global_output,
        )
        .expect("add global HTTP server");
        assert!(!String::from_utf8(global_output).expect("utf8").contains("mcp trust"));
        let global = std::fs::read_to_string(home.join(".thndrs/mcp.toml")).expect("read global config");
        assert!(global.contains("transport = \"streamable_http\""));
        assert!(global.contains("url = \"https://mcp.example.test/mcp\""));

        let mut remove_output = Vec::new();
        run_mcp_remove(
            &cli,
            "docs",
            crate::cli::commands::mcp::McpConfigScope::Project,
            &mut remove_output,
        )
        .expect("remove project server");
        assert!(
            String::from_utf8(remove_output)
                .expect("utf8")
                .contains("removed MCP server `docs`")
        );
        let project = std::fs::read_to_string(project_path).expect("read project config");
        assert!(project.contains("[servers.keep]"));
        assert!(!project.contains("[servers.docs]"));
    });
}

#[test]
fn mcp_catalog_commands_use_global_sources_and_offline_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("home");

    with_home(&home, || {
        mcp::catalog::add_source("community", "https://catalog.example", Some("community review"))
            .expect("add catalog");
        let entry = mcp::catalog::CatalogEntry {
            source: "community".to_string(),
            source_url: "https://catalog.example".to_string(),
            name: "io.example/weather".to_string(),
            title: Some("Weather".to_string()),
            description: "Weather forecasts".to_string(),
            claimed_publisher: "io.example".to_string(),
            version: "1.2.3".to_string(),
            status: Some("active".to_string()),
            transports: vec!["stdio".to_string(), "streamable-http".to_string()],
            packages: vec![mcp::catalog::CatalogPackage {
                registry_type: "npm".to_string(),
                registry_url: Some("https://registry.npmjs.org".to_string()),
                identifier: "@example/weather".to_string(),
                version: Some("1.2.3".to_string()),
                sha256: Some("catalog-assertion".to_string()),
                transports: vec!["stdio".to_string()],
                platform_constraints: vec!["linux".to_string()],
            }],
            platform_constraints: vec!["linux".to_string()],
            curation_claim: "community review".to_string(),
        };
        let cache = serde_json::json!({"retrieved_at": "2026-08-18T00:00:00Z", "entries": [entry]});
        let cache_path = home.join(".thndrs/mcp-catalog-cache/community.json");
        std::fs::create_dir_all(cache_path.parent().expect("cache parent")).expect("cache parent");
        std::fs::write(cache_path, serde_json::to_vec(&cache).expect("cache JSON")).expect("cache");

        let mut list = Vec::new();
        run_mcp_catalog_list(&mut list).expect("list catalogs");
        let list = String::from_utf8(list).expect("utf8");
        assert!(list.contains("official\tenabled\tbuilt-in"));
        assert!(list.contains("community\tenabled\tcustom"));
        assert!(list.contains("global only"));

        let mut search = Vec::new();
        run_mcp_catalog_search("weather", 20, None, true, &mut search).expect("offline search");
        let search = String::from_utf8(search).expect("utf8");
        assert!(search.contains("io.example/weather"));
        assert!(search.contains("cache from 2026-08-18T00:00:00Z"));
        assert!(search.contains("does not verify publisher identity"));

        let mut detail = Vec::new();
        run_mcp_catalog_show("io.example/weather", Some("community"), "latest", true, &mut detail)
            .expect("offline detail");
        let detail = String::from_utf8(detail).expect("utf8");
        assert!(detail.contains("claimed publisher: io.example (catalog claim)"));
        assert!(detail.contains("available transports: stdio, streamable-http"));
        assert!(detail.contains("digest=catalog-assertion"));
        assert!(detail.contains("does not start a server"));

        mcp::catalog::set_source_enabled("official", false).expect("disable official");
        assert!(
            std::fs::read_to_string(home.join(".thndrs/mcp-catalogs.toml"))
                .expect("catalog config")
                .contains("enabled = false")
        );
    });
}

#[test]
fn mcp_add_rejects_invalid_transport_and_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cli = Cli { cwd: tmp.path().to_path_buf(), ..Cli::default() };

    let transport_error = run_mcp_add(
        &cli,
        "docs",
        crate::cli::commands::mcp::McpConfigScope::Project,
        None,
        &[],
        None,
        &mut Vec::new(),
    )
    .expect_err("missing transport rejected");
    assert_eq!(transport_error.kind(), io::ErrorKind::InvalidInput);

    let name_error = run_mcp_add(
        &cli,
        "bad/name",
        crate::cli::commands::mcp::McpConfigScope::Project,
        Some("docs-mcp"),
        &[],
        None,
        &mut Vec::new(),
    )
    .expect_err("invalid name rejected");
    assert_eq!(name_error.kind(), io::ErrorKind::Other);
}

#[test]
fn clear_resets_application_and_render_surface() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.transcript
        .entries
        .push(app::Entry::User { text: "hello".to_string() });

    let mut surface = TestSurface::default();
    handle_msg(&mut app, Msg::Clear, &mut surface, &mut None).expect("clear");
    assert!(app.transcript.entries.is_empty());
    assert_eq!(surface.clears, 1);
}

#[test]
fn suspension_settles_the_current_frame_before_the_terminal_effect() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    let mut surface = TestSurface::default();

    suspend_terminal(&mut surface, &mut app, &mut None).expect("suspend");

    assert_eq!(surface.events, ["draw", "suspend"]);
    assert_eq!(surface.suspends, 1);
}

#[test]
fn flush_steering_sends_queued_messages_to_active_agent() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Working;
    app.composer.queue.push(
        app::QueueTarget::Steering,
        "use the failing test first".to_string(),
        "test".to_string(),
    );
    let (event_tx, event_rx) = mpsc::channel();
    drop(event_tx);
    let (steering_tx, steering_rx) = mpsc::channel();
    let slot = test_agent_slot(event_rx, CancelToken::new(), steering_tx);

    flush_steering(&mut app, &Some(slot));

    assert!(
        app.composer.queue.pending_count(app::QueueTarget::Steering) == 0,
        "sent steering should leave the app queue"
    );
    assert_eq!(
        steering_rx.try_recv().expect("active run should receive steering"),
        "use the failing test first"
    );
}

#[test]
fn cancel_effect_keeps_stopping_slot_until_terminal_event() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Stopping;
    let (_event_tx, event_rx) = mpsc::channel();
    let (steering_tx, _steering_rx) = mpsc::channel();
    let cancel = CancelToken::new();
    let mut agent = Some(test_agent_slot(event_rx, cancel.clone(), steering_tx));

    execute_effect(
        &mut app,
        &mut agent,
        &mut TestSurface::default(),
        Effect::CancelAgent(test_request()),
    )
    .expect("cancel effect");

    assert!(agent.is_some(), "stopping should keep the receiver for terminal events");
    assert!(
        cancel.is_cancelled(),
        "stopping should still signal cooperative cancellation"
    );
}

#[test]
fn stopping_timeout_marks_terminal_state_for_repaint() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Stopping;
    app.transcript
        .entries
        .push(app::Entry::Status { text: "cancelled".to_string() });
    app.runtime.stopping_deadline = Some(0);
    let mut surface = TestSurface::default();
    let before = app.runtime.run_state.clone();

    handle_msg(&mut app, Msg::Tick, &mut surface, &mut None).expect("tick");

    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert_eq!(app.status_label(), "Stopped");
    assert!(app.runtime.stopping_timed_out, "the UI must detach an unsettled worker");
    assert!(
        tick_requires_render(&before, false, &app),
        "the terminal state must replace the last stopping frame"
    );
}

#[test]
fn timed_out_stopping_agent_is_detached_without_blocking_the_ui() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Idle;
    app.runtime.stopping_timed_out = true;

    let run = thndrs_agent::AgentRun::<app::AgentEvent>::spawn(CancelToken::new(), |_sender, _cancel| {
        std::thread::sleep(std::time::Duration::from_millis(250));
    });
    let cancel = run.cancel().clone();
    let (steering_tx, _steering_rx) = mpsc::channel();
    let mut agent = Some(AgentSlot { request: test_request(), receiver: run, cancel, steering: steering_tx });
    let started = Instant::now();

    settle_agent(&mut agent, &test_request(), true);

    assert!(agent.is_none());
    assert!(
        started.elapsed() < std::time::Duration::from_millis(100),
        "a timed-out worker must not freeze the render loop"
    );
}

#[test]
fn disconnected_stopping_agent_returns_app_to_idle() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Stopping;
    let (event_tx, event_rx) = mpsc::channel();
    drop(event_tx);
    let (steering_tx, _steering_rx) = mpsc::channel();
    let mut agent = Some(test_agent_slot(event_rx, CancelToken::new(), steering_tx));
    let mut surface = TestSurface::default();
    drain_agent_events(&mut app, &mut agent, &mut surface, &None).expect("drain events");

    assert!(agent.is_none(), "disconnected slot should be cleared");
    assert_eq!(app.runtime.run_state, RunState::Idle);
}

#[test]
fn panicked_agent_worker_becomes_a_visible_failure() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Working;
    let receiver = thndrs_agent::AgentRun::spawn(CancelToken::new(), |_sender, _cancel| {
        panic!("test worker panic");
    });
    assert!(receiver.recv().is_err(), "worker should disconnect its event stream");
    let cancel = receiver.cancel().clone();
    let (steering_tx, _steering_rx) = mpsc::channel();
    let mut agent = Some(AgentSlot { request: test_request(), receiver, cancel, steering: steering_tx });
    let mut surface = TestSurface::default();

    drain_agent_events(&mut app, &mut agent, &mut surface, &None).expect("drain events");

    assert!(agent.is_none());
    assert!(matches!(
        app.transcript.entries.last(),
        Some(app::Entry::Error { text }) if text.contains("agent worker failed")
    ));
}

#[test]
fn drain_agent_events_limits_each_render_batch() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    let (event_tx, event_rx) = mpsc::channel();
    for index in 0..=MAX_AGENT_EVENTS_PER_RENDER {
        event_tx
            .send(app::AgentEvent::Status(format!("event {index}")))
            .expect("queue event");
    }
    let (steering_tx, _steering_rx) = mpsc::channel();
    let mut agent = Some(test_agent_slot(event_rx, CancelToken::new(), steering_tx));
    let mut surface = TestSurface::default();
    let changed = drain_agent_events(&mut app, &mut agent, &mut surface, &None).expect("drain events");

    assert!(changed);
    assert!(matches!(
        agent.as_ref().expect("agent remains active").receiver.try_recv(),
        Ok(app::AgentEvent::Status(text)) if text == format!("event {MAX_AGENT_EVENTS_PER_RENDER}")
    ));
}

#[test]
fn maybe_spawn_agent_uses_current_app_model_after_picker_switch() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.model = "fake-agent".to_string();
    app.runtime.cli.model = app.runtime.model.clone();
    app.runtime.run_state = RunState::Working;
    app.transcript
        .entries
        .push(app::Entry::User { text: "hello with switched model".to_string() });

    let mut agent = None;
    maybe_spawn_agent(&mut app, &mut agent);
    let slot = agent.as_ref().expect("agent spawned");

    let first_model_event = loop {
        let event = slot
            .receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("agent event");
        if !matches!(event, app::AgentEvent::Started) {
            break event;
        }
    };
    slot.cancel.cancel();

    assert!(
        matches!(first_model_event, app::AgentEvent::ReasoningDelta(_)),
        "switched fake model should run fake provider, got {first_model_event:?}"
    );
}

#[test]
fn active_provider_prompt_prefers_an_internal_turn_over_visible_history() {
    let cli = Cli::default();
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.transcript
        .entries
        .push(app::Entry::User { text: "visible user turn".to_string() });
    app.composer.last_input = Some("Summarize the active context.".to_string());

    assert_eq!(active_provider_prompt(&app), "Summarize the active context.");
}

#[test]
fn maybe_spawn_agent_auto_compacts_oversized_turn_before_spawning() {
    let mut config = config::Config::default();
    config.context.compaction.mode = agent_context::CompactionMode::Auto;
    let cli = Cli {
        model: "fake-agent".to_string(),
        config_layers: vec![config::LoadedConfigLayer {
            source: config::ConfigSource::ProjectFile,
            config,
            path: None,
            display_path: None,
            hash: None,
        }],
        ..Cli::default()
    };
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Working;
    let big = "x".repeat(5_000);
    for _ in 0..20 {
        app.transcript.entries.push(app::Entry::User { text: big.clone() });
        app.transcript
            .entries
            .push(app::Entry::Agent { text: big.clone(), streaming: false });
    }
    app.transcript
        .entries
        .push(app::Entry::User { text: "final oversized turn".to_string() });

    let mut agent = None;
    maybe_spawn_agent(&mut app, &mut agent);

    assert!(
        agent.is_some(),
        "the compaction request should be sent instead of the oversized turn"
    );
    assert!(
        app.compaction_in_flight(),
        "auto-compaction should be triggered instead"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .all(|entry| !matches!(entry, app::Entry::User { text } if text.contains("Summarize"))),
        "the internal compaction prompt must stay out of the visible transcript"
    );
    assert!(
        app.composer
            .last_input
            .as_deref()
            .is_some_and(|prompt| prompt.contains("Summarize")),
        "the internal compaction prompt should remain active for the provider"
    );
}

#[test]
fn maybe_spawn_agent_does_not_auto_compact_when_mode_is_manual() {
    let mut config = config::Config::default();
    config.context.compaction.mode = agent_context::CompactionMode::Manual;
    let cli = Cli {
        model: "fake-agent".to_string(),
        config_layers: vec![config::LoadedConfigLayer {
            source: config::ConfigSource::ProjectFile,
            config,
            path: None,
            display_path: None,
            hash: None,
        }],
        ..Cli::default()
    };
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Working;
    let big = "x".repeat(5_000);
    for _ in 0..20 {
        app.transcript.entries.push(app::Entry::User { text: big.clone() });
        app.transcript
            .entries
            .push(app::Entry::Agent { text: big.clone(), streaming: false });
    }
    app.transcript
        .entries
        .push(app::Entry::User { text: "final oversized turn".to_string() });

    let mut agent = None;
    maybe_spawn_agent(&mut app, &mut agent);

    assert!(!app.compaction_in_flight(), "manual mode must not auto-compact");
    assert!(agent.is_some(), "manual mode sends the request to the provider");
    if let Some(slot) = agent {
        slot.cancel.cancel();
    }
}

#[test]
fn maybe_spawn_agent_does_not_run_preflight_while_agent_in_flight() {
    let mut config = config::Config::default();
    config.context.compaction.mode = agent_context::CompactionMode::Auto;
    let cli = Cli {
        model: "fake-agent".to_string(),
        config_layers: vec![config::LoadedConfigLayer {
            source: config::ConfigSource::ProjectFile,
            config,
            path: None,
            display_path: None,
            hash: None,
        }],
        ..Cli::default()
    };
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    app.runtime.run_state = RunState::Working;
    let big = "x".repeat(5_000);
    for _ in 0..20 {
        app.transcript.entries.push(app::Entry::User { text: big.clone() });
        app.transcript
            .entries
            .push(app::Entry::Agent { text: big.clone(), streaming: false });
    }
    app.transcript
        .entries
        .push(app::Entry::User { text: "in-flight turn".to_string() });

    let (steering_tx, _steering_rx) = mpsc::channel();
    let existing = test_agent_slot(mpsc::channel().1, CancelToken::new(), steering_tx);
    let mut agent = Some(existing);
    maybe_spawn_agent(&mut app, &mut agent);

    assert!(
        !app.compaction_in_flight(),
        "in-flight requests must never be interrupted for compaction"
    );
    assert!(agent.is_some(), "the existing agent slot must be preserved");
}

#[test]
fn resize_event_dimensions_drive_the_full_viewport_repaint() {
    let mut surface = TestSurface::default();
    surface.resize(100, 30).expect("resize");
    assert_eq!(surface.size, (100, 30));
}

#[test]
fn git_status_watcher_reports_external_change() {
    let dir = tempfile::tempdir().expect("temp git dir");
    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test User"]);
    std::fs::write(dir.path().join("tracked.txt"), "clean\n").expect("write tracked file");
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    let watcher = GitStatusWatcher::spawn_with_interval(dir.path().to_path_buf(), Duration::from_millis(50));
    watcher.wait_until_initialized();
    std::fs::write(dir.path().join("tracked.txt"), "dirty\n").expect("modify tracked file");

    let status = watcher
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("watcher should report dirty git status")
        .expect("repo status should be available");
    assert_eq!(status.modified, 1);
    assert!(
        status.display().ends_with("+0 ~1 -0"),
        "dirty summary should show one modified file: {}",
        status.display()
    );
}
