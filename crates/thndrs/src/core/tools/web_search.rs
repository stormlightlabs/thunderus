//! Web search tool boundary.
//!
//! The executor delegates to the application-owned backend in [`crate::search`].
//! Provider adapters never select or lower a web-search backend.

use crate::search;
use crate::tools::registry::{ToolContext, ToolError, ToolExecution};
use crate::tools::{ToolDefinition, ToolOutput, ToolUseRequest};

const NAME: &str = "web_search";

/// Parsed provider input for `web_search`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSearchInput {
    pub query: String,
    pub max_results: usize,
}

/// Provider-visible definition for `web_search`.
pub fn definition() -> ToolDefinition {
    ToolDefinition::new(
        NAME,
        r#"web_search

Search the web for current information.

Use this when the workspace does not contain the answer and you need external
documentation, API specs, or current facts. Prefer reading local files and
searching the workspace first. Results are normalized and their public URLs are
read with Lectito when possible; private result URLs are rejected. The backend
is configured as duckduckgo, searxng, or none. Capped at 10 results."#,
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query." },
                "max_results": { "type": "integer", "description": "Maximum number of results to return (capped at 10)." }
            },
            "required": ["query"]
        }),
    )
}

/// Parse provider JSON arguments for `web_search`.
pub fn parse_arguments(arguments: &str) -> Result<WebSearchInput, ToolError> {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    Ok(WebSearchInput {
        query: args
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        max_results: args
            .get("max_results")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(search::DEFAULT_SEARCH_LIMIT),
    })
}

/// Execute a registry request for `web_search`.
pub fn execute_request(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
    match parse_arguments(&request.arguments) {
        Ok(input) => ToolExecution::output(exec_input(&input, &ctx.search)),
        Err(error) => ToolExecution::output(ToolOutput::failed(NAME, error.to_string())),
    }
}

fn exec_input(input: &WebSearchInput, config: &search::SearchConfig) -> ToolOutput {
    match search::search_and_extract(config, &input.query, input.max_results) {
        Ok(results) if results.is_empty() => ToolOutput::ok(NAME, vec!["no results found".to_string()]),
        Ok(results) => ToolOutput::ok(NAME, search::format_search_results_with_content(&results)),
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
    fn parse_arguments_uses_default_limit() {
        let input = parse_arguments(r#"{"query":"rust"}"#).expect("parse");
        assert_eq!(input.query, "rust");
        assert_eq!(input.max_results, search::DEFAULT_SEARCH_LIMIT);
    }

    #[test]
    fn parse_arguments_accepts_max_results() {
        let input = parse_arguments(r#"{"query":"rust","max_results":2}"#).expect("parse");
        assert_eq!(input.max_results, 2);
    }

    #[test]
    fn registry_execute_search_failure_is_structured() {
        let request = ToolUseRequest::new(
            "web_search".to_string(),
            r#"{"query":"","max_results":1}"#.to_string(),
            "call_1".to_string(),
        );

        let output = tools::registry::execute(&request, &tools::registry::ToolContext::new(Path::new("."))).output;

        assert_eq!(output.name, NAME);
        assert!(
            matches!(output.status, ToolStatus::Ok | ToolStatus::Failed),
            "network-backed search should return a structured tool output"
        );
    }
}
