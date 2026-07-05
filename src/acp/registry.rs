//! ACP Registry metadata and local install records.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::validate_acp_agent_name;

/// Official ACP Registry JSON endpoint.
pub const DEFAULT_REGISTRY_URL: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REGISTRY_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FIELD_CHARS: usize = 300;
const CONFIG_BLOCK_START: &str = "# thndrs acp registry install:";
const CONFIG_BLOCK_END: &str = "# /thndrs acp registry install:";

#[derive(Clone, Debug, Deserialize)]
struct RegistryDocument {
    version: String,
    agents: Vec<RegistryAgent>,
}

#[derive(Clone, Debug, Deserialize)]
struct RegistryAgent {
    id: String,
    name: String,
    version: String,
    description: String,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    distribution: Distribution,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Distribution {
    #[serde(default)]
    npx: Option<PackageDistribution>,
    #[serde(default)]
    uvx: Option<PackageDistribution>,
    #[serde(default)]
    binary: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct PackageDistribution {
    package: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InstallDistribution {
    Npx {
        package: String,
        args: Vec<String>,
        env_keys: Vec<String>,
    },
    Uvx {
        package: String,
        args: Vec<String>,
        env_keys: Vec<String>,
    },
}

/// Request to install one registry agent into the local workspace config.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallRequest {
    /// Registry agent id to install.
    pub agent_id: String,
    /// Optional local `acp:<name>` config name.
    pub name: Option<String>,
    /// Registry source label for metadata.
    pub source: RegistrySource,
    /// Require explicit `--yes` confirmation before mutating files.
    pub confirmed: bool,
    /// Timestamp used for installed metadata.
    pub timestamp: String,
}

/// Request to update one managed installed registry agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateRequest {
    /// Local `acp:<name>` config name.
    pub name: String,
    /// Registry source label for metadata.
    pub source: RegistrySource,
    /// Require explicit `--yes` confirmation before mutating files.
    pub confirmed: bool,
    /// Timestamp used for installed metadata.
    pub timestamp: String,
}

/// ACP Registry metadata source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrySource {
    /// Official CDN registry source.
    Official,
    /// Local registry JSON file.
    File(PathBuf),
}

impl RegistrySource {
    fn label(&self) -> String {
        match self {
            RegistrySource::Official => DEFAULT_REGISTRY_URL.to_string(),
            RegistrySource::File(path) => format!("file:{}", path.display()),
        }
    }
}

/// Result of installing or updating one registry agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallOutcome {
    /// Local `acp:<name>` config name.
    pub name: String,
    /// Registry id.
    pub agent_id: String,
    /// Registry-published version.
    pub agent_version: String,
    /// Config file that was written.
    pub config_path: PathBuf,
    /// Metadata file that was written.
    pub metadata_path: PathBuf,
    /// Command users can select.
    pub model: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct InstalledAgentsFile {
    #[serde(default)]
    agents: BTreeMap<String, InstalledAgentRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct InstalledAgentRecord {
    registry_source: String,
    registry_document_version: String,
    agent_id: String,
    agent_name: String,
    agent_version: String,
    distribution: String,
    package: String,
    command: String,
    args: Vec<String>,
    install_dir: String,
    installed_at: String,
    status: String,
    env_keys: Vec<String>,
}

/// One ACP Registry agent row safe for display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryAgentView {
    /// Stable registry id.
    pub id: String,
    /// Human-readable agent name.
    pub name: String,
    /// Registry-published agent version.
    pub version: String,
    /// Short description.
    pub description: String,
    /// Available distribution kinds, excluding env values and install commands.
    pub distributions: Vec<String>,
    /// Repository or website URL.
    pub homepage: Option<String>,
}

/// Parsed ACP Registry metadata safe for display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryView {
    /// Registry document version.
    pub version: String,
    /// Display-safe agent rows.
    pub agents: Vec<RegistryAgentView>,
}

impl fmt::Display for RegistryView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "ACP registry v{} ({})", self.version, DEFAULT_REGISTRY_URL)?;
        if self.agents.is_empty() {
            writeln!(formatter, "no ACP agents found")?;
            return Ok(());
        }

        for agent in &self.agents {
            let distributions =
                if agent.distributions.is_empty() { "unknown".to_string() } else { agent.distributions.join(", ") };
            let homepage = agent.homepage.as_deref().unwrap_or("-");
            writeln!(
                formatter,
                "{}\t{}\t{}\t{}\t{}",
                agent.id, agent.name, agent.version, distributions, homepage
            )?;
            writeln!(formatter, "  {}", agent.description)?;
        }
        writeln!(
            formatter,
            "install/update: use `thndrs acp install <registry-id> --yes` or `thndrs acp update <name> --yes`; package managers and binaries are not executed during install/update"
        )
    }
}

/// Fetch the official ACP Registry metadata.
pub fn fetch_official() -> Result<RegistryView, String> {
    parse(&read_source(&RegistrySource::Official)?)
}

/// Read ACP Registry metadata from a local JSON file.
pub fn read_file(path: &Path) -> Result<RegistryView, String> {
    parse(&read_source(&RegistrySource::File(path.to_path_buf()))?)
}

/// Parse ACP Registry JSON into display-safe rows.
pub fn parse(json: &str) -> Result<RegistryView, String> {
    let document: RegistryDocument =
        serde_json::from_str(json).map_err(|err| format!("failed to parse ACP registry JSON: {err}"))?;
    let agents = document
        .agents
        .into_iter()
        .map(|agent| RegistryAgentView {
            id: display_field(&agent.id),
            name: display_field(&agent.name),
            version: display_field(&agent.version),
            description: display_field(&agent.description),
            distributions: distribution_labels(&agent.distribution),
            homepage: agent.repository.or(agent.website).map(|value| display_field(&value)),
        })
        .collect();
    Ok(RegistryView { version: document.version, agents })
}

/// Install one registry agent into workspace ACP config and metadata.
pub fn install(workspace: &Path, request: &InstallRequest) -> Result<InstallOutcome, String> {
    if !request.confirmed {
        return Err("refusing to install ACP registry agent without --yes".to_string());
    }
    let text = read_source(&request.source)?;
    let document = parse_document(&text)?;
    let agent = find_agent(&document, &request.agent_id)?;
    let name = request
        .name
        .clone()
        .unwrap_or_else(|| local_name_from_agent_id(&agent.id));
    validate_acp_name(&name)?;
    let distribution = install_distribution(&agent.distribution).ok_or_else(|| {
        format!(
            "ACP registry agent `{}` has no supported install distribution",
            agent.id
        )
    })?;

    let config_path = workspace.join(".thndrs").join("config.toml");
    let metadata_path = workspace.join(".thndrs").join("acp-installed.toml");
    let block = config_block(&name, &distribution);
    let config = read_optional_string(&config_path)?;
    if config.contains(&format!("[acp_agents.{name}]")) && !has_managed_block(&config, &name) {
        return Err(format!(
            "ACP agent `{name}` already exists and is not managed by registry install"
        ));
    }

    let updated_config = upsert_managed_block(&config, &name, &block);
    write_string(&config_path, &updated_config)?;

    let mut installed = read_installed_agents(&metadata_path)?;
    installed.agents.insert(
        name.clone(),
        installed_record(
            &document,
            &agent,
            &distribution,
            &request.source,
            workspace,
            &request.timestamp,
        ),
    );

    write_installed_agents(&metadata_path, &installed)?;

    Ok(InstallOutcome {
        name: name.clone(),
        agent_id: agent.id,
        agent_version: agent.version,
        config_path,
        metadata_path,
        model: format!("acp:{name}"),
    })
}

/// Update one managed registry-installed ACP agent.
pub fn update(workspace: &Path, request: &UpdateRequest) -> Result<InstallOutcome, String> {
    if !request.confirmed {
        return Err("refusing to update ACP registry agent without --yes".to_string());
    }

    let _ = validate_acp_name(&request.name)?;

    let metadata_path = workspace.join(".thndrs").join("acp-installed.toml");
    let installed = read_installed_agents(&metadata_path)?;
    let existing = installed
        .agents
        .get(&request.name)
        .ok_or_else(|| format!("ACP agent `{}` is not managed by registry install", request.name))?;

    install(
        workspace,
        &InstallRequest {
            agent_id: existing.agent_id.clone(),
            name: Some(request.name.clone()),
            source: request.source.clone(),
            confirmed: true,
            timestamp: request.timestamp.clone(),
        },
    )
}

fn read_source(source: &RegistrySource) -> Result<String, String> {
    match source {
        RegistrySource::Official => fetch_official_text(),
        RegistrySource::File(path) => read_limited_file(path),
    }
}

fn fetch_official_text() -> Result<String, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    agent
        .get(DEFAULT_REGISTRY_URL)
        .call()
        .map_err(|err| format!("failed to fetch ACP registry: {err}"))?
        .body_mut()
        .with_config()
        .limit(MAX_REGISTRY_BYTES)
        .read_to_string()
        .map_err(|err| format!("failed to read ACP registry response: {err}"))
}

fn parse_document(json: &str) -> Result<RegistryDocument, String> {
    serde_json::from_str(json).map_err(|err| format!("failed to parse ACP registry JSON: {err}"))
}

fn find_agent(document: &RegistryDocument, agent_id: &str) -> Result<RegistryAgent, String> {
    document
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .cloned()
        .ok_or_else(|| format!("ACP registry agent `{agent_id}` was not found"))
}

fn distribution_labels(distribution: &Distribution) -> Vec<String> {
    let mut labels = Vec::new();
    if let Some(npx) = &distribution.npx {
        labels.push(format!("npx:{}", display_field(&npx.package)));
    }
    if let Some(uvx) = &distribution.uvx {
        labels.push(format!("uvx:{}", display_field(&uvx.package)));
    }
    if distribution.binary.is_some() {
        labels.push("binary".to_string());
    }
    labels
}

fn install_distribution(dist: &Distribution) -> Option<InstallDistribution> {
    dist.npx
        .as_ref()
        .map(|package| InstallDistribution::Npx {
            package: package.package.clone(),
            args: package.args.clone(),
            env_keys: sorted_env_keys(&package.env),
        })
        .or_else(|| {
            dist.uvx.as_ref().map(|package| InstallDistribution::Uvx {
                package: package.package.clone(),
                args: package.args.clone(),
                env_keys: sorted_env_keys(&package.env),
            })
        })
}

fn sorted_env_keys(env: &BTreeMap<String, String>) -> Vec<String> {
    env.keys().map(|key| display_field(key)).collect()
}

fn local_name_from_agent_id(agent_id: &str) -> String {
    agent_id
        .strip_suffix("-acp")
        .unwrap_or(agent_id)
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn validate_acp_name(name: &str) -> Result<(), String> {
    validate_acp_agent_name(name).map_err(|err| err.to_string())
}

fn config_block(name: &str, distribution: &InstallDistribution) -> String {
    let (command, args) = command_and_args(distribution);
    let mut block = String::new();
    block.push_str(&format!("{CONFIG_BLOCK_START} {name}\n"));
    block.push_str(&format!("[acp_agents.{name}]\n"));
    block.push_str(&format!("command = {}\n", toml_string(&command)));
    block.push_str("args = [");
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            block.push_str(", ");
        }
        block.push_str(&toml_string(arg));
    }
    block.push_str("]\n");
    block.push_str("enabled = true\n");
    block.push_str(&format!("{CONFIG_BLOCK_END} {name}\n"));
    block
}

fn command_and_args(distribution: &InstallDistribution) -> (String, Vec<String>) {
    match distribution {
        InstallDistribution::Npx { package, args, .. } => {
            let mut all_args = vec!["-y".to_string(), package.clone()];
            all_args.extend(args.clone());
            ("npx".to_string(), all_args)
        }
        InstallDistribution::Uvx { package, args, .. } => {
            let mut all_args = vec![package.clone()];
            all_args.extend(args.clone());
            ("uvx".to_string(), all_args)
        }
    }
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

fn read_optional_string(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(format!("failed to read `{}`: {err}", path.display())),
    }
}

fn has_managed_block(config: &str, name: &str) -> bool {
    config.contains(&format!("{CONFIG_BLOCK_START} {name}"))
}

fn upsert_managed_block(config: &str, name: &str, block: &str) -> String {
    let start_marker = format!("{CONFIG_BLOCK_START} {name}");
    let end_marker = format!("{CONFIG_BLOCK_END} {name}");
    let Some(start) = config.find(&start_marker) else {
        let mut updated = config.trim_end().to_string();
        if !updated.is_empty() {
            updated.push_str("\n\n");
        }
        updated.push_str(block);
        return updated;
    };
    let Some(relative_end) = config[start..].find(&end_marker) else {
        return config.to_string();
    };
    let end = start + relative_end + end_marker.len();
    let mut updated = String::new();
    updated.push_str(config[..start].trim_end());
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(block.trim_end());
    updated.push('\n');
    updated.push_str(config[end..].trim_start_matches(['\r', '\n']));
    updated
}

fn write_string(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("failed to create `{}`: {err}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|err| format!("failed to write `{}`: {err}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|err| format!("failed to write `{}`: {err}", path.display()))
}

fn read_installed_agents(path: &Path) -> Result<InstalledAgentsFile, String> {
    let content = read_optional_string(path)?;
    if content.trim().is_empty() {
        return Ok(InstalledAgentsFile::default());
    }
    toml::from_str(&content)
        .map_err(|err| format!("failed to parse installed ACP metadata `{}`: {err}", path.display()))
}

fn write_installed_agents(path: &Path, installed: &InstalledAgentsFile) -> Result<(), String> {
    let content = toml::to_string_pretty(installed)
        .map_err(|err| format!("failed to encode installed ACP metadata `{}`: {err}", path.display()))?;
    write_string(path, &content)
}

fn installed_record(
    document: &RegistryDocument, agent: &RegistryAgent, dist: &InstallDistribution, source: &RegistrySource, ws: &Path,
    timestamp: &str,
) -> InstalledAgentRecord {
    let (command, args) = command_and_args(dist);
    let (distribution_name, package, env_keys) = match dist {
        InstallDistribution::Npx { package, env_keys, .. } => ("npx", package, env_keys),
        InstallDistribution::Uvx { package, env_keys, .. } => ("uvx", package, env_keys),
    };
    InstalledAgentRecord {
        registry_source: source.label(),
        registry_document_version: document.version.clone(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        agent_version: agent.version.clone(),
        distribution: distribution_name.to_string(),
        package: package.clone(),
        command,
        args,
        install_dir: ws.join(".thndrs").display().to_string(),
        installed_at: timestamp.to_string(),
        status: "installed".to_string(),
        env_keys: env_keys.clone(),
    }
}

fn read_limited_file(path: &Path) -> Result<String, String> {
    if let Ok(metadata) = fs::metadata(path)
        && metadata.len() > MAX_REGISTRY_BYTES
    {
        return Err(format!(
            "ACP registry file `{}` is too large: {} bytes exceeds {} bytes",
            path.display(),
            metadata.len(),
            MAX_REGISTRY_BYTES
        ));
    }

    let mut file =
        File::open(path).map_err(|err| format!("failed to read ACP registry file `{}`: {err}", path.display()))?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_REGISTRY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read ACP registry file `{}`: {err}", path.display()))?;
    if bytes.len() as u64 > MAX_REGISTRY_BYTES {
        return Err(format!(
            "ACP registry file `{}` is too large: exceeds {} bytes",
            path.display(),
            MAX_REGISTRY_BYTES
        ));
    }
    String::from_utf8(bytes)
        .map_err(|err| format!("failed to read ACP registry file `{}` as UTF-8: {err}", path.display()))
}

fn display_field(value: &str) -> String {
    let mut out = String::new();
    for part in value.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(part.chars().filter(|character| !character.is_control()));
        if out.chars().count() >= MAX_FIELD_CHARS {
            break;
        }
    }
    truncate_chars(out, MAX_FIELD_CHARS)
}

fn truncate_chars(value: String, max: usize) -> String {
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max {
        let Some(character) = chars.next() else {
            return value;
        };
        out.push(character);
    }
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parse_returns_display_safe_rows() {
        let registry = parse(
            r#"{
                "version": "1.0.0",
                "agents": [{
                    "id": "codex-acp",
                    "name": "Codex",
                    "version": "1.1.0",
                    "description": "ACP adapter\nfor OpenAI",
                    "repository": "https://github.com/agentclientprotocol/codex-acp",
                    "distribution": {
                        "npx": {
                            "package": "@agentclientprotocol/codex-acp@1.1.0",
                            "args": ["--token", "sk-secret"],
                            "env": {"OPENAI_API_KEY": "sk-secret"}
                        }
                    }
                }]
            }"#,
        )
        .expect("parse registry");

        assert_eq!(registry.version, "1.0.0");
        assert_eq!(registry.agents.len(), 1);
        assert_eq!(registry.agents[0].id, "codex-acp");
        assert_eq!(
            registry.agents[0].distributions,
            vec!["npx:@agentclientprotocol/codex-acp@1.1.0"]
        );
        let output = registry.to_string();
        assert!(output.contains("codex-acp\tCodex\t1.1.0"));
        assert!(output.contains("install/update: use `thndrs acp install"));
        assert!(!output.contains("sk-secret"));
        assert!(!output.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn registry_parse_reports_invalid_json() {
        let err = parse("{not json").expect_err("invalid json should fail");

        assert!(err.contains("failed to parse ACP registry JSON"));
    }

    #[test]
    fn registry_parse_normalizes_display_fields() {
        let registry = parse(
            r#"{
                "version": "1.0.0",
                "agents": [{
                    "id": "bad\nid",
                    "name": "Name\tWith\tTabs",
                    "version": "1.0.0\r\nnext",
                    "description": "line\u0007 one\nline two",
                    "repository": "https://example.test/repo\ninjected",
                    "distribution": {
                        "npx": {"package": "pkg\n--bad"}
                    }
                }]
            }"#,
        )
        .expect("parse registry");

        let output = registry.to_string();
        assert!(
            output.contains("bad id\tName With Tabs\t1.0.0 next\tnpx:pkg --bad\thttps://example.test/repo injected")
        );
        assert!(output.contains("line one line two"));
        assert!(!output.contains("bad\nid"));
        assert!(!output.contains('\u{0007}'));
    }

    #[test]
    fn registry_read_file_reports_missing_path() {
        let err = read_file(Path::new("/definitely/missing/acp-registry.json")).expect_err("missing file should fail");
        assert!(err.contains("failed to read ACP registry file"));
    }

    #[test]
    fn registry_read_file_rejects_oversized_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("registry.json");
        fs::write(&path, vec![b' '; MAX_REGISTRY_BYTES as usize + 1]).expect("write registry");

        let err = read_file(&path).expect_err("oversized file should fail");
        assert!(err.contains("too large"));
    }

    #[test]
    fn registry_install_requires_confirmation() {
        let temp = tempfile::tempdir().expect("temp dir");
        let registry_path = write_registry(temp.path(), "1.1.0", "pkg@1.1.0", None);
        let err = install(
            temp.path(),
            &InstallRequest {
                agent_id: "codex-acp".to_string(),
                name: Some("codex".to_string()),
                source: RegistrySource::File(registry_path),
                confirmed: false,
                timestamp: "2026-07-05T00:00:00Z".to_string(),
            },
        )
        .expect_err("install without --yes should fail");

        assert!(err.contains("without --yes"));
    }

    #[test]
    fn registry_install_writes_managed_config_and_metadata() {
        let temp = tempfile::tempdir().expect("temp dir");
        let registry_path = write_registry(temp.path(), "1.1.0", "@agentclientprotocol/codex-acp@1.1.0", None);
        let outcome = install(
            temp.path(),
            &InstallRequest {
                agent_id: "codex-acp".to_string(),
                name: Some("codex".to_string()),
                source: RegistrySource::File(registry_path),
                confirmed: true,
                timestamp: "2026-07-05T00:00:00Z".to_string(),
            },
        )
        .expect("install");

        assert_eq!(outcome.model, "acp:codex");

        let config = fs::read_to_string(temp.path().join(".thndrs/config.toml")).expect("config");
        assert!(config.contains("# thndrs acp registry install: codex"));
        assert!(config.contains("[acp_agents.codex]"));
        assert!(config.contains("command = \"npx\""));
        assert!(config.contains("args = [\"-y\", \"@agentclientprotocol/codex-acp@1.1.0\", \"--acp\"]"));
        assert!(!config.contains("OPENAI_API_KEY"));

        let metadata = fs::read_to_string(temp.path().join(".thndrs/acp-installed.toml")).expect("metadata");
        assert!(metadata.contains("agent_id = \"codex-acp\""));
        assert!(metadata.contains("agent_version = \"1.1.0\""));
        assert!(metadata.contains("distribution = \"npx\""));
        assert!(metadata.contains("env_keys = [\"OPENAI_API_KEY\"]"));
        assert!(!metadata.contains("sk-secret"));
    }

    #[test]
    fn registry_update_replaces_managed_config_and_metadata() {
        let temp = tempfile::tempdir().expect("temp dir");
        let old_registry = write_registry(temp.path(), "1.1.0", "@agentclientprotocol/codex-acp@1.1.0", None);
        install(
            temp.path(),
            &InstallRequest {
                agent_id: "codex-acp".to_string(),
                name: Some("codex".to_string()),
                source: RegistrySource::File(old_registry),
                confirmed: true,
                timestamp: "2026-07-05T00:00:00Z".to_string(),
            },
        )
        .expect("install");
        let new_registry = write_registry(
            temp.path(),
            "1.2.0",
            "@agentclientprotocol/codex-acp@1.2.0",
            Some("new"),
        );

        let outcome = update(
            temp.path(),
            &UpdateRequest {
                name: "codex".to_string(),
                source: RegistrySource::File(new_registry),
                confirmed: true,
                timestamp: "2026-07-06T00:00:00Z".to_string(),
            },
        )
        .expect("update");

        assert_eq!(outcome.agent_version, "1.2.0");

        let config = fs::read_to_string(temp.path().join(".thndrs/config.toml")).expect("config");
        assert!(config.contains("@agentclientprotocol/codex-acp@1.2.0"));
        assert!(!config.contains("@agentclientprotocol/codex-acp@1.1.0"));

        let metadata = fs::read_to_string(temp.path().join(".thndrs/acp-installed.toml")).expect("metadata");
        assert!(metadata.contains("agent_version = \"1.2.0\""));
        assert!(metadata.contains("installed_at = \"2026-07-06T00:00:00Z\""));
    }

    #[test]
    fn registry_install_rejects_unmanaged_existing_agent() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(temp.path().join(".thndrs")).expect("config dir");
        fs::write(
            temp.path().join(".thndrs/config.toml"),
            "[acp_agents.codex]\ncommand = \"manual\"\n",
        )
        .expect("write config");
        let registry_path = write_registry(temp.path(), "1.1.0", "pkg@1.1.0", None);

        let err = install(
            temp.path(),
            &InstallRequest {
                agent_id: "codex-acp".to_string(),
                name: Some("codex".to_string()),
                source: RegistrySource::File(registry_path),
                confirmed: true,
                timestamp: "2026-07-05T00:00:00Z".to_string(),
            },
        )
        .expect_err("unmanaged existing agent should fail");

        assert!(err.contains("not managed by registry install"));
    }

    #[test]
    fn registry_install_rejects_binary_only_agent() {
        let temp = tempfile::tempdir().expect("temp dir");
        let registry_path = temp.path().join("binary-registry.json");
        fs::write(
            &registry_path,
            r#"{
                "version": "1.0.0",
                "agents": [{
                    "id": "bin-agent",
                    "name": "Binary Agent",
                    "version": "1.0.0",
                    "description": "binary only",
                    "distribution": {"binary": {"linux-x86_64": {"archive": "https://example.test/a.tar.gz", "cmd": "./a"}}}
                }]
            }"#,
        )
        .expect("write registry");

        let err = install(
            temp.path(),
            &InstallRequest {
                agent_id: "bin-agent".to_string(),
                name: None,
                source: RegistrySource::File(registry_path),
                confirmed: true,
                timestamp: "2026-07-05T00:00:00Z".to_string(),
            },
        )
        .expect_err("binary-only should fail closed");

        assert!(err.contains("no supported install distribution"));
    }

    fn write_registry(root: &Path, version: &str, package: &str, suffix: Option<&str>) -> PathBuf {
        let filename = suffix.map_or_else(
            || "registry.json".to_string(),
            |suffix| format!("registry-{suffix}.json"),
        );
        let path = root.join(filename);
        fs::write(
            &path,
            format!(
                r#"{{
                    "version": "1.0.0",
                    "agents": [{{
                        "id": "codex-acp",
                        "name": "Codex",
                        "version": "{version}",
                        "description": "ACP adapter for OpenAI",
                        "repository": "https://github.com/agentclientprotocol/codex-acp",
                        "distribution": {{
                            "npx": {{
                                "package": "{package}",
                                "args": ["--acp"],
                                "env": {{"OPENAI_API_KEY": "sk-secret"}}
                            }}
                        }}
                    }}]
                }}"#
            ),
        )
        .expect("write registry");
        path
    }
}
