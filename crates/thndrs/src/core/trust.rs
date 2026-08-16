//! Durable, workspace-scoped trust for project MCP server configuration.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::utils;

const TRUST_STORE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum StoredTrustScope {
    // Retained so stores written by the removed generic trust system remain readable.
    Configuration,
    PromptTemplates,
    Skills,
    Commands,
    Mcp,
    Hooks,
}

/// Current trust state for project MCP server configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectMcpTrust {
    /// The exact current resource fingerprint was explicitly trusted.
    Trusted,
    /// No MCP trust decision exists for this workspace.
    Untrusted,
    /// A decision exists, but it covers an older resource fingerprint.
    Stale {
        /// The fingerprint the user previously trusted.
        trusted_hash: String,
    },
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
    decisions: Vec<TrustDecision>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrustDecision {
    scope: StoredTrustScope,
    resource_sha256: String,
    trusted_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMcpTrustStore {
    version: u32,
    projects: Vec<LegacyMcpProjectTrustRecord>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMcpProjectTrustRecord {
    workspace: String,
    mcp: LegacyMcpTrustRecord,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMcpTrustRecord {
    config_sha256: String,
    trusted_at: String,
}

/// Inspect project MCP trust for one workspace and configuration fingerprint.
pub fn project_mcp_trust(workspace: &Path, hash: &str) -> io::Result<ProjectMcpTrust> {
    let store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let Some(record) = store.projects.iter().find(|record| record.workspace == workspace) else {
        return Ok(ProjectMcpTrust::Untrusted);
    };
    let Some(decision) = record
        .decisions
        .iter()
        .find(|decision| decision.scope == StoredTrustScope::Mcp)
    else {
        return Ok(ProjectMcpTrust::Untrusted);
    };
    if decision.resource_sha256 == hash {
        Ok(ProjectMcpTrust::Trusted)
    } else {
        Ok(ProjectMcpTrust::Stale { trusted_hash: decision.resource_sha256.clone() })
    }
}

/// Persist trust for the exact current project MCP configuration fingerprint.
pub fn trust_project_mcp(workspace: &Path, hash: &str) -> io::Result<()> {
    let mut store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let decision = TrustDecision {
        scope: StoredTrustScope::Mcp,
        resource_sha256: hash.to_string(),
        trusted_at: crate::utils::datetime::now_iso8601(),
    };
    if let Some(record) = store.projects.iter_mut().find(|record| record.workspace == workspace) {
        if let Some(existing) = record
            .decisions
            .iter_mut()
            .find(|existing| existing.scope == StoredTrustScope::Mcp)
        {
            *existing = decision;
        } else {
            record.decisions.push(decision);
        }
    } else {
        store
            .projects
            .push(ProjectTrustRecord { workspace, decisions: vec![decision] });
        store
            .projects
            .sort_by(|left, right| left.workspace.cmp(&right.workspace));
    }
    save_store(&store)
}

/// Revoke project MCP trust. Returns whether a decision existed.
pub fn revoke_project_mcp_trust(workspace: &Path) -> io::Result<bool> {
    let mut store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let Some(record) = store.projects.iter_mut().find(|record| record.workspace == workspace) else {
        return Ok(false);
    };
    let before = record.decisions.len();
    record
        .decisions
        .retain(|decision| decision.scope != StoredTrustScope::Mcp);
    let removed = record.decisions.len() != before;
    store.projects.retain(|record| !record.decisions.is_empty());
    if removed {
        save_store(&store)?;
    }
    Ok(removed)
}

fn trust_store_path() -> io::Result<PathBuf> {
    utils::home_dir()
        .map(|home| home.join(".thndrs").join("project-trust.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

fn workspace_identity(workspace: &Path) -> io::Result<String> {
    fs::canonicalize(workspace).map(|path| path.display().to_string())
}

fn load_store() -> io::Result<TrustStore> {
    let path = trust_store_path()?;
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return load_legacy_mcp_store(),
        Err(error) => return Err(error),
    };
    let store: TrustStore = serde_json::from_str(&content).map_err(io::Error::other)?;
    if store.version != TRUST_STORE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported project trust store version {}", store.version),
        ));
    }
    Ok(store)
}

fn load_legacy_mcp_store() -> io::Result<TrustStore> {
    let Some(home) = utils::home_dir() else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"));
    };
    let path = home.join(".thndrs").join("mcp-trust.json");
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(TrustStore { version: TRUST_STORE_VERSION, projects: Vec::new() });
        }
        Err(error) => return Err(error),
    };
    let legacy: LegacyMcpTrustStore = serde_json::from_str(&content).map_err(io::Error::other)?;
    if legacy.version != TRUST_STORE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported legacy MCP trust store version {}", legacy.version),
        ));
    }
    Ok(TrustStore {
        version: TRUST_STORE_VERSION,
        projects: legacy
            .projects
            .into_iter()
            .map(|record| ProjectTrustRecord {
                workspace: record.workspace,
                decisions: vec![TrustDecision {
                    scope: StoredTrustScope::Mcp,
                    resource_sha256: record.mcp.config_sha256,
                    trusted_at: record.mcp.trusted_at,
                }],
            })
            .collect(),
    })
}

fn save_store(store: &TrustStore) -> io::Result<()> {
    let path = trust_store_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("project trust store path has no parent"))?;
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
    temporary.persist(path).map_err(|error| error.error)?;
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
            if let Some(home) = old_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }
        result
    }

    #[test]
    fn mcp_trust_is_scoped_by_workspace_and_fingerprint() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        let other = temp.path().join("other");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&other).expect("other");

        with_home(&home, || {
            trust_project_mcp(&workspace, "first").expect("trust MCP");
            assert_eq!(
                project_mcp_trust(&workspace, "first").expect("inspect"),
                ProjectMcpTrust::Trusted
            );
            assert_eq!(
                project_mcp_trust(&other, "first").expect("inspect"),
                ProjectMcpTrust::Untrusted
            );
            assert_eq!(
                project_mcp_trust(&workspace, "second").expect("inspect"),
                ProjectMcpTrust::Stale { trusted_hash: "first".to_string() }
            );
        });
    }

    #[test]
    fn revocation_removes_mcp_trust() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&workspace).expect("workspace");

        with_home(&home, || {
            trust_project_mcp(&workspace, "mcp").expect("trust MCP");
            assert!(revoke_project_mcp_trust(&workspace).expect("revoke MCP"));
            assert_eq!(
                project_mcp_trust(&workspace, "mcp").expect("inspect"),
                ProjectMcpTrust::Untrusted
            );
        });
    }
}
