//! Compatibility wrappers for the shared project trust store.

use std::io;
use std::path::Path;

pub use crate::trust::ProjectTrust as ProjectMcpTrust;
use crate::trust::{self, ProjectTrustScope};

/// Inspect trust for the current project MCP file hash.
pub fn project_mcp_trust(workspace: &Path, config_hash: &str) -> io::Result<ProjectMcpTrust> {
    trust::project_trust(workspace, ProjectTrustScope::Mcp, config_hash)
}

/// Persist MCP trust for this workspace and exact project config hash.
pub fn trust_project_mcp(workspace: &Path, config_hash: &str) -> io::Result<()> {
    trust::trust_project(workspace, ProjectTrustScope::Mcp, config_hash)
}

/// Revoke MCP trust for this workspace. Returns whether a decision existed.
pub fn revoke_project_mcp(workspace: &Path) -> io::Result<bool> {
    trust::revoke_project_trust(workspace, ProjectTrustScope::Mcp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
