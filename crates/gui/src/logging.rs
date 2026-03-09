use std::path::{Path, PathBuf};
use thndrs_mem::{hash_workspace, memory_dir};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{self, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{LevelFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub const LOG_FILE_BASENAME: &str = "runtime.log";

#[derive(Debug)]
pub struct TracingRuntime {
    pub log_dir: PathBuf,
    pub _guard: WorkerGuard,
}

/// Initialize the tracing subscriber for the GUI.
///
/// Filters out iced_* logs from stdout and file if verbose is false.
pub fn init_gui_tracing(verbose: bool, workspace_path: &Path) -> Result<TracingRuntime, String> {
    let log_dir = workspace_log_dir(workspace_path)?;
    let file_appender = rolling::RollingFileAppender::new(Rotation::HOURLY, &log_dir, LOG_FILE_BASENAME);
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(file_appender);

    let max_level = if verbose { tracing::Level::DEBUG } else { tracing::Level::INFO };

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_filter(filter_fn(move |metadata| {
            if verbose { true } else { !metadata.target().starts_with("iced") }
        }));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking_writer)
        .with_filter(filter_fn(move |metadata| {
            if verbose { true } else { !metadata.target().starts_with("iced") }
        }));

    tracing_subscriber::registry()
        .with(LevelFilter::from_level(max_level))
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .map_err(|error| format!("Failed to set tracing subscriber: {error}"))?;

    Ok(TracingRuntime { log_dir, _guard: guard })
}

pub fn workspace_log_dir(workspace_path: &Path) -> Result<PathBuf, String> {
    let memory_root = memory_dir().map_err(|error| error.to_string())?;
    let thunderus_root = memory_root
        .parent()
        .ok_or_else(|| format!("Memory directory has no parent: {}", memory_root.display()))?;
    let log_dir = thunderus_root
        .join("logs")
        .join("workspaces")
        .join(hash_workspace(workspace_path));

    std::fs::create_dir_all(&log_dir)
        .map_err(|error| format!("Failed to create log directory {}: {error}", log_dir.display()))?;

    Ok(log_dir)
}

pub fn latest_workspace_log_file(log_dir: &Path) -> Result<Option<PathBuf>, String> {
    let mut entries = Vec::new();
    let read_dir = match std::fs::read_dir(log_dir) {
        Ok(read_dir) => read_dir,
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => return Err(format!("Failed to read {}: {error}", log_dir.display())),
        },
    };

    for entry in read_dir {
        let entry = entry.map_err(|error| format!("Failed to read entry in {}: {error}", log_dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.contains(LOG_FILE_BASENAME) {
            entries.push(path);
        }
    }

    entries.sort_by_key(|path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok());
    Ok(entries.into_iter().last())
}
