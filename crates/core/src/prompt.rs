//! System prompt assembly and response parsing
//!
//! Combines PROMPT.txt and RESPONSE.txt into a system message,
//! and parses model responses into structured sections.

use super::Message;

const DEFAULT_PROMPT: &str = include_str!("../../../meta/PROMPT.txt");
const DEFAULT_RESPONSE_FORMAT: &str = include_str!("../../../meta/RESPONSE.txt");
const DEFAULT_TOOLS: &str = include_str!("../../../meta/TOOLS.txt");

pub fn build_system_prompt() -> String {
    format!(
        "{}\n\n## Response Format\n\n{}\n\n## Available Tools\n\n{}",
        DEFAULT_PROMPT.trim(),
        DEFAULT_RESPONSE_FORMAT.trim(),
        DEFAULT_TOOLS.trim()
    )
}

pub fn build_system_message() -> Message {
    Message::system(build_system_prompt())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResponseSections {
    pub intent: Option<String>,
    pub actions: Option<String>,
    pub result: Option<String>,
    pub next: Option<String>,
}

impl ResponseSections {
    pub fn parse(content: &str) -> Self {
        let mut sections = Self::default();
        let mut current_section: Option<SectionName> = None;
        let mut current_content = String::new();

        for line in content.lines() {
            if let Some(next_section) = parse_section_heading(line) {
                if let Some(section_name) = current_section.take() {
                    sections.set_section(section_name, current_content.trim().to_string());
                }
                current_content = String::new();
                current_section = Some(next_section);
            } else if current_section.is_some() {
                if !current_content.is_empty() {
                    current_content.push('\n');
                }
                current_content.push_str(line);
            }
        }

        if let Some(section_name) = current_section {
            sections.set_section(section_name, current_content.trim().to_string());
        }

        sections
    }

    pub fn has_content(&self) -> bool {
        self.intent.is_some() || self.actions.is_some() || self.result.is_some() || self.next.is_some()
    }

    fn set_section(&mut self, section: SectionName, value: String) {
        match section {
            SectionName::Intent => self.intent = Some(value),
            SectionName::Actions => self.actions = Some(value),
            SectionName::Result => self.result = Some(value),
            SectionName::Next => self.next = Some(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionName {
    Intent,
    Actions,
    Result,
    Next,
}

fn parse_section_heading(line: &str) -> Option<SectionName> {
    let mut heading = line.trim();

    if heading.starts_with('#') {
        heading = heading.trim_start_matches('#').trim();
    }

    heading = heading.trim_end_matches(':').trim();
    heading = heading.trim_matches('*').trim();
    heading = heading.trim_matches('_').trim();
    heading = heading.trim_matches('`').trim();

    match heading.to_ascii_lowercase().as_str() {
        "intent" => Some(SectionName::Intent),
        "actions" => Some(SectionName::Actions),
        "result" => Some(SectionName::Result),
        "next" => Some(SectionName::Next),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("coding assistant"));
        assert!(prompt.contains("Response Format"));
        assert!(prompt.contains("Intent"));
        assert!(prompt.contains("Actions"));
        assert!(prompt.contains("Result"));
        assert!(prompt.contains("Next"));
        assert!(prompt.contains("Available Tools"));
        assert!(prompt.contains("read"));
        assert!(prompt.contains("write"));
        assert!(prompt.contains("edit"));
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("research"));
    }

    #[test]
    fn test_parse_response_sections() {
        let response = r#"Intent

Refactor the auth module to use middleware pattern.

Actions

- read src/auth.js
- edit src/auth.js

Result

Successfully refactored the authentication module.

Next

Run tests to verify the changes."#;

        let sections = ResponseSections::parse(response);

        assert_eq!(
            sections.intent,
            Some("Refactor the auth module to use middleware pattern.".to_string())
        );
        assert_eq!(
            sections.actions,
            Some("- read src/auth.js\n- edit src/auth.js".to_string())
        );
        assert_eq!(
            sections.result,
            Some("Successfully refactored the authentication module.".to_string())
        );
        assert_eq!(sections.next, Some("Run tests to verify the changes.".to_string()));
    }

    #[test]
    fn test_parse_partial_sections() {
        let response = r#"Intent

Do something.

Result

Done."#;

        let sections = ResponseSections::parse(response);

        assert_eq!(sections.intent, Some("Do something.".to_string()));
        assert!(sections.actions.is_none());
        assert_eq!(sections.result, Some("Done.".to_string()));
        assert!(sections.next.is_none());
    }

    #[test]
    fn test_response_sections_has_content() {
        let empty = ResponseSections::default();
        assert!(!empty.has_content());

        let with_content = ResponseSections { intent: Some("test".to_string()), ..Default::default() };
        assert!(with_content.has_content());
    }

    #[test]
    fn test_parse_markdown_style_headings() {
        let response = r#"## **Intent**:

Use provider-backed conversation in the TUI.

### **Actions**:

- create provider from config
- stream the response in chat

**Result**

Conversation now calls the configured provider.

`Next`

Done"#;

        let sections = ResponseSections::parse(response);

        assert_eq!(
            sections.intent,
            Some("Use provider-backed conversation in the TUI.".to_string())
        );
        assert_eq!(
            sections.actions,
            Some("- create provider from config\n- stream the response in chat".to_string())
        );
        assert_eq!(
            sections.result,
            Some("Conversation now calls the configured provider.".to_string())
        );
        assert_eq!(sections.next, Some("Done".to_string()));
    }
}
