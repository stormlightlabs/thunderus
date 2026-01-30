use futures::StreamExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use thunderus_core::TaskContextTracker;
use thunderus_core::memory::{MemoryRetriever, RetrievalPolicy, format_memory_context};
use thunderus_core::*;
use thunderus_providers::*;
use thunderus_tools::{SessionToolDispatcher, classify_shell_command_risk, extract_scope};
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
    messages: Arc<Mutex<Vec<ChatMessage>>>,
    /// Task context tracker for "WHY" field
    task_context: Arc<TaskContextTracker>,
    /// Memory retriever for querying relevant context
    memory_retriever: Option<Arc<dyn MemoryRetriever>>,
    /// Retrieval policy configuration
    retrieval_policy: Option<RetrievalPolicy>,
    /// Tool dispatcher for executing tool calls
    tool_dispatcher: Option<Arc<Mutex<SessionToolDispatcher>>>,
    /// Profile for sandbox/policy checks
    profile: Option<Profile>,
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
            messages: Arc::new(Mutex::new(Vec::new())),
            task_context: Arc::new(TaskContextTracker::new()),
            memory_retriever: None,
            retrieval_policy: None,
            tool_dispatcher: None,
            profile: None,
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

    /// Set the tool dispatcher for executing tool calls
    pub fn with_tool_dispatcher(mut self, dispatcher: Arc<Mutex<SessionToolDispatcher>>) -> Self {
        self.tool_dispatcher = Some(dispatcher);
        self
    }

    /// Set the profile for sandbox policy checks
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
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
                    system_message_content.push_str(&format!("\n\n## Relevant Memory\n{}", memory_context));
                }
                Err(e) => {
                    let _ = tx.send(AgentEvent::Error(format!("Memory retrieval failed: {}", e)));
                }
            }
        }

        let mut messages_for_request = self.messages.lock().unwrap().clone();
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

        self.messages
            .lock()
            .unwrap()
            .push(ChatMessage::user(user_input.to_string()));

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
        let approval_protocol = Arc::clone(&self.approval_protocol);
        let approval_gate = Arc::clone(&self.approval_gate);
        let tool_dispatcher = self.tool_dispatcher.clone();
        let profile = self.profile.clone();
        let messages = Arc::clone(&self.messages);

        tokio::spawn(async move {
            let mut assistant_buffer = String::new();
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
                        assistant_buffer.push_str(&text);
                        let _ = tx.send(AgentEvent::Token(text));
                    }
                    StreamEvent::ToolCall(calls) => {
                        for call in calls {
                            let args = call.function.arguments.clone();
                            let classification = classify_tool_risk(&call.function.name, &call.function.arguments);
                            let description = generate_tool_description(&call.function.name, &call.function.arguments);
                            let scope_info = extract_scope(&call.function.name, &call.function.arguments);
                            let scope = if !scope_info.is_empty() { Some(scope_info.to_detailed()) } else { None };
                            let task_context_str = task_context.brief_description();

                            let _ = tx.send(AgentEvent::ToolCall {
                                name: call.function.name.clone(),
                                args,
                                risk: classification.risk,
                                description: Some(description),
                                task_context: task_context_str,
                                scope,
                                classification_reasoning: Some(classification.reasoning),
                            });

                            if let Some(dispatcher) = &tool_dispatcher {
                                let (tool_result, metadata) =
                                    execute_tool_call(dispatcher, &approval_protocol, &approval_gate, &profile, &call);

                                if tool_result.is_success() {
                                    let msg = ChatMessage {
                                        role: Role::Tool,
                                        content: tool_result.content.clone(),
                                        tool_call_id: Some(tool_result.tool_call_id.clone()),
                                        tool_calls: None,
                                    };
                                    messages.lock().unwrap().push(msg);
                                }

                                let _ = tx.send(AgentEvent::ToolResult {
                                    name: call.function.name.clone(),
                                    result: tool_result.content.clone(),
                                    success: tool_result.is_success(),
                                    error: tool_result.error.clone(),
                                    metadata,
                                });
                            }
                        }
                    }
                    StreamEvent::Done => {
                        if !assistant_buffer.is_empty() {
                            let msg = ChatMessage::assistant(assistant_buffer);
                            messages.lock().unwrap().push(msg);
                        }
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
        self.messages.lock().unwrap().push(msg);
    }

    /// Get the current conversation history
    pub fn messages(&self) -> Vec<ChatMessage> {
        self.messages.lock().unwrap().clone()
    }
}

fn execute_tool_call(
    dispatcher: &Arc<Mutex<SessionToolDispatcher>>, approval_protocol: &Arc<dyn ApprovalProtocol>,
    approval_gate: &Arc<RwLock<ApprovalGate>>, profile: &Option<Profile>, call: &ToolCall,
) -> (ToolResult, ToolExecutionMetadata) {
    let tool_name = call.name();
    let args = call.arguments();
    let mut metadata = ToolExecutionMetadata::new();

    let tool_is_read_only = dispatcher
        .lock()
        .ok()
        .and_then(|guard| guard.dispatcher().registry().tool_is_read_only(tool_name))
        .unwrap_or(false);

    let risk = if tool_name == "shell" {
        args.get("command")
            .and_then(|v| v.as_str())
            .map(classify_shell_command_risk)
            .unwrap_or(ToolRisk::Risky)
    } else {
        dispatcher
            .lock()
            .ok()
            .and_then(|guard| guard.dispatcher().registry().tool_risk(tool_name))
            .unwrap_or_else(|| classify_tool_risk(tool_name, args).risk)
    };

    let action_type = tool_action_type(tool_name, args);
    let approval_mode = approval_gate.read().unwrap().mode();

    if approval_mode == ApprovalMode::ReadOnly && !tool_is_read_only {
        return (
            ToolResult::error(call.id.clone(), "Tool execution blocked: read-only mode"),
            metadata,
        );
    }

    let mut requires_approval = approval_gate
        .read()
        .unwrap()
        .check_requires_approval(risk, &action_type);

    if let Some(path) = extract_target_path(args) {
        metadata.affected_paths = vec![path.display().to_string()];
        if let Some(profile) = profile {
            match profile.check_path_access(&path, approval_mode) {
                thunderus_core::config::PathAccessResult::Denied(reason) => {
                    return (
                        ToolResult::error(
                            call.id.clone(),
                            format!("Tool execution blocked: access denied ({})", reason),
                        ),
                        metadata,
                    );
                }
                thunderus_core::config::PathAccessResult::ReadOnly => {
                    if !tool_is_read_only {
                        return (
                            ToolResult::error(call.id.clone(), "Tool execution blocked: read-only access"),
                            metadata,
                        );
                    }
                }
                thunderus_core::config::PathAccessResult::NeedsApproval(_) => {
                    requires_approval = true;
                }
                thunderus_core::config::PathAccessResult::Allowed => {}
            }
        }
    }

    if requires_approval && !request_tool_approval(approval_protocol, approval_gate, action_type, tool_name, args, risk)
    {
        return (
            ToolResult::error(call.id.clone(), "Tool execution rejected by user"),
            metadata,
        );
    }

    let start = std::time::Instant::now();
    let result = match dispatcher.lock() {
        Ok(mut guard) => guard.execute(call),
        Err(_) => Err(thunderus_core::Error::Tool("Tool dispatcher lock poisoned".to_string())),
    };
    metadata.execution_time_ms = Some(start.elapsed().as_millis() as u64);

    match result {
        Ok(tool_result) => {
            metadata.classification_reasoning = tool_result.classification_reasoning.clone();
            (tool_result, metadata)
        }
        Err(e) => (ToolResult::error(call.id.clone(), e.to_string()), metadata),
    }
}

fn request_tool_approval(
    approval_protocol: &Arc<dyn ApprovalProtocol>, approval_gate: &Arc<RwLock<ApprovalGate>>, action_type: ActionType,
    tool_name: &str, args: &serde_json::Value, risk: ToolRisk,
) -> bool {
    let approval_request = {
        let mut gate = approval_gate.write().unwrap();
        let id = gate.create_request(
            action_type,
            format!("Execute tool: {}", tool_name),
            ApprovalContext {
                name: Some(tool_name.to_string()),
                arguments: Some(args.clone()),
                affected_paths: Vec::new(),
                metadata: std::collections::HashMap::new(),
                classification_reasoning: None,
            },
            risk,
        );
        gate.get_request(id).cloned()
    };

    let Some(approval_request) = approval_request else {
        return false;
    };

    let decision = approval_protocol.request_approval(&approval_request);
    if let Ok(decision) = decision {
        let mut gate = approval_gate.write().unwrap();
        let _ = gate.record_decision(ApprovalResponse::new(approval_request.id, decision));
        matches!(decision, ApprovalDecision::Approved)
    } else {
        false
    }
}

fn extract_target_path(args: &serde_json::Value) -> Option<PathBuf> {
    args.get("file_path")
        .or_else(|| args.get("path"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
}

fn tool_action_type(tool_name: &str, args: &serde_json::Value) -> ActionType {
    if tool_name == "shell" {
        let is_network = args
            .get("command")
            .and_then(|v| v.as_str())
            .map(is_network_command)
            .unwrap_or(false);
        if is_network { ActionType::Network } else { ActionType::Shell }
    } else if tool_name.contains("patch") {
        ActionType::Patch
    } else if tool_name.contains("delete") || tool_name == "rm" {
        ActionType::FileDelete
    } else if tool_name.contains("write") || tool_name.contains("edit") || tool_name.contains("create") {
        ActionType::FileWrite
    } else {
        ActionType::Tool
    }
}

fn is_network_command(command: &str) -> bool {
    let cmd_lower = command.to_lowercase();
    cmd_lower.contains("curl ")
        || cmd_lower.contains("wget ")
        || cmd_lower.starts_with("curl")
        || cmd_lower.starts_with("wget")
        || cmd_lower.contains("ssh ")
        || cmd_lower.starts_with("ssh")
        || cmd_lower.contains("http://")
        || cmd_lower.contains("https://")
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
    use futures::stream;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::TempDir;
    use thunderus_core::memory::{MemoryKind, MemoryRetriever, RetrievalResult, RetrievedChunk};
    use thunderus_providers::Provider;
    use thunderus_tools::{EchoTool, SessionToolDispatcher, ToolDispatcher, ToolRegistry};

    type R<'a, T> = Result<Pin<Box<dyn futures::Stream<Item = T> + Send + 'a>>>;

    struct MockProvider {
        events: Vec<StreamEvent>,
    }

    #[async_trait::async_trait]
    impl Provider for MockProvider {
        async fn stream_chat<'a>(&'a self, _request: ChatRequest, _cancel_token: CancelToken) -> R<StreamEvent, 'a> {
            Ok(Box::pin(stream::iter(self.events.clone())))
        }
    }

    #[derive(Debug)]
    struct CaptureProvider {
        events: Vec<StreamEvent>,
        captured: Arc<Mutex<Option<ChatRequest>>>,
    }

    #[async_trait::async_trait]
    impl Provider for CaptureProvider {
        async fn stream_chat<'a>(&'a self, request: ChatRequest, _cancel_token: CancelToken) -> R<StreamEvent, 'a> {
            *self.captured.lock().unwrap() = Some(request);
            Ok(Box::pin(stream::iter(self.events.clone())))
        }
    }

    struct StubRetriever {
        called: Arc<AtomicBool>,
        policy: RetrievalPolicy,
    }

    impl StubRetriever {
        fn new(called: Arc<AtomicBool>) -> Self {
            Self { called, policy: RetrievalPolicy::default() }
        }
    }

    type O = std::result::Result<RetrievalResult, thunderus_core::memory::RetrievalError>;

    impl MemoryRetriever for StubRetriever {
        fn query<'a>(&'a self, _task_intent: &'a str) -> Pin<Box<dyn std::future::Future<Output = O> + Send + 'a>> {
            self.called.store(true, Ordering::SeqCst);
            Box::pin(async move {
                Ok(RetrievalResult {
                    chunks: vec![RetrievedChunk {
                        content: "Remember this".to_string(),
                        path: "memory/core/CORE.md".to_string(),
                        anchor: None,
                        event_ids: vec![],
                        kind: MemoryKind::Core,
                        score: -1.0,
                    }],
                    total_tokens: 5,
                    query: "test".to_string(),
                    search_time_ms: 1,
                })
            })
        }

        fn policy(&self) -> &RetrievalPolicy {
            &self.policy
        }
    }

    #[test]
    fn test_agent_creation() {
        let provider = Arc::new(MockProvider { events: vec![] }) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();
        let agent = Agent::new(provider, approval, gate, session_id);
        assert_eq!(agent.messages().len(), 0);
    }

    #[test]
    fn test_agent_approval_mode() {
        let provider = Arc::new(MockProvider { events: vec![] }) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();
        let agent = Agent::new(provider, approval, gate, session_id);
        assert_eq!(agent.approval_mode(), ApprovalMode::Auto);
    }

    #[test]
    fn test_agent_set_approval_mode() {
        let provider = Arc::new(MockProvider { events: vec![] }) as Arc<dyn Provider>;
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
        let provider = Arc::new(MockProvider { events: vec![] }) as Arc<dyn Provider>;
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
        let provider = Arc::new(MockProvider { events: vec![] }) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();
        let mut agent = Agent::new(provider, approval, gate, session_id);
        let result = ToolResult::success("call_1", "OK");
        agent.append_tool_result("test_tool".to_string(), "call_1".to_string(), result);

        let messages = agent.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::Tool);
        assert_eq!(messages[0].content, "OK");
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
        let provider = Arc::new(MockProvider { events: vec![] }) as Arc<dyn Provider>;
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
    async fn test_process_message_appends_assistant_response() {
        let events = vec![StreamEvent::Token("Hello".to_string()), StreamEvent::Done];
        let provider = Arc::new(MockProvider { events }) as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let mut agent = Agent::new(provider, approval, gate, session_id);

        let cancel = CancelToken::new();
        let mut rx = agent.process_message("Hi", None, cancel, Vec::new()).await.unwrap();

        while let Some(event) = rx.recv().await {
            if matches!(event, AgentEvent::Done) {
                break;
            }
        }

        let messages = agent.messages();
        assert!(
            messages
                .iter()
                .any(|m| m.role == Role::Assistant && m.content == "Hello")
        );
    }

    #[tokio::test]
    async fn test_system_message_includes_write_protection_and_memory() {
        let captured = Arc::new(Mutex::new(None));
        let provider = Arc::new(CaptureProvider { events: vec![StreamEvent::Done], captured: Arc::clone(&captured) })
            as Arc<dyn Provider>;
        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let called = Arc::new(AtomicBool::new(false));
        let retriever = Arc::new(StubRetriever::new(Arc::clone(&called))) as Arc<dyn MemoryRetriever>;

        let mut agent = Agent::new(provider, approval, gate, session_id).with_memory_retriever(retriever);

        let cancel = CancelToken::new();
        let mut rx = agent
            .process_message("Hi", None, cancel, vec![std::path::PathBuf::from("/tmp/owned.txt")])
            .await
            .unwrap();

        while let Some(event) = rx.recv().await {
            if matches!(event, AgentEvent::Done) {
                break;
            }
        }

        assert!(called.load(Ordering::SeqCst));

        let request = captured.lock().unwrap().clone().expect("expected request capture");
        let system = request
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        assert!(system.contains("Write Protection"));
        assert!(system.contains("Relevant Memory"));
        assert!(system.contains("Remember this"));
    }

    #[tokio::test]
    async fn test_tool_call_executes_with_dispatcher() {
        let provider = Arc::new(MockProvider {
            events: vec![
                StreamEvent::ToolCall(vec![ToolCall::new(
                    "call_1",
                    "echo",
                    serde_json::json!({"message": "hello"}),
                )]),
                StreamEvent::Done,
            ],
        }) as Arc<dyn Provider>;

        let approval = Arc::new(InMemoryApprovalProtocol::new(true)) as Arc<dyn ApprovalProtocol>;
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let session_id = SessionId::new();

        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session = Session::new(agent_dir).unwrap();

        let registry = ToolRegistry::new();
        registry.register(EchoTool).unwrap();
        let specs = registry.specs();
        let dispatcher = ToolDispatcher::new(registry);
        let session_dispatcher = SessionToolDispatcher::with_new_history(dispatcher, session);

        let mut agent = Agent::new(provider, approval, gate, session_id)
            .with_tool_dispatcher(Arc::new(Mutex::new(session_dispatcher)));

        let mut rx = agent
            .process_message("Hello", Some(specs), CancelToken::new(), Vec::new())
            .await
            .unwrap();

        let mut saw_tool_result = false;

        while let Ok(Some(event)) = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await {
            match event {
                AgentEvent::ToolResult { name, result, success, .. } => {
                    assert_eq!(name, "echo");
                    assert!(success);
                    assert_eq!(result, "hello");
                    saw_tool_result = true;
                }
                AgentEvent::Done => break,
                _ => {}
            }
        }

        assert!(saw_tool_result);
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
