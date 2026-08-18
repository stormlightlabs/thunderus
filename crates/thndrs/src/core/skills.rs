//! Agent Skills discovery and loading.
//!
//! Skills are filesystem packages. Startup discovery reads only `SKILL.md`
//! frontmatter, so the prompt can expose compact routing metadata without
//! loading full instructions. Full Markdown is read only when a skill is
//! explicitly opened from the UI or a later tool reads the file.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use markdown::{Constructs, ParseOptions, mdast::Node};
use serde::{Deserialize, Serialize};

use crate::tools;
use crate::utils;

enum SkillConstants {
    NameLen,
    DescriptionLen,
    DiscoveryDepth,
    ReferenceDepth,
    ReferenceFiles,
    ReferenceBytes,
    ReferenceFileBytes,
}

impl From<SkillConstants> for usize {
    fn from(val: SkillConstants) -> Self {
        match val {
            SkillConstants::NameLen => 64,
            SkillConstants::DescriptionLen => 1024,
            SkillConstants::DiscoveryDepth => 8,
            SkillConstants::ReferenceDepth => 3,
            SkillConstants::ReferenceFiles => 24,
            SkillConstants::ReferenceBytes => 64 * 1024,
            SkillConstants::ReferenceFileBytes => 16 * 1024,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum AllowedTools {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl AllowedTools {
    fn into_vec(self) -> Vec<String> {
        self.into()
    }
}

impl From<AllowedTools> for Vec<String> {
    fn from(val: AllowedTools) -> Self {
        match val {
            AllowedTools::None => Vec::new(),
            AllowedTools::One(tool) => vec![tool],
            AllowedTools::Many(tools) => tools,
        }
        .into_iter()
        .map(|tool| tool.trim().to_string())
        .filter(|tool| !tool.is_empty())
        .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SkillSource {
    User,
    Project,
    Configured,
}

impl SkillSource {
    pub fn label(self) -> &'static str {
        match self {
            SkillSource::User => "user",
            SkillSource::Project => "project",
            SkillSource::Configured => "configured",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillInventory {
    pub skills: Vec<SkillMetadata>,
    pub diagnostics: Vec<SkillDiagnostic>,
    /// Name collisions resolved by keeping the first skill found in discovery-root order.
    pub duplicates: Vec<SkillDuplicate>,
}

/// A duplicate skill name and the deterministic resolution chosen during discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillDuplicate {
    pub name: String,
    pub selected_path: PathBuf,
    pub ignored_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub root: PathBuf,
    pub content_hash: u64,
    pub byte_count: usize,
    pub source: SkillSource,
    pub allowed_tools: Vec<String>,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub references: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

impl SkillDiagnostic {
    fn new(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self { path: path.into(), message: message.into() }
    }

    pub fn summary(&self) -> String {
        format!("skill diagnostic  {}: {}", self.path.display(), self.message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillActivation {
    /// Skill name from frontmatter.
    pub name: String,
    /// Path to the activated `SKILL.md`.
    pub path: PathBuf,
    /// Hash of the raw `SKILL.md` file.
    pub content_hash: u64,
    /// Byte count of the raw `SKILL.md` file.
    pub byte_count: usize,
    /// Hash of the model-visible activation text after references are appended.
    pub rendered_content_hash: u64,
    /// Byte count of the model-visible activation text after references are appended.
    pub rendered_byte_count: usize,
    /// Metadata for references loaded into the rendered activation text.
    pub loaded_references: Vec<SkillReferenceMeta>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillReferenceMeta {
    pub path: PathBuf,
    pub content_hash: u64,
    pub byte_count: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSkill {
    pub activation: SkillActivation,
    pub diagnostics: Vec<SkillDiagnostic>,
    pub markdown: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedReferences {
    pub files: Vec<(SkillReferenceMeta, String)>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Clone, Debug)]
struct SkillRoot {
    path: PathBuf,
    source: SkillSource,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: AllowedTools,
    license: Option<String>,
    compatibility: Option<String>,
    metadata: Option<serde_json::Value>,
    references: Option<serde_yaml_ng::Value>,
}

pub fn default_skill_dirs(workspace_root: &Path, configured: &[PathBuf]) -> Vec<(PathBuf, SkillSource)> {
    let mut roots = Vec::new();
    if let Some(home) = utils::home_dir() {
        roots.push((home.join(".thndrs").join("skills"), SkillSource::User));
        roots.push((home.join(".agents").join("skills"), SkillSource::User));
        roots.push((home.join(".claude").join("skills"), SkillSource::User));
        roots.push((home.join(".codex").join("skills"), SkillSource::User));
        roots.push((home.join(".pi").join("skills"), SkillSource::User));
        roots.push((home.join(".pi").join("agent").join("skills"), SkillSource::User));
    }
    roots.push((workspace_root.join(".thndrs").join("skills"), SkillSource::Project));
    roots.push((workspace_root.join(".agents").join("skills"), SkillSource::Project));
    roots.push((workspace_root.join(".claude").join("skills"), SkillSource::Project));
    roots.push((workspace_root.join(".codex").join("skills"), SkillSource::Project));
    roots.push((workspace_root.join(".pi").join("skills"), SkillSource::Project));
    roots.push((
        workspace_root.join(".pi").join("agent").join("skills"),
        SkillSource::Project,
    ));

    roots.extend(configured.iter().map(|path| {
        let resolved = if path.is_absolute() { path.clone() } else { workspace_root.join(path) };
        (resolved, SkillSource::Configured)
    }));
    roots
}

pub fn discover(workspace_root: &Path, configured_dirs: &[PathBuf]) -> SkillInventory {
    let roots = default_skill_dirs(workspace_root, configured_dirs);
    let roots = roots
        .into_iter()
        .map(|(path, source)| SkillRoot { path, source })
        .collect::<Vec<_>>();
    discover_from_roots(roots)
}

pub fn load_skill(skill: &SkillMetadata) -> Result<LoadedSkill, SkillDiagnostic> {
    let mut markdown = fs::read_to_string(&skill.path)
        .map_err(|err| SkillDiagnostic::new(&skill.path, format!("failed to read skill: {err}")))?;
    let references = load_references(skill, &skill.references);
    append_references_to_markdown(&mut markdown, &references.files);
    let activation = SkillActivation {
        name: skill.name.clone(),
        path: skill.path.clone(),
        content_hash: skill.content_hash,
        byte_count: skill.byte_count,
        rendered_content_hash: tools::hash_content(&markdown),
        rendered_byte_count: markdown.len(),
        loaded_references: references.files.into_iter().map(|(meta, _)| meta).collect(),
    };
    Ok(LoadedSkill { activation, diagnostics: references.diagnostics, markdown })
}

pub fn load_references(skill: &SkillMetadata, relative_paths: &[PathBuf]) -> LoadedReferences {
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen = HashSet::new();
    let mut total_bytes = 0usize;

    for relative_path in relative_paths {
        load_reference_path(
            &skill.root,
            relative_path,
            0,
            &mut seen,
            &mut total_bytes,
            &mut files,
            &mut diagnostics,
        );
        if files.len() >= SkillConstants::ReferenceFiles.into() || total_bytes >= SkillConstants::ReferenceBytes.into()
        {
            diagnostics.push(SkillDiagnostic::new(&skill.root, "reference traversal budget reached"));
            break;
        }
    }

    LoadedReferences { files, diagnostics }
}

pub fn format_available_skills(skills: &[SkillMetadata]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "The following skills provide specialized instructions for matching tasks. Read the full SKILL.md only after the task matches its description.\n<available_skills>\n",
    );
    for skill in skills {
        out.push_str("  <skill>\n");
        out.push_str(&format!("    <name>{}</name>\n", utils::escape_xml(&skill.name)));
        out.push_str(&format!(
            "    <description>{}</description>\n",
            utils::escape_xml(&skill.description)
        ));
        out.push_str(&format!(
            "    <location>{}</location>\n",
            utils::escape_xml(&skill.path.display().to_string())
        ));
        out.push_str(&format!("    <source>{}</source>\n", skill.source.label()));
        if !skill.allowed_tools.is_empty() {
            out.push_str(&format!(
                "    <allowed_tools>{}</allowed_tools>\n",
                utils::escape_xml(&skill.allowed_tools.join(", "))
            ));
        }
        out.push_str("  </skill>\n");
    }
    out.push_str("</available_skills>");
    out
}

fn load_reference_path(
    root: &Path, relative_path: &Path, depth: usize, seen: &mut HashSet<PathBuf>, total_bytes: &mut usize,
    files: &mut Vec<(SkillReferenceMeta, String)>, diagnostics: &mut Vec<SkillDiagnostic>,
) {
    if depth > SkillConstants::ReferenceDepth.into() {
        diagnostics.push(SkillDiagnostic::new(
            root.join(relative_path),
            "maximum reference depth reached",
        ));
        return;
    }
    if files.len() >= SkillConstants::ReferenceFiles.into() || *total_bytes >= SkillConstants::ReferenceBytes.into() {
        return;
    }
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        diagnostics.push(SkillDiagnostic::new(
            root.join(relative_path),
            "reference path must stay inside skill root",
        ));
        return;
    }
    let path = root.join(relative_path);
    let Ok(canonical_root) = root.canonicalize() else {
        diagnostics.push(SkillDiagnostic::new(root, "failed to canonicalize skill root"));
        return;
    };
    let Ok(canonical_path) = path.canonicalize() else {
        diagnostics.push(SkillDiagnostic::new(path, "reference path does not exist"));
        return;
    };
    if !canonical_path.starts_with(&canonical_root) {
        diagnostics.push(SkillDiagnostic::new(path, "reference path must stay inside skill root"));
        return;
    }
    if !seen.insert(canonical_path.clone()) {
        return;
    }

    if canonical_path.is_dir() {
        let Ok(entries) = fs::read_dir(&canonical_path) else {
            diagnostics.push(SkillDiagnostic::new(
                canonical_path,
                "failed to read reference directory",
            ));
            return;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let child_rel = match entry.path().canonicalize().and_then(|path| {
                path.strip_prefix(&canonical_root)
                    .map(Path::to_path_buf)
                    .map_err(std::io::Error::other)
            }) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };
            load_reference_path(root, &child_rel, depth + 1, seen, total_bytes, files, diagnostics);
        }
        return;
    }

    let Ok(bytes) = fs::read(&canonical_path) else {
        diagnostics.push(SkillDiagnostic::new(canonical_path, "failed to read reference file"));
        return;
    };
    let truncated = bytes.len() > SkillConstants::ReferenceFileBytes.into();
    let capped_len = bytes.len().min(SkillConstants::ReferenceFileBytes.into());
    let content = String::from_utf8_lossy(&bytes[..capped_len]).to_string();
    *total_bytes = total_bytes.saturating_add(capped_len);
    files.push((
        SkillReferenceMeta {
            path: canonical_path,
            content_hash: tools::hash_content(&content),
            byte_count: bytes.len(),
            truncated,
        },
        content,
    ));
}

fn append_references_to_markdown(markdown: &mut String, references: &[(SkillReferenceMeta, String)]) {
    if references.is_empty() {
        return;
    }

    markdown.push_str("\n\n## Loaded References\n");
    for (meta, content) in references {
        markdown.push_str("\n### ");
        markdown.push_str(&meta.path.display().to_string());
        if meta.truncated {
            markdown.push_str(" (truncated)");
        }
        markdown.push_str("\n\n```text\n");
        markdown.push_str(content);
        if !content.ends_with('\n') {
            markdown.push('\n');
        }
        markdown.push_str("```\n");
    }
}

fn parse_frontmatter(raw: &str) -> Result<Frontmatter, String> {
    serde_yaml_ng::from_str(
        &match markdown::to_mdast(raw, &frontmatter_parse_options())
            .map_err(|err| format!("failed to parse Markdown: {err}"))?
        {
            Node::Root(root) => root.children.into_iter().find_map(|node| match node {
                Node::Yaml(yaml) => Some(yaml.value),
                _ => None,
            }),
            _ => None,
        }
        .ok_or_else(|| String::from("missing YAML frontmatter"))?,
    )
    .map_err(|err| format!("invalid YAML frontmatter: {err}"))
}

fn frontmatter_parse_options() -> ParseOptions {
    ParseOptions { constructs: Constructs { frontmatter: true, ..Constructs::default() }, ..ParseOptions::default() }
}

fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist" | "build")
}

fn load_metadata(path: &Path, root: &SkillRoot) -> Result<SkillMetadata, SkillDiagnostic> {
    let raw = fs::read_to_string(path)
        .map_err(|err| SkillDiagnostic::new(path, format!("failed to read SKILL.md: {err}")))?;
    let byte_count = raw.len();
    let content_hash = tools::hash_content(&raw);
    let frontmatter = parse_frontmatter(&raw).map_err(|message| SkillDiagnostic::new(path, message))?;
    let parent_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let name = frontmatter
        .name
        .clone()
        .ok_or_else(|| SkillDiagnostic::new(path, "frontmatter name is required"))?;
    let description = frontmatter
        .description
        .clone()
        .ok_or_else(|| SkillDiagnostic::new(path, "frontmatter description is required"))?;

    validate_name(path, &name, parent_name)?;
    if description.trim().is_empty() {
        return Err(SkillDiagnostic::new(path, "frontmatter description is required"));
    }
    if description.len() > SkillConstants::DescriptionLen.into() {
        let l: usize = SkillConstants::DescriptionLen.into();
        return Err(SkillDiagnostic::new(
            path,
            format!("description exceeds {l} characters"),
        ));
    }
    let references = parse_reference_paths(path, frontmatter.references)?;

    Ok(SkillMetadata {
        name,
        description,
        path: path.to_path_buf(),
        root: path.parent().unwrap_or(path).to_path_buf(),
        content_hash,
        byte_count,
        source: root.source,
        allowed_tools: frontmatter.allowed_tools.into_vec(),
        license: frontmatter.license,
        compatibility: frontmatter.compatibility,
        metadata: frontmatter.metadata,
        references,
    })
}

fn parse_reference_paths(
    skill_path: &Path, references: Option<serde_yaml_ng::Value>,
) -> Result<Vec<PathBuf>, SkillDiagnostic> {
    let Some(references) = references else {
        return Ok(Vec::new());
    };
    let serde_yaml_ng::Value::Sequence(values) = references else {
        return Err(SkillDiagnostic::new(
            skill_path,
            "frontmatter references must be a list of relative paths",
        ));
    };
    if values.is_empty() {
        return Err(SkillDiagnostic::new(
            skill_path,
            "frontmatter references must not be empty",
        ));
    }

    let mut paths = Vec::with_capacity(values.len());
    for value in values {
        let serde_yaml_ng::Value::String(raw) = value else {
            return Err(SkillDiagnostic::new(
                skill_path,
                "frontmatter references entries must be strings",
            ));
        };
        let path = normalize_reference_path(&raw).map_err(|message| {
            SkillDiagnostic::new(skill_path, format!("invalid frontmatter reference {raw:?}: {message}"))
        })?;
        paths.push(path);
    }
    Ok(paths)
}

fn normalize_reference_path(raw: &str) -> Result<PathBuf, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("path must not be empty");
    }

    let path = Path::new(raw);
    if path.is_absolute() {
        return Err("path must be relative");
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => return Err("path must not contain parent traversal"),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err("path must be relative");
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("path must not be empty");
    }
    Ok(normalized)
}

fn validate_name(path: &Path, name: &str, parent_name: &str) -> Result<(), SkillDiagnostic> {
    if name != parent_name {
        return Err(SkillDiagnostic::new(
            path,
            format!("name {name:?} must match parent directory {parent_name:?}"),
        ));
    }
    if name.len() > SkillConstants::NameLen.into() {
        let l: usize = SkillConstants::NameLen.into();
        return Err(SkillDiagnostic::new(path, format!("name exceeds {l} characters")));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err(SkillDiagnostic::new(
            path,
            "name must contain only lowercase letters, numbers, and hyphens",
        ));
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return Err(SkillDiagnostic::new(
            path,
            "name must not start or end with a hyphen or contain consecutive hyphens",
        ));
    }
    Ok(())
}

fn discover_from_roots(roots: Vec<SkillRoot>) -> SkillInventory {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();

    for root in roots {
        if !root.path.is_dir() {
            continue;
        }
        discover_dir(&root.path, &root, 0, &mut skills, &mut diagnostics);
    }

    let mut selected_skills: Vec<SkillMetadata> = Vec::new();
    let mut selected_by_name: HashMap<String, usize> = HashMap::new();
    let mut duplicates: BTreeMap<String, SkillDuplicate> = BTreeMap::new();
    for skill in skills {
        if let Some(&selected_index) = selected_by_name.get(&skill.name) {
            let duplicate = duplicates.entry(skill.name.clone()).or_insert_with(|| SkillDuplicate {
                name: skill.name.clone(),
                selected_path: selected_skills[selected_index].path.clone(),
                ignored_paths: Vec::new(),
            });
            duplicate.ignored_paths.push(skill.path);
            continue;
        }
        selected_by_name.insert(skill.name.clone(), selected_skills.len());
        selected_skills.push(skill);
    }

    SkillInventory {
        skills: selected_skills,
        diagnostics: collapse_redundant_diagnostics(diagnostics),
        duplicates: duplicates.into_values().collect(),
    }
}

fn collapse_redundant_diagnostics(diagnostics: Vec<SkillDiagnostic>) -> Vec<SkillDiagnostic> {
    let mut collapsed = Vec::new();
    let mut by_skill_install = BTreeMap::<(PathBuf, String), (SkillDiagnostic, usize)>::new();

    for diagnostic in diagnostics {
        let Some(suffix) = skill_install_suffix(&diagnostic.path) else {
            collapsed.push(diagnostic);
            continue;
        };
        let key = (suffix, diagnostic.message.clone());
        by_skill_install
            .entry(key)
            .and_modify(|(_, count)| *count += 1)
            .or_insert((diagnostic, 1));
    }

    for (_, (mut diagnostic, count)) in by_skill_install {
        if count > 1 {
            diagnostic.message = format!(
                "{} (also found in {} other skill root(s))",
                diagnostic.message,
                count - 1
            );
        }
        collapsed.push(diagnostic);
    }
    collapsed
}

fn skill_install_suffix(path: &Path) -> Option<PathBuf> {
    let mut suffix = PathBuf::new();
    let mut found = false;
    for component in path.components() {
        if found {
            suffix.push(component.as_os_str());
            continue;
        }
        if component.as_os_str() == "skills" {
            found = true;
        }
    }
    found.then_some(suffix).filter(|suffix| !suffix.as_os_str().is_empty())
}

fn discover_dir(
    dir: &Path, root: &SkillRoot, depth: usize, skills: &mut Vec<SkillMetadata>, diagnostics: &mut Vec<SkillDiagnostic>,
) {
    if depth > SkillConstants::DiscoveryDepth.into() {
        diagnostics.push(SkillDiagnostic::new(dir, "maximum skill discovery depth reached"));
        return;
    }

    let skill_file = dir.join("SKILL.md");
    if skill_file.is_file() {
        match load_metadata(&skill_file, root) {
            Ok(skill) => skills.push(skill),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        diagnostics.push(SkillDiagnostic::new(dir, "failed to read directory"));
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_dir(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            discover_dir(&path, root, depth + 1, skills, diagnostics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        let mut file = fs::File::create(path).expect("create file");
        file.write_all(content.as_bytes()).expect("write file");
    }

    fn discover_only(path: &Path) -> SkillInventory {
        discover_from_roots(vec![SkillRoot {
            path: path.to_path_buf(),
            source: SkillSource::Project,
        }])
    }

    #[test]
    fn default_project_skill_dirs_cover_standard_and_compatibility_roots() {
        let workspace = Path::new("/repo");
        let project_dirs = default_skill_dirs(workspace, &[PathBuf::from("vendor/skills")])
            .into_iter()
            .filter_map(|(path, source)| (source == SkillSource::Project).then_some(path))
            .collect::<Vec<_>>();

        assert!(project_dirs.contains(&PathBuf::from("/repo/.thndrs/skills")));
        assert!(project_dirs.contains(&PathBuf::from("/repo/.agents/skills")));
        assert!(project_dirs.contains(&PathBuf::from("/repo/.claude/skills")));
        assert!(project_dirs.contains(&PathBuf::from("/repo/.codex/skills")));
        assert!(project_dirs.contains(&PathBuf::from("/repo/.pi/skills")));
        assert!(project_dirs.contains(&PathBuf::from("/repo/.pi/agent/skills")));
        assert!(!project_dirs.contains(&PathBuf::from("/repo/.thdrs/skills")));
    }

    #[test]
    fn discover_valid_nested_skill_metadata() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(
            &dir.path().join(".thndrs/skills/example-skill/SKILL.md"),
            "---\nname: example-skill\ndescription: Helps with examples.\nallowed-tools: [read_file_range]\nlicense: MIT\ncompatibility: thndrs\nmetadata:\n  audience: tests\n  priority: 2\n---\n# Body\n",
        );

        let inventory = discover_only(&dir.path().join(".thndrs/skills"));
        assert_eq!(inventory.skills.len(), 1);
        assert_eq!(inventory.skills[0].name, "example-skill");
        assert_eq!(inventory.skills[0].allowed_tools, vec!["read_file_range"]);
        assert_eq!(inventory.skills[0].license.as_deref(), Some("MIT"));
        assert_eq!(inventory.skills[0].compatibility.as_deref(), Some("thndrs"));
        assert!(inventory.skills[0].references.is_empty());
        assert_eq!(
            inventory.skills[0].metadata,
            Some(serde_json::json!({ "audience": "tests", "priority": 2 }))
        );
        assert!(inventory.diagnostics.is_empty());
    }

    #[test]
    fn discover_recurses_into_agent_skills_collection_shape() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(
            &dir.path()
                .join("vendor/agent-skills/skills/react-best-practices/SKILL.md"),
            "---\nname: react-best-practices\ndescription: Review React code.\n---\n# React\n",
        );

        let inventory = discover_from_roots(vec![SkillRoot {
            path: dir.path().join("vendor/agent-skills"),
            source: SkillSource::Configured,
        }]);
        assert_eq!(inventory.skills.len(), 1);
        assert_eq!(inventory.skills[0].name, "react-best-practices");
        assert_eq!(inventory.skills[0].source, SkillSource::Configured);
    }

    #[test]
    fn duplicate_skill_installs_use_the_first_discovery_root_and_are_reported_separately() {
        let dir = tempfile::tempdir().expect("temp dir");
        for root in [".agents/skills", ".claude/skills", ".codex/skills"] {
            write(
                &dir.path().join(root).join("example-skill/SKILL.md"),
                "---\nname: example-skill\ndescription: Helps.\n---\n# Skill\n",
            );
        }

        let inventory = discover_from_roots(vec![
            SkillRoot { path: dir.path().join(".agents/skills"), source: SkillSource::User },
            SkillRoot { path: dir.path().join(".claude/skills"), source: SkillSource::User },
            SkillRoot { path: dir.path().join(".codex/skills"), source: SkillSource::User },
        ]);

        assert_eq!(inventory.skills.len(), 1);
        assert_eq!(
            inventory.skills[0].path,
            dir.path().join(".agents/skills/example-skill/SKILL.md")
        );
        assert!(inventory.diagnostics.is_empty());
        assert_eq!(
            inventory.duplicates,
            vec![SkillDuplicate {
                name: "example-skill".to_string(),
                selected_path: dir.path().join(".agents/skills/example-skill/SKILL.md"),
                ignored_paths: vec![
                    dir.path().join(".claude/skills/example-skill/SKILL.md"),
                    dir.path().join(".codex/skills/example-skill/SKILL.md"),
                ],
            }]
        );
    }

    #[test]
    fn repeated_malformed_skill_installs_are_collapsed() {
        let dir = tempfile::tempdir().expect("temp dir");
        for root in [".claude/skills", ".codex/skills"] {
            write(
                &dir.path().join(root).join("skill-deslop/SKILL.md"),
                "---\nname: deslop\ndescription: Helps.\n---\n# Skill\n",
            );
        }

        let inventory = discover_from_roots(vec![
            SkillRoot { path: dir.path().join(".claude/skills"), source: SkillSource::User },
            SkillRoot { path: dir.path().join(".codex/skills"), source: SkillSource::User },
        ]);

        assert!(inventory.skills.is_empty());
        assert_eq!(inventory.diagnostics.len(), 1);
        assert!(inventory.diagnostics[0].message.contains("must match parent directory"));
        assert!(
            inventory.diagnostics[0]
                .message
                .contains("also found in 1 other skill root")
        );
    }

    #[test]
    fn malformed_skill_is_ignored_with_diagnostic() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(
            &dir.path().join(".thndrs/skills/bad/SKILL.md"),
            "# Missing frontmatter\n",
        );

        let inventory = discover_only(&dir.path().join(".thndrs/skills"));
        assert!(inventory.skills.is_empty());
        assert_eq!(inventory.diagnostics.len(), 1);
        assert!(inventory.diagnostics[0].message.contains("frontmatter"));
    }

    #[test]
    fn load_skill_reads_full_markdown_only_on_demand() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(".thndrs/skills/example-skill/SKILL.md");
        write(
            &path,
            "---\nname: example-skill\ndescription: Helps.\n---\n# Full Body\n",
        );
        let skill = discover_only(&dir.path().join(".thndrs/skills")).skills.remove(0);

        let loaded = load_skill(&skill).expect("load skill");
        assert!(loaded.markdown.contains("# Full Body"));
        assert_eq!(loaded.activation.name, "example-skill");
        assert_eq!(loaded.activation.path, path);
    }

    #[test]
    fn discover_skill_with_default_references_and_loads_them_on_activation() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_path = dir.path().join(".thndrs/skills/cloudflare/SKILL.md");
        let skill_markdown = "---\nname: cloudflare\ndescription: Build on Cloudflare.\nreferences: [workers, ./pages, d1]\n---\n# Cloudflare\n";
        write(&skill_path, skill_markdown);
        write(
            &dir.path().join(".thndrs/skills/cloudflare/workers/intro.md"),
            "Workers docs",
        );
        write(
            &dir.path().join(".thndrs/skills/cloudflare/pages/intro.md"),
            "Pages docs",
        );
        write(&dir.path().join(".thndrs/skills/cloudflare/d1/intro.md"), "D1 docs");

        let inventory = discover_from_roots(vec![SkillRoot {
            path: dir.path().join(".thndrs/skills"),
            source: SkillSource::User,
        }]);
        assert!(inventory.diagnostics.is_empty());
        assert_eq!(inventory.skills.len(), 1);
        assert_eq!(
            inventory.skills[0].references,
            vec![PathBuf::from("workers"), PathBuf::from("pages"), PathBuf::from("d1")]
        );

        let loaded = load_skill(&inventory.skills[0]).expect("load skill");
        assert_eq!(loaded.activation.loaded_references.len(), 3);
        assert!(loaded.diagnostics.is_empty());
        assert_eq!(loaded.activation.content_hash, tools::hash_content(skill_markdown));
        assert_eq!(loaded.activation.byte_count, skill_markdown.len());
        assert_ne!(loaded.activation.rendered_content_hash, loaded.activation.content_hash);
        assert!(loaded.activation.rendered_byte_count > loaded.activation.byte_count);
        assert!(loaded.markdown.contains("# Cloudflare"));
        assert!(loaded.markdown.contains("Workers docs"));
        assert!(loaded.markdown.contains("Pages docs"));
        assert!(loaded.markdown.contains("D1 docs"));
    }

    #[test]
    fn discover_rejects_invalid_reference_paths() {
        let cases = [
            ("not-a-list", "references: workers", "must be a list"),
            ("empty-list", "references: []", "must not be empty"),
            ("empty-entry", "references: ['']", "path must not be empty"),
            ("absolute", "references: ['/tmp/workers']", "path must be relative"),
            ("parent", "references: ['../workers']", "parent traversal"),
            ("non-string", "references: [workers, 42]", "entries must be strings"),
        ];

        for (name, references, expected) in cases {
            let dir = tempfile::tempdir().expect("temp dir");
            write(
                &dir.path().join(format!(".thndrs/skills/{name}/SKILL.md")),
                &format!("---\nname: {name}\ndescription: Helps.\n{references}\n---\n# Skill\n"),
            );

            let inventory = discover_only(&dir.path().join(".thndrs/skills"));
            assert!(inventory.skills.is_empty(), "{name} should not load");
            assert_eq!(inventory.diagnostics.len(), 1, "{name} should have one diagnostic");
            assert!(
                inventory.diagnostics[0].message.contains(expected),
                "{name} diagnostic {:?} should contain {expected:?}",
                inventory.diagnostics[0].message
            );
        }
    }

    #[test]
    fn reference_loading_stays_inside_skill_root() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_path = dir.path().join(".thndrs/skills/example-skill/SKILL.md");
        write(
            &skill_path,
            "---\nname: example-skill\ndescription: Helps.\n---\n# Skill\n",
        );
        write(
            &dir.path().join(".thndrs/skills/example-skill/references/a.md"),
            "reference",
        );
        let skill = discover_only(&dir.path().join(".thndrs/skills")).skills.remove(0);

        let loaded = load_references(&skill, &[PathBuf::from("references"), PathBuf::from("../secret")]);
        assert_eq!(loaded.files.len(), 1);
        assert!(!loaded.diagnostics.is_empty());
    }

    #[test]
    fn load_skill_surfaces_missing_default_reference_diagnostics() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_path = dir.path().join(".thndrs/skills/example-skill/SKILL.md");
        write(
            &skill_path,
            "---\nname: example-skill\ndescription: Helps.\nreferences: [missing.md]\n---\n# Skill\n",
        );
        let skill = discover_only(&dir.path().join(".thndrs/skills")).skills.remove(0);

        let loaded = load_skill(&skill).expect("load skill");
        assert!(loaded.markdown.contains("# Skill"));
        assert!(loaded.activation.loaded_references.is_empty());
        assert!(loaded.diagnostics.iter().any(|diagnostic| {
            diagnostic.path.ends_with("missing.md") && diagnostic.message.contains("does not exist")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn load_skill_surfaces_symlink_escape_diagnostics() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("temp dir");
        let skill_root = dir.path().join(".thndrs/skills/example-skill");
        let skill_path = skill_root.join("SKILL.md");
        write(
            &skill_path,
            "---\nname: example-skill\ndescription: Helps.\nreferences: [outside.md]\n---\n# Skill\n",
        );
        write(&dir.path().join("outside.md"), "secret");
        symlink(dir.path().join("outside.md"), skill_root.join("outside.md")).expect("create symlink");
        let skill = discover_only(&dir.path().join(".thndrs/skills")).skills.remove(0);

        let loaded = load_skill(&skill).expect("load skill");
        assert!(loaded.activation.loaded_references.is_empty());
        assert!(loaded.diagnostics.iter().any(|diagnostic| {
            diagnostic.path.ends_with("outside.md") && diagnostic.message.contains("inside skill root")
        }));
    }

    #[test]
    fn load_skill_surfaces_reference_traversal_budget_diagnostics() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_root = dir.path().join(".thndrs/skills/example-skill");
        let skill_path = skill_root.join("SKILL.md");
        write(
            &skill_path,
            "---\nname: example-skill\ndescription: Helps.\nreferences: [references]\n---\n# Skill\n",
        );
        for idx in 0..30 {
            write(&skill_root.join(format!("references/{idx:02}.md")), "reference");
        }
        let skill = discover_only(&dir.path().join(".thndrs/skills")).skills.remove(0);

        let loaded = load_skill(&skill).expect("load skill");
        let max_reference_files: usize = SkillConstants::ReferenceFiles.into();
        assert_eq!(loaded.activation.loaded_references.len(), max_reference_files);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("reference traversal budget reached") })
        );
    }
}
