//! Public URL read tool boundary.
//!
//! The executor delegates to [`crate::search::fetch_url`], which owns URL scheme
//! validation, private-network rejection, redirect checks, content-type
//! allow-listing, response caps, timeout handling, and truncation metadata.

use crate::search;
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::tools::{ToolDefinition, ToolOutput, ToolUseRequest};

const NAME: &str = "read_url";

/// Parsed provider input for `read_url`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadUrlInput {
    url: String,
}

/// Provider-visible definition for `read_url`.
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: NAME,
        description: r#"read_url

Fetch a public HTTP/HTTPS URL and extract readable text.

Use to read a page found via web_search or referenced in the workspace. Prefer
local files when available. HTML is extracted to Markdown with Lectito; JSON,
XML, plain text, feeds, and YAML are returned raw. Binary content is rejected.
Private targets, redirects, and non-http(s) schemes are rejected. Size,
redirects, and timeouts are capped; output may truncate."#,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The public HTTP/HTTPS URL to fetch." }
            },
            "required": ["url"]
        }),
    }
}

/// Parse provider JSON arguments for `read_url`.
pub fn parse_arguments(arguments: &str) -> Result<ReadUrlInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(ReadUrlInput {
        url: args
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Execute a registry request for `read_url`.
pub fn execute_request(request: &ToolUseRequest, _ctx: ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec_input(&input)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn exec_input(input: &ReadUrlInput) -> ToolOutput {
    match search::fetch_url(&input.url) {
        Ok(content) => {
            let mut lines = vec![
                format!("title: {}", content.title),
                format!("url: {}", content.final_url),
                format!("status: {}", content.status),
            ];
            if content.truncated {
                lines.push("(content truncated)".to_string());
            }
            lines.push(format!("diagnostics: {}", content.diagnostics.join(", ")));
            lines.push(content.markdown);
            ToolOutput::ok(NAME, lines)
        }
        Err(error) => ToolOutput::failed(NAME, error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::app::ToolStatus;
    use crate::tools::{self, ToolUseRequest};

    #[test]
    fn parse_arguments_reads_url() {
        let input = parse_arguments(r#"{"url":"https://example.com"}"#).expect("parse");
        assert_eq!(input.url, "https://example.com");
    }

    #[test]
    fn registry_execute_rejects_non_http_scheme() {
        let request = ToolUseRequest::new(
            "read_url".to_string(),
            r#"{"url":"file:///etc/passwd"}"#.to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, tools::registry::ToolContext::new(Path::new("."))).output;

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|error| error.contains("unsupported URL scheme")),
            "expected scheme rejection, got {output:?}"
        );
    }

    #[test]
    fn registry_execute_rejects_private_target() {
        let request = ToolUseRequest::new(
            "read_url".to_string(),
            r#"{"url":"http://127.0.0.1:8080"}"#.to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, tools::registry::ToolContext::new(Path::new("."))).output;

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(
            output
                .error
                .as_ref()
                .is_some_and(|error| error.contains("private network target rejected")),
            "expected private target rejection, got {output:?}"
        );
    }
}
