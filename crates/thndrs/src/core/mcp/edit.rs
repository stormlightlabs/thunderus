//! Atomic, comment-preserving MCP configuration edits.

use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, value};

use crate::config::edit_toml_file;

use super::config::{
    McpConfig, McpServerConfig, McpTransport, global_mcp_config_path, project_mcp_config_path, validate_mcp_config,
    validate_mcp_server_name,
};

/// Destination for one MCP configuration edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum McpConfigTarget {
    /// The user-wide configuration file.
    Global,
    /// The active workspace configuration file.
    Project,
}

/// Add or replace one MCP server definition and atomically write its file.
pub(crate) fn add_server(
    workspace: &Path, target: McpConfigTarget, name: &str, server: McpServerConfig,
) -> io::Result<PathBuf> {
    validate_mcp_server_name(name).map_err(io::Error::other)?;
    let path = config_path(workspace, target)?;
    edit_config(&path, |document| {
        let mut table = Table::new();
        match server.transport {
            McpTransport::Stdio => {
                table["transport"] = value("stdio");
                table["command"] = value(server.command.clone());
                if !server.args.is_empty() {
                    table["args"] = Item::Value(server.args.iter().collect());
                }
            }
            McpTransport::StreamableHttp => {
                table["transport"] = value("streamable_http");
                table["url"] = value(server.url.clone().unwrap_or_default());
            }
        }
        table["enabled"] = value(server.enabled);
        table["timeout_secs"] = value(server.timeout_secs as i64);
        if document.get("servers").is_none() {
            document["servers"] = Item::Table(Table::new());
        }
        let servers = document
            .get_mut("servers")
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "MCP `servers` must be a table"))?;
        servers.insert(name, Item::Table(table));
        Ok(())
    })?;
    Ok(path)
}

/// Remove one MCP server definition and atomically write its file.
pub(crate) fn remove_server(workspace: &Path, target: McpConfigTarget, name: &str) -> io::Result<PathBuf> {
    validate_mcp_server_name(name).map_err(io::Error::other)?;
    let path = config_path(workspace, target)?;
    edit_config(&path, |document| {
        let Some(servers) = document.get_mut("servers").and_then(Item::as_table_like_mut) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("MCP server `{name}` is not configured"),
            ));
        };
        if servers.remove(name).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("MCP server `{name}` is not configured"),
            ));
        }
        Ok(())
    })?;
    Ok(path)
}

fn config_path(workspace: &Path, target: McpConfigTarget) -> io::Result<PathBuf> {
    match target {
        McpConfigTarget::Global => global_mcp_config_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine the home directory for global MCP configuration",
            )
        }),
        McpConfigTarget::Project => Ok(project_mcp_config_path(workspace)),
    }
}

fn edit_config(path: &Path, edit: impl FnOnce(&mut DocumentMut) -> io::Result<()>) -> io::Result<()> {
    edit_toml_file(
        path,
        "MCP configuration",
        |source| validate_config_text(path, source),
        |document| edit(document).map(Some),
    )
    .map(|_| ())
}

fn validate_config_text(path: &Path, source: &str) -> io::Result<()> {
    let config: McpConfig = toml::from_str(source).map_err(|source| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse MCP configuration {}: {source}", path.display()),
        )
    })?;
    validate_mcp_config(&config).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn stdio(command: &str) -> McpServerConfig {
        McpServerConfig { command: command.to_string(), ..McpServerConfig::default() }
    }

    #[test]
    fn add_preserves_unrelated_comments_and_definitions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.toml");
        fs::write(
            &path,
            "# Keep this comment.\n[servers.keep]\n# Keep this definition too.\ncommand = \"keep\"\n\n[servers.docs]\ncommand = \"old\"\n",
        )
        .expect("write config");

        edit_config(&path, |document| {
            let mut table = Table::new();
            table["command"] = value("new");
            document["servers"]["docs"] = Item::Table(table);
            Ok(())
        })
        .expect("replace server");

        let result = fs::read_to_string(&path).expect("read config");
        assert!(result.contains("# Keep this comment."));
        assert!(result.contains("# Keep this definition too."));
        assert!(result.contains("[servers.keep]"));
        assert!(result.contains("command = \"keep\""));
        assert!(result.contains("command = \"new\""));
    }

    #[test]
    fn remove_keeps_unrelated_definition() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.toml");
        fs::write(
            &path,
            "[servers.keep]\ncommand = \"keep\"\n\n[servers.remove]\ncommand = \"remove\"\n",
        )
        .expect("write config");

        edit_config(&path, |document| {
            document
                .get_mut("servers")
                .and_then(Item::as_table_like_mut)
                .expect("servers table")
                .remove("remove");
            Ok(())
        })
        .expect("remove server");

        let result = fs::read_to_string(&path).expect("read config");
        assert!(result.contains("[servers.keep]"));
        assert!(!result.contains("[servers.remove]"));
    }

    #[test]
    fn malformed_configuration_is_not_replaced() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.toml");
        let original = "[servers.docs\ncommand = \"broken\"\n";
        fs::write(&path, original).expect("write config");

        let error = edit_config(&path, |_| Ok(())).expect_err("malformed config rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read_to_string(path).expect("read config"), original);
    }

    #[test]
    fn invalid_new_definition_is_not_written() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let path = workspace.join(".thndrs/mcp.toml");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(&path, "# unchanged\n").expect("write config");
        let mut invalid = stdio("");
        invalid.args.push("--bad".to_string());

        let error =
            add_server(&workspace, McpConfigTarget::Project, "docs", invalid).expect_err("invalid server rejected");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read_to_string(path).expect("read config"), "# unchanged\n");
    }
}
