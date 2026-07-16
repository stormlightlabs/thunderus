use crate::app::{Entry, ToolStatus};
use crate::session::{SessionReader, SessionRecord, SessionWriter};
use crate::tools::{self, MAX_LINE_LEN};

use super::*;

/// Helper: build a ShellArgs for `echo`.
fn echo(args: &[&str]) -> ShellArgs {
    let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    one_shot("echo", v)
}

/// Helper: build a ShellArgs for `sh -c` (used for portable test scripts).
fn sh(script: &str) -> ShellArgs {
    one_shot("sh", vec!["-c".to_string(), script.to_string()])
}

fn one_shot(program: &str, args: Vec<String>) -> ShellArgs {
    ShellArgs { program: program.to_string(), args, cwd: None, timeout: None, kind: ProcessKind::OneShot }
}

#[test]
fn process_status_labels() {
    assert_eq!(ProcessStatus::Running.label(), "running");
    assert_eq!(ProcessStatus::Ok.label(), "ok");
    assert_eq!(ProcessStatus::Failed.label(), "failed");
    assert_eq!(ProcessStatus::Timeout.label(), "timeout");
    assert_eq!(ProcessStatus::Cancelled.label(), "cancelled");
}

#[test]
fn process_kind_labels() {
    assert_eq!(ProcessKind::OneShot.label(), "one-shot");
    assert_eq!(ProcessKind::Background.label(), "background");
}

#[test]
fn cancel_flag_starts_false() {
    let flag = CancelToken::new();
    assert!(!flag.is_cancelled());
}

#[test]
fn cancel_flag_sets_true() {
    let flag = CancelToken::new();
    flag.cancel();
    assert!(flag.is_cancelled());
}

#[test]
fn cancel_flag_is_shared() {
    let flag = CancelToken::new();
    let clone = flag.clone();
    flag.cancel();
    assert!(clone.is_cancelled());
}

#[test]
fn run_command_success() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = echo(&["hello", "world"]);
    let result = run_command(&args, root, &cancel).expect("run");

    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.kind, ProcessKind::OneShot);
    assert_eq!(result.stdout, vec!["hello world".to_string()]);
    assert!(result.stderr.is_empty());
    assert_eq!(
        result.command,
        vec!["echo".to_string(), "hello".to_string(), "world".to_string()]
    );
}

#[test]
fn run_command_captures_stdout() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = sh("printf 'line1\\nline2\\nline3\\n'");
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout, vec!["line1", "line2", "line3"]);
}

#[test]
fn run_command_captures_stderr() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = sh("echo 'to stderr' >&2");
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, vec!["to stderr".to_string()]);
}

#[test]
fn run_command_failure_nonzero_exit() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = sh("exit 3");
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Failed);
    assert_eq!(result.exit_code, Some(3));
}

#[test]
fn run_command_nonexistent_program_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = one_shot("definitely_not_a_real_program_xyz", vec![]);
    let result = run_command(&args, root, &cancel);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("failed to spawn"));
}

#[test]
fn run_command_timeout_kills_process() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = ShellArgs {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "sleep 30".to_string()],
        cwd: None,
        timeout: Some(Duration::from_secs(1)),
        kind: ProcessKind::OneShot,
    };
    let result = run_command(&args, root, &cancel).expect("run");

    assert_eq!(result.status, ProcessStatus::Timeout);
    assert!(result.exit_code.is_none());
    assert!(result.elapsed < Duration::from_secs(5));
}

#[test]
fn run_command_cancellation_kills_process() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = ShellArgs {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "sleep 30".to_string()],
        cwd: None,
        timeout: Some(Duration::from_secs(60)),
        kind: ProcessKind::OneShot,
    };

    let cancel_clone = cancel.clone();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        cancel_clone.cancel();
    });

    let result = run_command(&args, root, &cancel).expect("run");
    handle.join().expect("thread");

    assert_eq!(result.status, ProcessStatus::Cancelled);
    assert!(result.exit_code.is_none());
    assert!(result.elapsed < Duration::from_secs(5));
}

#[test]
fn run_command_runs_in_workspace_root_by_default() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();

    std::fs::write(root.join("marker.txt"), "found").expect("write");

    let cancel = CancelToken::new();
    let args = sh("cat marker.txt");
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout, vec!["found".to_string()]);
    assert_eq!(result.cwd, root.to_path_buf());
}

#[test]
fn run_command_runs_in_specified_cwd() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("subdir")).expect("mkdir");
    std::fs::write(root.join("subdir/nested.txt"), "nested").expect("write");
    let cancel = CancelToken::new();
    let args = ShellArgs {
        program: "cat".to_string(),
        args: vec!["nested.txt".to_string()],
        cwd: Some(PathBuf::from("subdir")),
        timeout: None,
        kind: ProcessKind::OneShot,
    };

    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout, vec!["nested".to_string()]);
    assert!(result.cwd.ends_with("subdir"));
}

#[test]
fn run_command_rejects_cwd_outside_root() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let parent = root.parent().unwrap();
    let escape = parent.to_string_lossy().to_string();
    let cancel = CancelToken::new();
    let args = ShellArgs {
        program: "echo".to_string(),
        args: vec![],
        cwd: Some(PathBuf::from(escape)),
        timeout: None,
        kind: ProcessKind::OneShot,
    };
    let result = run_command(&args, root, &cancel);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("escapes workspace root"));
}

#[test]
fn run_command_rejects_nonexistent_cwd() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = ShellArgs {
        program: "echo".to_string(),
        args: vec![],
        cwd: Some(PathBuf::from("nonexistent_dir")),
        timeout: None,
        kind: ProcessKind::OneShot,
    };
    let result = run_command(&args, root, &cancel);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not a directory"));
}

#[test]
fn run_command_caps_output_lines() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let script = format!("for i in $(seq 1 {}); do echo line$i; done", MAX_OUTPUT_LINES + 50);
    let args = sh(&script);
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout.len(), MAX_OUTPUT_LINES + 1);
    assert!(result.stdout.last().unwrap().starts_with("…("));
}

#[test]
fn run_command_truncates_long_lines() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let max_len: usize = MAX_LINE_LEN;
    let long_line = "x".repeat(max_len + 100);
    let script = format!("printf '{}\\n'", long_line);
    let args = sh(&script);
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout.len(), 1);
    assert!(result.stdout[0].ends_with("..."));
    assert!(result.stdout[0].chars().count() <= max_len + 3);
}

#[test]
fn run_command_captures_elapsed_time() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = sh("sleep 0.2");
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert!(result.elapsed >= Duration::from_millis(150));
    assert!(result.elapsed < Duration::from_secs(2));
}

#[test]
fn process_result_summary_includes_command_and_status() {
    let result = ProcessResult {
        command: vec!["cargo".to_string(), "test".to_string()],
        cwd: PathBuf::from("/repo"),
        status: ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec![],
        stderr: vec![],
        elapsed: Duration::from_millis(500),
        kind: ProcessKind::OneShot,
    };
    let summary = result.summary();
    assert!(summary.contains("cargo test"));
    assert!(summary.contains("ok"));
    assert!(summary.contains("500ms"));
    assert!(summary.contains("one-shot"));
}

#[test]
fn process_result_to_output_lines_includes_markers() {
    let result = ProcessResult {
        command: vec!["echo".to_string()],
        cwd: PathBuf::from("/repo"),
        status: ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec!["out1".to_string(), "out2".to_string()],
        stderr: vec!["err1".to_string()],
        elapsed: Duration::from_millis(10),
        kind: ProcessKind::OneShot,
    };
    let lines = result.to_output_lines();
    assert!(lines[0].contains("echo"));
    assert!(lines.contains(&"── stdout ──".to_string()));
    assert!(lines.contains(&"out1".to_string()));
    assert!(lines.contains(&"── stderr ──".to_string()));
    assert!(lines.contains(&"err1".to_string()));
}

#[test]
fn process_result_to_output_lines_omits_empty_streams() {
    let result = ProcessResult {
        command: vec!["echo".to_string()],
        cwd: PathBuf::from("/repo"),
        status: ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec![],
        stderr: vec![],
        elapsed: Duration::from_millis(10),
        kind: ProcessKind::OneShot,
    };
    let lines = result.to_output_lines();
    assert_eq!(lines.len(), 1);
}

#[test]
fn process_result_to_failed_output_for_timeout() {
    let result = ProcessResult {
        command: vec!["sleep".to_string(), "30".to_string()],
        cwd: PathBuf::from("/repo"),
        status: ProcessStatus::Timeout,
        exit_code: None,
        stdout: vec![],
        stderr: vec![],
        elapsed: Duration::from_millis(1000),
        kind: ProcessKind::OneShot,
    };
    let output = result.to_failed_output();
    assert_eq!(output.name, "run_shell");
    assert_eq!(output.status, ToolStatus::Failed);
    assert!(output.error.as_ref().is_some_and(|e| e.contains("timed out")));
}

#[test]
fn process_result_to_failed_output_for_cancelled() {
    let result = ProcessResult {
        command: vec!["sleep".to_string(), "30".to_string()],
        cwd: PathBuf::from("/repo"),
        status: ProcessStatus::Cancelled,
        exit_code: None,
        stdout: vec![],
        stderr: vec![],
        elapsed: Duration::from_millis(500),
        kind: ProcessKind::OneShot,
    };
    let output = result.to_failed_output();
    assert_eq!(output.status, ToolStatus::Failed);
    assert!(output.error.as_ref().is_some_and(|e| e.contains("cancelled")));
}

#[test]
fn process_result_to_failed_output_for_failure() {
    let result = ProcessResult {
        command: vec!["false".to_string()],
        cwd: PathBuf::from("/repo"),
        status: ProcessStatus::Failed,
        exit_code: Some(1),
        stdout: vec![],
        stderr: vec![],
        elapsed: Duration::from_millis(10),
        kind: ProcessKind::OneShot,
    };
    let output = result.to_failed_output();
    assert!(output.error.as_ref().is_some_and(|e| e.contains("exit 1")));
}

#[test]
fn registry_starts_empty() {
    let reg = ProcessRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
    assert_eq!(reg.background_count(), 0);
    assert_eq!(reg.one_shot_count(), 0);
}

#[test]
fn registry_register_assigns_incrementing_ids() {
    let mut reg = ProcessRegistry::new();
    let id1 = reg.register(
        vec!["echo".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        CancelToken::new(),
    );
    let id2 = reg.register(
        vec!["ls".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );
    assert_eq!(id1, 0);
    assert_eq!(id2, 1);
    assert_eq!(reg.len(), 2);
}

#[test]
fn registry_get_returns_active_process() {
    let mut reg = ProcessRegistry::new();
    let id = reg.register(
        vec!["cargo".to_string(), "test".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        CancelToken::new(),
    );
    let p = reg.get(id).expect("should exist");
    assert_eq!(p.command, vec!["cargo".to_string(), "test".to_string()]);
    assert_eq!(p.kind, ProcessKind::OneShot);
}

#[test]
fn registry_get_missing_returns_none() {
    let reg = ProcessRegistry::new();
    assert!(reg.get(999).is_none());
}

#[test]
fn registry_remove_removes_process() {
    let mut reg = ProcessRegistry::new();
    let id = reg.register(
        vec!["echo".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        CancelToken::new(),
    );
    assert!(reg.remove(id).is_some());
    assert!(reg.is_empty());
    assert!(reg.get(id).is_none());
}

#[test]
fn registry_remove_missing_returns_none() {
    let mut reg = ProcessRegistry::new();
    assert!(reg.remove(999).is_none());
}

#[test]
fn registry_counts_by_kind() {
    let mut reg = ProcessRegistry::new();
    reg.register(
        vec!["a".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        CancelToken::new(),
    );
    reg.register(
        vec!["b".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );
    reg.register(
        vec!["c".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );

    assert_eq!(reg.len(), 3);
    assert_eq!(reg.one_shot_count(), 1);
    assert_eq!(reg.background_count(), 2);
}

#[test]
fn registry_cancel_signals_flag() {
    let mut reg = ProcessRegistry::new();
    let cancel = CancelToken::new();
    let id = reg.register(
        vec!["sleep".to_string(), "30".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        cancel.clone(),
    );
    assert!(reg.cancel(id));
    assert!(cancel.is_cancelled());
}

#[test]
fn registry_cancel_missing_returns_false() {
    let mut reg = ProcessRegistry::new();
    assert!(!reg.cancel(999));
}

#[test]
fn registry_cancel_all_signals_all_flags() {
    let mut reg = ProcessRegistry::new();
    let c1 = CancelToken::new();
    let c2 = CancelToken::new();
    reg.register(
        vec!["a".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        c1.clone(),
    );
    reg.register(
        vec!["b".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        c2.clone(),
    );

    reg.cancel_all();
    assert!(c1.is_cancelled());
    assert!(c2.is_cancelled());
}

#[test]
fn registry_ids_iterates_all() {
    let mut reg = ProcessRegistry::new();
    reg.register(
        vec!["a".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        CancelToken::new(),
    );
    reg.register(
        vec!["b".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );

    let mut ids: Vec<u64> = reg.ids().collect();
    ids.sort();
    assert_eq!(ids, vec![0, 1]);
}

#[test]
fn registry_background_ids_filters() {
    let mut reg = ProcessRegistry::new();
    reg.register(
        vec!["a".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::OneShot,
        CancelToken::new(),
    );
    reg.register(
        vec!["b".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );
    reg.register(
        vec!["c".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );

    let mut bg_ids: Vec<u64> = reg.background_ids().collect();
    bg_ids.sort();
    assert_eq!(bg_ids, vec![1, 2]);
}

#[test]
fn registry_active_process_elapsed_tracks_time() {
    let mut reg = ProcessRegistry::new();
    let id = reg.register(
        vec!["sleep".to_string()],
        PathBuf::from("/repo"),
        ProcessKind::Background,
        CancelToken::new(),
    );
    std::thread::sleep(Duration::from_millis(50));
    let p = reg.get(id).expect("exists");
    assert!(p.elapsed() >= Duration::from_millis(40));
}

#[test]
fn exec_success_returns_ok_tool_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let args = echo(&["hello"]);
    let output = exec(&args, root);
    assert_eq!(output.status, ToolStatus::Ok);
    assert_eq!(output.name, "run_shell");
    assert!(output.output.iter().any(|l| l.contains("hello")));
}

#[test]
fn exec_failure_returns_failed_tool_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let args = sh("exit 1");
    let output = exec(&args, root);
    assert_eq!(output.status, ToolStatus::Failed);
    assert!(output.error.as_ref().is_some_and(|e| e.contains("exit 1")));
}

#[test]
fn exec_timeout_returns_failed_tool_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let args = ShellArgs {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "sleep 30".to_string()],
        cwd: None,
        timeout: Some(Duration::from_secs(1)),
        kind: ProcessKind::OneShot,
    };
    let output = exec(&args, root);
    assert_eq!(output.status, ToolStatus::Failed);
    assert!(output.error.as_ref().is_some_and(|e| e.contains("timed out")));
}

#[test]
fn exec_includes_command_summary_in_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let args = echo(&["test"]);
    let output = exec(&args, root);
    assert!(output.output[0].contains("echo test"));
    assert!(output.output[0].contains("one-shot"));
}

#[test]
fn shell_args_argv_joins_program_and_args() {
    let args = one_shot("cargo", vec!["test".to_string(), "--lib".to_string()]);
    assert_eq!(args.argv(), vec!["cargo", "test", "--lib"]);
}

#[test]
fn parse_arguments_reads_all_fields() {
    let args = parse_arguments(
        r#"{"program":"cargo","args":["test","tools"],"cwd":"src","timeout_secs":7,"background":true}"#,
    )
    .expect("parse");

    assert_eq!(args.program, "cargo");
    assert_eq!(args.args, vec!["test".to_string(), "tools".to_string()]);
    assert_eq!(args.cwd, Some(PathBuf::from("src")));
    assert_eq!(args.timeout, Some(Duration::from_secs(7)));
    assert_eq!(args.kind, ProcessKind::Background);
}

#[test]
fn parse_arguments_rejects_malformed_json() {
    let error = parse_arguments("not json").expect_err("malformed JSON should fail");
    assert!(error.to_string().contains("invalid JSON"));
}

#[test]
fn parse_arguments_reads_argv_and_timeout_ms() {
    let args = parse_arguments(r#"{"argv":["pnpm","--dir","docs","build"],"timeout_ms":120000}"#)
        .expect("parse canonical arguments");

    assert_eq!(args.program, "pnpm");
    assert_eq!(args.args, vec!["--dir", "docs", "build"]);
    assert_eq!(args.timeout, Some(Duration::from_secs(120)));
}

#[test]
fn parse_arguments_accepts_legacy_command_argv_array() {
    let args = parse_arguments(r#"{"command":["sh","-lc","printf ok"]}"#).expect("parse compatibility arguments");

    assert_eq!(args.program, "sh");
    assert_eq!(args.args, vec!["-lc", "printf ok"]);
}

#[test]
fn registry_execute_foreground_command_returns_shell_result() {
    let dir = tempfile::tempdir().expect("temp dir");
    let request = tools::ToolUseRequest::new(
        "run_shell".to_string(),
        r#"{"program":"echo","args":["hello"]}"#.to_string(),
        "call_1".to_string(),
    );

    let execution = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path()));

    assert_eq!(execution.output.status, ToolStatus::Ok);
    let result = execution.shell_result.expect("shell audit metadata");
    assert_eq!(result.kind, ProcessKind::OneShot);
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout, vec!["hello".to_string()]);
}

#[test]
fn registry_execute_background_command_preserves_kind_for_registration() {
    let dir = tempfile::tempdir().expect("temp dir");
    let request = tools::ToolUseRequest::new(
        "run_shell".to_string(),
        r#"{"program":"echo","args":["background"],"background":true}"#.to_string(),
        "call_1".to_string(),
    );

    let execution = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path()));

    assert_eq!(execution.output.status, ToolStatus::Ok);
    let result = execution.shell_result.expect("shell audit metadata");
    assert_eq!(result.kind, ProcessKind::Background);
    assert_eq!(result.stdout, vec!["background".to_string()]);
}

#[test]
fn registry_execute_missing_program_fails_without_shell_result() {
    let dir = tempfile::tempdir().expect("temp dir");
    let request = tools::ToolUseRequest::new("run_shell".to_string(), r#"{}"#.to_string(), "call_1".to_string());

    let execution = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path()));

    assert_eq!(execution.output.status, ToolStatus::Failed);
    assert!(execution.shell_result.is_none());
    assert!(
        execution
            .output
            .error
            .as_ref()
            .is_some_and(|error| error.contains("missing command"))
    );
}

#[test]
fn registry_execute_rejects_cwd_escape() {
    let dir = tempfile::tempdir().expect("temp dir");
    let parent = dir.path().parent().unwrap();
    let request = tools::ToolUseRequest::new(
        "run_shell".to_string(),
        format!(r#"{{"program":"echo","cwd":"{}"}}"#, parent.display()),
        "call_1".to_string(),
    );

    let execution = tools::registry::execute(&request, &tools::registry::ToolContext::new(dir.path()));

    assert_eq!(execution.output.status, ToolStatus::Failed);
    assert!(execution.shell_result.is_none());
    assert!(
        execution
            .output
            .error
            .as_ref()
            .is_some_and(|error| error.contains("escapes workspace root"))
    );
}

/// The shell module never exposes a shell-string execution path. Commands
/// are argv arrays, never `/bin/sh -c` with model-controlled content.
#[test]
fn no_shell_string_execution_exposed() {
    let args = one_shot("echo", vec!["hello".to_string()]);
    assert!(!args.program.contains("sh -c"), "program must not be a shell string");
    assert!(args.argv().len() >= 1, "argv must have at least the program");
}

#[test]
fn redact_secrets_redacts_sk_prefixed_api_keys() {
    let line = "export API_KEY=sk-abc123def456ghi789";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("[REDACTED]"));
    assert!(!redacted.contains("abc123def456ghi789"));
}

#[test]
fn redact_secrets_redacts_bare_sk_key() {
    let line = "found key: sk-abcdefgh1234567890";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("sk-[REDACTED]"));
    assert!(!redacted.contains("abcdefgh1234567890"));
}

#[test]
fn redact_secrets_does_not_redact_short_sk_prefix() {
    let line = "sk-short";
    let redacted = redact_secrets(line);
    assert_eq!(redacted, line);
}

#[test]
fn redact_secrets_redacts_bearer_tokens() {
    let line = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("Bearer [REDACTED]"));
    assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
}

#[test]
fn redact_secrets_redacts_password_assignment() {
    let line = "password=hunter2supersecret";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("password=[REDACTED]"));
    assert!(!redacted.contains("hunter2supersecret"));
}

#[test]
fn redact_secrets_redacts_api_key_assignment() {
    let line = "api_key: my_secret_value_12345";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("api_key=[REDACTED]"));
    assert!(!redacted.contains("my_secret_value_12345"));
}

#[test]
fn redact_secrets_redacts_access_token_assignment() {
    let line = "access_token=ghp_abc123def456ghi789jkl";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("access_token=[REDACTED]"));
    assert!(!redacted.contains("ghp_abc123def456ghi789jkl"));
}

#[test]
fn redact_secrets_redacts_secret_assignment() {
    let line = "secret: my_very_secret_value";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("secret=[REDACTED]"));
    assert!(!redacted.contains("my_very_secret_value"));
}

#[test]
fn redact_secrets_does_not_redact_short_values() {
    let line = "password=ab";
    let redacted = redact_secrets(line);
    assert_eq!(redacted, line);
}

#[test]
fn redact_secrets_redacts_multiple_secrets_in_one_line() {
    let line = "key1=sk-abc123def456 key2=Bearer token1234567890";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("sk-[REDACTED]"));
    assert!(redacted.contains("Bearer [REDACTED]"));
}

#[test]
fn redact_secrets_preserves_non_secret_content() {
    let line = "cargo test --lib -- --nocapture";
    let redacted = redact_secrets(line);
    assert_eq!(redacted, line);
}

#[test]
fn redact_secrets_handles_empty_line() {
    assert_eq!(redact_secrets(""), "");
}

#[test]
fn redact_secrets_is_case_insensitive_for_keywords() {
    let line = "API_KEY=supersecretvalue123";
    let redacted = redact_secrets(line);
    assert!(redacted.contains("API_KEY=[REDACTED]"));
    assert!(!redacted.contains("supersecretvalue123"));
}

#[test]
fn redact_secrets_redacts_in_command_output() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = sh("echo 'token=sk-abcdefgh1234567890'");
    let result = run_command(&args, root, &cancel).expect("run");
    assert_eq!(result.status, ProcessStatus::Ok);
    assert_eq!(result.stdout.len(), 1);
    assert!(
        result.stdout[0].contains("[REDACTED]"),
        "output should be redacted: {}",
        result.stdout[0]
    );
    assert!(
        !result.stdout[0].contains("abcdefgh1234567890"),
        "secret should not appear"
    );
}

/// Verify that shell output is persisted in the tool_finished session record
/// and that secrets in that output are redacted before persistence.
#[test]
fn shell_output_persisted_in_tool_finished_is_redacted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = sh("echo 'api_key=sk-secretvalue12345'");
    let result = run_command(&args, root, &cancel).expect("run");

    assert!(result.stdout.iter().all(|l| !l.contains("secretvalue12345")));

    let entry = Entry::Tool {
        name: String::from("run_shell#0"),
        arguments: String::from("{}"),
        status: ToolStatus::Ok,
        output: result.to_output_lines(),
    };

    let session_dir = tempfile::tempdir().expect("session dir");
    let mut writer = SessionWriter::create(
        session_dir.path(),
        "redact-persist",
        "/repo",
        "test",
        "umans",
        "umans-coder",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    writer.append_entry(&entry, "turn_1").expect("append");

    let json = std::fs::read_to_string(writer.path()).expect("read file");
    assert!(
        !json.contains("secretvalue12345"),
        "persisted tool_finished output must not contain the secret"
    );
    assert!(
        json.contains("[REDACTED]"),
        "persisted tool_finished output should contain redacted marker"
    );
}

/// Verify that the ShellExec session record captures the full lifecycle
/// (command, status, exit_code, elapsed, kind) for a successful command.
#[test]
fn shell_exec_persists_full_lifecycle_for_success() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let cancel = CancelToken::new();
    let args = echo(&["hello"]);
    let result = run_command(&args, root, &cancel).expect("run");

    let session_dir = tempfile::tempdir().expect("session dir");
    let mut writer = SessionWriter::create(
        session_dir.path(),
        "lifecycle-success",
        "/repo",
        "test",
        "umans",
        "umans-coder",
        "duckduckgo",
        "0.1.0",
        None,
    )
    .expect("create writer");

    writer.append_shell_exec("turn_1", &result).expect("append");

    let path = writer.path().to_path_buf();
    drop(writer);

    let records = SessionReader::read_records(&path);
    let se = records
        .iter()
        .find(|r| matches!(r, SessionRecord::ShellExec { .. }))
        .expect("should find shell_exec record");

    let SessionRecord::ShellExec { command, process_status, exit_code, kind, .. } = se else {
        panic!("expected ShellExec record");
    };

    assert_eq!(command, "echo hello");
    assert_eq!(*process_status, "ok");
    assert_eq!(*exit_code, Some(0));
    assert_eq!(*kind, "one-shot");
}
