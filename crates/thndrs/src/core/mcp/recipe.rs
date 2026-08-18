//! Resolution of catalog metadata into reviewable MCP configuration recipes.
//!
//! Resolution is pure: it does not contact the proposed MCP endpoint, execute
//! a launcher, or modify a package cache.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::catalog::{CatalogArgument, CatalogEntry, CatalogPackage, CatalogRemote, CatalogSource};
use super::config::{McpCatalogProvenance, McpServerConfig, McpTransport};

/// Transport selected from a catalog entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogRecipeTransport {
    /// A local stdio process started by a supported package runner.
    Stdio,
    /// A hosted Streamable HTTP endpoint.
    StreamableHttp,
}

impl CatalogRecipeTransport {
    /// Parse the CLI spelling of a catalog transport.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stdio" => Some(Self::Stdio),
            "streamable-http" => Some(Self::StreamableHttp),
            _ => None,
        }
    }
}

/// A local or hosted launch recipe ready for user review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogRecipe {
    /// Generated MCP server configuration.
    pub server: McpServerConfig,
    /// Catalog provenance written with the generated configuration after approval.
    pub provenance: McpCatalogProvenance,
    /// Environment variable names referenced by the generated configuration.
    pub environment_names: Vec<String>,
    /// Whether normal later startup can download package code.
    pub launcher_may_download: bool,
}

/// Resolve one selected catalog variant into an exact launch recipe.
pub fn resolve(
    source: &CatalogSource, entry: &CatalogEntry, transport: CatalogRecipeTransport, package_identifier: Option<&str>,
    retrieved_at: String,
) -> Result<CatalogRecipe, String> {
    ensure_platform_compatible(&entry.platform_constraints, "server")?;
    match transport {
        CatalogRecipeTransport::Stdio => resolve_package(source, entry, package_identifier, retrieved_at),
        CatalogRecipeTransport::StreamableHttp => resolve_remote(source, entry, package_identifier, retrieved_at),
    }
}

fn resolve_package(
    source: &CatalogSource, entry: &CatalogEntry, package_identifier: Option<&str>, retrieved_at: String,
) -> Result<CatalogRecipe, String> {
    let candidates = entry
        .packages
        .iter()
        .filter(|package| package.transports.iter().any(|transport| transport == "stdio"))
        .filter(|package| package_identifier.is_none_or(|identifier| identifier == package.identifier))
        .filter(|package| platform_compatible(&package.platform_constraints))
        .collect::<Vec<_>>();
    let package = match candidates.as_slice() {
        [] => {
            if entry
                .packages
                .iter()
                .any(|package| package.transports.iter().any(|transport| transport == "stdio"))
            {
                return Err(current_platform_error("no stdio package variant is compatible"));
            }
            return Err("the catalog entry has no stdio package variant".to_string());
        }
        [package] => *package,
        _ => return Err("multiple compatible stdio package variants exist; select one with `--package`".to_string()),
    };
    ensure_platform_compatible(&package.platform_constraints, "package")?;
    let version = exact_version(package.version.as_deref())?;
    let package_arguments = fixed_arguments(&package.package_arguments, "package")?;
    let runtime_arguments = fixed_arguments(&package.runtime_arguments, "runtime")?;
    let environment_names = environment_names(package)?;
    let (command, mut args, launcher_may_download) = package_launcher(package, version, runtime_arguments)?;
    args.extend(package_arguments);
    let env = environment_names
        .iter()
        .map(|name| (name.clone(), format!("${{{name}}}")))
        .collect::<BTreeMap<_, _>>();
    let server = McpServerConfig { command, args, env, ..McpServerConfig::default() };
    Ok(CatalogRecipe {
        provenance: provenance(
            source,
            entry,
            retrieved_at,
            "package",
            package_origin(package),
            Some(version),
            Some(&package.identifier),
            package.sha256.clone(),
            &server,
        ),
        server,
        environment_names,
        launcher_may_download,
    })
}

fn resolve_remote(
    source: &CatalogSource, entry: &CatalogEntry, package_identifier: Option<&str>, retrieved_at: String,
) -> Result<CatalogRecipe, String> {
    if package_identifier.is_some() {
        return Err("`--package` only selects stdio package variants".to_string());
    }
    let candidates = entry
        .remotes
        .iter()
        .filter(|remote| remote.transport == "streamable-http")
        .collect::<Vec<_>>();
    let remote = match candidates.as_slice() {
        [] => return Err("the catalog entry has no Streamable HTTP endpoint".to_string()),
        [remote] => *remote,
        _ => return Err("multiple Streamable HTTP endpoints exist; catalog selection is ambiguous".to_string()),
    };
    if !remote.header_names.is_empty() {
        return Err(format!(
            "the selected remote requires catalog-supplied headers ({}); configure it manually so thndrs never records catalog header values",
            remote.header_names.join(", ")
        ));
    }
    let url = remote_url(remote)?;
    let server = McpServerConfig {
        transport: McpTransport::StreamableHttp,
        url: Some(url.clone()),
        ..McpServerConfig::default()
    };
    Ok(CatalogRecipe {
        provenance: provenance(
            source,
            entry,
            retrieved_at,
            "remote",
            remote_host(&url)?,
            None,
            None,
            None,
            &server,
        ),
        server,
        environment_names: Vec::new(),
        launcher_may_download: false,
    })
}

fn package_launcher(
    package: &CatalogPackage, version: &str, runtime_arguments: Vec<String>,
) -> Result<(String, Vec<String>, bool), String> {
    let registry_type = package.registry_type.to_ascii_lowercase();
    let (expected_hint, command, mut args, may_download) = match registry_type.as_str() {
        "npm" => (
            "npx",
            "npx",
            vec!["--yes".to_string(), format!("{}@{version}", package.identifier)],
            true,
        ),
        "pypi" => ("uvx", "uvx", vec![format!("{}=={version}", package.identifier)], true),
        "nuget" => ("dnx", "dnx", vec![format!("{}@{version}", package.identifier)], true),
        "oci" => (
            "docker",
            "docker",
            vec![
                "run".to_string(),
                "--rm".to_string(),
                "-i".to_string(),
                oci_image(&package.identifier, version)?,
            ],
            true,
        ),
        "mcpb" => {
            return Err(
                "MCPB packages require a downloader and hash enforcement; thndrs does not install catalog packages"
                    .to_string(),
            );
        }
        _ => {
            return Err(format!(
                "unsupported local package registry type `{}`",
                package.registry_type
            ));
        }
    };
    if let Some(hint) = &package.runtime_hint
        && !hint.eq_ignore_ascii_case(expected_hint)
    {
        return Err(format!(
            "catalog runtime hint `{hint}` does not match the supported {registry_type} launcher `{expected_hint}`"
        ));
    }
    args.extend(runtime_arguments);
    Ok((command.to_string(), args, may_download))
}

fn fixed_arguments(arguments: &[CatalogArgument], kind: &str) -> Result<Vec<String>, String> {
    let mut resolved = Vec::with_capacity(arguments.len() * 2);
    for argument in arguments {
        if argument.secret {
            return Err(format!(
                "the catalog {kind} argument is marked secret and cannot be configured"
            ));
        }
        let value = argument
            .value
            .as_deref()
            .filter(|value| !value.contains('{') && !value.contains('}'))
            .ok_or_else(|| format!("the catalog {kind} argument requires user input and cannot be configured"))?;
        match argument.kind.as_str() {
            "positional" => resolved.push(value.to_string()),
            "named" => {
                let name = argument
                    .name
                    .as_deref()
                    .filter(|name| name.starts_with('-'))
                    .ok_or_else(|| format!("the catalog {kind} named argument has no flag name"))?;
                resolved.push(name.to_string());
                resolved.push(value.to_string());
            }
            _ => return Err(format!("unsupported catalog {kind} argument type `{}`", argument.kind)),
        }
    }
    Ok(resolved)
}

fn environment_names(package: &CatalogPackage) -> Result<Vec<String>, String> {
    let mut names = package
        .environment_variables
        .iter()
        .map(|variable| variable.name.clone())
        .collect::<Vec<_>>();
    if names
        .iter()
        .any(|name| name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
    {
        return Err("the catalog contains an invalid environment variable name".to_string());
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn exact_version(version: Option<&str>) -> Result<&str, String> {
    let Some(version) = version.map(str::trim).filter(|version| !version.is_empty()) else {
        return Err("the catalog package has no exact version".to_string());
    };
    if version.eq_ignore_ascii_case("latest") || version.contains(['*', '^', '~', '>', '<', '=', '|', ' ']) {
        return Err(format!("the catalog package version `{version}` is not exact"));
    }
    Ok(version)
}

fn oci_image(identifier: &str, version: &str) -> Result<String, String> {
    if identifier.contains("@sha256:") {
        return Ok(identifier.to_string());
    }
    let last_slash = identifier.rfind('/').unwrap_or(0);
    if identifier[last_slash..].contains(':') {
        if identifier.ends_with(":latest") {
            return Err("the OCI image uses `latest` instead of its exact catalog version".to_string());
        }
        return Ok(identifier.to_string());
    }
    Ok(format!("{identifier}:{version}"))
}

fn remote_url(remote: &CatalogRemote) -> Result<String, String> {
    if remote.url.contains('{') || remote.url.contains('}') {
        return Err("the catalog remote URL requires variables and cannot be configured without values".to_string());
    }
    let parsed = url::Url::parse(&remote.url).map_err(|error| format!("invalid catalog remote URL: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("the catalog remote URL must be an absolute HTTP or HTTPS URL".to_string());
    }
    Ok(remote.url.clone())
}

fn remote_host(url: &str) -> Result<String, String> {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .ok_or_else(|| "the catalog remote URL has no host".to_string())
}

fn package_origin(package: &CatalogPackage) -> String {
    package
        .registry_url
        .clone()
        .unwrap_or_else(|| package.registry_type.clone())
}

fn provenance(
    source: &CatalogSource, entry: &CatalogEntry, retrieved_at: String, origin_type: &str, origin: String,
    package_version: Option<&str>, package_identifier: Option<&str>, supplied_sha256: Option<String>,
    server: &McpServerConfig,
) -> McpCatalogProvenance {
    McpCatalogProvenance {
        catalog_url: source.url.clone(),
        catalog_name: source.name.clone(),
        entry_name: entry.name.clone(),
        metadata_version: entry.version.clone(),
        retrieved_at,
        origin_type: origin_type.to_string(),
        origin,
        package_version: package_version.map(str::to_string),
        package_identifier: package_identifier.map(str::to_string),
        supplied_sha256,
        digest_status: "catalog assertion; thndrs did not download or verify this artifact".to_string(),
        generated_transport_sha256: transport_fingerprint(server),
        generated_transport: server.clone(),
    }
}

/// Return the stable fingerprint used to bind catalog provenance to its generated transport.
pub(crate) fn transport_fingerprint(server: &McpServerConfig) -> String {
    let value = serde_json::to_vec(server).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(value);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn ensure_platform_compatible(constraints: &[String], subject: &str) -> Result<(), String> {
    if platform_compatible(constraints) {
        Ok(())
    } else {
        Err(current_platform_error(&format!("the {subject} platform constraints")))
    }
}

fn current_platform_error(subject: &str) -> String {
    format!(
        "{subject} do not support this platform ({} {})",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn platform_compatible(constraints: &[String]) -> bool {
    let current_os = std::env::consts::OS;
    let fallback = [current_os];
    let os_aliases: &[&str] = match current_os {
        "macos" => &["macos", "darwin"],
        "windows" => &["windows", "win32"],
        _ => &fallback,
    };
    let known_os = ["linux", "macos", "darwin", "windows", "win32"];
    let stated_os = constraints
        .iter()
        .map(|constraint| constraint.to_ascii_lowercase())
        .filter(|constraint| known_os.contains(&constraint.as_str()))
        .collect::<Vec<_>>();
    stated_os.is_empty()
        || stated_os
            .iter()
            .any(|constraint| os_aliases.contains(&constraint.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::catalog::CatalogEnvironmentVariable;

    fn source() -> CatalogSource {
        CatalogSource {
            name: "official".to_string(),
            url: "https://registry.example".to_string(),
            enabled: true,
            built_in: true,
            curation_claim: "preview; uncurated".to_string(),
        }
    }

    fn entry() -> CatalogEntry {
        CatalogEntry {
            source: "official".to_string(),
            source_url: "https://registry.example".to_string(),
            name: "io.example/weather".to_string(),
            title: None,
            description: "Weather".to_string(),
            claimed_publisher: "io.example".to_string(),
            version: "1.2.3".to_string(),
            status: None,
            transports: vec!["stdio".to_string()],
            packages: vec![CatalogPackage {
                registry_type: "npm".to_string(),
                registry_url: Some("https://registry.npmjs.org".to_string()),
                identifier: "@example/weather".to_string(),
                version: Some("1.2.3".to_string()),
                sha256: Some("catalog-assertion".to_string()),
                transports: vec!["stdio".to_string()],
                platform_constraints: Vec::new(),
                package_arguments: vec![CatalogArgument {
                    kind: "positional".to_string(),
                    value: Some("serve".to_string()),
                    ..CatalogArgument::default()
                }],
                runtime_arguments: Vec::new(),
                runtime_hint: Some("npx".to_string()),
                environment_variables: vec![CatalogEnvironmentVariable {
                    name: "WEATHER_TOKEN".to_string(),
                    required: true,
                    secret: true,
                }],
            }],
            remotes: Vec::new(),
            platform_constraints: Vec::new(),
            curation_claim: "preview; uncurated".to_string(),
        }
    }

    #[test]
    fn npm_recipe_is_pinned_and_records_only_environment_names() {
        let recipe = resolve(
            &source(),
            &entry(),
            CatalogRecipeTransport::Stdio,
            None,
            "2026-08-18T00:00:00Z".to_string(),
        )
        .expect("resolve npm recipe");
        assert_eq!(recipe.server.command, "npx");
        assert_eq!(recipe.server.args, ["--yes", "@example/weather@1.2.3", "serve"]);
        assert_eq!(recipe.server.env["WEATHER_TOKEN"], "${WEATHER_TOKEN}");
        assert!(recipe.launcher_may_download);
        assert!(recipe.provenance.digest_status.contains("catalog assertion"));
    }

    #[test]
    fn unpinned_and_secret_arguments_are_rejected() {
        let mut unpinned = entry();
        unpinned.packages[0].version = Some("latest".to_string());
        assert!(
            resolve(
                &source(),
                &unpinned,
                CatalogRecipeTransport::Stdio,
                None,
                "now".to_string()
            )
            .is_err()
        );

        let mut secret = entry();
        secret.packages[0].package_arguments[0].secret = true;
        assert!(
            resolve(
                &source(),
                &secret,
                CatalogRecipeTransport::Stdio,
                None,
                "now".to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn supported_local_package_types_generate_pinned_commands() {
        for (registry_type, identifier, expected_command, expected_package) in [
            ("pypi", "weather-mcp", "uvx", "weather-mcp==1.2.3"),
            ("nuget", "Weather.Mcp", "dnx", "Weather.Mcp@1.2.3"),
            (
                "oci",
                "ghcr.io/example/weather",
                "docker",
                "ghcr.io/example/weather:1.2.3",
            ),
        ] {
            let mut catalog_entry = entry();
            let package = &mut catalog_entry.packages[0];
            package.registry_type = registry_type.to_string();
            package.identifier = identifier.to_string();
            package.runtime_hint = None;
            package.package_arguments.clear();
            let recipe = resolve(
                &source(),
                &catalog_entry,
                CatalogRecipeTransport::Stdio,
                None,
                "now".to_string(),
            )
            .expect("resolve supported package type");
            assert_eq!(recipe.server.command, expected_command);
            assert!(recipe.server.args.iter().any(|arg| arg == expected_package));
        }
    }

    #[test]
    fn incompatible_platform_and_ambiguous_package_variants_are_rejected() {
        let mut incompatible = entry();
        incompatible.platform_constraints = vec!["windows".to_string()];
        if std::env::consts::OS != "windows" {
            assert!(
                resolve(
                    &source(),
                    &incompatible,
                    CatalogRecipeTransport::Stdio,
                    None,
                    "now".to_string()
                )
                .is_err()
            );
        }

        let mut ambiguous = entry();
        ambiguous.packages.push(ambiguous.packages[0].clone());
        assert!(
            resolve(
                &source(),
                &ambiguous,
                CatalogRecipeTransport::Stdio,
                None,
                "now".to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn remote_recipe_never_uses_catalog_headers() {
        let mut remote = entry();
        remote.remotes.push(CatalogRemote {
            transport: "streamable-http".to_string(),
            url: "https://weather.example/mcp".to_string(),
            header_names: Vec::new(),
        });
        let recipe = resolve(
            &source(),
            &remote,
            CatalogRecipeTransport::StreamableHttp,
            None,
            "now".to_string(),
        )
        .expect("resolve remote");
        assert_eq!(recipe.server.url.as_deref(), Some("https://weather.example/mcp"));

        remote.remotes[0].header_names.push("Authorization".to_string());
        assert!(
            resolve(
                &source(),
                &remote,
                CatalogRecipeTransport::StreamableHttp,
                None,
                "now".to_string()
            )
            .is_err()
        );
    }
}
