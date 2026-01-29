//! Lua tool wrapper - integrates Lua plugins with the Tool system.
//!
//! Lua plugins are wrapped as Tool implementations so they can be registered
//! and executed through the existing tool infrastructure.

use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use thunderus_core::{Classification, Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult, ToolSpec};
use thunderus_skills::{HostContext, PluginFunction, Skill, SkillMeta, SkillPermissions, SkillRisk};

use crate::Tool;

/// A Lua plugin wrapped as a Tool for integration with the tool registry.
#[derive(Debug, Clone)]
pub struct LuaTool {
    /// The skill metadata
    meta: Arc<SkillMeta>,

    /// Path to the Lua script
    script_path: PathBuf,

    /// Permissions for the Lua plugin
    permissions: SkillPermissions,

    /// Available functions for this plugin
    functions: Vec<PluginFunction>,
}

impl LuaTool {
    /// Create a new LuaTool from a Skill.
    ///
    /// Returns None if the skill doesn't have a Lua driver.
    pub fn new(skill: Skill) -> Option<Self> {
        if skill.meta.driver != thunderus_skills::SkillDriver::Lua {
            return None;
        }

        let entry = if skill.meta.entry.is_empty() {
            format!("{}.lua", skill.meta.name)
        } else {
            skill.meta.entry.clone()
        };

        let script_path = skill.meta.path.join(&entry);

        if !script_path.exists() {
            return None;
        }

        let permissions = skill.meta.permissions.clone();

        Some(Self {
            meta: Arc::new(skill.meta.clone()),
            script_path,
            permissions,
            functions: skill.meta.functions.clone(),
        })
    }

    /// Get the tool name with "skill:" prefix.
    pub fn tool_name(&self) -> String {
        format!("skill:{}", self.meta.name)
    }

    /// Get the risk level as a ToolRisk.
    fn risk_level_from_skill(&self) -> ToolRisk {
        match self.meta.risk_level {
            SkillRisk::Safe => ToolRisk::Safe,
            SkillRisk::Moderate | SkillRisk::Risky => ToolRisk::Risky,
        }
    }

    /// Execute the Lua plugin.
    fn execute_lua_plugin(&self, arguments: &Value) -> Result<ToolResult> {
        #[cfg(feature = "lua")]
        {
            let mut engine = thunderus_skills::LuaEngine::new();

            let workspace_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let context = HostContext::new(self.meta.name.clone(), workspace_root, self.permissions.clone());

            let functions = self.functions.clone();
            engine
                .load(self.meta.name.clone(), &self.script_path, context, functions)
                .map_err(|e| thunderus_core::Error::extension(e.to_string()))?;

            let function_name = self.resolve_function(arguments)?;
            let input = self.prepare_input(arguments);
            let output = engine
                .execute(&self.meta.name, &function_name, &input)
                .map_err(|e| thunderus_core::Error::extension(e.to_string()))?;

            Ok(ToolResult {
                tool_call_id: String::new(),
                content: String::from_utf8_lossy(&output).to_string(),
                error: None,
                risk_level: Some(self.risk_level()),
                classification_reasoning: self.classification().map(|c| c.reasoning),
            })
        }

        #[cfg(not(feature = "lua"))]
        {
            Ok(ToolResult {
                tool_call_id: String::new(),
                content: "Lua support is not enabled. Build with --features lua to enable Lua plugins.".to_string(),
                error: Some("Feature not enabled".to_string()),
                risk_level: Some(ToolRisk::Safe),
                classification_reasoning: None,
            })
        }
    }
}

impl Tool for LuaTool {
    fn name(&self) -> &str {
        &self.meta.name
    }

    fn description(&self) -> &str {
        &self.meta.description
    }

    fn parameters(&self) -> ToolParameter {
        self.tool_parameters()
    }

    fn risk_level(&self) -> ToolRisk {
        self.risk_level_from_skill()
    }

    fn is_read_only(&self) -> bool {
        matches!(self.meta.risk_level, SkillRisk::Safe)
    }

    fn classification(&self) -> Option<Classification> {
        let risk = self.risk_level_from_skill();
        let reasoning = match risk {
            ToolRisk::Safe => format!("Lua plugin '{}' is marked as safe", self.meta.name),
            ToolRisk::Risky => format!(
                "Lua plugin '{}' may perform network or file operations - risky",
                self.meta.name
            ),
            ToolRisk::Blocked => format!("Lua plugin '{}' is blocked", self.meta.name),
        };

        Some(Classification::new(risk, reasoning))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let mut result = self.execute_lua_plugin(arguments)?;
        result.tool_call_id = tool_call_id;
        Ok(result)
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            self.tool_name(),
            format!("Lua Plugin: {}", self.meta.description),
            self.parameters(),
        )
    }
}

impl LuaTool {
    fn tool_parameters(&self) -> ToolParameter {
        let function = self.default_function();
        if self.functions.len() > 1 {
            let function_list = self
                .functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return ToolParameter::new_object(vec![
                (
                    "function".to_string(),
                    ToolParameter::new_string(format!("Function name to invoke. Available: {function_list}")),
                ),
                (
                    "input".to_string(),
                    ToolParameter::new_string("JSON input for the selected function"),
                ),
            ]);
        }

        if !function.parameters.is_null() {
            return tool_parameter_from_schema(&function.parameters);
        }

        ToolParameter::new_object(vec![(
            "input".to_string(),
            ToolParameter::new_string("Input data for the Lua plugin"),
        )])
    }

    fn resolve_function(&self, arguments: &Value) -> Result<String> {
        if self.functions.is_empty() {
            return Ok(self.meta.name.clone());
        }

        if self.functions.len() == 1 {
            return Ok(self.functions[0].name.clone());
        }

        if let Some(name) = arguments.get("function").and_then(|v| v.as_str()) {
            if self.functions.iter().any(|f| f.name == name) {
                return Ok(name.to_string());
            }
            return Err(thunderus_core::Error::Tool(format!(
                "Function '{}' not found in plugin '{}'",
                name, self.meta.name
            )));
        }

        Err(thunderus_core::Error::Tool(format!(
            "Function name required for multi-function plugin '{}'",
            self.meta.name
        )))
    }

    fn prepare_input(&self, arguments: &Value) -> Vec<u8> {
        let mut payload = arguments.clone();
        if self.functions.len() > 1
            && let Some(obj) = payload.as_object_mut()
        {
            obj.remove("function");
        }
        serde_json::to_vec(&payload).unwrap_or_default()
    }

    fn default_function(&self) -> PluginFunction {
        if let Some(first) = self.functions.first() {
            return first.clone();
        }
        PluginFunction {
            name: self.meta.name.clone(),
            description: self.meta.description.clone(),
            parameters: self.meta.parameters.clone(),
        }
    }
}

fn tool_parameter_from_schema(schema: &Value) -> ToolParameter {
    let schema_obj = schema.as_object();
    let schema_type = schema_obj.and_then(|obj| obj.get("type")).and_then(|v| v.as_str());

    match schema_type {
        Some("string") => ToolParameter::String {
            description: schema_obj
                .and_then(|o| o.get("description"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string()),
        },
        Some("number") | Some("integer") => ToolParameter::Number {
            description: schema_obj
                .and_then(|o| o.get("description"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string()),
        },
        Some("boolean") => ToolParameter::Boolean {
            description: schema_obj
                .and_then(|o| o.get("description"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string()),
        },
        Some("array") => {
            let items_value = schema_obj
                .and_then(|o| o.get("items"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            ToolParameter::Array {
                items: Box::new(tool_parameter_from_schema(&items_value)),
                description: schema_obj
                    .and_then(|o| o.get("description"))
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string()),
            }
        }
        Some("object") | None => {
            let properties = schema_obj
                .and_then(|o| o.get("properties"))
                .and_then(|v| v.as_object())
                .map(|props| {
                    props
                        .iter()
                        .map(|(name, value)| (name.clone(), tool_parameter_from_schema(value)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let required = schema_obj
                .and_then(|o| o.get("required"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                });
            ToolParameter::Object {
                properties,
                description: schema_obj
                    .and_then(|o| o.get("description"))
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string()),
                required,
            }
        }
        _ => ToolParameter::new_string("Input data for the Lua plugin"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_lua_tool_requires_lua_driver() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();

        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: test-skill
description: A test skill
driver: shell
---
Test skill
"#,
        )
        .unwrap();

        let skill = thunderus_skills::parse_skill(&skill_dir).unwrap();
        assert!(LuaTool::new(skill).is_none());
    }
}
