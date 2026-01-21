//! Task context tracking for the "WHY" field in teaching cards
//!
//! This module provides a system for tracking the current task or goal across
//! multiple tool calls. It extracts and maintains context from user messages and
//! model responses, making it available for display in action cards.
//!
//! The task context answers the question "WHY is this tool being called?" by
//! tracking the higher-level intent (e.g., "Fix authentication bug", "Add dark mode").

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Current task context being tracked
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskContext {
    /// The primary task or goal
    pub task: String,
    /// Additional context or sub-task
    pub subtask: Option<String>,
    /// Files or components being worked on
    pub focus: Option<String>,
    /// When this context was set
    pub updated_at: String,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(task: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        Self { task: task.into(), subtask: None, focus: None, updated_at: now }
    }

    /// Set the subtask
    pub fn with_subtask(mut self, subtask: impl Into<String>) -> Self {
        self.subtask = Some(subtask.into());
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Set the focus area
    pub fn with_focus(mut self, focus: impl Into<String>) -> Self {
        self.focus = Some(focus.into());
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Format as a brief description
    pub fn to_brief(&self) -> String {
        if let Some(ref subtask) = self.subtask {
            format!("{}: {}", self.task, subtask)
        } else {
            self.task.clone()
        }
    }

    /// Format as a detailed description
    pub fn to_detailed(&self) -> String {
        let mut parts = vec![self.task.clone()];
        if let Some(ref subtask) = self.subtask {
            parts.push(format!("Subtask: {}", subtask));
        }
        if let Some(ref focus) = self.focus {
            parts.push(format!("Working on: {}", focus));
        }
        parts.join(" | ")
    }
}

/// Tracker for task context across a session
#[derive(Debug, Clone)]
pub struct TaskContextTracker {
    /// Current task context
    context: Arc<RwLock<Option<TaskContext>>>,
}

impl TaskContextTracker {
    /// Create a new task context tracker
    pub fn new() -> Self {
        Self { context: Arc::new(RwLock::new(None)) }
    }

    /// Get the current task context
    pub fn get(&self) -> Option<TaskContext> {
        self.context.read().unwrap().as_ref().cloned()
    }

    /// Set the task context
    pub fn set(&self, context: TaskContext) {
        *self.context.write().unwrap() = Some(context);
    }

    /// Clear the task context
    pub fn clear(&self) {
        *self.context.write().unwrap() = None;
    }

    /// Update the task context from a user message
    ///
    /// This analyzes the user message to extract task intent and updates the context accordingly.
    pub fn update_from_user_message(&self, message: &str) {
        let extracted = extract_task_from_message(message);
        if let Some(task) = extracted {
            self.set(task);
        }
    }

    /// Update the task context from a model response
    ///
    /// This analyzes the model response to detect changes in task focus or subtasks.
    pub fn update_from_model_response(&self, response: &str) {
        if let Some(ref current) = self.get() {
            if indicates_completion(response) {
                self.clear();
            } else if let Some(subtask) = extract_subtask_from_response(response) {
                let updated = TaskContext {
                    task: current.task.clone(),
                    subtask: Some(subtask),
                    focus: current.focus.clone(),
                    updated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                };
                self.set(updated);
            }
        }
    }

    /// Get a brief description for display in action cards
    pub fn brief_description(&self) -> Option<String> {
        self.get().map(|ctx| ctx.to_brief())
    }

    /// Get a detailed description for display
    pub fn detailed_description(&self) -> Option<String> {
        self.get().map(|ctx| ctx.to_detailed())
    }
}

impl Default for TaskContextTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract task context from a user message
///
/// This analyzes the message for patterns that indicate task intent, such as "Fix X", "Add Y", "Implement Z", etc.
fn extract_task_from_message(message: &str) -> Option<TaskContext> {
    let message = message.trim();

    let action_patterns = [
        ("add", "Adding"),
        ("create", "Creating"),
        ("implement", "Implementing"),
        ("fix", "Fixing"),
        ("fix the", "Fixing"),
        ("fix a", "Fixing"),
        ("fix an", "Fixing"),
        ("resolve", "Resolving"),
        ("debug", "Debugging"),
        ("update", "Updating"),
        ("change", "Changing"),
        ("modify", "Modifying"),
        ("refactor", "Refactoring"),
        ("remove", "Removing"),
        ("delete", "Deleting"),
        ("test", "Testing"),
        ("write", "Writing"),
        ("build", "Building"),
        ("deploy", "Deploying"),
        ("install", "Installing"),
        ("setup", "Setting up"),
        ("configure", "Configuring"),
        ("optimize", "Optimizing"),
        ("improve", "Improving"),
        ("enhance", "Enhancing"),
        ("check", "Checking"),
        ("verify", "Verifying"),
        ("validate", "Validating"),
        ("search", "Searching"),
        ("find", "Finding"),
        ("locate", "Locating"),
        ("list", "Listing"),
        ("show", "Showing"),
        ("explain", "Explaining"),
        ("help", "Help"),
    ];

    let lower_message = message.to_lowercase();

    for (pattern, action) in action_patterns {
        if lower_message.starts_with(pattern) {
            let rest = &message[pattern.len()..].trim();
            if !rest.is_empty() {
                let task = extract_task_subject(rest);
                return Some(TaskContext::new(format!("{} {}", action, task)));
            }
        }
    }

    if lower_message.starts_with("how do i") || lower_message.starts_with("how can i") {
        let rest = &message[if lower_message.starts_with("how do i") { 9 } else { 10 }..].trim();
        if !rest.is_empty() {
            return Some(TaskContext::new(format!("Learn how to {}", rest)));
        }
    }

    if lower_message.starts_with("what is") || lower_message.starts_with("what's") {
        let rest = &message[if lower_message.starts_with("what is") { 8 } else { 7 }..].trim();
        if !rest.is_empty() {
            return Some(TaskContext::new(format!("Understand: {}", rest)));
        }
    }

    if lower_message == "continue" || lower_message.starts_with("continue") {
        return None;
    }

    if let Some(end) = message.find('.') {
        let first_sentence = &message[..end].trim();
        if first_sentence.len() > 3 && first_sentence.len() < 100 {
            return Some(TaskContext::new(first_sentence.to_string()));
        }
    }

    if message.len() < 80 { Some(TaskContext::new(message.to_string())) } else { None }
}

/// Extract the subject of a task from the rest of a sentence
fn extract_task_subject(text: &str) -> String {
    let text = text.trim();

    let text = text
        .strip_prefix("a ")
        .or_else(|| text.strip_prefix("an "))
        .or_else(|| text.strip_prefix("the "))
        .unwrap_or(text);

    if text.len() > 50 {
        match text.find(['.', ',', ';']) {
            Some(end) => text[..end].trim().to_string(),
            None => format!("{}...", &text[..47]),
        }
    } else {
        text.to_string()
    }
}

/// Check if a response indicates task completion
fn indicates_completion(response: &str) -> bool {
    let lower = response.to_lowercase();
    let completion_phrases = [
        "done",
        "complete",
        "finished",
        "that's all",
        "that is all",
        "task complete",
        "all done",
        "successfully implemented",
        "successfully added",
        "successfully fixed",
    ];

    completion_phrases.iter().any(|phrase| lower.contains(phrase))
}

/// Extract subtask information from a model response
fn extract_subtask_from_response(response: &str) -> Option<String> {
    let response = response.trim();

    let transition_patterns = [
        ("now i'll", "Now"),
        ("next, i'll", "Next"),
        ("let me", ""),
        ("first, i'll", "First"),
        ("then, i'll", "Then"),
        ("after that, i'll", "Then"),
    ];

    let lower_response = response.to_lowercase();

    for (pattern, prefix) in transition_patterns {
        if let Some(idx) = lower_response.find(pattern) {
            let start = idx + pattern.len();
            if start < response.len() {
                let rest = &response[start..].trim();
                if let Some(end) = rest.find(['.', '\n']) {
                    let action = &rest[..end].trim();
                    if !action.is_empty() {
                        return Some(if prefix.is_empty() {
                            action.to_string()
                        } else {
                            format!("{}: {}", prefix, action)
                        });
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_context_new() {
        let ctx = TaskContext::new("Fix authentication bug");
        assert_eq!(ctx.task, "Fix authentication bug");
        assert!(ctx.subtask.is_none());
        assert!(ctx.focus.is_none());
    }

    #[test]
    fn test_task_context_with_subtask() {
        let ctx = TaskContext::new("Fix authentication").with_subtask("Add JWT validation");
        assert_eq!(ctx.task, "Fix authentication");
        assert_eq!(ctx.subtask, Some("Add JWT validation".to_string()));
    }

    #[test]
    fn test_task_context_with_focus() {
        let ctx = TaskContext::new("Refactor code").with_focus("Authentication module");
        assert_eq!(ctx.focus, Some("Authentication module".to_string()));
    }

    #[test]
    fn test_task_context_to_brief() {
        let ctx = TaskContext::new("Add feature");
        assert_eq!(ctx.to_brief(), "Add feature");

        let ctx = TaskContext::new("Add feature").with_subtask("Create UI");
        assert_eq!(ctx.to_brief(), "Add feature: Create UI");
    }

    #[test]
    fn test_task_context_to_detailed() {
        let ctx = TaskContext::new("Add feature")
            .with_subtask("Create UI")
            .with_focus("Login screen");
        let detailed = ctx.to_detailed();
        assert!(detailed.contains("Add feature"));
        assert!(detailed.contains("Create UI"));
        assert!(detailed.contains("Login screen"));
    }

    #[test]
    fn test_task_context_tracker_new() {
        let tracker = TaskContextTracker::new();
        assert!(tracker.get().is_none());
        assert!(tracker.brief_description().is_none());
    }

    #[test]
    fn test_task_context_tracker_set() {
        let tracker = TaskContextTracker::new();
        let ctx = TaskContext::new("Test task");
        tracker.set(ctx.clone());

        let retrieved = tracker.get();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().task, "Test task");
    }

    #[test]
    fn test_task_context_tracker_clear() {
        let tracker = TaskContextTracker::new();
        tracker.set(TaskContext::new("Test task"));
        assert!(tracker.get().is_some());

        tracker.clear();
        assert!(tracker.get().is_none());
    }

    #[test]
    fn test_extract_task_add_feature() {
        let message = "Add dark mode to the application";
        let task = extract_task_from_message(message);
        assert!(task.is_some());
        assert_eq!(task.unwrap().task, "Adding dark mode to the application");
    }

    #[test]
    fn test_extract_task_fix_bug() {
        let message = "Fix authentication bug in login";
        let task = extract_task_from_message(message);
        assert!(task.is_some());
        assert!(task.unwrap().task.contains("Fixing"));
    }

    #[test]
    fn test_extract_task_implement() {
        let message = "Implement OAuth2 integration";
        let task = extract_task_from_message(message);
        assert!(task.is_some());
        assert!(task.unwrap().task.contains("Implementing"));
    }

    #[test]
    fn test_extract_task_how_to() {
        let message = "How do I add CORS support?";
        let task = extract_task_from_message(message);
        assert!(task.is_some());
        assert!(task.unwrap().task.contains("Learn how to"));
    }

    #[test]
    fn test_extract_task_continue() {
        let message = "continue";
        let task = extract_task_from_message(message);
        assert!(task.is_none());
    }

    #[test]
    fn test_extract_task_question() {
        let message = "What is the best way to handle errors?";
        let task = extract_task_from_message(message);
        assert!(task.is_some());
        assert!(task.unwrap().task.contains("Understand"));
    }

    #[test]
    fn test_extract_task_subject_articles() {
        assert_eq!(extract_task_subject("a new feature"), "new feature");
        assert_eq!(extract_task_subject("the database connection"), "database connection");
        assert_eq!(extract_task_subject("an API endpoint"), "API endpoint");
        assert_eq!(extract_task_subject("user authentication"), "user authentication");
    }

    #[test]
    fn test_extract_task_subject_truncation() {
        let long = "a very long description of something that should be truncated";
        let result = extract_task_subject(long);
        assert!(result.len() <= 53);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_indicates_completion() {
        assert!(indicates_completion("I'm done with that task."));
        assert!(indicates_completion("All complete!"));
        assert!(indicates_completion("Successfully implemented the feature."));
        assert!(!indicates_completion("Now I'll move on to the next step."));
        assert!(!indicates_completion("Let me check something."));
    }

    #[test]
    fn test_extract_subtask_from_response() {
        let response = "Now I'll update the configuration file.";
        let subtask = extract_subtask_from_response(response);
        assert!(subtask.is_some());
        assert!(subtask.unwrap().contains("update"));
    }

    #[test]
    fn test_tracker_update_from_user_message() {
        let tracker = TaskContextTracker::new();
        tracker.update_from_user_message("Add user authentication");

        let ctx = tracker.get();
        assert!(ctx.is_some());
        assert!(ctx.unwrap().task.contains("Adding"));
    }

    #[test]
    fn test_tracker_update_from_model_response() {
        let tracker = TaskContextTracker::new();
        tracker.set(TaskContext::new("Implement feature"));

        tracker.update_from_model_response("Now I'll add the tests.");
        let ctx = tracker.get();
        assert!(ctx.is_some());

        tracker.update_from_model_response("Done! The feature is complete.");
        assert!(tracker.get().is_none());
    }

    #[test]
    fn test_tracker_brief_description() {
        let tracker = TaskContextTracker::new();
        assert!(tracker.brief_description().is_none());

        tracker.set(TaskContext::new("Test task"));
        assert_eq!(tracker.brief_description(), Some("Test task".to_string()));

        tracker.set(TaskContext::new("Test").with_subtask("Subtask"));
        assert!(tracker.brief_description().unwrap().contains("Subtask"));
    }

    #[test]
    fn test_task_context_serialization() {
        let ctx = TaskContext::new("Test task")
            .with_subtask("Subtask")
            .with_focus("File.rs");
        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: TaskContext = serde_json::from_str(&json).unwrap();

        assert_eq!(ctx.task, deserialized.task);
        assert_eq!(ctx.subtask, deserialized.subtask);
        assert_eq!(ctx.focus, deserialized.focus);
    }
}
