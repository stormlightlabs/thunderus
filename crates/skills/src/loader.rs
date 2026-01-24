//! Skill discovery and loading from `.thunderus/skills/` directories.
//!
//! The loader supports:
//! - Global skills: `~/.thunderus/skills/`
//! - Project skills: `.thunderus/skills/`
//! - Lazy discovery (metadata only)
//! - On-demand full content loading

use crate::parser::parse_skill;
use crate::types::{Result, Skill, SkillMatch, SkillMeta, SkillsConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Skill loader with discovery and caching.
#[derive(Debug, Clone)]
pub struct SkillLoader {
    /// Global skills directory
    global_dir: PathBuf,

    /// Project skills directory
    project_dir: PathBuf,

    /// Cache of loaded skills (full content)
    loaded: HashMap<String, Skill>,

    /// Skills configuration
    config: SkillsConfig,
}

impl SkillLoader {
    /// Create a new skill loader with default paths.
    pub fn new(config: SkillsConfig) -> Result<Self> {
        let global_dir = Self::default_global_dir();
        let project_dir = Self::default_project_dir();

        std::fs::create_dir_all(&global_dir)?;
        std::fs::create_dir_all(&project_dir)?;

        Ok(Self { global_dir, project_dir, loaded: HashMap::new(), config })
    }

    /// Create a new loader with custom paths.
    pub fn with_paths(global_dir: PathBuf, project_dir: PathBuf, config: SkillsConfig) -> Result<Self> {
        std::fs::create_dir_all(&global_dir)?;
        std::fs::create_dir_all(&project_dir)?;
        Ok(Self { global_dir, project_dir, loaded: HashMap::new(), config })
    }

    /// Get the default global skills directory.
    pub fn default_global_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".thunderus")
            .join("skills")
    }

    /// Get the default project skills directory.
    pub fn default_project_dir() -> PathBuf {
        PathBuf::from(".thunderus").join("skills")
    }

    /// Get the search directories for skills, respecting config override.
    fn search_dirs(&self) -> Vec<PathBuf> {
        if let Some(custom_dir) = &self.config.skills_dir {
            return vec![custom_dir.clone()];
        }
        vec![self.project_dir.clone(), self.global_dir.clone()]
    }

    /// Discover all available skills (metadata only, lazy loading).
    pub fn discover(&self) -> Result<Vec<SkillMeta>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();
        let search_dirs = self.search_dirs();

        for dir in &search_dirs {
            if dir.exists() {
                let dir_skills = self.discover_in_dir(dir)?;
                for dir_skill in dir_skills {
                    if !skills.iter().any(|s: &SkillMeta| s.name == dir_skill.name) {
                        skills.push(dir_skill);
                    }
                }
            }
        }

        Ok(skills)
    }

    /// Discover skills in a specific directory.
    fn discover_in_dir(&self, dir: &Path) -> Result<Vec<SkillMeta>> {
        let mut skills = Vec::new();

        if !dir.exists() {
            return Ok(skills);
        }

        for entry in WalkDir::new(dir)
            .min_depth(1)
            .max_depth(2)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.file_name() == Some(std::ffi::OsStr::new("SKILL.md"))
                && let Some(skill_dir) = path.parent()
                && let Ok(skill) = parse_skill(skill_dir)
            {
                skills.push(skill.meta);
            }
        }

        Ok(skills)
    }

    /// Load a skill's full content into context.
    pub fn load(&mut self, name: &str) -> Result<&Skill> {
        if self.loaded.contains_key(name) {
            return Ok(self.loaded.get(name).unwrap());
        }
        let skill = self.find_skill(name)?;
        self.loaded.insert(name.to_string(), skill.clone());
        Ok(self.loaded.get(name).unwrap())
    }

    /// Find a skill by name, respecting config override.
    pub fn find_skill(&self, name: &str) -> Result<Skill> {
        if !self.config.enabled {
            return Err(crate::types::SkillError::NotFound(name.to_string()));
        }

        for dir in self.search_dirs() {
            let skill_path = dir.join(name);
            if skill_path.exists() && skill_path.join("SKILL.md").exists() {
                return parse_skill(&skill_path);
            }
        }

        Err(crate::types::SkillError::NotFound(name.to_string()))
    }

    /// Query skills by task intent (returns ranked matches).
    ///
    /// This implements simple keyword matching. In production, you might
    /// want to use embeddings or more sophisticated NLP.
    ///
    /// Respects `config.auto_discovery` - if disabled, returns empty results.
    pub fn query(&self, intent: &str) -> Result<Vec<SkillMatch>> {
        if !self.config.auto_discovery {
            return Ok(Vec::new());
        }

        let skills = self.discover()?;
        let mut matches = Vec::new();

        let intent_lower = intent.to_lowercase();
        let intent_words: Vec<&str> = intent_lower.split_whitespace().collect();

        for meta in skills {
            let score = self.calculate_relevance(&intent_lower, &intent_words, &meta);
            if score > 0.0 {
                let reason = self.explain_match(&intent_lower, &meta);
                if let Ok(skill) = self.find_skill(&meta.name) {
                    matches.push(SkillMatch { skill, score, reason });
                }
            }
        }

        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        Ok(matches)
    }

    /// Calculate relevance score for a skill against an intent.
    fn calculate_relevance(&self, intent: &str, intent_words: &[&str], meta: &SkillMeta) -> f64 {
        let mut score = 0.0;
        let description_lower = meta.description.to_lowercase();

        if description_lower.contains(intent) || intent.contains(&meta.name) {
            score += 0.8;
        }
        let words_matched = intent_words.iter().filter(|w| description_lower.contains(*w)).count();

        if !intent_words.is_empty() {
            score += (words_matched as f64) / (intent_words.len() as f64) * 0.5;
        }

        let tag_matches = meta.tags.iter().filter(|t| intent_words.contains(&t.as_str())).count();

        if !intent_words.is_empty() {
            score += (tag_matches as f64) / (intent_words.len() as f64) * 0.3;
        }

        if intent_words.contains(&meta.name.as_str()) {
            score += 0.7;
        }
        score.min(1.0)
    }

    /// Explain why a skill matched the intent.
    fn explain_match(&self, intent: &str, meta: &SkillMeta) -> String {
        let mut reasons: Vec<String> = Vec::new();

        if meta.description.to_lowercase().contains(intent) {
            reasons.push("description match".to_string());
        }

        if intent.contains(&meta.name) {
            reasons.push("name match".to_string());
        }

        for tag in &meta.tags {
            if intent.to_lowercase().contains(tag) {
                reasons.push(format!("tag: {tag}"));
            }
        }

        if reasons.is_empty() { "keyword match".to_string() } else { reasons.join(", ") }
    }

    /// Reload all skills from disk (clears cache).
    pub fn reload(&mut self) -> Result<()> {
        self.loaded.clear();
        Ok(())
    }

    /// Get list of loaded skill names.
    pub fn loaded_skills(&self) -> Vec<String> {
        self.loaded.keys().cloned().collect()
    }

    /// Check if a skill exists (without loading it).
    pub fn exists(&self, name: &str) -> bool {
        if !self.config.enabled {
            return false;
        }

        for dir in self.search_dirs() {
            if dir.join(name).join("SKILL.md").exists() {
                return true;
            }
        }
        false
    }

    /// Get the number of skills currently loaded.
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, description: &str, tags: &[&str]) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        let tags_str = tags.iter().map(|t| format!("  - {t}")).collect::<Vec<_>>().join("\n");

        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: {description}
tags:
{tags_str}
---

# {name}
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn test_discover_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "test-skill", "A test skill", &["test", "example"]);

        let loader = SkillLoader::with_paths(skills_dir.clone(), PathBuf::new(), SkillsConfig::default()).unwrap();
        let skills = loader.discover().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test-skill");
    }

    #[test]
    fn test_load_skill() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "test-skill", "A test skill", &["test", "example"]);

        let mut loader = SkillLoader::with_paths(skills_dir, PathBuf::new(), SkillsConfig::default()).unwrap();
        let skill = loader.load("test-skill").unwrap();
        assert_eq!(skill.meta.name, "test-skill");
        assert!(!skill.content.is_empty());
    }

    #[test]
    fn test_query_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "web-search", "Search the web", &["web"]);
        create_test_skill(&skills_dir, "git-advanced", "Advanced git operations", &["git"]);

        let loader = SkillLoader::with_paths(skills_dir, PathBuf::new(), SkillsConfig::default()).unwrap();

        let matches = loader.query("I need to search the web").unwrap();
        assert!(!matches.is_empty());
        assert!(matches[0].skill.meta.name == "web-search");
        assert!(matches[0].score > 0.1);
    }

    #[test]
    fn test_project_overrides_global() {
        let temp = TempDir::new().unwrap();
        let global_dir = temp.path().join("global");
        let project_dir = temp.path().join("project");
        fs::create_dir_all(&global_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        create_test_skill(&global_dir, "test-skill", "Global version", &[]);
        create_test_skill(&project_dir, "test-skill", "Project version", &["project"]);

        let loader = SkillLoader::with_paths(global_dir, project_dir, SkillsConfig::default()).unwrap();

        let skills = loader.discover().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Project version");
    }

    #[test]
    fn test_skill_caching() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "test-skill", "A test skill", &["test", "example"]);

        let mut loader = SkillLoader::with_paths(skills_dir, PathBuf::new(), SkillsConfig::default()).unwrap();
        assert_eq!(loader.loaded_count(), 0);

        loader.load("test-skill").unwrap();
        assert_eq!(loader.loaded_count(), 1);

        loader.load("test-skill").unwrap();
        assert_eq!(loader.loaded_count(), 1);
    }

    #[test]
    fn test_relevance_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        fs::create_dir(&skills_dir).unwrap();

        create_test_skill(
            &skills_dir,
            "brave-search",
            "Web search via Brave Search API",
            &["web", "search", "api"],
        );

        let loader = SkillLoader::with_paths(skills_dir, PathBuf::new(), SkillsConfig::default()).unwrap();
        let matches = loader.query("I need to search for something").unwrap();
        assert!(!matches.is_empty());

        let matches = loader.query("web search").unwrap();
        assert!(!matches.is_empty());
        assert!(matches[0].skill.meta.name == "brave-search");
    }
}
