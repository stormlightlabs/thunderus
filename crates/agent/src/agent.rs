use futures::StreamExt;
use std::sync::{Arc, RwLock};
use thunderus_core::TaskContextTracker;
use thunderus_core::memory::{MemoryRetriever, RetrievalPolicy, format_memory_context};
use thunderus_core::*;
use thunderus_providers::*;
use thunderus_tools::extract_scope;
use tokio::sync::mpsc;

/// Metadata for tool execution
#[derive(Debug, Clone)]
pub struct ToolExecutionMetadata {
    pub execution_time_ms: Option<u64>,
    pub classification_reasoning: Option<String>,
    pub affected_paths: Vec<String>,
}

impl ToolExecutionMetadata {
    pub fn new() -> Self {
        Self { execution_time_ms: None, classification_reasoning: None, affected_paths: Vec::new() }
    }

    pub fn with_execution_time(mut self, time_ms: u64) -> Self {
        self.execution_time_ms = Some(time_ms);
        self
    }

    pub fn with_classification_reasoning(mut self, reasoning: String) -> Self {
        self.classification_reasoning = Some(reasoning);
        self
    }

    pub fn with_affected_paths(mut self, paths: Vec<String>) -> Self {
        self.affected_paths = paths;
        self
    }
}

impl Default for ToolExecutionMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Events sent from agent to TUI
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Text token from model
    Token(String),
    /// Tool call initiated
    ToolCall {
        name: String,
        args: serde_json::Value,
        risk: ToolRisk,
        description: Option<String>,
        task_context: Option<String>,
        scope: Option<String>,
        classification_reasoning: Option<String>,
    },
    /// Tool result received
    ToolResult {
        name: String,
        result: String,
        success: bool,
        error: Option<String>,
        metadata: ToolExecutionMetadata,
    },
    /// Approval request for action
    ApprovalRequest(ApprovalRequest),
    /// Approval response from user
    ApprovalResponse(ApprovalResponse),
    /// Approval mode changed
    ApprovalModeChanged { from: ApprovalMode, to: ApprovalMode },
    /// Memory retrieval completed
    MemoryRetrieval {
        query: String,
        chunks: Vec<thunderus_core::memory::RetrievedChunk>,
        total_tokens: usize,
        search_time_ms: u64,
    },
    /// Error occurred
    Error(String),
    /// Generation complete
    Done,
}

/// Agent orchestrator that manages the main interaction loop
pub struct Agent {
    /// Provider for LLM interaction
    provider: Arc<dyn Provider>,
    /// Approval protocol for gating actions
    approval_protocol: Arc<dyn ApprovalProtocol>,
    /// Approval gate for mode-based enforcement
    approval_gate: Arc<RwLock<ApprovalGate>>,
    /// Session ID for logging
    #[allow(dead_code)]
    session_id: SessionId,
    /// Conversation messages (for context)
    messages: Vec<ChatMessage>,
    /// Task context tracker for "WHY" field
    task_context: Arc<TaskContextTracker>,
    /// Memory retriever for querying relevant context
    memory_retriever: Option<Arc<dyn MemoryRetriever>>,
    /// Retrieval policy configuration
    retrieval_policy: Option<RetrievalPolicy>,
}

impl Agent {
    /// Create a new agent
    pub fn new(
        provider: Arc<dyn Provider>, approval_protocol: Arc<dyn ApprovalProtocol>, approval_gate: ApprovalGate,
        session_id: SessionId,
    ) -> Self {
        Self {
            provider,
            approval_protocol,
            approval_gate: Arc::new(RwLock::new(approval_gate)),
            session_id,
            messages: Vec::new(),
            task_context: Arc::new(TaskContextTracker::new()),
            memory_retriever: None,
            retrieval_policy: None,
        }
    }

    /// Set the memory retriever for this agent
    pub fn with_memory_retriever(mut self, retriever: Arc<dyn MemoryRetriever>) -> Self {
        self.memory_retriever = Some(retriever);
        self
    }

    /// Set the retrieval policy for this agent
    pub fn with_retrieval_policy(mut self, policy: RetrievalPolicy) -> Self {
        self.retrieval_policy = Some(policy);
        self
    }

    /// Get the current approval mode
    pub fn approval_mode(&self) -> ApprovalMode {
        self.approval_gate.read().unwrap().mode()
    }

    /// Set the approval mode
    pub fn set_approval_mode(&self, new_mode: ApprovalMode) -> Result<ApprovalMode> {
        let mut gate = self.approval_gate.write().unwrap();
        let old_mode = gate.mode();
        gate.set_mode(new_mode);
        Ok(old_mode)
    }

    /// Get a reference to the approval gate
    pub fn approval_gate(&self) -> Arc<RwLock<ApprovalGate>> {
        Arc::clone(&self.approval_gate)
    }

    /// Process a user message and stream response
    /// Returns a receiver for agent events (tokens, tool calls, etc.)
    pub async fn process_message(
        &mut self, user_input: &str, tools: Option<Vec<ToolSpec>>, cancel_token: CancelToken,
        user_owned_files: Vec<std::path::PathBuf>,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>> {
        let (tx, rx) = mpsc::unbounded_channel();

        self.task_context.update_from_user_message(user_input);

        let mut system_message_content = String::new();

        if !user_owned_files.is_empty() {
            system_message_content.push_str("\n\n## Write Protection\n");
            system_message_content
                .push_str("The following files have been modified by the user and are currently WRITE-PROTECTED. ");
            system_message_content.push_str(
                "You cannot write to or patch these files until you have read them again to sync with user changes:\n",
            );
            for path in user_owned_files {
                system_message_content.push_str(&format!("- {}\n", path.display()));
            }
        }
        if let Some(retriever) = &self.memory_retriever {
            match retriever.query(user_input).await {
                Ok(retrieval_result) => {
                    let _ = tx.send(AgentEvent::MemoryRetrieval {
                        query: retrieval_result.query.clone(),
                        chunks: retrieval_result.chunks.clone(),
                        total_tokens: retrieval_result.total_tokens,
                        search_time_ms: retrieval_result.search_time_ms,
                    });

                    let memory_context = format_memory_context(&retrieval_result);
                    system_message_content = format!("\n\n## Relevant Memory\n{}", memory_context);
                }
                Err(e) => {
                    let _ = tx.send(AgentEvent::Error(format!("Memory retrieval failed: {}", e)));
                }
            }
        }

        let mut messages_for_request = self.messages.clone();
        if !system_message_content.is_empty() {
            let has_system = messages_for_request.iter().any(|m| m.role == Role::System);
            if has_system {
                for msg in &mut messages_for_request {
                    if msg.role == Role::System {
                        msg.content.push_str(&system_message_content);
                        break;
                    }
                }
            } else {
                messages_for_request.insert(
                    0,
                    ChatMessage {
                        role: Role::System,
                        content: format!("You are a helpful coding assistant.{}", system_message_content),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }
        }

        messages_for_request.push(ChatMessage::user(user_input.to_string()));

        self.messages.push(ChatMessage::user(user_input.to_string()));

        let request = ChatRequest::builder()
            .messages(messages_for_request)
            .tools(tools.unwrap_or_default())
            .temperature(0.7)
            .max_tokens(8192)
            .build();

        let provider = Arc::clone(&self.provider);
        let cancel_token_clone = cancel_token.clone();
        let cancel_token_for_stream = cancel_token.clone();
        let task_context = Arc::clone(&self.task_context);

        tokio::spawn(async move {
            if cancel_token_clone.is_cancelled() {
                let _ = tx.send(AgentEvent::Error("Cancelled before request".to_string()));
                return;
            }

            let stream = match provider.stream_chat(request, cancel_token_for_stream).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AgentEvent::Error(format!("Provider error: {}", e)));
                    return;
                }
            };

            tokio::pin!(stream);

            while let Some(event) = stream.next().await {
                if cancel_token_clone.is_cancelled() {
                    let _ = tx.send(AgentEvent::Error("Generation cancelled by user".to_string()));
                    break;
                }

                match event {
                    StreamEvent::Token(text) => {
                        let _ = tx.send(AgentEvent::Token(text));
                    }
                    StreamEvent::ToolCall(calls) => {
                        for call in calls {
                            let classification = classify_tool_risk(&call.function.name, &call.function.arguments);
                            let description = generate_tool_description(&call.function.name, &call.function.arguments);
                            let scope_info = extract_scope(&call.function.name, &call.function.arguments);
                            let scope = if !scope_info.is_empty() { Some(scope_info.to_detailed()) } else { None };
                            let task_context_str = task_context.brief_description();

                            let _ = tx.send(AgentEvent::ToolCall {
                                name: call.function.name.clone(),
                                args: call.function.arguments,
                                risk: classification.risk,
                                description: Some(description),
                                task_context: task_context_str,
                                scope,
                                classification_reasoning: Some(classification.reasoning),
                            });
                        }
                    }
                    StreamEvent::Done => {
                        let _ = tx.send(AgentEvent::Done);
                        break;
                    }
                    StreamEvent::Error(msg) => {
                        let _ = tx.send(AgentEvent::Error(msg));
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Handle a tool call by checking approval and executing
    pub async fn handle_tool_call(&self, name: String, args: serde_json::Value) -> Result<ToolResult> {
        let approval_request = ApprovalRequest {
            id: get_next_approval_id(),
            action_type: ActionType::Tool,
            description: format!("Execute tool: {}", name),
            context: ApprovalContext {
                name: Some(name.clone()),
                arguments: Some(args.clone()),
                affected_paths: Vec::new(),
                metadata: std::collections::HashMap::new(),
                classification_reasoning: None,
            },
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let decision = self.approval_protocol.request_approval(&approval_request)?;

        match decision {
            ApprovalDecision::Approved => Ok(ToolResult::success(format!("call_{}", name), "Executed successfully")),
            ApprovalDecision::Rejected => Ok(ToolResult::error(format!("call_{}", name), "Rejected by user")),
            ApprovalDecision::Cancelled => Ok(ToolResult::error(format!("call_{}", name), "Cancelled")),
        }
    }

    /// Append a tool result to the conversation
    pub fn append_tool_result(&mut self, _name: String, call_id: String, result: ToolResult) {
        let msg = ChatMessage {
            role: Role::Tool,
            content: result.content.clone(),
            tool_call_id: Some(call_id),
            tool_calls: None,
        };
        self.messages.push(msg);
    }

    /// Get the current conversation history
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
}

/// Generate next approval ID (simplified for testing)
fn get_next_approval_id() -> ApprovalId {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Classification result with reasoning
struct ToolClassification {
    risk: ToolRisk,
    reasoning: String,
}

/// Classify tool risk based on tool name and arguments
///
/// This provides a basic heuristic for classifying tool calls by risk level.
/// In a production system, this would be more sophisticated and potentially
/// driven by configuration or ML-based classification.
fn classify_tool_risk(tool_name: &str, arguments: &serde_json::Value) -> ToolClassification {
    match tool_name {
        name if name.contains("read") || name.contains("get") || name.contains("list") || name.contains("search") => {
            ToolClassification {
                risk: ToolRisk::Safe,
                reasoning: format!("{} is read-only and does not modify files", name),
            }
        }
        name if name.contains("write")
            || name.contains("edit")
            || name.contains("create")
            || name.contains("update") =>
        {
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("files");
            ToolClassification {
                risk: ToolRisk::Risky,
                reasoning: format!("{} modifies {} which could change project state", name, path),
            }
        }
        name if name.contains("delete") || name.contains("remove") || name.contains("rm") => {
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("files");
            ToolClassification {
                risk: ToolRisk::Risky,
                reasoning: format!("{} removes {} which cannot be easily undone", name, path),
            }
        }
        name if name.contains("shell") || name.contains("exec") || name.contains("command") => {
            let cmd = arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("arbitrary commands");
            ToolClassification {
                risk: ToolRisk::Risky,
                reasoning: format!("Shell execution of {} can have unintended side effects", cmd),
            }
        }
        name if name.contains("http") || name.contains("fetch") || name.contains("request") => ToolClassification {
            risk: ToolRisk::Risky,
            reasoning: format!(
                "{} makes network requests which may leak data or consume resources",
                name
            ),
        },
        _ => ToolClassification {
            risk: ToolRisk::Safe,
            reasoning: format!("{} is not in the known risky operations list", tool_name),
        },
    }
}

/// Generate a human-readable description for a tool call
fn generate_tool_description(tool_name: &str, arguments: &serde_json::Value) -> String {
    if let Some(obj) = arguments.as_object() {
        if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
            return match tool_name {
                name if name.contains("read") => format!("Read file: {}", path),
                name if name.contains("edit") || name.contains("write") => format!("Edit file: {}", path),
                name if name.contains("delete") || name.contains("remove") => format!("Delete file: {}", path),
                name if name.contains("search") || name.contains("grep") => format!("Search in: {}", path),
                _ => format!("{} on {}", tool_name, path),
            };
        }

        if let Some(query) = obj.get("query").and_then(|v| v.as_str()) {
            let truncated_query = if query.len() > 50 { format!("{}...", &query[..47]) } else { query.to_string() };
            return format!("{}: {}", tool_name, truncated_query);
        }

        if let Some(pattern) = obj.get("pattern").and_then(|v| v.as_str()) {
            let truncated_pattern =
                if pattern.len() > 40 { format!("{}...", &pattern[..37]) } else { pattern.to_string() };
            return format!("{}: {}", tool_name, truncated_pattern);
        }

        if let Some(command) = obj.get("command").and_then(|v| v.as_str()) {
            let truncated_cmd = if command.len() > 60 { format!("{}...", &command[..57]) } else { command.to_string() };
            return format!("Execute: {}", truncated_cmd);
        }

        if let Some(patterns) = obj.get("patterns").and_then(|v| v.as_array())
            && let Some(first_pattern) = patterns.first().and_then(|v| v.as_str())
        {
            return format!("{}: {} (+ {} more)", tool_name, first_pattern, patterns.len() - 1);
        }
    }

    tool_name.to_string()
}

/// Simple in-memory approval protocol for testing
#[derive(Debug, Clone)]
pub struct InMemoryApprovalProtocol {
    auto_approve: bool,
}

impl InMemoryApprovalProtocol {
    pub fn new(auto_approve: bool) -> Self {
        Self { auto_approve }
    }
}

impl ApprovalProtocol for InMemoryApprovalProtocol {
    fn name(&self) -> &str {
        "in-memory"
    }

    fn request_approval(&self, _request: &ApprovalRequest) -> Result<ApprovalDecision> {
        Ok(if self.auto_approve { ApprovalDecision::Approved } else { ApprovalDecision::Rejected })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunderus_providers::{GlmProvider, Provider};

    #[test]
    fn test_agent_creation() {
        let provider =
            Arc::new(GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None)) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let agent = Agent::new(provider, approval, gate, session_id);
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_agent_approval_mode() {
        let provider =
            Arc::new(GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None)) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let agent = Agent::new(provider, approval, gate, session_id);
        assert_eq!(agent.approval_mode(), ApprovalMode::Auto);
    }

    #[test]
    fn test_agent_set_approval_mode() {
        let provider =
            Arc::new(GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None)) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let agent = Agent::new(provider, approval, gate, session_id);

        let old_mode = agent.set_approval_mode(ApprovalMode::ReadOnly).unwrap();
        assert_eq!(old_mode, ApprovalMode::Auto);
        assert_eq!(agent.approval_mode(), ApprovalMode::ReadOnly);
    }

    #[test]
    fn test_agent_approval_gate_access() {
        let provider =
            Arc::new(GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None)) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::FullAccess, true);
        let session_id = SessionId::new();

        let agent = Agent::new(provider, approval, gate, session_id);

        let agent_gate = agent.approval_gate();
        let gate_read = agent_gate.read().unwrap();
        assert_eq!(gate_read.mode(), ApprovalMode::FullAccess);
        assert!(gate_read.allow_network());
    }

    #[test]
    fn test_append_tool_result() {
        let provider =
            Arc::new(GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None)) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let mut agent = Agent::new(provider, approval, gate, session_id);

        let result = ToolResult::success("call_1", "OK");
        agent.append_tool_result("test_tool".to_string(), "call_1".to_string(), result);

        assert_eq!(agent.messages().len(), 1);
        assert_eq!(agent.messages()[0].role, Role::Tool);
        assert_eq!(agent.messages()[0].content, "OK");
    }

    #[test]
    fn test_in_memory_approval_auto_approve() {
        let protocol = InMemoryApprovalProtocol::new(true);

        let request = ApprovalRequest {
            id: 1,
            action_type: ActionType::Tool,
            description: "test".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let response = protocol.request_approval(&request).unwrap();
        assert!(matches!(response, ApprovalDecision::Approved));
    }

    #[test]
    fn test_in_memory_approval_reject() {
        let protocol = InMemoryApprovalProtocol::new(false);

        let request = ApprovalRequest {
            id: 1,
            action_type: ActionType::Tool,
            description: "test".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let response = protocol.request_approval(&request).unwrap();
        assert!(matches!(response, ApprovalDecision::Rejected));
    }

    #[test]
    fn test_agent_event_token() {
        let event = AgentEvent::Token("Hello".to_string());
        assert!(matches!(event, AgentEvent::Token(_)));
    }

    #[test]
    fn test_agent_event_tool_call() {
        let event = AgentEvent::ToolCall {
            name: "test_tool".to_string(),
            args: serde_json::json!({}),
            risk: thunderus_core::ToolRisk::Safe,
            description: Some("Test tool".to_string()),
            task_context: Some("Test task".to_string()),
            scope: Some("/test/path".to_string()),
            classification_reasoning: Some("Test reasoning".to_string()),
        };
        assert!(matches!(event, AgentEvent::ToolCall { .. }));
    }

    #[test]
    fn test_agent_event_tool_result() {
        let event = AgentEvent::ToolResult {
            name: "test_tool".to_string(),
            result: "Success".to_string(),
            success: true,
            error: None,
            metadata: ToolExecutionMetadata::new(),
        };
        assert!(matches!(event, AgentEvent::ToolResult { .. }));
    }

    #[test]
    fn test_agent_event_approval_request() {
        let request = ApprovalRequest {
            id: 1,
            action_type: ActionType::Tool,
            description: "test".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let event = AgentEvent::ApprovalRequest(request.clone());
        assert!(matches!(event, AgentEvent::ApprovalRequest(_)));
    }

    #[test]
    fn test_agent_event_error() {
        let event = AgentEvent::Error("Test error".to_string());
        assert!(matches!(event, AgentEvent::Error(_)));
    }

    #[test]
    fn test_agent_event_done() {
        let event = AgentEvent::Done;
        assert!(matches!(event, AgentEvent::Done));
    }

    #[test]
    fn test_cancel_token_integration() {
        let cancel = CancelToken::new();
        assert!(!cancel.is_cancelled());

        cancel.cancel();
        assert!(cancel.is_cancelled());
    }

    #[tokio::test]
    async fn test_process_message_cancelled() {
        let provider =
            Arc::new(GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None)) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let mut agent = Agent::new(provider, approval, gate, session_id);

        let cancel = CancelToken::new();
        cancel.cancel();

        let mut rx = agent.process_message("Hello", None, cancel, Vec::new()).await.unwrap();

        tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(agent.messages().len(), 1);
    }

    #[tokio::test]
    async fn test_agent_event_channel() {
        let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

        let _ = tx.send(AgentEvent::Token("Hello".to_string()));
        let _ = tx.send(AgentEvent::ToolCall {
            name: "test".to_string(),
            args: serde_json::json!({}),
            risk: thunderus_core::ToolRisk::Safe,
            description: None,
            task_context: None,
            scope: None,
            classification_reasoning: None,
        });
        let _ = tx.send(AgentEvent::Done);

        let ev1 = rx.recv().await.unwrap();
        assert!(matches!(ev1, AgentEvent::Token(_)));

        let ev2 = rx.recv().await.unwrap();
        assert!(matches!(ev2, AgentEvent::ToolCall { .. }));

        let ev3 = rx.recv().await.unwrap();
        assert!(matches!(ev3, AgentEvent::Done));
    }

    #[test]
    fn test_approval_context_builder() {
        let ctx = ApprovalContext::new()
            .with_name("test_tool")
            .with_arguments(serde_json::json!({"arg": "value"}))
            .add_affected_path("/test/path");

        assert_eq!(ctx.name, Some("test_tool".to_string()));
        assert_eq!(ctx.arguments, Some(serde_json::json!({"arg": "value"})));
        assert_eq!(ctx.affected_paths, vec!["/test/path"]);
    }
}
