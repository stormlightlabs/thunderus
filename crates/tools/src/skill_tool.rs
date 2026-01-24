//! Skill tool wrapper - integrates Skills with the Tool system.
//!
//! Skills are wrapped as Tool implementations so they can be registered
//! and executed through the existing tool infrastructure.

use serde_json::Value;
use std::process::Command;
use std::sync::Arc;
use thunderus_core::{Classification, Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult, ToolSpec};
use thunderus_skills::{Skill, SkillMeta, SkillScript};

use crate::Tool;

/// A skill wrapped as a Tool for integration with the tool registry.
#[derive(Debug, Clone)]
pub struct SkillTool {
    /// The skill metadata (for name, description, etc.)
    meta: Arc<SkillMeta>,

    /// The main script to execute
    script: Arc<SkillScript>,
}

impl SkillTool {
    /// Create a new SkillTool from a Skill.
    pub fn new(skill: Skill) -> Self {
        let meta = Arc::new(skill.meta.clone());

        let script = Self::find_main_script(&skill)
            .unwrap_or_else(|| skill.scripts.first().expect("Skill must have at least one script"))
            .clone();

        let script = Arc::new(script);

        Self { meta, script }
    }

    /// Get the skill name formatted as a tool name ("skill:name").
    pub fn tool_name(&self) -> String {
        format!("skill:{}", self.meta.name)
    }

    /// Execute the skill's main script with the given arguments.
    fn execute_skill_script(&self, arguments: &Value) -> Result<ToolResult> {
        let args = self.extract_command_args(arguments);

        let output = match self.script.script_type {
            thunderus_skills::ScriptType::Bash => {
                let mut cmd = Command::new("bash");
                cmd.arg(&self.script.path);
                for arg in &args {
                    cmd.arg(arg);
                }
                cmd.output()?
            }
            thunderus_skills::ScriptType::Python => {
                let mut cmd = Command::new("python3");
                cmd.arg(&self.script.path);
                for arg in &args {
                    cmd.arg(arg);
                }
                cmd.output()?
            }
            thunderus_skills::ScriptType::JavaScript => {
                let mut cmd = Command::new("node");
                cmd.arg(&self.script.path);
                for arg in &args {
                    cmd.arg(arg);
                }
                cmd.output()?
            }
            thunderus_skills::ScriptType::Lua => {
                let mut cmd = Command::new("lua");
                cmd.arg(&self.script.path);
                for arg in &args {
                    cmd.arg(arg);
                }
                cmd.output()?
            }
            thunderus_skills::ScriptType::Unknown => {
                return Err(thunderus_core::Error::Tool(format!(
                    "Unknown script type for {}",
                    self.script.name
                )));
            }
        };

        let content = String::from_utf8_lossy(&output.stdout).to_string();
        let error = if !output.stderr.is_empty() {
            Some(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            None
        };

        Ok(ToolResult {
            tool_call_id: String::new(),
            content,
            error,
            risk_level: Some(self.risk_level()),
            classification_reasoning: self.classification().map(|c| c.reasoning),
        })
    }

    /// Find the main script to execute.
    fn find_main_script(skill: &Skill) -> Option<&SkillScript> {
        let entry_points = ["run.sh", "main.sh", "index.sh", &format!("{}.sh", skill.meta.name)];

        for name in &entry_points {
            if let Some(script) = skill.scripts.iter().find(|s| &s.name == name) {
                return Some(script);
            }
        }

        skill.scripts.first()
    }

    /// Extract command arguments from tool arguments.
    fn extract_command_args(&self, arguments: &Value) -> Vec<String> {
        let mut args = Vec::new();

        if let Some(arr) = arguments.get("args").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    args.push(s.to_string());
                }
            }
        }

        if let Some(query) = arguments.get("query").and_then(|v| v.as_str()) {
            args.push(query.to_string());
        }

        if let Some(input) = arguments.get("input").and_then(|v| v.as_str()) {
            args.push(input.to_string());
        }

        args
    }

    /// Get the risk level as a ToolRisk.
    fn risk_level_from_skill(&self) -> ToolRisk {
        match self.meta.risk_level {
            thunderus_skills::SkillRisk::Safe => ToolRisk::Safe,
            thunderus_skills::SkillRisk::Moderate | thunderus_skills::SkillRisk::Risky => ToolRisk::Risky,
        }
    }
}

impl Tool for SkillTool {
    fn name(&self) -> &str {
        &self.meta.name
    }

    fn description(&self) -> &str {
        &self.meta.description
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "args".to_string(),
                ToolParameter::new_array(ToolParameter::new_string(
                    "Array of string arguments to pass to the skill script",
                )),
            ),
            (
                "query".to_string(),
                ToolParameter::new_string("Query string to pass to the skill"),
            ),
            (
                "input".to_string(),
                ToolParameter::new_string("Input string to pass to the skill"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        self.risk_level_from_skill()
    }

    fn is_read_only(&self) -> bool {
        matches!(self.meta.risk_level, thunderus_skills::SkillRisk::Safe)
    }

    fn classification(&self) -> Option<Classification> {
        let risk = self.risk_level_from_skill();
        let reasoning = match risk {
            ToolRisk::Safe => format!("Skill '{}' is marked as safe (read-only)", self.meta.name),
            ToolRisk::Risky => format!(
                "Skill '{}' may perform network or destructive operations - risky",
                self.meta.name
            ),
            ToolRisk::Blocked => format!("Skill '{}' is blocked", self.meta.name),
        };

        Some(Classification::new(risk, reasoning))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let mut result = self.execute_skill_script(arguments)?;
        result.tool_call_id = tool_call_id;
        Ok(result)
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            self.tool_name(),
            format!("Skill: {}", self.meta.description),
            self.parameters(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str) -> Skill {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: A test skill for {name}
risk_level: safe
---

# {name}

A test skill.
"#
            ),
        )
        .unwrap();

        fs::write(
            skill_dir.join("run.sh"),
            r#"#!/bin/bash
echo "Skill executed successfully"
"#,
        )
        .unwrap();

        thunderus_skills::parse_skill(&skill_dir).unwrap()
    }

    #[test]
    fn test_skill_tool_creation() {
        let temp_dir = TempDir::new().unwrap();
        let skill = create_test_skill(temp_dir.path(), "test-skill");
        let tool = SkillTool::new(skill);
        assert_eq!(tool.name(), "test-skill");
        assert!(tool.description().contains("test skill"));
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_skill_tool_name() {
        let temp_dir = TempDir::new().unwrap();
        let skill = create_test_skill(temp_dir.path(), "my-skill");
        let tool = SkillTool::new(skill);
        assert_eq!(tool.tool_name(), "skill:my-skill");
    }

    #[test]
    fn test_skill_tool_classification() {
        let temp_dir = TempDir::new().unwrap();
        let skill = create_test_skill(temp_dir.path(), "safe-skill");
        let tool = SkillTool::new(skill);
        let classification = tool.classification();
        assert!(classification.is_some());

        let cls = classification.unwrap();
        assert_eq!(cls.risk, ToolRisk::Safe);
        assert!(cls.reasoning.contains("safe"));
    }

    #[test]
    fn test_skill_tool_spec() {
        let temp_dir = TempDir::new().unwrap();
        let skill = create_test_skill(temp_dir.path(), "test-skill");

        let tool = SkillTool::new(skill);
        let spec = tool.spec();
        assert_eq!(spec.name(), "skill:test-skill");
        assert!(spec.description().is_some_and(|d| d.contains("Skill:")));
    }
}
