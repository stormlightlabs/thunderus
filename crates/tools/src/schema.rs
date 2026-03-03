//! Tool schema definitions for LLM consumption
//!
//! These schemas are serialized into the OpenAI tool format.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

/// A complete tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: ToolSchema,
}

impl Tool {
    /// Convert to OpenAI function format
    pub fn to_openai_function(&self) -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": &self.name,
                "description": &self.description,
                "parameters": self.parameters.to_json_schema()
            }
        })
    }

    /// Create the read tool definition
    pub fn read() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "path",
            ToolParameter {
                param_type: "string".to_string(),
                description: "Path to the file (relative to repo root or absolute).".to_string(),
                required: true,
                ..Default::default()
            },
        );
        params.add_property(
            "offset",
            ToolParameter {
                param_type: "integer".to_string(),
                description: "Line number to start reading from (1-indexed).".to_string(),
                required: false,
                minimum: Some(1.0),
                ..Default::default()
            },
        );
        params.add_property(
            "limit",
            ToolParameter {
                param_type: "integer".to_string(),
                description: "Maximum number of lines to read.".to_string(),
                required: false,
                minimum: Some(1.0),
                ..Default::default()
            },
        );

        Self {
            name: "read".to_string(),
            description: "Read the contents of a file. Returns line-numbered text for text files, or an image attachment for image files. Defaults to the first 2000 lines. Use offset and limit to paginate large files.".to_string(),
            parameters: params,
        }
    }

    /// Create the write tool definition
    pub fn write() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "path",
            ToolParameter {
                param_type: "string".to_string(),
                description: "Path to the file (relative to repo root or absolute).".to_string(),
                required: true,
                ..Default::default()
            },
        );
        params.add_property(
            "content",
            ToolParameter {
                param_type: "string".to_string(),
                description: "The full file contents to write.".to_string(),
                required: true,
                ..Default::default()
            },
        );

        Self {
            name: "write".to_string(),
            description: "Write content to a file. Creates the file if it doesn't exist, or overwrites it entirely if it does. Creates parent directories as needed.".to_string(),
            parameters: params,
        }
    }

    /// Create the edit tool definition
    pub fn edit() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "path",
            ToolParameter {
                param_type: "string".to_string(),
                description: "Path to the file to edit.".to_string(),
                required: true,
                ..Default::default()
            },
        );
        params.add_property(
            "diff",
            ToolParameter {
                param_type: "string".to_string(),
                description: "A unified diff patch to apply. Use standard unified diff format with @@ hunk headers."
                    .to_string(),
                required: true,
                ..Default::default()
            },
        );

        Self {
            name: "edit".to_string(),
            description:
                "Apply a surgical edit to an existing file. Provide a unified diff patch. The file must already exist."
                    .to_string(),
            parameters: params,
        }
    }

    /// Create the bash tool definition
    pub fn bash() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "command",
            ToolParameter {
                param_type: "string".to_string(),
                description: "The shell command to run.".to_string(),
                required: true,
                ..Default::default()
            },
        );

        Self {
            name: "bash".to_string(),
            description: "Execute a bash command in the repository sandbox. Use for: running builds, tests, git operations, file searches, and any task the other tools don't cover. Commands run with the repo root as the working directory.".to_string(),
            parameters: params,
        }
    }

    /// Create the research tool definition
    pub fn research() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "url",
            ToolParameter {
                param_type: "string".to_string(),
                description: "The URL to fetch. Must be HTTPS.".to_string(),
                required: true,
                ..Default::default()
            },
        );

        Self {
            name: "research".to_string(),
            description: "Fetch a URL and return its contents. For HTML pages, extracts article content and metadata (title, author, date) using readability analysis. Use this to read API documentation, library references, standards, CLI flag descriptions, or any public technical resource. Do not use for authentication-gated content.".to_string(),
            parameters: params,
        }
    }

    /// Create the memory_store tool definition
    pub fn memory_store() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "content",
            ToolParameter {
                param_type: "string".to_string(),
                description: "The information to remember. Be specific and self-contained.".to_string(),
                required: true,
                ..Default::default()
            },
        );
        params.add_property(
            "kind",
            ToolParameter {
                param_type: "string".to_string(),
                description: "The type of memory: 'fact', 'preference', 'procedure', or 'context'.".to_string(),
                required: true,
                enum_values: Some(vec![
                    "fact".to_string(),
                    "preference".to_string(),
                    "procedure".to_string(),
                    "context".to_string(),
                ]),
                ..Default::default()
            },
        );
        params.add_property(
            "tags",
            ToolParameter {
                param_type: "array".to_string(),
                description: "Tags for categorization (e.g., ['rust', 'testing', 'deployment']).".to_string(),
                required: false,
                items: Some(Box::new(ToolParameter {
                    param_type: "string".to_string(),
                    description: "A tag string".to_string(),
                    required: true,
                    ..Default::default()
                })),
                ..Default::default()
            },
        );

        Self {
            name: "memory_store".to_string(),
            description: "Save information to persistent memory for future conversations. Use this to remember facts about the codebase, user preferences, procedures that worked, or important context.".to_string(),
            parameters: params,
        }
    }

    /// Create the memory_recall tool definition
    pub fn memory_recall() -> Self {
        let mut params = ToolSchema::new();
        params.add_property(
            "query",
            ToolParameter {
                param_type: "string".to_string(),
                description: "What to search for. Describe the information you need.".to_string(),
                required: true,
                ..Default::default()
            },
        );
        params.add_property(
            "kind",
            ToolParameter {
                param_type: "string".to_string(),
                description: "Filter by memory type: 'fact', 'preference', 'procedure', or 'context'.".to_string(),
                required: false,
                enum_values: Some(vec![
                    "fact".to_string(),
                    "preference".to_string(),
                    "procedure".to_string(),
                    "context".to_string(),
                ]),
                ..Default::default()
            },
        );
        params.add_property(
            "count",
            ToolParameter {
                param_type: "integer".to_string(),
                description: "Number of results to return (1-20).".to_string(),
                required: false,
                minimum: Some(1.0),
                maximum: Some(20.0),
                ..Default::default()
            },
        );

        Self {
            name: "memory_recall".to_string(),
            description: "Search persistent memory for relevant information from past conversations. Use this when you need context about the codebase, user preferences, or previously learned procedures.".to_string(),
            parameters: params,
        }
    }
}

/// Schema for tool parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolSchema {
    pub properties: HashMap<String, ToolParameter>,
    pub required: Vec<String>,
}

impl ToolSchema {
    /// Create a new empty schema
    pub fn new() -> Self {
        Self { properties: HashMap::new(), required: Vec::new() }
    }

    /// Add a property to the schema
    pub fn add_property(&mut self, name: &str, param: ToolParameter) {
        if param.required {
            self.required.push(name.to_string());
        }
        self.properties.insert(name.to_string(), param);
    }

    /// Convert to JSON Schema format
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        for (name, param) in &self.properties {
            properties.insert(name.clone(), param.to_json_schema());
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": self.required
        })
    }
}

/// A single parameter definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolParameter {
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(skip)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<ToolParameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

impl ToolParameter {
    /// Convert to JSON Schema format
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("type".to_string(), json!(self.param_type));
        obj.insert("description".to_string(), json!(self.description));
        if let Some(min) = self.minimum {
            obj.insert("minimum".to_string(), json!(min));
        }
        if let Some(max) = self.maximum {
            obj.insert("maximum".to_string(), json!(max));
        }
        if let Some(items) = &self.items {
            obj.insert("items".to_string(), items.to_json_schema());
        }
        if let Some(enum_vals) = &self.enum_values {
            obj.insert("enum".to_string(), json!(enum_vals));
        }
        json!(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_tool_schema() {
        let tool = Tool::read();
        assert_eq!(tool.name, "read");
        assert!(tool.description.contains("Read the contents"));
        assert!(tool.parameters.required.contains(&"path".to_string()));
    }

    #[test]
    fn test_openai_function_format() {
        let tool = Tool::bash();
        let json = tool.to_openai_function();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "bash");
    }

    #[test]
    fn test_tool_parameter_serialization() {
        let param = ToolParameter {
            param_type: "integer".to_string(),
            description: "A number".to_string(),
            required: false,
            minimum: Some(1.0),
            maximum: None,
            items: None,
            enum_values: None,
        };
        let json = param.to_json_schema();
        assert_eq!(json["type"], "integer");
        assert_eq!(json["description"], "A number");
        assert_eq!(json["minimum"], serde_json::json!(1.0));
    }
}
