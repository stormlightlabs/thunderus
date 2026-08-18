//! Read-only discovery from configurable MCP server catalogs.
//!
//! Catalogs propose metadata only. They never configure, start, or trust MCP
//! servers. Catalog configuration and its cache live under the user's home
//! directory and are therefore independent of workspace MCP configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::config;
use crate::utils;
use crate::utils::datetime;

/// Official MCP Registry base URL.
pub const OFFICIAL_CATALOG_URL: &str = "https://registry.modelcontextprotocol.io";

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_QUERY_CHARS: usize = 200;
const MAX_PAGE_SIZE: usize = 50;
const MAX_CACHED_ENTRIES: usize = 200;
const MAX_FIELD_CHARS: usize = 500;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
struct CatalogFile {
    catalogs: BTreeMap<String, CatalogFileSource>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct CatalogFileSource {
    url: Option<String>,
    enabled: Option<bool>,
    curation: Option<String>,
}

/// A catalog source selected from global catalog configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSource {
    /// Stable local source name.
    pub name: String,
    /// API-compatible catalog base URL.
    pub url: String,
    /// Whether the source is searched.
    pub enabled: bool,
    /// Whether this is the built-in official source.
    pub built_in: bool,
    /// Labels that the catalog supplies or the user configured; never a verdict.
    pub curation_claim: String,
}

/// Package origin supplied by a catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CatalogPackage {
    /// Catalog-supplied artifact registry kind, such as `npm` or `pypi`.
    pub registry_type: String,
    /// Catalog-supplied artifact registry URL, when present.
    pub registry_url: Option<String>,
    /// Catalog-supplied package identifier.
    pub identifier: String,
    /// Catalog-supplied package version, when present.
    pub version: Option<String>,
    /// Catalog-supplied SHA-256, when present. thndrs does not verify it here.
    pub sha256: Option<String>,
    /// Transports advertised for this package.
    pub transports: Vec<String>,
    /// Catalog-supplied platform constraints, when present.
    pub platform_constraints: Vec<String>,
}

/// Display-safe metadata for one catalog entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CatalogEntry {
    /// Source that supplied this entry.
    pub source: String,
    /// Source endpoint that supplied this entry.
    pub source_url: String,
    /// Server name used by the registry API.
    pub name: String,
    /// Optional human-readable title.
    pub title: Option<String>,
    /// Catalog-supplied description.
    pub description: String,
    /// Namespace-derived publisher identity claimed through the catalog.
    pub claimed_publisher: String,
    /// Catalog-supplied server version.
    pub version: String,
    /// Catalog-supplied server status, when present.
    pub status: Option<String>,
    /// All advertised transport types.
    pub transports: Vec<String>,
    /// Package origins supplied by the catalog.
    pub packages: Vec<CatalogPackage>,
    /// Catalog-supplied platform constraints, when present.
    pub platform_constraints: Vec<String>,
    /// Curation label supplied by the selected catalog configuration.
    pub curation_claim: String,
}

/// One source result, including its retrieval time and pagination cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSearchResult {
    /// Source queried or read from cache.
    pub source: CatalogSource,
    /// Entries returned for the requested page or cached snapshot.
    pub entries: Vec<CatalogEntry>,
    /// Time at which metadata was retrieved, when it came from cache.
    pub retrieved_at: Option<String>,
    /// Opaque cursor supplied by the catalog for the next page.
    pub next_cursor: Option<String>,
    /// Whether these results came from an offline cache.
    pub from_cache: bool,
}

/// Aggregated results from all enabled sources.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogSearch {
    /// Successful source results.
    pub results: Vec<CatalogSearchResult>,
    /// Source-local failures. Other sources still contribute results.
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CatalogCache {
    retrieved_at: String,
    entries: Vec<CatalogEntry>,
}

#[derive(Deserialize)]
struct RawList {
    servers: Vec<RawEnvelope>,
    #[serde(default)]
    metadata: RawListMetadata,
}

#[derive(Default, Deserialize)]
struct RawListMetadata {
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct RawEnvelope {
    server: RawServer,
    #[serde(rename = "_meta", default)]
    meta: serde_json::Value,
}

#[derive(Deserialize)]
struct RawServer {
    name: String,
    description: String,
    version: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    packages: Vec<RawPackage>,
    #[serde(default)]
    remotes: Vec<RawTransport>,
    #[serde(default, alias = "platformConstraints")]
    platforms: Vec<String>,
}

#[derive(Deserialize)]
struct RawPackage {
    #[serde(rename = "registryType")]
    registry_type: String,
    #[serde(rename = "registryBaseUrl", default)]
    registry_url: Option<String>,
    identifier: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(rename = "fileSha256", default)]
    sha256: Option<String>,
    transport: RawTransport,
    #[serde(default, alias = "platformConstraints")]
    platforms: Vec<String>,
}

#[derive(Deserialize)]
struct RawTransport {
    #[serde(rename = "type")]
    kind: String,
}

trait CatalogFetcher {
    fn get(&self, url: &Url) -> Result<String, String>;
}

struct HttpCatalogFetcher;

impl CatalogFetcher for HttpCatalogFetcher {
    fn get(&self, url: &Url) -> Result<String, String> {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(FETCH_TIMEOUT))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent
            .get(url.as_str())
            .call()
            .map_err(|error| format!("request failed: {error}"))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("catalog returned HTTP {status}"));
        }
        response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(|error| format!("could not read response: {error}"))
    }
}

/// Read configured catalog sources. This reads only global configuration.
pub fn sources() -> Result<Vec<CatalogSource>, String> {
    let path = catalog_config_path()?;
    let file = match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| format!("failed to parse catalog configuration `{}`: {error}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CatalogFile::default(),
        Err(error) => {
            return Err(format!(
                "failed to read catalog configuration `{}`: {error}",
                path.display()
            ));
        }
    };
    sources_from_file(file)
}

/// Add one globally configured API-compatible catalog source.
pub fn add_source(name: &str, url: &str, curation: Option<&str>) -> Result<PathBuf, String> {
    validate_source_name(name)?;
    if name == "official" {
        return Err("`official` is built in; use `thndrs mcp catalog disable official` instead".to_string());
    }
    validate_catalog_url(url)?;
    let path = catalog_config_path()?;
    let mut file = read_catalog_file(&path)?;
    file.catalogs.insert(
        name.to_string(),
        CatalogFileSource {
            url: Some(url.trim_end_matches('/').to_string()),
            enabled: Some(true),
            curation: curation.map(clean_required_field).transpose()?,
        },
    );
    write_catalog_file(&path, &file)?;
    Ok(path)
}

/// Remove one global custom catalog source.
pub fn remove_source(name: &str) -> Result<PathBuf, String> {
    validate_source_name(name)?;
    if name == "official" {
        return Err("the built-in `official` catalog cannot be removed; disable it instead".to_string());
    }
    let path = catalog_config_path()?;
    let mut file = read_catalog_file(&path)?;
    if file.catalogs.remove(name).is_none() {
        return Err(format!("catalog source `{name}` is not configured"));
    }
    write_catalog_file(&path, &file)?;
    Ok(path)
}

/// Enable or disable a global catalog source.
pub fn set_source_enabled(name: &str, enabled: bool) -> Result<PathBuf, String> {
    validate_source_name(name)?;
    let path = catalog_config_path()?;
    let mut file = read_catalog_file(&path)?;
    if name == "official" {
        let source = file.catalogs.entry(name.to_string()).or_default();
        source.enabled = Some(enabled);
        source.url = None;
    } else {
        let source = file
            .catalogs
            .get_mut(name)
            .ok_or_else(|| format!("catalog source `{name}` is not configured"))?;
        source.enabled = Some(enabled);
    }
    write_catalog_file(&path, &file)?;
    Ok(path)
}

/// Search each enabled source. A source failure falls back to its cached snapshot.
pub fn search(query: &str, limit: usize, cursor: Option<&str>, offline: bool) -> Result<CatalogSearch, String> {
    search_with_fetcher(query, limit, cursor, offline, &HttpCatalogFetcher)
}

/// Fetch one entry from enabled sources, or their cache when offline/unavailable.
pub fn detail(name: &str, source_name: Option<&str>, version: &str, offline: bool) -> Result<CatalogSearch, String> {
    if name.trim().is_empty() || name.chars().count() > MAX_FIELD_CHARS {
        return Err("catalog server name must be between 1 and 500 characters".to_string());
    }
    let requested = source_name.map(str::to_string);
    let sources = sources()?;
    if let Some(source_name) = &requested
        && !sources.iter().any(|source| &source.name == source_name)
    {
        return Err(format!("catalog source `{source_name}` is not enabled"));
    }
    let mut search = CatalogSearch::default();
    for source in sources
        .into_iter()
        .filter(|source| requested.as_deref().is_none_or(|name| name == source.name))
    {
        if !source.enabled {
            continue;
        }
        if offline {
            match cached_detail(&source, name) {
                Ok(result) => search.results.push(result),
                Err(error) => search.diagnostics.push(format!("catalog `{}`: {error}", source.name)),
            }
            continue;
        }
        match detail_from_source(&source, name, version, &HttpCatalogFetcher) {
            Ok(result) => {
                if let Err(error) = update_cache(&source, &result.entries) {
                    search
                        .diagnostics
                        .push(format!("catalog `{}`: could not update cache: {error}", source.name));
                }
                search.results.push(result);
            }
            Err(error) => match cached_detail(&source, name) {
                Ok(result) => {
                    search.diagnostics.push(format!(
                        "catalog `{}`: {error}; using cached metadata from {}",
                        source.name,
                        result.retrieved_at.as_deref().unwrap_or("an unknown time")
                    ));
                    search.results.push(result);
                }
                Err(_) => search.diagnostics.push(format!("catalog `{}`: {error}", source.name)),
            },
        }
    }
    Ok(search)
}

fn search_with_fetcher(
    query: &str, limit: usize, cursor: Option<&str>, offline: bool, fetcher: &impl CatalogFetcher,
) -> Result<CatalogSearch, String> {
    let query = clean_required_field(query)?;
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(format!("catalog query exceeds {MAX_QUERY_CHARS} characters"));
    }
    let limit = limit.clamp(1, MAX_PAGE_SIZE);
    let mut search = CatalogSearch::default();
    for source in sources()?.into_iter().filter(|source| source.enabled) {
        if offline {
            match cached_search(&source, &query) {
                Ok(result) => search.results.push(result),
                Err(error) => search.diagnostics.push(format!("catalog `{}`: {error}", source.name)),
            }
            continue;
        }
        match search_source(&source, &query, limit, cursor, fetcher) {
            Ok(result) => {
                if let Err(error) = update_cache(&source, &result.entries) {
                    search
                        .diagnostics
                        .push(format!("catalog `{}`: could not update cache: {error}", source.name));
                }
                search.results.push(result);
            }
            Err(error) => match cached_search(&source, &query) {
                Ok(result) => {
                    search.diagnostics.push(format!(
                        "catalog `{}`: {error}; using cached metadata from {}",
                        source.name,
                        result.retrieved_at.as_deref().unwrap_or("an unknown time")
                    ));
                    search.results.push(result);
                }
                Err(_) => search.diagnostics.push(format!("catalog `{}`: {error}", source.name)),
            },
        }
    }
    Ok(search)
}

fn search_source(
    source: &CatalogSource, query: &str, limit: usize, cursor: Option<&str>, fetcher: &impl CatalogFetcher,
) -> Result<CatalogSearchResult, String> {
    let mut url = server_url(source)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("search", query);
        pairs.append_pair("limit", &limit.to_string());
        if let Some(cursor) = cursor.filter(|cursor| !cursor.is_empty()) {
            pairs.append_pair("cursor", cursor);
        }
    }
    let raw = fetcher.get(&url)?;
    let document: RawList = serde_json::from_str(&raw).map_err(|error| format!("invalid catalog response: {error}"))?;
    if document.servers.len() > MAX_PAGE_SIZE {
        return Err(format!("catalog returned more than {MAX_PAGE_SIZE} entries"));
    }
    let entries = document
        .servers
        .into_iter()
        .map(|entry| catalog_entry(source, entry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CatalogSearchResult {
        source: source.clone(),
        entries,
        retrieved_at: None,
        next_cursor: document
            .metadata
            .next_cursor
            .as_deref()
            .map(clean_optional_field)
            .transpose()?,
        from_cache: false,
    })
}

fn detail_from_source(
    source: &CatalogSource, name: &str, version: &str, fetcher: &impl CatalogFetcher,
) -> Result<CatalogSearchResult, String> {
    let mut url = server_url(source)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "catalog URL cannot accept API paths")?;
        segments.pop_if_empty();
        segments.push(name);
        segments.push("versions");
        segments.push(version);
    }
    let raw = fetcher.get(&url)?;
    let entry: RawEnvelope =
        serde_json::from_str(&raw).map_err(|error| format!("invalid catalog detail response: {error}"))?;
    let entry = catalog_entry(source, entry)?;
    Ok(CatalogSearchResult {
        source: source.clone(),
        entries: vec![entry],
        retrieved_at: None,
        next_cursor: None,
        from_cache: false,
    })
}

fn cached_search(source: &CatalogSource, query: &str) -> Result<CatalogSearchResult, String> {
    let cache = read_cache(source)?;
    let query = query.to_ascii_lowercase();
    let entries = cache
        .entries
        .into_iter()
        .filter(|entry| {
            entry.name.to_ascii_lowercase().contains(&query) || entry.description.to_ascii_lowercase().contains(&query)
        })
        .collect();
    Ok(CatalogSearchResult {
        source: source.clone(),
        entries,
        retrieved_at: Some(cache.retrieved_at),
        next_cursor: None,
        from_cache: true,
    })
}

fn cached_detail(source: &CatalogSource, name: &str) -> Result<CatalogSearchResult, String> {
    let cache = read_cache(source)?;
    let entries = cache
        .entries
        .into_iter()
        .filter(|entry| entry.name == name)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(format!("no cached metadata for `{name}`"));
    }
    Ok(CatalogSearchResult {
        source: source.clone(),
        entries,
        retrieved_at: Some(cache.retrieved_at),
        next_cursor: None,
        from_cache: true,
    })
}

fn catalog_entry(source: &CatalogSource, envelope: RawEnvelope) -> Result<CatalogEntry, String> {
    let server = envelope.server;
    let name = clean_required_field(&server.name)?;
    if !name.contains('/') {
        return Err("catalog entry has an invalid server name".to_string());
    }
    let claimed_publisher = name
        .split_once('/')
        .map(|(publisher, _)| publisher)
        .unwrap_or_default()
        .to_string();
    let mut transports = BTreeSet::new();
    for remote in server.remotes {
        transports.insert(clean_required_field(&remote.kind)?);
    }
    let mut platform_constraints = clean_fields(server.platforms)?;
    let mut packages = Vec::new();
    if server.packages.len() > 32 {
        return Err("catalog entry has too many package origins".to_string());
    }
    for package in server.packages {
        let transport = clean_required_field(&package.transport.kind)?;
        transports.insert(transport.clone());
        let package_platforms = clean_fields(package.platforms)?;
        platform_constraints.extend(package_platforms.iter().cloned());
        packages.push(CatalogPackage {
            registry_type: clean_required_field(&package.registry_type)?,
            registry_url: package.registry_url.as_deref().map(clean_optional_field).transpose()?,
            identifier: clean_required_field(&package.identifier)?,
            version: package.version.as_deref().map(clean_optional_field).transpose()?,
            sha256: package.sha256.as_deref().map(clean_optional_field).transpose()?,
            transports: vec![transport],
            platform_constraints: package_platforms,
        });
    }
    platform_constraints.sort();
    platform_constraints.dedup();
    let status = envelope
        .meta
        .pointer("/io.modelcontextprotocol.registry~1official/status")
        .and_then(serde_json::Value::as_str)
        .map(clean_optional_field)
        .transpose()?;
    Ok(CatalogEntry {
        source: source.name.clone(),
        source_url: source.url.clone(),
        name,
        title: server.title.as_deref().map(clean_optional_field).transpose()?,
        description: clean_required_field(&server.description)?,
        claimed_publisher,
        version: clean_required_field(&server.version)?,
        status,
        transports: transports.into_iter().collect(),
        packages,
        platform_constraints,
        curation_claim: source.curation_claim.clone(),
    })
}

fn sources_from_file(file: CatalogFile) -> Result<Vec<CatalogSource>, String> {
    let official = file.catalogs.get("official");
    if let Some(source) = official
        && source.url.is_some()
    {
        return Err("the built-in `official` catalog URL cannot be replaced".to_string());
    }
    let mut sources = vec![CatalogSource {
        name: "official".to_string(),
        url: OFFICIAL_CATALOG_URL.to_string(),
        enabled: official.and_then(|source| source.enabled).unwrap_or(true),
        built_in: true,
        curation_claim: "preview; uncurated".to_string(),
    }];
    for (name, source) in file.catalogs {
        if name == "official" {
            continue;
        }
        validate_source_name(&name)?;
        let url = source
            .url
            .ok_or_else(|| format!("catalog source `{name}` is missing `url`"))?;
        validate_catalog_url(&url)?;
        sources.push(CatalogSource {
            name,
            url: url.trim_end_matches('/').to_string(),
            enabled: source.enabled.unwrap_or(true),
            built_in: false,
            curation_claim: source
                .curation
                .as_deref()
                .map(clean_required_field)
                .transpose()?
                .unwrap_or_else(|| "not stated by catalog configuration".to_string()),
        });
    }
    Ok(sources)
}

fn server_url(source: &CatalogSource) -> Result<Url, String> {
    let mut url = Url::parse(&source.url).map_err(|error| format!("invalid catalog URL `{}`: {error}", source.url))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "catalog URL cannot accept API paths")?;
        segments.pop_if_empty();
        segments.push("v0.1");
        segments.push("servers");
    }
    Ok(url)
}

fn catalog_config_path() -> Result<PathBuf, String> {
    utils::home_dir()
        .map(|home| home.join(".thndrs").join("mcp-catalogs.toml"))
        .ok_or_else(|| "HOME is not available for global catalog configuration".to_string())
}

fn cache_path(source: &CatalogSource) -> Result<PathBuf, String> {
    let home = utils::home_dir().ok_or_else(|| "HOME is not available for catalog cache".to_string())?;
    Ok(home
        .join(".thndrs")
        .join("mcp-catalog-cache")
        .join(format!("{}.json", source.name)))
}

fn read_catalog_file(path: &Path) -> Result<CatalogFile, String> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| format!("failed to parse catalog configuration `{}`: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(CatalogFile::default()),
        Err(error) => Err(format!(
            "failed to read catalog configuration `{}`: {error}",
            path.display()
        )),
    }
}

fn write_catalog_file(path: &Path, file: &CatalogFile) -> Result<(), String> {
    let content =
        toml::to_string_pretty(file).map_err(|error| format!("failed to encode catalog configuration: {error}"))?;
    config::write_toml_file(path, "MCP catalog configuration", &content)
        .map_err(|error| format!("failed to write catalog configuration `{}`: {error}", path.display()))
}

fn read_cache(source: &CatalogSource) -> Result<CatalogCache, String> {
    let path = cache_path(source)?;
    let text = fs::read_to_string(&path).map_err(|error| format!("could not read cached metadata: {error}"))?;
    if text.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("cached metadata exceeds the response limit".to_string());
    }
    let cache: CatalogCache =
        serde_json::from_str(&text).map_err(|error| format!("cached metadata is invalid: {error}"))?;
    if cache.entries.len() > MAX_CACHED_ENTRIES {
        return Err("cached metadata has too many entries".to_string());
    }
    Ok(cache)
}

fn update_cache(source: &CatalogSource, entries: &[CatalogEntry]) -> Result<(), String> {
    let path = cache_path(source)?;
    let mut merged = read_cache(source).map(|cache| cache.entries).unwrap_or_default();
    for entry in entries {
        merged.retain(|cached| cached.name != entry.name || cached.version != entry.version);
        merged.push(entry.clone());
    }
    merged.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
    });
    if merged.len() > MAX_CACHED_ENTRIES {
        let excess = merged.len() - MAX_CACHED_ENTRIES;
        merged.drain(..excess);
    }
    let cache = CatalogCache { retrieved_at: datetime::now_iso8601(), entries: merged };
    let content = serde_json::to_string(&cache).map_err(|error| format!("could not encode catalog cache: {error}"))?;
    if content.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("catalog cache would exceed the response limit".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("could not create catalog cache directory: {error}"))?;
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, content).map_err(|error| format!("could not write catalog cache: {error}"))?;
    fs::rename(&temp, &path).map_err(|error| format!("could not replace catalog cache: {error}"))
}

fn validate_source_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err("catalog source name must match [A-Za-z0-9_-]+".to_string());
    }
    Ok(())
}

fn validate_catalog_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|error| format!("invalid catalog URL: {error}"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("catalog URL must be an HTTPS base URL without a query or fragment".to_string());
    }
    Ok(())
}

fn clean_fields(fields: Vec<String>) -> Result<Vec<String>, String> {
    fields.into_iter().map(|field| clean_required_field(&field)).collect()
}

fn clean_required_field(value: &str) -> Result<String, String> {
    let value = clean_optional_field(value)?;
    if value.is_empty() {
        return Err("catalog metadata contains an empty required field".to_string());
    }
    Ok(value)
}

fn clean_optional_field(value: &str) -> Result<String, String> {
    if value.chars().any(char::is_control) {
        return Err("catalog metadata contains control characters".to_string());
    }
    let value = value.trim();
    if value.chars().count() > MAX_FIELD_CHARS {
        return Err(format!("catalog metadata field exceeds {MAX_FIELD_CHARS} characters"));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct FixtureFetcher {
        responses: BTreeMap<String, Result<String, String>>,
        requested: RefCell<Vec<String>>,
    }

    impl CatalogFetcher for FixtureFetcher {
        fn get(&self, url: &Url) -> Result<String, String> {
            self.requested.borrow_mut().push(url.to_string());
            self.responses
                .get(url.as_str())
                .cloned()
                .unwrap_or_else(|| Err("missing fixture".to_string()))
        }
    }

    fn fixture() -> String {
        r#"{
          "servers": [{
            "server": {
              "name": "io.example/weather",
              "title": "Weather",
              "description": "Weather forecasts",
              "version": "1.2.3",
              "platforms": ["linux", "macos"],
              "remotes": [{"type": "streamable-http", "url": "https://weather.example/mcp"}],
              "packages": [{"registryType": "npm", "registryBaseUrl": "https://registry.npmjs.org", "identifier": "@example/weather", "version": "1.2.3", "fileSha256": "abcd", "transport": {"type": "stdio"}}]
            },
            "_meta": {"io.modelcontextprotocol.registry/official": {"status": "active"}}
          }],
          "metadata": {"nextCursor": "next page"}
        }"#.to_string()
    }

    fn source(name: &str) -> CatalogSource {
        CatalogSource {
            name: name.to_string(),
            url: "https://catalog.example".to_string(),
            enabled: true,
            built_in: false,
            curation_claim: "community review".to_string(),
        }
    }

    #[test]
    fn parses_search_metadata_without_executing_any_recipe() {
        let source = source("fixture");
        let entry = catalog_entry(
            &source,
            serde_json::from_str::<RawList>(&fixture()).unwrap().servers.remove(0),
        )
        .unwrap();

        assert_eq!(entry.claimed_publisher, "io.example");
        assert_eq!(entry.transports, vec!["stdio", "streamable-http"]);
        assert_eq!(entry.packages[0].identifier, "@example/weather");
        assert_eq!(entry.platform_constraints, vec!["linux", "macos"]);
        assert_eq!(entry.status.as_deref(), Some("active"));
    }

    #[test]
    fn search_uses_api_pagination_and_bounds_result_count() {
        let source = source("fixture");
        let url = "https://catalog.example/v0.1/servers?search=weather&limit=2&cursor=opaque";
        let fetcher = FixtureFetcher {
            responses: BTreeMap::from([(url.to_string(), Ok(fixture()))]),
            requested: RefCell::new(Vec::new()),
        };
        let result = search_source(&source, "weather", 2, Some("opaque"), &fetcher).unwrap();

        assert_eq!(result.next_cursor.as_deref(), Some("next page"));
        assert_eq!(result.entries.len(), 1);
        assert_eq!(fetcher.requested.borrow().as_slice(), [url]);
    }

    #[test]
    fn unavailable_catalog_does_not_hide_another_catalogs_results() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let _guard = crate::test_env::lock();
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &home) };
        add_source("community", "https://catalog.example", None).unwrap();
        let fetcher = FixtureFetcher {
            responses: BTreeMap::from([(
                "https://catalog.example/v0.1/servers?search=weather&limit=20".to_string(),
                Ok(fixture()),
            )]),
            requested: RefCell::new(Vec::new()),
        };

        let result = search_with_fetcher("weather", 20, None, false, &fetcher).unwrap();
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].source.name, "community");
        assert_eq!(result.results[0].entries[0].name, "io.example/weather");
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].contains("official"));

        unsafe {
            if let Some(home) = old_home {
                std::env::set_var("HOME", home)
            } else {
                std::env::remove_var("HOME")
            }
        }
    }

    #[test]
    fn response_with_too_many_entries_is_rejected() {
        let source = source("fixture");
        let server = r#"{"server":{"name":"io.example/weather","description":"Weather","version":"1.0.0"}}"#;
        let document = format!(r#"{{"servers":[{}]}}"#, vec![server; MAX_PAGE_SIZE + 1].join(","));
        let url = "https://catalog.example/v0.1/servers?search=weather&limit=20";
        let fetcher = FixtureFetcher {
            responses: BTreeMap::from([(url.to_string(), Ok(document))]),
            requested: RefCell::new(Vec::new()),
        };

        let error = search_source(&source, "weather", 20, None, &fetcher).expect_err("entry limit must apply");
        assert!(error.contains("more than"));
    }

    #[test]
    fn malformed_entry_is_rejected() {
        let source = source("fixture");
        let invalid = r#"{"servers":[{"server":{"name":"bad","description":"x","version":"1"}}]}"#;
        let err = serde_json::from_str::<RawList>(invalid)
            .map_err(|error| error.to_string())
            .and_then(|list| catalog_entry(&source, list.servers.into_iter().next().unwrap()))
            .expect_err("invalid name must fail");

        assert!(err.contains("invalid server name"));
    }

    #[test]
    fn custom_sources_are_global_and_official_url_cannot_change() {
        let err = sources_from_file(CatalogFile {
            catalogs: BTreeMap::from([(
                "official".to_string(),
                CatalogFileSource { url: Some("https://other.example".to_string()), enabled: None, curation: None },
            )]),
        })
        .expect_err("official endpoint is immutable");
        assert!(err.contains("cannot be replaced"));

        let sources = sources_from_file(CatalogFile {
            catalogs: BTreeMap::from([(
                "community".to_string(),
                CatalogFileSource {
                    url: Some("https://catalog.example".to_string()),
                    enabled: Some(false),
                    curation: Some("community review".to_string()),
                },
            )]),
        })
        .unwrap();
        assert_eq!(sources[0].curation_claim, "preview; uncurated");
        assert!(!sources[1].enabled);
    }

    #[test]
    fn cache_search_and_detail_are_offline() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let _guard = crate::test_env::lock();
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &home) };
        let source = source("fixture");
        let entry = catalog_entry(
            &source,
            serde_json::from_str::<RawList>(&fixture()).unwrap().servers.remove(0),
        )
        .unwrap();
        update_cache(&source, &[entry]).unwrap();

        let search = cached_search(&source, "weather").unwrap();
        assert!(search.from_cache);
        assert_eq!(search.entries.len(), 1);
        assert!(cached_detail(&source, "io.example/weather").unwrap().from_cache);

        unsafe {
            if let Some(home) = old_home {
                std::env::set_var("HOME", home)
            } else {
                std::env::remove_var("HOME")
            }
        }
    }

    #[test]
    fn catalog_urls_require_https_base_urls() {
        assert!(validate_catalog_url("https://catalog.example").is_ok());
        assert!(validate_catalog_url("http://catalog.example").is_err());
        assert!(validate_catalog_url("https://catalog.example?token=no").is_err());
    }
}
