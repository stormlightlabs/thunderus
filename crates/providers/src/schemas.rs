//! Tool schema definitions for provider integration
//!
//! This module exports tool specifications in formats suitable for
//! GLM-4.7 (OpenAI-compatible) and Gemini providers.

use crate::types::ToolParameter;

/// Returns tool schemas for GLM-4.7 provider
///
/// GLM-4.7 uses OpenAI-compatible tool format with the following structure:
/// - tools: [{ type: "function", function: { name, description, parameters } }]
///
/// The schemas are defined with JSON Schema parameters that match the tool
/// implementations in crate thunderus-tools.
pub fn glm_tool_schemas() -> Vec<GlmToolSchema> {
    vec![
        GlmToolSchema {
            spec_type: "function".to_string(),
            function: GlmFunction {
                name: "grep".to_string(),
                description: Some(
                    "Search for patterns in files using ripgrep. Use this instead of bash grep for code search."
                        .to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "pattern".to_string(),
                        ToolParameter::new_string("Regex pattern to search")
                            .with_description("The regex pattern to search for in files"),
                    ),
                    (
                        "path".to_string(),
                        ToolParameter::new_string("Directory or file to search")
                            .with_description("Directory or file path to search in (defaults to current directory)"),
                    ),
                    (
                        "glob".to_string(),
                        ToolParameter::new_string("File filter pattern")
                            .with_description("Glob pattern to filter files (e.g., '*.rs', '*.{ts,tsx}')"),
                    ),
                    (
                        "output_mode".to_string(),
                        ToolParameter::new_string("Output format")
                            .with_description("Output mode: 'files_with_matches' (default), 'content', or 'count'"),
                    ),
                    (
                        "context_before".to_string(),
                        ToolParameter::new_number("Lines before match")
                            .with_description("Number of lines to show before each match (like grep -B)"),
                    ),
                    (
                        "context_after".to_string(),
                        ToolParameter::new_number("Lines after match")
                            .with_description("Number of lines to show after each match (like grep -A)"),
                    ),
                    (
                        "case_insensitive".to_string(),
                        ToolParameter::new_boolean("Case-insensitive search")
                            .with_description("Perform case-insensitive search (like grep -i)"),
                    ),
                    (
                        "head_limit".to_string(),
                        ToolParameter::new_number("Max results")
                            .with_description("Maximum number of results to return (default: 100 files/lines)"),
                    ),
                ]),
            },
        },
        GlmToolSchema {
            spec_type: "function".to_string(),
            function: GlmFunction {
                name: "glob".to_string(),
                description: Some(
                    "Find files matching glob patterns. Fast file discovery with .gitignore awareness.".to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "pattern".to_string(),
                        ToolParameter::new_string("Glob pattern")
                            .with_description("File pattern (e.g., '**/*.rs', 'src/**/test_*.rs')"),
                    ),
                    (
                        "path".to_string(),
                        ToolParameter::new_string("Directory to search")
                            .with_description("Directory path to search in (defaults to current directory)"),
                    ),
                    (
                        "sort_order".to_string(),
                        ToolParameter::new_string("Sort order")
                            .with_description("Sort by: 'modified' (newest first), 'path' (alphabetical), or 'none'"),
                    ),
                    (
                        "respect_gitignore".to_string(),
                        ToolParameter::new_boolean("Respect .gitignore")
                            .with_description("Respect .gitignore rules (default: true)"),
                    ),
                    (
                        "limit".to_string(),
                        ToolParameter::new_number("Max results")
                            .with_description("Maximum number of files to return (default: unlimited)"),
                    ),
                ]),
            },
        },
        GlmToolSchema {
            spec_type: "function".to_string(),
            function: GlmFunction {
                name: "read".to_string(),
                description: Some(
                    "Read file contents with line numbers. Always read files before editing them.".to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "file_path".to_string(),
                        ToolParameter::new_string("Absolute path to file")
                            .with_description("Absolute path to the file to read"),
                    ),
                    (
                        "offset".to_string(),
                        ToolParameter::new_number("Line offset")
                            .with_description("Line number to start reading from (default: 1)"),
                    ),
                    (
                        "limit".to_string(),
                        ToolParameter::new_number("Line limit")
                            .with_description("Maximum number of lines to read (default: 2000)"),
                    ),
                ]),
            },
        },
        GlmToolSchema {
            spec_type: "function".to_string(),
            function: GlmFunction {
                name: "edit".to_string(),
                description: Some(
                    "Make safe find-replace edits to files. Use Edit instead of sed for safer file modifications."
                        .to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "file_path".to_string(),
                        ToolParameter::new_string("Absolute path to file")
                            .with_description("Absolute path to the file to edit"),
                    ),
                    (
                        "old_string".to_string(),
                        ToolParameter::new_string("Text to replace")
                            .with_description("Exact text string to replace (must be unique in file)"),
                    ),
                    (
                        "new_string".to_string(),
                        ToolParameter::new_string("Replacement text")
                            .with_description("Text to replace old_string with"),
                    ),
                    (
                        "replace_all".to_string(),
                        ToolParameter::new_boolean("Replace all occurrences")
                            .with_description("Replace all occurrences of old_string (default: false)"),
                    ),
                ]),
            },
        },
        GlmToolSchema {
            spec_type: "function".to_string(),
            function: GlmFunction {
                name: "multiedit".to_string(),
                description: Some(
                    "Apply multiple find-replace edits to a file atomically. All edits succeed or none are applied."
                        .to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "file_path".to_string(),
                        ToolParameter::new_string("Absolute path to file")
                            .with_description("Absolute path to the file to edit"),
                    ),
                    (
                        "edits".to_string(),
                        ToolParameter::new_array(ToolParameter::new_object(vec![
                            (
                                "old_string".to_string(),
                                ToolParameter::new_string("Text to replace")
                                    .with_description("Exact text string to replace"),
                            ),
                            (
                                "new_string".to_string(),
                                ToolParameter::new_string("Replacement text")
                                    .with_description("Text to replace old_string with"),
                            ),
                        ]))
                        .with_description("Array of edit operations to apply"),
                    ),
                ]),
            },
        },
    ]
}

/// Returns tool schemas for Gemini provider
///
/// Gemini uses a different tool format:
/// - tools: [{ functionDeclarations: [{ name, description, parameters }] }]
///
/// The parameters use JSON Schema format compatible with Gemini's OpenAPI 3.0 subset.
pub fn gemini_tool_schemas() -> Vec<GeminiToolSchema> {
    vec![GeminiToolSchema {
        function_declarations: vec![
            GeminiFunctionDeclaration {
                name: "grep".to_string(),
                description: Some(
                    "Search for patterns in files using ripgrep. Use this instead of bash grep for code search."
                        .to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "pattern".to_string(),
                        ToolParameter::new_string("Regex pattern to search")
                            .with_description("The regex pattern to search for in files"),
                    ),
                    (
                        "path".to_string(),
                        ToolParameter::new_string("Directory or file to search")
                            .with_description("Directory or file path to search in (defaults to current directory)"),
                    ),
                    (
                        "glob".to_string(),
                        ToolParameter::new_string("File filter pattern")
                            .with_description("Glob pattern to filter files (e.g., '*.rs', '*.{ts,tsx}')"),
                    ),
                    (
                        "output_mode".to_string(),
                        ToolParameter::new_string("Output format")
                            .with_description("Output mode: 'files_with_matches' (default), 'content', or 'count'"),
                    ),
                    (
                        "context_before".to_string(),
                        ToolParameter::new_number("Lines before match")
                            .with_description("Number of lines to show before each match (like grep -B)"),
                    ),
                    (
                        "context_after".to_string(),
                        ToolParameter::new_number("Lines after match")
                            .with_description("Number of lines to show after each match (like grep -A)"),
                    ),
                    (
                        "case_insensitive".to_string(),
                        ToolParameter::new_boolean("Case-insensitive search")
                            .with_description("Perform case-insensitive search (like grep -i)"),
                    ),
                    (
                        "head_limit".to_string(),
                        ToolParameter::new_number("Max results")
                            .with_description("Maximum number of results to return (default: 100 files/lines)"),
                    ),
                ]),
            },
            GeminiFunctionDeclaration {
                name: "glob".to_string(),
                description: Some(
                    "Find files matching glob patterns. Fast file discovery with .gitignore awareness.".to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "pattern".to_string(),
                        ToolParameter::new_string("Glob pattern")
                            .with_description("File pattern (e.g., '**/*.rs', 'src/**/test_*.rs')"),
                    ),
                    (
                        "path".to_string(),
                        ToolParameter::new_string("Directory to search")
                            .with_description("Directory path to search in (defaults to current directory)"),
                    ),
                    (
                        "sort_order".to_string(),
                        ToolParameter::new_string("Sort order")
                            .with_description("Sort by: 'modified' (newest first), 'path' (alphabetical), or 'none'"),
                    ),
                    (
                        "respect_gitignore".to_string(),
                        ToolParameter::new_boolean("Respect .gitignore")
                            .with_description("Respect .gitignore rules (default: true)"),
                    ),
                    (
                        "limit".to_string(),
                        ToolParameter::new_number("Max results")
                            .with_description("Maximum number of files to return (default: unlimited)"),
                    ),
                ]),
            },
            GeminiFunctionDeclaration {
                name: "read".to_string(),
                description: Some(
                    "Read file contents with line numbers. Always read files before editing them.".to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "file_path".to_string(),
                        ToolParameter::new_string("Absolute path to file")
                            .with_description("Absolute path to the file to read"),
                    ),
                    (
                        "offset".to_string(),
                        ToolParameter::new_number("Line offset")
                            .with_description("Line number to start reading from (default: 1)"),
                    ),
                    (
                        "limit".to_string(),
                        ToolParameter::new_number("Line limit")
                            .with_description("Maximum number of lines to read (default: 2000)"),
                    ),
                ]),
            },
            GeminiFunctionDeclaration {
                name: "edit".to_string(),
                description: Some(
                    "Make safe find-replace edits to files. Use Edit instead of sed for safer file modifications."
                        .to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "file_path".to_string(),
                        ToolParameter::new_string("Absolute path to file")
                            .with_description("Absolute path to the file to edit"),
                    ),
                    (
                        "old_string".to_string(),
                        ToolParameter::new_string("Text to replace")
                            .with_description("Exact text string to replace (must be unique in file)"),
                    ),
                    (
                        "new_string".to_string(),
                        ToolParameter::new_string("Replacement text")
                            .with_description("Text to replace old_string with"),
                    ),
                    (
                        "replace_all".to_string(),
                        ToolParameter::new_boolean("Replace all occurrences")
                            .with_description("Replace all occurrences of old_string (default: false)"),
                    ),
                ]),
            },
            GeminiFunctionDeclaration {
                name: "multiedit".to_string(),
                description: Some(
                    "Apply multiple find-replace edits to a file atomically. All edits succeed or none are applied."
                        .to_string(),
                ),
                parameters: ToolParameter::new_object(vec![
                    (
                        "file_path".to_string(),
                        ToolParameter::new_string("Absolute path to file")
                            .with_description("Absolute path to the file to edit"),
                    ),
                    (
                        "edits".to_string(),
                        ToolParameter::new_array(ToolParameter::new_object(vec![
                            (
                                "old_string".to_string(),
                                ToolParameter::new_string("Text to replace")
                                    .with_description("Exact text string to replace"),
                            ),
                            (
                                "new_string".to_string(),
                                ToolParameter::new_string("Replacement text")
                                    .with_description("Text to replace old_string with"),
                            ),
                        ]))
                        .with_description("Array of edit operations to apply"),
                    ),
                ]),
            },
        ],
    }]
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GlmToolSchema {
    #[serde(rename = "type")]
    pub spec_type: String,
    pub function: GlmFunction,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GlmFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: ToolParameter,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GeminiToolSchema {
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub parameters: ToolParameter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm_schemas_structure() {
        let schemas = glm_tool_schemas();
        assert!(!schemas.is_empty());
        assert_eq!(schemas[0].spec_type, "function");
        assert!(!schemas[0].function.name.is_empty());
    }

    #[test]
    fn test_gemini_schemas_structure() {
        let schemas = gemini_tool_schemas();
        assert!(!schemas.is_empty());
        assert!(!schemas[0].function_declarations.is_empty());
        assert!(!schemas[0].function_declarations[0].name.is_empty());
    }

    #[test]
    fn test_grep_tool_in_schemas() {
        let glm_schemas = glm_tool_schemas();
        let gemini_schemas = gemini_tool_schemas();
        assert!(glm_schemas.iter().any(|s| s.function.name == "grep"));
        assert!(gemini_schemas[0].function_declarations.iter().any(|f| f.name == "grep"));
    }

    #[test]
    fn test_all_core_tools_present() {
        let glm_schemas = glm_tool_schemas();
        let gemini_declarations = &gemini_tool_schemas()[0].function_declarations;

        let expected_tools = vec!["grep", "glob", "read", "edit", "multiedit"];

        for tool in expected_tools {
            assert!(
                glm_schemas.iter().any(|s| s.function.name == tool),
                "GLM missing {}",
                tool
            );
            assert!(
                gemini_declarations.iter().any(|f| f.name == tool),
                "Gemini missing {}",
                tool
            );
        }
    }

    #[test]
    fn test_tool_descriptions_exist() {
        let glm_schemas = glm_tool_schemas();

        for schema in glm_schemas {
            assert!(
                schema.function.description.is_some(),
                "GLM tool {} missing description",
                schema.function.name
            );
            assert!(
                !schema.function.description.as_ref().unwrap().is_empty(),
                "GLM tool {} has empty description",
                schema.function.name
            );
        }
    }

    #[test]
    fn test_gemini_tool_descriptions_exist() {
        let gemini_declarations = &gemini_tool_schemas()[0].function_declarations;

        for func in gemini_declarations {
            assert!(
                func.description.is_some(),
                "Gemini tool {} missing description",
                func.name
            );
            assert!(
                !func.description.as_ref().unwrap().is_empty(),
                "Gemini tool {} has empty description",
                func.name
            );
        }
    }
}
