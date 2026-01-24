//! Parser for SKILL.md files with YAML frontmatter.
//!
//! SKILL.md format:
//! ```markdown
//! ---
//! name: brave-search
//! description: Web search via Brave Search API
//! version: 1.0.0
//! risk_level: safe
//! ---
//!
//! # Brave Search
//!
//! ## Setup
//! ...
//! ```

use crate::types::{Result, ScriptType, Skill, SkillMeta, SkillRisk, SkillScript};
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Parse a SKILL.md file and extract metadata and content.
pub fn parse_skill(skill_dir: &Path) -> Result<Skill> {
    let skill_md = skill_dir.join("SKILL.md");

    if !skill_md.exists() {
        return Err(crate::types::SkillError::NotFound(skill_md.display().to_string()));
    }

    let content = fs::read_to_string(&skill_md)?;
    let (meta, body) = extract_frontmatter(&content)?;
    let scripts = discover_scripts(skill_dir)?;

    Ok(Skill { meta: SkillMeta { path: skill_dir.to_path_buf(), ..meta }, content: body, scripts })
}

/// Extract YAML frontmatter and body from markdown content.
fn extract_frontmatter(content: &str) -> Result<(SkillMeta, String)> {
    if !content.starts_with("---") {
        return Err(crate::types::SkillError::InvalidFrontmatter(
            "SKILL.md must start with ---".to_string(),
        ));
    }

    let rest = &content[3..];
    let frontmatter_end = rest
        .find("---")
        .ok_or_else(|| crate::types::SkillError::InvalidFrontmatter("Closing --- not found".to_string()))?;

    let frontmatter_str = &rest[..frontmatter_end];
    let body = &rest[frontmatter_end + 3..];

    let frontmatter: Frontmatter = serde_yml::from_str(frontmatter_str)
        .map_err(|e| crate::types::SkillError::InvalidFrontmatter(format!("YAML parse error: {e}")))?;

    if frontmatter.name.is_empty() {
        return Err(crate::types::SkillError::InvalidFrontmatter(
            "name is required".to_string(),
        ));
    }

    if frontmatter.description.is_empty() {
        return Err(crate::types::SkillError::InvalidFrontmatter(
            "description is required".to_string(),
        ));
    }

    if !frontmatter
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(crate::types::SkillError::InvalidFrontmatter(
            "name must be lowercase with hyphens/underscores only".to_string(),
        ));
    }

    if frontmatter.description.len() > 1024 {
        return Err(crate::types::SkillError::InvalidFrontmatter(
            "description must be <= 1024 characters".to_string(),
        ));
    }

    let meta = SkillMeta {
        name: frontmatter.name,
        description: frontmatter.description,
        version: frontmatter.version.unwrap_or_else(|| "1.0.0".to_string()),
        author: frontmatter.author.unwrap_or_default(),
        tags: frontmatter.tags.unwrap_or_default(),
        requires: frontmatter.requires.unwrap_or_default(),
        path: PathBuf::new(), // Will be set by caller
        risk_level: frontmatter
            .risk_level
            .and_then(|s| parse_risk_level(&s).ok())
            .unwrap_or_default(),
    };

    Ok((meta, body.trim().to_string()))
}

/// Parse risk level string into SkillRisk enum.
fn parse_risk_level(s: &str) -> Result<SkillRisk> {
    match s.to_lowercase().as_str() {
        "safe" => Ok(SkillRisk::Safe),
        "moderate" => Ok(SkillRisk::Moderate),
        "risky" => Ok(SkillRisk::Risky),
        _ => Err(crate::types::SkillError::InvalidFrontmatter(format!(
            "invalid risk_level: {s}"
        ))),
    }
}

/// Discover executable scripts in the skill directory.
fn discover_scripts(skill_dir: &Path) -> Result<Vec<SkillScript>> {
    let mut scripts = Vec::new();

    let entries = fs::read_dir(skill_dir).map_err(|e| {
        crate::types::SkillError::Io(io::Error::new(
            e.kind(),
            format!("Failed to read skill directory {}: {}", skill_dir.display(), e),
        ))
    })?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() || path.file_name() == Some(std::ffi::OsStr::new("SKILL.md")) {
            continue;
        }

        if let Some(name) = path.file_name()
            && name.to_string_lossy().to_lowercase() == "readme.md"
        {
            continue;
        }

        let script_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext.to_lowercase().as_str() {
                "sh" => ScriptType::Bash,
                "js" => ScriptType::JavaScript,
                "py" => ScriptType::Python,
                "lua" => ScriptType::Lua,
                _ => ScriptType::Unknown,
            })
            .unwrap_or(ScriptType::Unknown);

        if let Some(name) = path.file_name() {
            scripts.push(SkillScript { name: name.to_string_lossy().to_string(), path, script_type });
        }
    }

    Ok(scripts)
}

/// YAML frontmatter structure.
#[derive(Debug, serde::Deserialize)]
struct Frontmatter {
    #[serde(default)]
    name: String,

    #[serde(default)]
    description: String,

    #[serde(default)]
    version: Option<String>,

    #[serde(default)]
    author: Option<String>,

    #[serde(default)]
    tags: Option<Vec<String>>,

    #[serde(default)]
    requires: Option<Vec<String>>,

    #[serde(default)]
    risk_level: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_frontmatter_valid() {
        let content = r#"---
name: test-skill
description: A test skill
version: 1.0.0
---

# Test Skill

This is the body.
"#;

        let (meta, body) = extract_frontmatter(content).unwrap();
        assert_eq!(meta.name, "test-skill");
        assert_eq!(meta.description, "A test skill");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(body, "# Test Skill\n\nThis is the body.");
    }

    #[test]
    fn test_extract_frontmatter_missing_name() {
        let content = r#"---
description: A test skill
---

# Test Skill
"#;

        let result = extract_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_frontmatter_missing_description() {
        let content = r#"---
name: test-skill
---

# Test Skill
"#;

        let result = extract_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_frontmatter_invalid_name() {
        let content = r#"---
name: TestSkill
description: A test skill
---

# Test Skill
"#;

        let result = extract_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_skill_from_directory() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("test-skill");
        fs::create_dir(&skill_dir).unwrap();

        let skill_md = skill_dir.join("SKILL.md");
        fs::write(
            &skill_md,
            r#"---
name: test-skill
description: A test skill
version: 1.0.0
---

# Test Skill
"#,
        )
        .unwrap();

        let script_path = skill_dir.join("run.sh");
        fs::write(&script_path, "#!/bin/bash\necho 'test'").unwrap();

        let skill = parse_skill(&skill_dir).unwrap();
        assert_eq!(skill.meta.name, "test-skill");
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(skill.scripts[0].name, "run.sh");
        assert_eq!(skill.scripts[0].script_type, ScriptType::Bash);
    }

    #[test]
    fn test_risk_level_parsing() {
        assert!(matches!(parse_risk_level("safe"), Ok(SkillRisk::Safe)));
        assert!(matches!(parse_risk_level("SAFE"), Ok(SkillRisk::Safe)));
        assert!(matches!(parse_risk_level("moderate"), Ok(SkillRisk::Moderate)));
        assert!(matches!(parse_risk_level("risky"), Ok(SkillRisk::Risky)));
        assert!(parse_risk_level("invalid").is_err());
    }
}
