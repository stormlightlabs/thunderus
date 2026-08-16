//! ACP terminal callback process registry.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse, ReleaseTerminalRequest,
    ReleaseTerminalResponse, TerminalExitStatus, TerminalOutputRequest, TerminalOutputResponse,
    WaitForTerminalExitRequest, WaitForTerminalExitResponse,
};

use crate::app::{AgentEvent, ToolStatus};
use crate::tools::shell::{ProcessKind, ProcessResult, ProcessStatus, redact_secrets};
use crate::tools::{self, MAX_OUTPUT_BYTES};

const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Registry for ACP-created terminal processes.
#[derive(Debug, Default)]
pub struct TerminalRegistry {
    next_id: AtomicU64,
    terminals: Mutex<HashMap<String, Arc<TerminalProcess>>>,
}

impl TerminalRegistry {
    /// Create an empty terminal registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a terminal command and register it by ACP terminal id.
    pub fn create(
        &self, request: &CreateTerminalRequest, root: &Path,
    ) -> Result<(CreateTerminalResponse, AgentEvent), String> {
        if request.command.trim().is_empty() {
            return Err("terminal/create denied: command is empty".to_string());
        }

        let id = format!("acp-terminal-{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let cwd = resolve_cwd(root, request.cwd.as_deref())?;
        let output = Arc::new(OutputBuffer::new(output_limit(request.output_byte_limit)));
        let start = Instant::now();
        let argv = std::iter::once(request.command.clone())
            .chain(request.args.iter().cloned())
            .collect::<Vec<_>>();

        let mut command = Command::new(&request.command);
        command
            .args(&request.args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for env in &request.env {
            command.env(&env.name, &env.value);
        }

        let mut child = command
            .spawn()
            .map_err(|err| format!("terminal/create failed to spawn `{}`: {err}", request.command))?;
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, output.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, output.clone());
        }

        let process = TerminalProcess {
            child: Mutex::new(Some(child)),
            command: argv.clone(),
            cwd: cwd.clone(),
            output,
            start,
            status: Mutex::new(None),
            audited: Mutex::new(false),
        };
        self.terminals
            .lock()
            .expect("terminal registry lock")
            .insert(id.clone(), Arc::new(process));

        Ok((
            CreateTerminalResponse::new(id.clone()),
            AgentEvent::ToolStarted {
                id,
                name: "acp.terminal".to_string(),
                arguments: terminal_arguments(&argv, &cwd, &request.env),
            },
        ))
    }

    /// Return current terminal output and status.
    pub fn output(&self, request: &TerminalOutputRequest) -> Result<(TerminalOutputResponse, AgentEvent), String> {
        let id = request.terminal_id.to_string();
        let terminal = self.get(&id)?;
        terminal.refresh_status();
        let snapshot = terminal.snapshot();
        Ok((
            TerminalOutputResponse::new(snapshot.output.clone(), snapshot.truncated)
                .exit_status(snapshot.exit_status.clone()),
            terminal_event(id, &snapshot, None),
        ))
    }

    /// Wait until a terminal command exits.
    pub fn wait_for_exit(
        &self, request: &WaitForTerminalExitRequest,
    ) -> Result<(WaitForTerminalExitResponse, AgentEvent), String> {
        let id = request.terminal_id.to_string();
        let terminal = self.get(&id)?;
        let exit_status = terminal.wait_for_exit();
        let snapshot = terminal.snapshot();
        Ok((
            WaitForTerminalExitResponse::new(exit_status),
            terminal_event(id, &snapshot, terminal.audit_result()),
        ))
    }

    /// Kill a terminal command without releasing it.
    pub fn kill(&self, request: &KillTerminalRequest) -> Result<(KillTerminalResponse, AgentEvent), String> {
        let id = request.terminal_id.to_string();
        let terminal = self.get(&id)?;
        terminal.kill();
        let snapshot = terminal.snapshot();
        Ok((
            KillTerminalResponse::new(),
            terminal_event(id, &snapshot, terminal.audit_result()),
        ))
    }

    /// Release a terminal and kill it if it is still running.
    pub fn release(&self, request: &ReleaseTerminalRequest) -> Result<(ReleaseTerminalResponse, AgentEvent), String> {
        let id = request.terminal_id.to_string();
        let terminal = self
            .terminals
            .lock()
            .expect("terminal registry lock")
            .remove(&id)
            .ok_or_else(|| format!("terminal `{id}` is not active"))?;
        terminal.kill_if_running();
        let snapshot = terminal.snapshot();
        Ok((
            ReleaseTerminalResponse::new(),
            terminal_event(id, &snapshot, terminal.audit_result()),
        ))
    }

    fn get(&self, id: &str) -> Result<Arc<TerminalProcess>, String> {
        self.terminals
            .lock()
            .expect("terminal registry lock")
            .get(id)
            .cloned()
            .ok_or_else(|| format!("terminal `{id}` is not active"))
    }
}

impl Drop for TerminalRegistry {
    fn drop(&mut self) {
        for terminal in self.terminals.get_mut().expect("terminal registry lock").values() {
            terminal.kill_if_running();
        }
    }
}

#[derive(Debug)]
struct TerminalProcess {
    child: Mutex<Option<Child>>,
    command: Vec<String>,
    cwd: PathBuf,
    output: Arc<OutputBuffer>,
    start: Instant,
    status: Mutex<Option<ExitStatus>>,
    audited: Mutex<bool>,
}

impl TerminalProcess {
    fn refresh_status(&self) -> Option<TerminalExitStatus> {
        if let Some(status) = *self.status.lock().expect("terminal status lock") {
            return Some(exit_status(status));
        }

        let mut guard = self.child.lock().expect("terminal child lock");
        let Some(child) = guard.as_mut() else {
            return self.status.lock().expect("terminal status lock").map(exit_status);
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                *self.status.lock().expect("terminal status lock") = Some(status);
                *guard = None;
                Some(exit_status(status))
            }
            Ok(None) | Err(_) => None,
        }
    }

    fn wait_for_exit(&self) -> TerminalExitStatus {
        loop {
            if let Some(status) = self.refresh_status() {
                return status;
            }
            thread::sleep(WAIT_POLL_INTERVAL);
        }
    }

    fn kill(&self) {
        let mut guard = self.child.lock().expect("terminal child lock");
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
            if let Ok(status) = child.wait() {
                *self.status.lock().expect("terminal status lock") = Some(status);
            }
            *guard = None;
        }
    }

    fn kill_if_running(&self) {
        if self.refresh_status().is_none() {
            self.kill();
        }
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let exit_status = self.refresh_status();
        let (output, truncated) = self.output.snapshot();
        TerminalSnapshot { output, truncated, exit_status }
    }

    fn process_result(&self) -> ProcessResult {
        let snapshot = self.snapshot();
        let process_status = match &snapshot.exit_status {
            Some(status) if status.exit_code == Some(0) => ProcessStatus::Ok,
            Some(_) => ProcessStatus::Failed,
            None => ProcessStatus::Running,
        };
        ProcessResult {
            process_id: None,
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            status: process_status,
            exit_code: snapshot
                .exit_status
                .and_then(|status| status.exit_code.map(|code| code as i32)),
            stdout: split_output(&snapshot.output),
            stderr: Vec::new(),
            output_truncated: snapshot.truncated,
            elapsed: self.start.elapsed(),
            kind: ProcessKind::OneShot,
        }
    }

    fn audit_result(&self) -> Option<Box<ProcessResult>> {
        self.refresh_status()?;
        let mut audited = self.audited.lock().expect("terminal audit lock");
        if *audited {
            return None;
        }
        *audited = true;
        Some(Box::new(self.process_result()))
    }
}

#[derive(Debug)]
struct TerminalSnapshot {
    output: String,
    truncated: bool,
    exit_status: Option<TerminalExitStatus>,
}

#[derive(Debug)]
struct OutputBuffer {
    bytes: Mutex<Vec<u8>>,
    limit: usize,
    truncated: Mutex<bool>,
}

impl OutputBuffer {
    fn new(limit: usize) -> Self {
        Self { bytes: Mutex::new(Vec::new()), limit, truncated: Mutex::new(false) }
    }

    fn append(&self, chunk: &[u8]) {
        let mut bytes = self.bytes.lock().expect("terminal output lock");
        bytes.extend_from_slice(chunk);
        if bytes.len() > self.limit {
            let keep_from = utf8_boundary(&bytes, bytes.len() - self.limit);
            bytes.drain(..keep_from);
            *self.truncated.lock().expect("terminal truncated lock") = true;
        }
    }

    fn snapshot(&self) -> (String, bool) {
        let bytes = self.bytes.lock().expect("terminal output lock").clone();
        let output = String::from_utf8_lossy(&bytes)
            .lines()
            .map(redact_secrets)
            .collect::<Vec<_>>()
            .join("\n");
        let truncated = *self.truncated.lock().expect("terminal truncated lock");
        (output, truncated)
    }
}

fn spawn_reader(mut reader: impl Read + Send + 'static, output: Arc<OutputBuffer>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => output.append(&buffer[..n]),
            }
        }
    });
}

fn resolve_cwd(root: &Path, cwd: Option<&Path>) -> Result<PathBuf, String> {
    match cwd {
        Some(path) => {
            if !path.is_absolute() {
                return Err("terminal/create denied: cwd must be absolute".to_string());
            }
            let resolved = tools::resolve_workspace_path(root, path).map_err(|err| err.to_string())?;
            if !resolved.is_dir() {
                return Err(format!(
                    "terminal/create denied: {} is not a directory",
                    resolved.display()
                ));
            }
            Ok(resolved)
        }
        None => root
            .canonicalize()
            .map_err(|err| format!("terminal/create denied: invalid workspace root: {err}")),
    }
}

fn terminal_arguments(argv: &[String], cwd: &Path, env: &[agent_client_protocol::schema::v1::EnvVariable]) -> String {
    serde_json::json!({
        "argv": argv,
        "cwd": cwd.display().to_string(),
        "env_keys": env.iter().map(|item| item.name.clone()).collect::<Vec<_>>(),
    })
    .to_string()
}

fn terminal_event(id: String, snapshot: &TerminalSnapshot, result: Option<Box<ProcessResult>>) -> AgentEvent {
    let status = match snapshot.exit_status.as_ref().and_then(|status| status.exit_code) {
        Some(0) => ToolStatus::Ok,
        Some(_) => ToolStatus::Failed,
        None => ToolStatus::Running,
    };
    let mut output = if snapshot.output.is_empty() { Vec::new() } else { split_output(&snapshot.output) };
    if snapshot.truncated {
        output.insert(0, "[terminal output truncated]".to_string());
    }
    AgentEvent::ToolFinished { id, output, status, write_result: None, shell_result: result }
}

fn output_limit(limit: Option<u64>) -> usize {
    limit
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(MAX_OUTPUT_BYTES))
        .unwrap_or(MAX_OUTPUT_BYTES)
}

fn exit_status(status: ExitStatus) -> TerminalExitStatus {
    TerminalExitStatus::new().exit_code(status.code().and_then(|code| u32::try_from(code).ok()))
}

fn split_output(output: &str) -> Vec<String> {
    output.lines().map(str::to_string).collect()
}

fn utf8_boundary(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && (bytes[index] & 0b1100_0000) == 0b1000_0000 {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::SessionId;

    #[test]
    fn terminal_registry_runs_command_and_returns_output() {
        let root = tempfile::tempdir().expect("temp dir");
        let registry = TerminalRegistry::new();
        let request = CreateTerminalRequest::new(SessionId::new("s"), "sh")
            .args(vec!["-c".to_string(), "printf hello".to_string()])
            .cwd(Some(root.path().to_path_buf()));

        let (response, started) = registry.create(&request, root.path()).expect("create terminal");
        let AgentEvent::ToolStarted { name, arguments, .. } = started else {
            panic!("terminal should report a start event");
        };
        assert_eq!(name, "acp.terminal");
        let arguments: serde_json::Value = serde_json::from_str(&arguments).expect("terminal arguments JSON");
        assert_eq!(
            arguments["cwd"],
            root.path()
                .canonicalize()
                .expect("canonical root")
                .display()
                .to_string()
        );

        let wait = WaitForTerminalExitRequest::new(SessionId::new("s"), response.terminal_id);
        let (_, event) = registry.wait_for_exit(&wait).expect("wait");

        assert!(matches!(
            event,
            AgentEvent::ToolFinished { status: ToolStatus::Ok, shell_result: Some(_), .. }
        ));
    }

    #[test]
    fn terminal_registry_rejects_cwd_escape() {
        let root = tempfile::tempdir().expect("temp dir");
        let outside = tempfile::tempdir().expect("outside");
        let registry = TerminalRegistry::new();
        let request = CreateTerminalRequest::new(SessionId::new("s"), "sh").cwd(Some(outside.path().to_path_buf()));

        let err = registry.create(&request, root.path()).unwrap_err();

        assert!(err.contains("escapes workspace root"));
    }
}
