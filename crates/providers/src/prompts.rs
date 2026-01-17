//! System prompt guidance for LLM providers
//!
//! This module provides system prompt templates and guidance to help models
//! use tools correctly and safely. The prompts emphasize the coding agent
//! workflow and safety practices.

/// Base system prompt for the coding agent
///
/// This prompt establishes the agent's role and core working principles.
/// It should be combined with tool-specific guidance.
pub fn base_system_prompt() -> &'static str {
    "You are Thunderus, a coding agent built to help users work with codebases \
    effectively and safely. You have access to tools for searching, reading, \
    and editing files. Always prioritize safety and clarity in your actions."
}

/// Tool usage guidance for the agent
///
/// This prompt provides specific instructions on how to use tools safely
/// and effectively. It emphasizes the "Read before Edit" pattern and
/// safer alternatives to shell commands.
pub fn tool_usage_guidance() -> &'static str {
    "## Tool Usage Guidelines

### Search Tools
- **Use Grep for code search, not bash grep**: The `grep` tool provides \
  structured, parseable output with file filtering and context control.
- **Use Glob for file discovery**: The `glob` tool finds files by pattern \
  with .gitignore awareness.

### Read Before Edit
- **Always Read before Edit**: You MUST use the `read` tool to examine a \
  file's contents before using `edit` or `multiedit` on it. This ensures \
  you understand the file's structure and can make precise edits.
- The read history is tracked, and Edit operations require a prior Read in \
  the current session.

### Safe Editing
- **Prefer Edit over sed for safety**: The `edit` tool provides safe \
  find-replace operations with validation. Use `edit` instead of shell `sed` \
  commands.
- Use `multiedit` for applying multiple related changes atomically.
- Edit operations require exact string matches; ambiguous patterns will fail \
  safely rather than silently corrupting files.

### Shell Commands
- Shell commands via the `shell` tool are subject to approval and sandbox \
  policies.
- Prefer specialized tools (Grep, Glob, Read, Edit) over shell equivalents \
  whenever possible.

### Error Messages
- Tool errors include teaching context. Read error messages carefully as \
  they explain what went wrong and how to fix it."
}

/// Complete system prompt combining all guidance
///
/// Returns the full system prompt that should be sent to the model at
/// the start of each conversation. This combines the base agent role
/// with tool-specific guidance.
pub fn system_prompt() -> String {
    format!("{}\n\n{}", base_system_prompt(), tool_usage_guidance())
}

/// Result formatting guidance for tool outputs
///
/// Provides instructions on how to format tool results when returning
/// them to the user. This helps maintain consistency and clarity.
pub fn result_formatting_guidance() -> &'static str {
    "## Result Formatting

When reporting tool results:
- For search tools: Summarize findings and highlight relevant matches
- For read tools: Reference key sections by line number
- For edit tools: Describe what changed and why
- For errors: Explain the error and suggest next steps

Always provide context around tool results so the user understands the \
impact and can verify the changes."
}

/// Teaching-focused error messages
///
/// Returns error messages that include pedagogical context. These
/// messages help users understand why an operation failed and how
/// to use the system correctly.
pub fn teaching_error_messages() -> &'static str {
    "## Common Error Patterns

### Edit Failed: old_string not unique
This error occurs when the text you're trying to replace appears multiple \
times in the file. The Edit tool requires exact, unique matches to prevent \
accidental changes.

**Solution**: Include more surrounding context in `old_string` to make \
the match unique, or use `read` to examine the file and identify the \
specific occurrence you want to edit.

### Read required before Edit
The Edit tool tracks read history to ensure you've seen the file contents \
before making changes. This safety check prevents blind edits.

**Solution**: Use the `read` tool on the file first, then retry the edit \
operation.

### Shell command blocked by approval policy
Shell commands are gated by approval modes and sandbox policies for safety.

**Solution**: Use specialized tools (Grep, Glob, Read, Edit) when possible, \
or wait for user approval if the command requires it."
}

/// Provider-specific prompt adaptations
///
/// Returns any prompt modifications needed for specific providers.
/// Currently, GLM-4.7 and Gemini use the same base prompts, but
/// this function allows for future differentiation.
pub fn provider_prompt_adaptation(provider_type: ProviderType) -> Option<String> {
    match provider_type {
        ProviderType::Glm => Some(
            "You are using the GLM-4.7 model. You have access to thinking \
            mode for complex reasoning. Use it to break down multi-step \
            tasks before executing them."
                .to_string(),
        ),
        ProviderType::Gemini => Some(
            "You are using the Gemini model. You have native function calling \
            support. Call tools directly when needed rather than explaining \
            what you would do."
                .to_string(),
        ),
    }
}

/// Supported provider types for prompt customization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    Glm,
    Gemini,
}

/// Build a complete system prompt for a specific provider
///
/// Combines the base system prompt, tool usage guidance, and any
/// provider-specific adaptations into a single system message.
pub fn build_system_prompt_for_provider(provider_type: ProviderType) -> String {
    let base = system_prompt();
    let result_fmt = result_formatting_guidance();
    let teaching = teaching_error_messages();

    if let Some(adaptation) = provider_prompt_adaptation(provider_type) {
        format!("{}\n\n{}\n\n{}\n\n{}", base, result_fmt, teaching, adaptation)
    } else {
        format!("{}\n\n{}\n\n{}", base, result_fmt, teaching)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_system_prompt_not_empty() {
        let prompt = base_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("Thunderus"));
    }

    #[test]
    fn test_tool_usage_guidance_contains_key_rules() {
        let guidance = tool_usage_guidance();
        assert!(guidance.contains("Grep for code search"));
        assert!(guidance.contains("Read before Edit"));
        assert!(guidance.contains("Edit over sed"));
    }

    #[test]
    fn test_system_prompt_combines_components() {
        let prompt = system_prompt();
        assert!(prompt.contains("Thunderus"));
        assert!(prompt.contains("Tool Usage Guidelines"));
    }

    #[test]
    fn test_teaching_error_messages_cover_common_cases() {
        let errors = teaching_error_messages();
        assert!(errors.contains("old_string not unique"));
        assert!(errors.contains("Read required before Edit"));
        assert!(errors.contains("approval policy"));
    }

    #[test]
    fn test_provider_adaptations_differ() {
        let glm_adaptation = provider_prompt_adaptation(ProviderType::Glm);
        let gemini_adaptation = provider_prompt_adaptation(ProviderType::Gemini);

        assert!(glm_adaptation.is_some());
        assert!(gemini_adaptation.is_some());
        assert_ne!(glm_adaptation, gemini_adaptation);
    }

    #[test]
    fn test_build_system_prompt_for_provider() {
        let glm_prompt = build_system_prompt_for_provider(ProviderType::Glm);
        let gemini_prompt = build_system_prompt_for_provider(ProviderType::Gemini);

        assert!(glm_prompt.contains("Thunderus"));
        assert!(gemini_prompt.contains("Thunderus"));

        assert!(glm_prompt.contains("Grep for code search"));
        assert!(gemini_prompt.contains("Grep for code search"));

        assert!(glm_prompt.contains("GLM-4.7"));
        assert!(gemini_prompt.contains("Gemini"));
        assert!(!glm_prompt.contains("Gemini"));
        assert!(!gemini_prompt.contains("GLM-4.7"));
    }

    #[test]
    fn test_result_formatting_guidance() {
        let guidance = result_formatting_guidance();
        assert!(!guidance.is_empty());
        let guidance_lower = guidance.to_lowercase();
        assert!(guidance_lower.contains("summarize"));
        assert!(guidance_lower.contains("line number"));
    }
}
