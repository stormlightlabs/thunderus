//! Durable, capability-scoped trust for project MCP configuration.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::utils;

const TRUST_STORE_VERSION: u32 = 1;

/// Current trust state for a discovered project MCP configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectMcpTrust {
    /// The current project MCP file hash is explicitly trusted.
    Trusted,
    /// No MCP trust decision exists for this project.
    Untrusted,
    /// A decision exists, but it applies to an older file hash.
    Stale { trusted_hash: String },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct TrustStore {
    version: u32,
    projects: Vec<ProjectTrustRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectTrustRecord {
    workspace: String,
    mcp: McpTrustRecord,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct McpTrustRecord {
    config_sha256: String,
    trusted_at: String,
}

/// Inspect trust for the current project MCP file hash.
pub fn project_mcp_trust(workspace: &Path, config_hash: &str) -> io::Result<ProjectMcpTrust> {
    let store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let Some(record) = store.projects.iter().find(|record| record.workspace == workspace) else {
        return Ok(ProjectMcpTrust::Untrusted);
    };
    if record.mcp.config_sha256 == config_hash {
        Ok(ProjectMcpTrust::Trusted)
    } else {
        Ok(ProjectMcpTrust::Stale { trusted_hash: record.mcp.config_sha256.clone() })
    }
}

/// Persist MCP trust for this workspace and exact project config hash.
pub fn trust_project_mcp(workspace: &Path, config_hash: &str) -> io::Result<()> {
    let mut store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let mcp =
        McpTrustRecord { config_sha256: config_hash.to_string(), trusted_at: crate::utils::datetime::now_iso8601() };
    if let Some(record) = store.projects.iter_mut().find(|record| record.workspace == workspace) {
        record.mcp = mcp;
    } else {
        store.projects.push(ProjectTrustRecord { workspace, mcp });
        store
            .projects
            .sort_by(|left, right| left.workspace.cmp(&right.workspace));
    }
    save_store(&store)
}

/// Revoke MCP trust for this workspace. Returns whether a decision existed.
pub fn revoke_project_mcp(workspace: &Path) -> io::Result<bool> {
    let mut store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let original_len = store.projects.len();
    store.projects.retain(|record| record.workspace != workspace);
    let removed = store.projects.len() != original_len;
    if removed {
        save_store(&store)?;
    }
    Ok(removed)
}

fn trust_store_path() -> io::Result<PathBuf> {
    utils::home_dir()
        .map(|home| home.join(".thndrs").join("mcp-trust.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

fn workspace_identity(workspace: &Path) -> io::Result<String> {
    fs::canonicalize(workspace).map(|path| path.display().to_string())
}

fn load_store() -> io::Result<TrustStore> {
    let path = trust_store_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(TrustStore { version: TRUST_STORE_VERSION, projects: Vec::new() });
        }
        Err(error) => return Err(error),
    };
    let store: TrustStore = serde_json::from_str(&content).map_err(io::Error::other)?;
    if store.version != TRUST_STORE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported MCP trust store version {}", store.version),
        ));
    }
    Ok(store)
}

fn save_store(store: &TrustStore) -> io::Result<()> {
    let path = trust_store_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("MCP trust store path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer_pretty(&mut temporary, store).map_err(io::Error::other)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(&path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn trust_is_scoped_to_workspace_and_exact_config_hash() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        let other = temp.path().join("other");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&other).expect("other workspace");

        with_home(&home, || {
            assert_eq!(
                project_mcp_trust(&workspace, "first").unwrap(),
                ProjectMcpTrust::Untrusted
            );
            trust_project_mcp(&workspace, "first").unwrap();
            assert_eq!(
                project_mcp_trust(&workspace, "first").unwrap(),
                ProjectMcpTrust::Trusted
            );
            assert_eq!(project_mcp_trust(&other, "first").unwrap(), ProjectMcpTrust::Untrusted);
            assert_eq!(
                project_mcp_trust(&workspace, "second").unwrap(),
                ProjectMcpTrust::Stale { trusted_hash: "first".to_string() }
            );
        });
    }

    #[test]
    fn trust_can_be_revoked() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&workspace).expect("workspace");

        with_home(&home, || {
            trust_project_mcp(&workspace, "hash").unwrap();
            assert!(revoke_project_mcp(&workspace).unwrap());
            assert!(!revoke_project_mcp(&workspace).unwrap());
            assert_eq!(
                project_mcp_trust(&workspace, "hash").unwrap(),
                ProjectMcpTrust::Untrusted
            );
        });
    }
}
