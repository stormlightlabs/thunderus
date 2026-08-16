//! Durable, workspace-scoped trust for project-owned runtime resources.
//!
//! Trust controls whether `thndrs` loads project resources. It does not grant
//! tool, filesystem, network, provider, or sandbox authority.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::utils;

const TRUST_STORE_VERSION: u32 = 1;

/// A project resource class with its own trust decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectTrustScope {
    /// `.thndrs/config.toml`, including configured ACP agents.
    Configuration,
    /// Project prompt templates and slash commands.
    PromptTemplates,
    /// Project skill packages and their references.
    Skills,
    /// Reserved for project-defined commands.
    Commands,
    /// Project MCP server definitions.
    Mcp,
    /// Reserved for project lifecycle hooks.
    Hooks,
}

impl ProjectTrustScope {
    /// Stable command-line and store label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::PromptTemplates => "prompt-templates",
            Self::Skills => "skills",
            Self::Commands => "commands",
            Self::Mcp => "mcp",
            Self::Hooks => "hooks",
        }
    }
}

/// Current state of one project trust decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectTrust {
    /// The exact current resource fingerprint was explicitly trusted.
    Trusted,
    /// No trust decision exists for this workspace and resource class.
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
    scope: ProjectTrustScope,
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

/// Inspect a trust decision for one workspace, resource class, and fingerprint.
pub fn project_trust(workspace: &Path, scope: ProjectTrustScope, hash: &str) -> io::Result<ProjectTrust> {
    let store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let Some(record) = store.projects.iter().find(|record| record.workspace == workspace) else {
        return Ok(ProjectTrust::Untrusted);
    };
    let Some(decision) = record.decisions.iter().find(|decision| decision.scope == scope) else {
        return Ok(ProjectTrust::Untrusted);
    };
    if decision.resource_sha256 == hash {
        Ok(ProjectTrust::Trusted)
    } else {
        Ok(ProjectTrust::Stale { trusted_hash: decision.resource_sha256.clone() })
    }
}

/// Persist trust for the exact current fingerprint of one project resource class.
pub fn trust_project(workspace: &Path, scope: ProjectTrustScope, hash: &str) -> io::Result<()> {
    let mut store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let decision =
        TrustDecision { scope, resource_sha256: hash.to_string(), trusted_at: crate::utils::datetime::now_iso8601() };
    if let Some(record) = store.projects.iter_mut().find(|record| record.workspace == workspace) {
        if let Some(existing) = record.decisions.iter_mut().find(|existing| existing.scope == scope) {
            *existing = decision;
        } else {
            record.decisions.push(decision);
            record.decisions.sort_by_key(|decision| decision.scope.label());
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

/// Revoke trust for one project resource class. Returns whether a decision existed.
pub fn revoke_project_trust(workspace: &Path, scope: ProjectTrustScope) -> io::Result<bool> {
    let mut store = load_store()?;
    let workspace = workspace_identity(workspace)?;
    let Some(record) = store.projects.iter_mut().find(|record| record.workspace == workspace) else {
        return Ok(false);
    };
    let before = record.decisions.len();
    record.decisions.retain(|decision| decision.scope != scope);
    let removed = record.decisions.len() != before;
    store.projects.retain(|record| !record.decisions.is_empty());
    if removed {
        save_store(&store)?;
    }
    Ok(removed)
}

/// Hash all regular files below the supplied project resource roots.
///
/// The fingerprint includes workspace-relative paths as well as bytes, so
/// renaming, adding, removing, or changing a resource invalidates trust.
pub fn fingerprint_directories(workspace: &Path, roots: &[PathBuf]) -> io::Result<Option<String>> {
    let mut files = Vec::new();
    for root in roots {
        collect_regular_files(root, &mut files)?;
    }
    if files.is_empty() {
        return Ok(None);
    }
    files.sort();
    files.dedup();

    let mut hasher = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(workspace).unwrap_or(&path);
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update([0]);
        hasher.update(fs::read(&path)?);
        hasher.update([0]);
    }
    Ok(Some(hex_encode(&hasher.finalize())))
}

fn collect_regular_files(root: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            files.push(path);
        } else if file_type.is_dir() {
            collect_regular_files(&path, files)?;
        }
    }
    Ok(())
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
                    scope: ProjectTrustScope::Mcp,
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

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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
    fn trust_is_scoped_by_workspace_resource_and_fingerprint() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        let other = temp.path().join("other");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&other).expect("other");

        with_home(&home, || {
            trust_project(&workspace, ProjectTrustScope::Skills, "first").expect("trust skills");
            assert_eq!(
                project_trust(&workspace, ProjectTrustScope::Skills, "first").expect("inspect"),
                ProjectTrust::Trusted
            );
            assert_eq!(
                project_trust(&workspace, ProjectTrustScope::PromptTemplates, "first").expect("inspect"),
                ProjectTrust::Untrusted
            );
            assert_eq!(
                project_trust(&other, ProjectTrustScope::Skills, "first").expect("inspect"),
                ProjectTrust::Untrusted
            );
            assert_eq!(
                project_trust(&workspace, ProjectTrustScope::Skills, "second").expect("inspect"),
                ProjectTrust::Stale { trusted_hash: "first".to_string() }
            );
        });
    }

    #[test]
    fn revocation_removes_only_the_requested_scope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&workspace).expect("workspace");

        with_home(&home, || {
            trust_project(&workspace, ProjectTrustScope::Skills, "skills").expect("trust skills");
            trust_project(&workspace, ProjectTrustScope::PromptTemplates, "prompts").expect("trust prompts");
            assert!(revoke_project_trust(&workspace, ProjectTrustScope::Skills).expect("revoke skills"));
            assert_eq!(
                project_trust(&workspace, ProjectTrustScope::Skills, "skills").expect("inspect"),
                ProjectTrust::Untrusted
            );
            assert_eq!(
                project_trust(&workspace, ProjectTrustScope::PromptTemplates, "prompts").expect("inspect"),
                ProjectTrust::Trusted
            );
        });
    }

    #[test]
    fn directory_fingerprint_changes_for_paths_and_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join(".thndrs/prompts");
        fs::create_dir_all(&root).expect("prompt directory");
        fs::write(root.join("review.md"), "first").expect("write prompt");
        let first = fingerprint_directories(temp.path(), std::slice::from_ref(&root))
            .expect("fingerprint")
            .expect("has files");
        fs::write(root.join("review.md"), "second").expect("change prompt");
        let second = fingerprint_directories(temp.path(), std::slice::from_ref(&root))
            .expect("fingerprint")
            .expect("has files");
        assert_ne!(first, second);
    }
}
