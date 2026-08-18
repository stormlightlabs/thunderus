//! Agent lifecycle and provider-neutral turn orchestration.

use super::*;

/// Handle for a single agent run: provider kind, config, prompt, and cancel.
#[derive(Debug)]
pub struct RunHandle {
    pub provider: ProviderKind,
    pub config: AgentRunConfig,
    pub prompt: String,
    pub messages: Vec<ProviderMessage>,
    pub expects_write: bool,
    pub steering: Option<Receiver<String>>,
    pub cancel: CancelToken,
    pub permission_hook: Option<ToolPermissionHook>,
    pub execution_hook: Option<ToolExecutionHook>,
}

impl RunHandle {
    /// Spawn the unified agent loop on an owned background run.
    ///
    /// The thread closes its sender when done, so the run's `try_recv` will
    /// return `Err(Disconnected)` once the run completes.
    ///
    /// Dropping the run requests cooperative cancellation, disconnects event
    /// delivery, and joins the worker.
    pub fn spawn(self) -> thndrs_agent::AgentRun<AgentEvent> {
        let cancel = self.cancel.clone();
        tracing::info!(provider = ?self.provider, "starting agent thread");
        thndrs_agent::AgentRun::spawn(cancel, move |sender, cancel| self.run_agent(&sender, &cancel))
    }

    /// Create a fake-provider run handle.
    pub fn fake(config: AgentRunConfig, prompt: String) -> Self {
        RunHandle {
            provider: ProviderKind::Fake,
            config,
            prompt,
            messages: Vec::new(),
            expects_write: false,
            steering: None,
            cancel: CancelToken::new(),
            permission_hook: None,
            execution_hook: None,
        }
    }

    /// Create a provider run handle with a steering-message receiver.
    pub fn provider_with_steering(
        config: AgentRunConfig, messages: Vec<ProviderMessage>, expects_write: bool, steering: Receiver<String>,
    ) -> Self {
        let provider = ProviderKind::for_model(&config.model);
        RunHandle {
            provider,
            config,
            prompt: String::new(),
            messages,
            expects_write,
            steering: Some(steering),
            cancel: CancelToken::new(),
            permission_hook: None,
            execution_hook: None,
        }
    }

    /// Attach a permission hook for sensitive tool calls.
    pub fn with_permission_hook(mut self, hook: ToolPermissionHook) -> Self {
        self.permission_hook = Some(hook);
        self
    }

    /// Attach an execution hook for front-end-specific tool handling.
    pub fn with_execution_hook(mut self, hook: ToolExecutionHook) -> Self {
        self.execution_hook = Some(hook);
        self
    }

    /// The unified agent loop. Dispatches to a built-in provider, handles
    /// tool-use requests, and checks cancellation cooperatively.
    fn run_agent(&self, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
        if send(tx, AgentEvent::Started, cancel).is_none() {
            return;
        }
        step();

        match self.provider {
            ProviderKind::Fake => self.run_fake(tx, cancel),
            ProviderKind::Unsupported => {
                let _ = send(
                    tx,
                    AgentEvent::Failed(crate::cli::commands::setup::UNSUPPORTED_PROVIDER_ROUTE_MESSAGE.to_string()),
                    cancel,
                );
            }
            ProviderKind::OpenCodeGo => self.run_provider::<opencode::OpenCodeGoClient>(tx, cancel),
            ProviderKind::OpenCodeZen => self.run_provider::<opencode::zen::OpenCodeZenClient>(tx, cancel),
            ProviderKind::ChatGptCodex => self.run_provider::<codex::ChatGptCodexClient>(tx, cancel),
        }
    }

    /// A streaming provider sends the prompt to its API, streams the response,
    /// dispatches any tool-use requests, feeds the tool results back as
    /// provider-native tool result messages, and repeats until the model stops
    /// requesting tools.
    #[expect(
        clippy::cognitive_complexity,
        reason = "Provider turns intentionally centralize cancellation, tool permissions, and continuation state."
    )]
    pub(super) fn run_provider<P>(&self, tx: &Sender<AgentEvent>, cancel: &CancelToken)
    where
        P: StreamingProvider,
    {
        let provider = match P::from_env_or_dotenv(&self.config.root) {
            Ok(provider) => provider,
            Err(e) => {
                let message = P::request_error_message(&e);
                tracing::error!(error = %message, "failed to load provider client");
                let _ = send(tx, AgentEvent::Failed(message), cancel);
                return;
            }
        };

        tracing::info!(
            provider = provider.name(),
            model = %self.config.model,
            cwd = %self.config.root.display(),
            messages = self.messages.len(),
            "starting provider agent run"
        );
        if send(tx, AgentEvent::Status(provider.load_status()), cancel).is_none() {
            return;
        }

        let model_metadata = match load_provider_metadata(&provider, &self.config.model, tx, cancel) {
            MetadataLoaded::Abort => return,
            MetadataLoaded::Loaded(metadata) => Some(metadata),
            MetadataLoaded::Unavailable => None,
        };

        let tool_defs = tools::runtime_tool_definitions_for(self.config.authority, self.config.mcp_manager.as_deref());
        let tool_schemas = tools::tool_catalog_schemas(&tool_defs);
        let mut messages = if self.messages.is_empty() {
            vec![ProviderMessage::user(&self.prompt)]
        } else {
            self.messages.clone()
        };
        let mut tool_budget = thndrs_agent::ToolIterationBudget::unbounded();
        let mut wrote_file = false;
        let mut continuation = ProviderContinuation::default();
        let mut pending_reduction_receipts = Vec::new();
        let mut state_history = Vec::new();
        let mut workspace_freshness = 0_u64;

        loop {
            if cancel.is_cancelled() {
                tracing::warn!(
                    provider = provider.name(),
                    "provider run cancelled before provider request"
                );
                let _ = send(tx, AgentEvent::Cancelled, cancel);
                return;
            }

            match tool_budget.before_provider_request() {
                thndrs_agent::ToolBudgetDecision::Continue => {}
                thndrs_agent::ToolBudgetDecision::ContinueAfterBudgetMessage
                | thndrs_agent::ToolBudgetDecision::Exhausted { .. } => {
                    unreachable!("the primary agent uses an unbounded tool budget");
                }
            }

            if send(
                tx,
                AgentEvent::Status(provider.request_status(&self.config.model)),
                cancel,
            )
            .is_none()
            {
                return;
            }

            let max_tokens = provider.token_budget(&self.config.model, model_metadata.as_ref());
            let request = ProviderTurnRequest {
                provider: &provider,
                model: &self.config.model,
                messages: &messages,
                max_tokens,
                reasoning_effort: self.config.reasoning_effort,
                reasoning_summary: self.config.reasoning_summary,
                tool_schemas: &tool_schemas,
                continuation: &continuation,
                turn_id: self.config.accounting_turn_id.as_deref().unwrap_or("turn_unknown"),
                context: &self.config.accounting_context,
                reduction_receipts: &pending_reduction_receipts,
            };
            let Some(mut turn) = request_provider_turn_with_retries(&request, tool_budget.total_batches(), tx, cancel)
            else {
                return;
            };
            pending_reduction_receipts.clear();
            if matches!(self.provider, ProviderKind::ChatGptCodex | ProviderKind::OpenCodeZen)
                && matches!(
                    provider.stream_format(&self.config.model),
                    Ok(StreamFormat::ChatGptCodexResponses)
                )
            {
                codex::record_response_items(&mut continuation, &messages, std::mem::take(&mut turn.response_items));
            }
            tracing::info!(
                text_chars = turn.assistant_text.chars().count(),
                tool_calls = turn.tool_requests.len(),
                "provider turn completed"
            );

            if turn.tool_requests.is_empty() {
                if turn.assistant_text.is_empty() && turn.stop_reason.as_deref() == Some("max_tokens") {
                    let _ = send(
                        tx,
                        AgentEvent::Failed(format!(
                            "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
                        )),
                        cancel,
                    );
                    return;
                }
                if stopped_without_expected_write(&turn.assistant_text, self.expects_write, wrote_file) {
                    let _ = send(
                        tx,
                        AgentEvent::Failed(String::from(
                            "model stopped without writing a file for an edit-like request",
                        )),
                        cancel,
                    );
                    return;
                }
                if append_steering_messages(&mut messages, self) {
                    tracing::info!(
                        provider = provider.name(),
                        "continuing provider run with queued steering messages"
                    );
                    continue;
                }
                let _ = send(tx, AgentEvent::Finished, cancel);
                return;
            }

            tool_budget.record_tool_batch();

            let mut assistant_blocks = Vec::new();
            if !turn.assistant_text.is_empty() {
                assistant_blocks.push(ProviderContentBlock::Text { text: turn.assistant_text });
            }

            let mut tool_results: Vec<ProviderMessage> = Vec::new();
            let mut response_tool_outputs = Vec::new();
            for req in &turn.tool_requests {
                if cancel.is_cancelled() {
                    tracing::warn!(provider = provider.name(), tool = %req.name, tool_id = %req.tool_use_id, "provider run cancelled before tool dispatch");
                    let _ = send(tx, AgentEvent::Cancelled, cancel);
                    return;
                }

                let tool_id = req.tool_use_id.clone();
                tracing::info!(tool = %req.name, tool_id = %tool_id, "dispatching tool request");
                if send(
                    tx,
                    AgentEvent::ToolStarted {
                        id: tool_id.clone(),
                        name: req.name.clone(),
                        arguments: req.arguments.clone(),
                    },
                    cancel,
                )
                .is_none()
                {
                    return;
                }

                let (mut output, write_result, shell_result) = match approve_tool_request(req, self, cancel) {
                    ToolPermissionDecision::Allow => dispatch_tool_request(req, self, cancel),
                    ToolPermissionDecision::Reject => (
                        ToolOutput::failed(&req.name, String::from("tool call rejected by ACP client")),
                        None,
                        None,
                    ),
                    ToolPermissionDecision::Cancelled => {
                        let _ = send(tx, AgentEvent::Cancelled, cancel);
                        return;
                    }
                };
                let status = output.status;
                let display_output = output.display_lines();
                if let Some(store) = &self.config.artifact_store {
                    match store.create_tool_evidence(&format!("tool:{tool_id}"), &display_output) {
                        Ok(artifact) => {
                            output.evidence.identity = format!("tool:{tool_id}");
                            output.evidence.artifact_handle = Some(artifact.metadata.handle);
                        }
                        Err(error) => {
                            tracing::warn!(tool = %req.name, tool_id = %tool_id, %error, "failed to preserve bounded tool evidence")
                        }
                    }
                }
                if write_result.is_some() && status == ToolStatus::Ok {
                    wrote_file = true;
                }
                let state_identity = tools::state_identity_for(req, &output, &self.config.root, workspace_freshness);
                let state_protected = status != ToolStatus::Ok || write_result.is_some();
                let (tool_result, result_content, reduced, projection_decision, state_record) = model_tool_result(
                    &tool_id,
                    &output,
                    shell_result.as_ref(),
                    &self.config.model_reduction,
                    state_identity,
                    state_protected,
                    &state_history,
                );
                if let Some(record) = state_record {
                    state_history.push(record);
                }
                if write_result.is_some() || req.name == tools::shell::NAME {
                    workspace_freshness = workspace_freshness.saturating_add(1);
                }
                tracing::info!(tool = %req.name, tool_id = %tool_id, status = ?status, "tool request finished");
                if !matches!(
                    &projection_decision,
                    thndrs_agent::context::StateProjectionDecision::Retained
                ) && send(
                    tx,
                    AgentEvent::StateProjectionDecision { id: tool_id.clone(), decision: projection_decision },
                    cancel,
                )
                .is_none()
                {
                    return;
                }
                if send(
                    tx,
                    AgentEvent::ToolFinished {
                        id: tool_id.clone(),
                        output: display_output.clone(),
                        status,
                        write_result,
                        shell_result: shell_result.map(Box::new),
                    },
                    cancel,
                )
                .is_none()
                {
                    return;
                }

                let (input, input_reduction) = project_failed_tool_input(req, &output, &self.config.model_reduction);
                assistant_blocks.push(ProviderContentBlock::ToolUse {
                    id: tool_id.clone(),
                    name: req.name.clone(),
                    input,
                });

                for diagnostic in &reduced.diagnostics {
                    tracing::warn!(
                        reducer = diagnostic.reducer.map(|reducer| reducer.label()).unwrap_or("pipeline"),
                        code = %diagnostic.code,
                        message = %diagnostic.message,
                        "model projection reducer kept the baseline"
                    );
                }
                pending_reduction_receipts.extend(reduced.receipts);
                if let Some(receipt) = input_reduction {
                    pending_reduction_receipts.push(receipt);
                }
                tool_results.push(tool_result);
                response_tool_outputs.push((tool_id, result_content));
            }

            messages.push(ProviderMessage::assistant_blocks(assistant_blocks));
            messages.extend(tool_results);
            if matches!(self.provider, ProviderKind::ChatGptCodex | ProviderKind::OpenCodeZen)
                && matches!(
                    provider.stream_format(&self.config.model),
                    Ok(StreamFormat::ChatGptCodexResponses)
                )
            {
                for (call_id, output) in response_tool_outputs {
                    codex::record_tool_output(&mut continuation, &call_id, &output, messages.len());
                }
            }
            append_steering_messages(&mut messages, self);
        }
    }

    /// Deterministic fake provider: emits reasoning, a tool-use request, assistant
    /// text, and finishes. Demonstrates the tool dispatch path end-to-end.
    fn run_fake(&self, tx: &Sender<AgentEvent>, cancel: &CancelToken) {
        use AgentEvent::*;
        match send(tx, ReasoningDelta(String::from("Let me think about this... ")), cancel) {
            None => return,
            Some(_) => step(),
        }

        match send(
            tx,
            ReasoningDelta(String::from("The repo is a Rust terminal coding harness.")),
            cancel,
        ) {
            None => return,
            Some(_) => step(),
        }

        if self.config.model == "fake-agent-slow" {
            for _ in 0..200 {
                if cancel.is_cancelled() {
                    let _ = send(tx, Cancelled, cancel);
                    return;
                }
                step();
            }
        }

        let tool_id = fake_tool_id(&self.config, "0");
        let tool_req = ToolUseRequest::new(
            String::from("read_file_range"),
            serde_json::json!({ "path": "Cargo.toml", "start_line": 1, "end_line": 5 }).to_string(),
            tool_id.clone(),
        );

        match send(
            tx,
            ToolStarted { id: tool_id.clone(), name: tool_req.name.clone(), arguments: tool_req.arguments.clone() },
            cancel,
        ) {
            None => return,
            Some(_) => step(),
        }

        let (output, _, _) = tools::dispatch_full(&tool_req, &self.config.root);
        let status = output.status;
        let display_output = output.display_lines();
        match send(
            tx,
            ToolFinished { id: tool_id, output: display_output, status, write_result: None, shell_result: None },
            cancel,
        ) {
            None => return,
            Some(_) => step(),
        }

        if self.config.model == "fake-agent-shell" {
            let shell_id = fake_tool_id(&self.config, "shell-0");
            let shell_req = ToolUseRequest::new(
                String::from("run_shell"),
                serde_json::json!({ "program": "printf", "args": ["acp-permission-smoke\n"] }).to_string(),
                shell_id.clone(),
            );
            match send(
                tx,
                ToolStarted {
                    id: shell_id.clone(),
                    name: shell_req.name.clone(),
                    arguments: shell_req.arguments.clone(),
                },
                cancel,
            ) {
                None => return,
                Some(_) => step(),
            }

            let (shell_output, write_result, shell_result) = match approve_tool_request(&shell_req, self, cancel) {
                ToolPermissionDecision::Allow => dispatch_tool_request(&shell_req, self, cancel),
                ToolPermissionDecision::Reject => (
                    ToolOutput::failed(&shell_req.name, String::from("tool call rejected by ACP client")),
                    None,
                    None,
                ),
                ToolPermissionDecision::Cancelled => {
                    let _ = send(tx, AgentEvent::Cancelled, cancel);
                    return;
                }
            };
            let shell_status = shell_output.status;
            let shell_display_output = shell_output.display_lines();
            match send(
                tx,
                ToolFinished {
                    id: shell_id,
                    output: shell_display_output,
                    status: shell_status,
                    write_result,
                    shell_result: shell_result.map(Box::new),
                },
                cancel,
            ) {
                None => return,
                Some(_) => step(),
            }
        }
        match send(tx, AssistantDelta(String::from("This is a ")), cancel) {
            None => return,
            Some(_) => step(),
        }

        match send(tx, AssistantDelta(String::from("fake streaming response.")), cancel) {
            None => return,
            Some(_) => step(),
        }
        let _ = tx.send(Finished);
    }
}
