//! Agent Skills discovery and bounded loading.
//!
//! Skills are filesystem packages. Startup discovery reads only `SKILL.md`
//! frontmatter, so the prompt can expose compact routing metadata without
//! loading full instructions. Full Markdown is read only when a skill is
//! explicitly opened from the UI or a later tool reads the file.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use markdown::{Constructs, ParseOptions, mdast::Node};
use serde::{Deserialize, Serialize};

use crate::tools;
use crate::utils;

const SKILL_MD: &str = "SKILL.md";
const MAX_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_DISCOVERY_DEPTH: usize = 8;
const MAX_REFERENCE_DEPTH: usize = 3;
const MAX_REFERENCE_FILES: usize = 24;
const MAX_REFERENCE_BYTES: usize = 64 * 1024;
const MAX_REFERENCE_FILE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillInventory {
    pub skills: Vec<SkillMetadata>,
    pub diagnostics: Vec<SkillDiagnostic>,
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
    pub name: String,
    pub path: PathBuf,
    pub content_hash: u64,
    pub byte_count: usize,
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
        match self {
            Self::None => Vec::new(),
            Self::One(tool) => vec![tool],
            Self::Many(tools) => tools,
        }
        .into_iter()
        .map(|tool| tool.trim().to_string())
        .filter(|tool| !tool.is_empty())
        .collect()
    }
}

pub fn default_skill_dirs(workspace_root: &Path, configured: &[PathBuf]) -> Vec<(PathBuf, SkillSource)> {
    let mut roots = Vec::new();
    if let Some(home) = utils::home_dir() {
        roots.push((home.join(".thndrs").join("skills"), SkillSource::User));
        roots.push((home.join(".pi").join("agent").join("skills"), SkillSource::User));
    }
    roots.push((workspace_root.join(".thndrs").join("skills"), SkillSource::Project));
    roots.push((workspace_root.join(".thdrs").join("skills"), SkillSource::Project));
    roots.push((workspace_root.join(".pi").join("skills"), SkillSource::Project));

    roots.extend(configured.iter().map(|path| {
        let resolved = if path.is_absolute() { path.clone() } else { workspace_root.join(path) };
        (resolved, SkillSource::Configured)
    }));
    roots
}

pub fn discover(workspace_root: &Path, configured_dirs: &[PathBuf]) -> SkillInventory {
    let roots = default_skill_dirs(workspace_root, configured_dirs)
        .into_iter()
        .map(|(path, source)| SkillRoot { path, source })
        .collect::<Vec<_>>();
    discover_from_roots(roots)
}

fn discover_from_roots(roots: Vec<SkillRoot>) -> SkillInventory {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen_names: HashMap<String, PathBuf> = HashMap::new();

    for root in roots {
        if !root.path.is_dir() {
            continue;
        }
        discover_dir(&root.path, &root, 0, &mut skills, &mut diagnostics);
    }

    let mut deduped = Vec::new();
    for skill in skills {
        if let Some(existing) = seen_names.get(&skill.name) {
            diagnostics.push(SkillDiagnostic::new(
                &skill.path,
                format!(
                    "name {:?} already loaded from {}; ignoring duplicate",
                    skill.name,
                    existing.display()
                ),
            ));
            continue;
        }
        seen_names.insert(skill.name.clone(), skill.path.clone());
        deduped.push(skill);
    }

    SkillInventory { skills: deduped, diagnostics }
}

fn discover_dir(
    dir: &Path, root: &SkillRoot, depth: usize, skills: &mut Vec<SkillMetadata>, diagnostics: &mut Vec<SkillDiagnostic>,
) {
    if depth > MAX_DISCOVERY_DEPTH {
        diagnostics.push(SkillDiagnostic::new(dir, "maximum skill discovery depth reached"));
        return;
    }

    let skill_file = dir.join(SKILL_MD);
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
    if description.len() > MAX_DESCRIPTION_LEN {
        return Err(SkillDiagnostic::new(
            path,
            format!("description exceeds {MAX_DESCRIPTION_LEN} characters"),
        ));
    }

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
    })
}

fn validate_name(path: &Path, name: &str, parent_name: &str) -> Result<(), SkillDiagnostic> {
    if name != parent_name {
        return Err(SkillDiagnostic::new(
            path,
            format!("name {:?} must match parent directory {:?}", name, parent_name),
        ));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(SkillDiagnostic::new(
            path,
            format!("name exceeds {MAX_NAME_LEN} characters"),
        ));
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

fn parse_frontmatter(raw: &str) -> Result<Frontmatter, String> {
    let ast = markdown::to_mdast(raw, &frontmatter_parse_options())
        .map_err(|err| format!("failed to parse Markdown: {err}"))?;
    let yaml = match ast {
        Node::Root(root) => root.children.into_iter().find_map(|node| match node {
            Node::Yaml(yaml) => Some(yaml.value),
            _ => None,
        }),
        _ => None,
    }
    .ok_or_else(|| String::from("missing YAML frontmatter"))?;

    serde_yaml_ng::from_str(&yaml).map_err(|err| format!("invalid YAML frontmatter: {err}"))
}

fn frontmatter_parse_options() -> ParseOptions {
    ParseOptions { constructs: Constructs { frontmatter: true, ..Constructs::default() }, ..ParseOptions::default() }
}

pub fn load_skill(skill: &SkillMetadata) -> Result<LoadedSkill, SkillDiagnostic> {
    let markdown = fs::read_to_string(&skill.path)
        .map_err(|err| SkillDiagnostic::new(&skill.path, format!("failed to read skill: {err}")))?;
    let references = load_references(skill, &[]);
    let activation = SkillActivation {
        name: skill.name.clone(),
        path: skill.path.clone(),
        content_hash: tools::hash_content(&markdown),
        byte_count: markdown.len(),
        loaded_references: references.files.into_iter().map(|(meta, _)| meta).collect(),
    };
    Ok(LoadedSkill { activation, markdown })
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
        if files.len() >= MAX_REFERENCE_FILES || total_bytes >= MAX_REFERENCE_BYTES {
            diagnostics.push(SkillDiagnostic::new(&skill.root, "reference traversal budget reached"));
            break;
        }
    }

    LoadedReferences { files, diagnostics }
}

fn load_reference_path(
    root: &Path, relative_path: &Path, depth: usize, seen: &mut HashSet<PathBuf>, total_bytes: &mut usize,
    files: &mut Vec<(SkillReferenceMeta, String)>, diagnostics: &mut Vec<SkillDiagnostic>,
) {
    if depth > MAX_REFERENCE_DEPTH {
        diagnostics.push(SkillDiagnostic::new(
            root.join(relative_path),
            "maximum reference depth reached",
        ));
        return;
    }
    if files.len() >= MAX_REFERENCE_FILES || *total_bytes >= MAX_REFERENCE_BYTES {
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
    if !canonical_path.starts_with(&canonical_root) || !seen.insert(canonical_path.clone()) {
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
    let truncated = bytes.len() > MAX_REFERENCE_FILE_BYTES;
    let capped_len = bytes.len().min(MAX_REFERENCE_FILE_BYTES);
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
}
