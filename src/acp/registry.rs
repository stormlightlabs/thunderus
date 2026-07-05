//! Read-only ACP Registry metadata.

use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

/// Official ACP Registry JSON endpoint.
pub const DEFAULT_REGISTRY_URL: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REGISTRY_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FIELD_CHARS: usize = 300;

#[derive(Debug, Deserialize)]
struct RegistryDocument {
    version: String,
    agents: Vec<RegistryAgent>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Default, Deserialize)]
struct Distribution {
    #[serde(default)]
    npx: Option<PackageDistribution>,
    #[serde(default)]
    uvx: Option<PackageDistribution>,
    #[serde(default)]
    binary: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PackageDistribution {
    package: String,
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
            "install/update: unavailable pending command provenance, package-manager behavior, and security review"
        )
    }
}

/// Fetch the official ACP Registry metadata.
pub fn fetch_official() -> Result<RegistryView, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let text = agent
        .get(DEFAULT_REGISTRY_URL)
        .call()
        .map_err(|err| format!("failed to fetch ACP registry: {err}"))?
        .body_mut()
        .with_config()
        .limit(MAX_REGISTRY_BYTES)
        .read_to_string()
        .map_err(|err| format!("failed to read ACP registry response: {err}"))?;
    parse(&text)
}

/// Read ACP Registry metadata from a local JSON file.
pub fn read_file(path: &Path) -> Result<RegistryView, String> {
    let text = read_limited_file(path)?;
    parse(&text)
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
    file.by_ref()
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
        assert!(output.contains("install/update: unavailable"));
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
}
