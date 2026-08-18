//! MCP configuration loading.
//!
//! MCP server definitions live in separate files from ordinary `thndrs`
//! runtime config:
//! - Global: `~/.thndrs/mcp.toml`
//! - Project: `.thndrs/mcp.toml`
//!
//! Project server definitions override global definitions by server name.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, ConfigSource};
use crate::trust::{self, ProjectMcpTrust};
use crate::utils;

const DEFAULT_TIMEOUT_SECS: u64 = 20;

/// Supported MCP transport types.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// Launch a local MCP server subprocess and communicate over stdin/stdout.
    #[default]
    Stdio,
    /// Use MCP Streamable HTTP.
    StreamableHttp,
}

/// Configuration for one MCP server.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpServerConfig {
    /// Transport used to reach this server.
    pub transport: McpTransport,
    /// Executable command for stdio servers.
    pub command: String,
    /// Command-line arguments passed after [`McpServerConfig::command`].
    pub args: Vec<String>,
    /// Environment variables passed to stdio child processes.
    pub env: BTreeMap<String, String>,
    /// URL used by Streamable HTTP servers.
    pub url: Option<String>,
    /// Headers sent to Streamable HTTP servers.
    pub headers: BTreeMap<String, String>,
    /// Whether this server is discoverable and callable.
    pub enabled: bool,
    /// Timeout for startup and tool calls in seconds.
    pub timeout_secs: u64,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            transport: McpTransport::Stdio,
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            url: None,
            headers: BTreeMap::new(),
            enabled: true,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

/// MCP server map keyed by configured server name.
pub type McpServersConfig = BTreeMap<String, McpServerConfig>;

/// Catalog metadata recorded beside a generated MCP server definition.
///
/// This is an audit record. It never grants trust, starts a server, or permits
/// MCP tool calls.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct McpCatalogProvenance {
    /// Catalog endpoint that supplied the selected entry.
    pub catalog_url: String,
    /// Local name of the catalog source.
    pub catalog_name: String,
    /// Registry entry identity.
    pub entry_name: String,
    /// Version of metadata selected from the catalog.
    pub metadata_version: String,
    /// Time at which thndrs retrieved the selected metadata.
    pub retrieved_at: String,
    /// Whether the recipe came from a package or remote endpoint.
    pub origin_type: String,
    /// Package registry or remote host supplied by the catalog.
    pub origin: String,
    /// Exact package version, when the recipe uses a package.
    pub package_version: Option<String>,
    /// Digest supplied by the catalog, when present.
    pub supplied_sha256: Option<String>,
    /// How the selected launcher treats the supplied digest.
    pub digest_status: String,
    /// SHA-256 of the generated transport configuration.
    pub generated_transport_sha256: String,
    /// Transport configuration generated from the selected catalog variant.
    pub generated_transport: McpServerConfig,
}

/// User-editable MCP configuration loaded from TOML.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    /// Named MCP server definitions.
    pub servers: McpServersConfig,
    /// Catalog provenance for definitions generated through `mcp catalog configure`.
    pub provenance: BTreeMap<String, McpCatalogProvenance>,
}

impl McpConfig {
    /// Merge `other` over `self`, replacing servers and provenance with the same name.
    pub fn merge(mut self, other: McpConfig) -> Self {
        self.servers.extend(other.servers);
        self.provenance.extend(other.provenance);
        self
    }
}

/// Fully resolved MCP configuration.
#[derive(Clone, Debug)]
pub struct EffectiveMcpConfig {
    /// Final resolved server definitions after precedence and env expansion.
    pub config: McpConfig,
    /// Loaded MCP config file layers in precedence order.
    pub layers: Vec<LoadedMcpConfigLayer>,
    /// Effective source for each active server definition.
    pub server_sources: BTreeMap<String, ConfigSource>,
    /// Project definitions discovered but not activated because trust is absent or stale.
    pub blocked_project_servers: BTreeMap<String, BlockedMcpServer>,
    /// Trust state for the discovered project MCP configuration, when present.
    pub project_trust: Option<ProjectMcpTrust>,
    /// Project definition names that replace global definitions when active.
    pub project_overrides_global: BTreeSet<String>,
    /// Non-fatal loading diagnostics.
    pub diagnostics: Vec<String>,
}

/// MCP server lifecycle state used consistently by CLI and interactive status surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpLifecycleState {
    /// The configuration explicitly disables the server.
    Disabled,
    /// The project configuration needs an explicit trust decision.
    BlockedByTrust,
    /// A client connection is being initialized.
    Starting,
    /// The server initialized and all advertised startup operations succeeded.
    Ready,
    /// The server initialized but reported a non-fatal diagnostic.
    Degraded,
    /// Initialization or a capability operation failed.
    Failed,
    /// The server is configured but no client is currently running.
    Stopped,
}

impl McpLifecycleState {
    /// Human-readable label for terminal and TUI status rows.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::BlockedByTrust => "blocked by trust",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// Semantic status row for one effective or trust-blocked MCP definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpServerStatus {
    /// Configured server name.
    pub name: String,
    /// Configured transport.
    pub transport: McpTransport,
    /// Configuration scope that supplied this definition.
    pub source: ConfigSource,
    /// Current lifecycle state.
    pub state: McpLifecycleState,
    /// Whether an inactive project definition would replace a global one.
    pub overrides_global: bool,
}

/// Project MCP definitions discovered but not activated because trust is absent or stale.
///
/// The data intentionally excludes commands, URLs, headers, and environment values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockedMcpServer {
    pub transport: McpTransport,
    pub enabled: bool,
    /// Whether this inactive project definition would override a global one.
    pub overrides_global: bool,
}

/// A single loaded MCP config file layer.
#[derive(Clone, Debug)]
pub struct LoadedMcpConfigLayer {
    pub source: ConfigSource,
    /// Redacted path label safe for diagnostics and metadata.
    pub display_path: Option<String>,
    /// Lowercase hex SHA-256 of file bytes.
    pub hash: Option<String>,
}

/// Validate an MCP server name accepted by `mcp__{server}__{tool}` namespacing.
pub fn validate_mcp_server_name(name: &str) -> Result<(), ConfigError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(ConfigError::InvalidConfig {
            key: format!("mcp.servers.{name}"),
            message: "name must match [A-Za-z0-9_-]+".to_string(),
        });
    }
    Ok(())
}

/// Load and merge MCP config layers.
pub fn load_effective_mcp(workspace: &Path, env_vars: &[(String, String)]) -> Result<EffectiveMcpConfig, ConfigError> {
    let mut layers = Vec::new();
    let mut merged = McpConfig::default();
    let mut server_sources = BTreeMap::new();
    let mut blocked_project_servers = BTreeMap::new();
    let mut project_trust = None;
    let mut project_overrides_global = BTreeSet::new();
    let mut diagnostics = Vec::new();

    if let Some(global_path) = global_mcp_config_path()
        && global_path.is_file()
    {
        let (global_config, hash) = load_mcp_file(&global_path)?;
        let display_path = mcp_global_path_display(&global_path);
        layers.push(LoadedMcpConfigLayer {
            source: ConfigSource::GlobalFile,
            display_path: Some(display_path),
            hash: Some(hash),
        });
        server_sources.extend(
            global_config
                .servers
                .keys()
                .cloned()
                .map(|name| (name, ConfigSource::GlobalFile)),
        );
        merged = merged.merge(global_config);
    }

    let project_path = project_mcp_config_path(workspace);
    if project_path.is_file() {
        let (project_config, hash) = load_mcp_file(&project_path)?;
        let display_path = mcp_project_path_display(&project_path, workspace);
        layers.push(LoadedMcpConfigLayer {
            source: ConfigSource::ProjectFile,
            display_path: Some(display_path),
            hash: Some(hash.clone()),
        });
        project_overrides_global = project_config
            .servers
            .keys()
            .filter(|name| merged.servers.contains_key(*name))
            .cloned()
            .collect();
        let trust = match trust::project_mcp_trust(workspace, &hash) {
            Ok(trust) => trust,
            Err(error) => {
                diagnostics.push(format!(
                    "project MCP configuration blocked: could not inspect MCP trust: {error}"
                ));
                ProjectMcpTrust::Untrusted
            }
        };
        match &trust {
            ProjectMcpTrust::Trusted => {
                server_sources.extend(
                    project_config
                        .servers
                        .keys()
                        .cloned()
                        .map(|name| (name, ConfigSource::ProjectFile)),
                );
                merged = merged.merge(project_config);
            }
            ProjectMcpTrust::Untrusted | ProjectMcpTrust::Stale { .. } => {
                for (name, server) in &project_config.servers {
                    blocked_project_servers.insert(
                        name.clone(),
                        BlockedMcpServer {
                            transport: server.transport,
                            enabled: server.enabled,
                            overrides_global: merged.servers.contains_key(name),
                        },
                    );
                }
                let reason = match trust {
                    ProjectMcpTrust::Untrusted => "has not been trusted for MCP",
                    ProjectMcpTrust::Stale { .. } => "changed since it was trusted for MCP",
                    ProjectMcpTrust::Trusted => unreachable!(),
                };
                diagnostics.push(format!(
                    "project MCP configuration is inactive because it {reason}; inspect with `thndrs mcp status` and approve with `thndrs mcp trust`"
                ));
            }
        }
        project_trust = Some(trust);
    }

    expand_mcp_env(&mut merged, env_vars, &mut diagnostics);
    server_sources.retain(|name, _| merged.servers.contains_key(name));
    validate_mcp_config(&merged)?;

    Ok(EffectiveMcpConfig {
        config: merged,
        layers,
        server_sources,
        blocked_project_servers,
        project_trust,
        project_overrides_global,
        diagnostics,
    })
}

/// Return the validated project MCP file hash, when a file is present.
/// Project effective and blocked definitions into semantic status rows.
///
/// Enabled definitions are stopped until a normal discovery or call path starts
/// a client. Runtime startup transitions are recorded by [`crate::mcp::manager`].
pub fn server_statuses(effective: &EffectiveMcpConfig) -> Vec<McpServerStatus> {
    let mut statuses = effective
        .config
        .servers
        .iter()
        .map(|(name, server)| McpServerStatus {
            name: name.clone(),
            transport: server.transport,
            source: *effective.server_sources.get(name).unwrap_or(&ConfigSource::Default),
            state: if server.enabled { McpLifecycleState::Stopped } else { McpLifecycleState::Disabled },
            overrides_global: false,
        })
        .collect::<Vec<_>>();
    statuses.extend(
        effective
            .blocked_project_servers
            .iter()
            .map(|(name, server)| McpServerStatus {
                name: name.clone(),
                transport: server.transport,
                source: ConfigSource::ProjectFile,
                state: McpLifecycleState::BlockedByTrust,
                overrides_global: server.overrides_global,
            }),
    );
    statuses.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.source.as_str().cmp(right.source.as_str()))
    });
    statuses
}

/// Return the validated project MCP file hash, when a file is present.
pub fn project_mcp_config_hash(workspace: &Path) -> Result<Option<String>, ConfigError> {
    let path = project_mcp_config_path(workspace);
    path.is_file()
        .then(|| load_mcp_file(&path).map(|(_, hash)| hash))
        .transpose()
}

pub(crate) fn global_mcp_config_path() -> Option<PathBuf> {
    utils::home_dir().map(|home| home.join(".thndrs").join("mcp.toml"))
}

pub(crate) fn project_mcp_config_path(workspace: &Path) -> PathBuf {
    workspace.join(".thndrs").join("mcp.toml")
}

fn mcp_global_path_display(path: &Path) -> String {
    if let Some(home) = utils::home_dir()
        && let Ok(rel) = path.strip_prefix(&home)
    {
        return format!("~/{}", rel.display());
    }
    path.display().to_string()
}

fn mcp_project_path_display(path: &Path, workspace: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(workspace) {
        return rel.display().to_string();
    }
    path.display().to_string()
}

fn load_mcp_file(path: &Path) -> Result<(McpConfig, String), ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::Read { path: path.to_path_buf(), source })?;
    let config: McpConfig =
        toml::from_str(&content).map_err(|source| ConfigError::Parse { path: path.to_path_buf(), source })?;
    validate_mcp_config(&config)?;
    let hash = sha256_hex(content.as_bytes());
    Ok((config, hash))
}

pub(crate) fn validate_mcp_config(config: &McpConfig) -> Result<(), ConfigError> {
    for (name, server) in &config.servers {
        validate_mcp_server_name(name)?;
        if server.timeout_secs == 0 {
            return Err(ConfigError::InvalidConfig {
                key: format!("mcp.servers.{name}.timeout_secs"),
                message: "timeout_secs must be greater than 0".to_string(),
            });
        }
        match server.transport {
            McpTransport::Stdio if server.command.trim().is_empty() => {
                return Err(ConfigError::InvalidConfig {
                    key: format!("mcp.servers.{name}.command"),
                    message: "command is required for stdio transport".to_string(),
                });
            }
            McpTransport::StreamableHttp if server.url.as_ref().is_none_or(|url| url.trim().is_empty()) => {
                return Err(ConfigError::InvalidConfig {
                    key: format!("mcp.servers.{name}.url"),
                    message: "url is required for streamable_http transport".to_string(),
                });
            }
            McpTransport::StreamableHttp => {
                let url = server.url.as_deref().unwrap_or_default();
                let valid_url = url::Url::parse(url)
                    .ok()
                    .is_some_and(|parsed| matches!(parsed.scheme(), "http" | "https"));
                if !valid_url {
                    return Err(ConfigError::InvalidConfig {
                        key: format!("mcp.servers.{name}.url"),
                        message: "url must be an absolute HTTP or HTTPS URL".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn expand_mcp_env(config: &mut McpConfig, env_vars: &[(String, String)], diagnostics: &mut Vec<String>) {
    let env = env_vars.iter().cloned().collect::<BTreeMap<_, _>>();
    let mut skipped = Vec::new();

    for (name, server) in &mut config.servers {
        let missing = expand_server_env(server, &env);
        if !missing.is_empty() {
            diagnostics.push(format!(
                "mcp server `{name}` skipped: unresolved environment variable{} {}",
                if missing.len() == 1 { "" } else { "s" },
                missing.into_iter().collect::<Vec<_>>().join(", ")
            ));
            skipped.push(name.clone());
        }
    }

    for name in skipped {
        config.servers.remove(&name);
    }
}

fn expand_server_env(server: &mut McpServerConfig, env: &BTreeMap<String, String>) -> BTreeSet<String> {
    let mut missing = BTreeSet::new();
    server.command = expand_value(&server.command, env, &mut missing);
    server.args = server
        .args
        .iter()
        .map(|value| expand_value(value, env, &mut missing))
        .collect();
    server.env = expand_map(&server.env, env, &mut missing);
    server.url = server.url.as_ref().map(|value| expand_value(value, env, &mut missing));
    server.headers = expand_map(&server.headers, env, &mut missing);
    missing
}

fn expand_map(
    values: &BTreeMap<String, String>, env: &BTreeMap<String, String>, missing: &mut BTreeSet<String>,
) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.clone(), expand_value(value, env, missing)))
        .collect()
}

fn expand_value(value: &str, env: &BTreeMap<String, String>, missing: &mut BTreeSet<String>) -> String {
    let mut expanded = String::new();
    let mut rest = value;

    while let Some(start) = rest.find("${") {
        expanded.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            expanded.push_str(&rest[start..]);
            return expanded;
        };

        let name = &after_start[..end];
        if let Some(replacement) = env.get(name) {
            expanded.push_str(replacement);
        } else {
            missing.insert(name.to_string());
            expanded.push_str(&rest[start..start + end + 3]);
        }
        rest = &after_start[end + 1..];
    }

    expanded.push_str(rest);
    expanded
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    hex_encode(&result)
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

        unsafe {
            std::env::set_var("HOME", home);
        }

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
    fn parses_stdio_server_config() {
        let config: McpConfig = toml::from_str(
            r#"
            [servers.docs]
            transport = "stdio"
            command = "docs-mcp"
            args = ["--workspace", "${THNDRS_WORKSPACE}"]
            env = { TOKEN = "${DOCS_TOKEN}" }
            enabled = false
            timeout_secs = 15
            "#,
        )
        .expect("mcp config parses");

        let server = &config.servers["docs"];
        assert_eq!(server.transport, McpTransport::Stdio);
        assert_eq!(server.command, "docs-mcp");
        assert_eq!(server.args, vec!["--workspace", "${THNDRS_WORKSPACE}"]);
        assert_eq!(server.env["TOKEN"], "${DOCS_TOKEN}");
        assert!(!server.enabled);
        assert_eq!(server.timeout_secs, 15);
    }

    #[test]
    fn parses_streamable_http_server_config() {
        let config: McpConfig = toml::from_str(
            r#"
            [servers.web]
            transport = "streamable_http"
            url = "https://mcp.example.test"
            headers = { Authorization = "Bearer ${MCP_TOKEN}" }
            "#,
        )
        .expect("mcp config parses");

        let server = &config.servers["web"];
        assert_eq!(server.transport, McpTransport::StreamableHttp);
        assert_eq!(server.url.as_deref(), Some("https://mcp.example.test"));
        assert_eq!(server.headers["Authorization"], "Bearer ${MCP_TOKEN}");
        assert_eq!(server.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = toml::from_str::<McpConfig>(
            r#"
            [servers.docs]
            command = "docs-mcp"
            prompt_injection = true
            "#,
        )
        .expect_err("unknown fields rejected");
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_invalid_server_names() {
        let err = validate_mcp_server_name("bad/name").expect_err("invalid name rejected");
        assert!(
            matches!(err, ConfigError::InvalidConfig { key, message } if key == "mcp.servers.bad/name" && message.contains("[A-Za-z0-9_-]+"))
        );
    }

    #[test]
    fn requires_stdio_command() {
        let config: McpConfig = toml::from_str(
            r#"
            [servers.docs]
            transport = "stdio"
            "#,
        )
        .expect("mcp config parses");

        let err = validate_mcp_config(&config).expect_err("missing command rejected");
        assert!(
            matches!(err, ConfigError::InvalidConfig { key, message } if key == "mcp.servers.docs.command" && message.contains("stdio"))
        );
    }

    #[test]
    fn requires_http_url() {
        let config: McpConfig = toml::from_str(
            r#"
            [servers.web]
            transport = "streamable_http"
            "#,
        )
        .expect("mcp config parses");

        let err = validate_mcp_config(&config).expect_err("missing url rejected");
        assert!(
            matches!(err, ConfigError::InvalidConfig { key, message } if key == "mcp.servers.web.url" && message.contains("streamable_http"))
        );
    }

    #[test]
    fn project_servers_override_global_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(home.join(".thndrs")).unwrap();
        fs::write(
            home.join(".thndrs").join("mcp.toml"),
            r#"
            [servers.shared]
            command = "global"

            [servers.global_only]
            command = "global-only"
            "#,
        )
        .unwrap();

        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(workspace.join(".thndrs")).unwrap();
        fs::write(
            workspace.join(".thndrs").join("mcp.toml"),
            r#"
            [servers.shared]
            command = "project"

            [servers.project_only]
            command = "project-only"
            "#,
        )
        .unwrap();

        let effective = with_home(&home, || {
            let hash = project_mcp_config_hash(&workspace).unwrap().unwrap();
            trust::trust_project_mcp(&workspace, &hash).unwrap();
            load_effective_mcp(&workspace, &[]).unwrap()
        });

        assert_eq!(effective.config.servers["shared"].command, "project");
        assert_eq!(effective.config.servers["global_only"].command, "global-only");
        assert_eq!(effective.config.servers["project_only"].command, "project-only");
        assert_eq!(effective.layers.len(), 2);
        assert_eq!(effective.layers[0].display_path.as_deref(), Some("~/.thndrs/mcp.toml"));
        assert_eq!(effective.layers[1].display_path.as_deref(), Some(".thndrs/mcp.toml"));
    }

    #[test]
    fn expands_environment_values() {
        let mut config: McpConfig = toml::from_str(
            r#"
            [servers.docs]
            command = "${DOCS_BIN}"
            args = ["--workspace", "${THNDRS_WORKSPACE}"]
            env = { TOKEN = "${DOCS_TOKEN}" }
            "#,
        )
        .expect("mcp config parses");
        let mut diagnostics = Vec::new();

        expand_mcp_env(
            &mut config,
            &[
                ("DOCS_BIN".to_string(), "docs-mcp".to_string()),
                ("THNDRS_WORKSPACE".to_string(), "/repo".to_string()),
                ("DOCS_TOKEN".to_string(), "secret".to_string()),
            ],
            &mut diagnostics,
        );

        let server = &config.servers["docs"];
        assert_eq!(server.command, "docs-mcp");
        assert_eq!(server.args, vec!["--workspace", "/repo"]);
        assert_eq!(server.env["TOKEN"], "secret");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn skips_servers_with_unresolved_environment_values() {
        let mut config: McpConfig = toml::from_str(
            r#"
            [servers.docs]
            command = "docs-mcp"
            args = ["${MISSING_WORKSPACE}"]

            [servers.ready]
            command = "ready-mcp"
            "#,
        )
        .expect("mcp config parses");
        let mut diagnostics = Vec::new();

        expand_mcp_env(&mut config, &[], &mut diagnostics);

        assert!(!config.servers.contains_key("docs"));
        assert!(config.servers.contains_key("ready"));
        assert_eq!(
            diagnostics,
            vec!["mcp server `docs` skipped: unresolved environment variable MISSING_WORKSPACE"]
        );
    }

    #[test]
    fn loaded_layers_record_only_safe_file_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(home.join(".thndrs")).unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(workspace.join(".thndrs")).unwrap();
        fs::write(
            workspace.join(".thndrs").join("mcp.toml"),
            r#"
            [servers.web]
            transport = "streamable_http"
            url = "https://mcp.example.test"
            env = { TOKEN = "env-secret" }
            headers = { Authorization = "Bearer header-secret" }
            "#,
        )
        .unwrap();

        let effective = with_home(&home, || {
            let hash = project_mcp_config_hash(&workspace).unwrap().unwrap();
            trust::trust_project_mcp(&workspace, &hash).unwrap();
            load_effective_mcp(&workspace, &[]).unwrap()
        });

        assert_eq!(effective.config.servers["web"].env["TOKEN"], "env-secret");
        assert_eq!(
            effective.config.servers["web"].headers["Authorization"],
            "Bearer header-secret"
        );
        assert_eq!(effective.layers[0].source, ConfigSource::ProjectFile);
        assert_eq!(effective.layers[0].display_path.as_deref(), Some(".thndrs/mcp.toml"));
        assert!(effective.layers[0].hash.is_some());
    }

    #[test]
    fn diagnostics_do_not_include_secret_values_for_unresolved_env() {
        let mut config: McpConfig = toml::from_str(
            r#"
            [servers.web]
            transport = "streamable_http"
            url = "https://mcp.example.test"
            headers = { Authorization = "Bearer ${MISSING_TOKEN}" }
            "#,
        )
        .expect("mcp config parses");
        let mut diagnostics = Vec::new();

        expand_mcp_env(&mut config, &[], &mut diagnostics);

        assert_eq!(
            diagnostics,
            vec!["mcp server `web` skipped: unresolved environment variable MISSING_TOKEN"]
        );
    }

    #[test]
    fn status_projection_labels_disabled_and_blocked_servers() {
        let mut effective = EffectiveMcpConfig {
            config: McpConfig::default(),
            layers: Vec::new(),
            server_sources: BTreeMap::new(),
            blocked_project_servers: BTreeMap::new(),
            project_trust: Some(ProjectMcpTrust::Untrusted),
            project_overrides_global: BTreeSet::new(),
            diagnostics: Vec::new(),
        };
        effective.config.servers.insert(
            "disabled".to_string(),
            McpServerConfig { enabled: false, command: "server".to_string(), ..McpServerConfig::default() },
        );
        effective.blocked_project_servers.insert(
            "blocked".to_string(),
            BlockedMcpServer { transport: McpTransport::Stdio, enabled: true, overrides_global: false },
        );

        let statuses = server_statuses(&effective);

        assert_eq!(statuses[0].name, "blocked");
        assert_eq!(statuses[0].state.label(), "blocked by trust");
        assert_eq!(statuses[1].name, "disabled");
        assert_eq!(statuses[1].state.label(), "disabled");
    }

    #[test]
    fn project_servers_are_inactive_until_the_exact_config_is_trusted() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(workspace.join(".thndrs")).unwrap();
        let path = workspace.join(".thndrs/mcp.toml");
        fs::write(&path, "[servers.docs]\ncommand = \"docs-mcp\"\n").unwrap();

        with_home(&home, || {
            let blocked = load_effective_mcp(&workspace, &[]).unwrap();
            assert!(blocked.config.servers.is_empty());
            assert!(blocked.blocked_project_servers.contains_key("docs"));
            assert_eq!(blocked.project_trust, Some(ProjectMcpTrust::Untrusted));

            let hash = project_mcp_config_hash(&workspace).unwrap().unwrap();
            trust::trust_project_mcp(&workspace, &hash).unwrap();
            let active = load_effective_mcp(&workspace, &[]).unwrap();
            assert!(active.config.servers.contains_key("docs"));
            assert_eq!(active.server_sources["docs"], ConfigSource::ProjectFile);

            fs::write(&path, "[servers.docs]\ncommand = \"changed-mcp\"\n").unwrap();
            let changed = load_effective_mcp(&workspace, &[]).unwrap();
            assert!(changed.config.servers.is_empty());
            assert!(matches!(changed.project_trust, Some(ProjectMcpTrust::Stale { .. })));
        });
    }
}
