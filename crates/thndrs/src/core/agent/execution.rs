//! Provider request execution, retry policy, permissions, and steering.

use super::*;

pub(crate) struct ProviderTurnRequest<'a, P>
where
    P: StreamingProvider,
{
    pub(crate) provider: &'a P,
    pub(crate) model: &'a str,
    pub(crate) messages: &'a [ProviderMessage],
    pub(crate) max_tokens: u32,
    pub(crate) reasoning_effort: ReasoningEffort,
    pub(crate) reasoning_summary: ReasoningSummary,
    pub(crate) tool_schemas: &'a serde_json::Value,
    pub(crate) continuation: &'a ProviderContinuation,
    pub(crate) turn_id: &'a str,
    pub(crate) context: &'a [thndrs_agent::ContextItemSnapshot],
    pub(crate) reduction_receipts: &'a [thndrs_agent::ContextReductionReceipt],
}

pub(crate) fn is_retryable_stream_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("cancel")
        || lower.contains("aborted")
        || lower.contains("max_tokens")
        || lower.contains("without writing")
        || lower.contains("provider returned only provider-side content blocks")
    {
        return false;
    }

    [
        "429",
        "500",
        "502",
        "503",
        "504",
        "overloaded",
        "rate limit",
        "server error",
        "service unavailable",
        "stream read error",
        "stream ended without",
        "connection",
        "timed out",
        "timeout",
        "provider error",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn sleep_with_cancel(delay: Duration, tx: &Sender<AgentEvent>, cancel: &CancelToken) -> bool {
    let mut slept = Duration::ZERO;
    let tick = Duration::from_millis(100);
    while slept < delay {
        if cancel.is_cancelled() {
            let _ = tx.send(AgentEvent::Cancelled);
            return false;
        }
        let remaining = delay.saturating_sub(slept);
        let nap = remaining.min(tick);
        thread::sleep(nap);
        slept += nap;
    }
    true
}

pub(crate) fn dispatch_tool_request(
    request: &ToolUseRequest, handle: &RunHandle, cancel: &CancelToken,
) -> (ToolOutput, Option<WriteResult>, Option<ProcessResult>) {
    if let Some(hook) = &handle.execution_hook
        && let Some(output) = hook.execute(request, &handle.config, cancel)
    {
        return output;
    }
    tools::dispatch_authorized_runtime_full_with_cancel_and_search_and_registry(request, &handle.config, cancel)
}

pub(crate) fn approve_tool_request(
    request: &ToolUseRequest, handle: &RunHandle, cancel: &CancelToken,
) -> ToolPermissionDecision {
    if !requires_runtime_permission(&request.name) {
        return ToolPermissionDecision::Allow;
    }
    match &handle.permission_hook {
        Some(hook) => hook.decide(request, &handle.config, cancel),
        None => ToolPermissionDecision::Allow,
    }
}

fn requires_runtime_permission(tool_name: &str) -> bool {
    tool_name.starts_with("mcp__") || matches!(tool_name, "create_file" | "replace_range" | "write_patch" | "run_shell")
}

pub(crate) fn load_provider_metadata<P>(
    provider: &P, model: &str, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> MetadataLoaded<P::Metadata>
where
    P: StreamingProvider,
{
    match provider.load_metadata() {
        Ok(models) => {
            tracing::info!("loaded provider model metadata");
            if let Some(event) = provider.metadata_loaded_event(&models)
                && send(tx, event, cancel).is_none()
            {
                return MetadataLoaded::Abort;
            }
            if let Some(status) = provider.metadata_status(model, &models)
                && send(tx, AgentEvent::Status(status), cancel).is_none()
            {
                return MetadataLoaded::Abort;
            }
            MetadataLoaded::Loaded(models)
        }
        Err(e) => {
            let message = P::request_error_message(&e);
            if e.is_credential_rejected() {
                tracing::warn!(error = %message, "provider rejected credentials while loading model metadata");
                let _ = send(tx, AgentEvent::Failed(message), cancel);
                return MetadataLoaded::Abort;
            }
            tracing::warn!(error = %message, "failed to load provider model metadata; using fallback token budget");
            match send(
                tx,
                AgentEvent::Status(String::from(
                    "provider: model metadata unavailable; using fallback token budget",
                )),
                cancel,
            ) {
                None => MetadataLoaded::Abort,
                Some(_) => MetadataLoaded::Unavailable,
            }
        }
    }
}

pub(crate) fn request_provider_turn_with_retries<P>(
    request: &ProviderTurnRequest<'_, P>, iteration: usize, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Option<ProviderTurn>
where
    P: StreamingProvider,
{
    let mut retry_attempt = 0;
    loop {
        tracing::info!(
            iteration,
            messages = request.messages.len(),
            retry_attempt,
            "requesting provider turn"
        );
        let attempt_result = provider_request_attempt(request, iteration, retry_attempt + 1, tx, cancel);

        match attempt_result {
            Ok(turn) => return Some(turn),
            Err(error) if error.is_retryable::<P>() && retry_attempt < PROVIDER_RETRY_POLICY.max_retries => {
                retry_attempt += 1;
                if !send_retry_event(request.provider, error.message::<P>(), retry_attempt, tx, cancel) {
                    return None;
                }
            }
            Err(error) => {
                let message = error.message::<P>();
                tracing::error!(provider = request.provider.name(), error = %message, "provider attempt failed");
                let _ = send(tx, AgentEvent::Failed(message), cancel);
                return None;
            }
        }
    }
}

fn provider_request_attempt<P>(
    request: &ProviderTurnRequest<'_, P>, iteration: usize, attempt: u32, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<ProviderTurn, ProviderAttemptError>
where
    P: StreamingProvider,
{
    let provider_request = StreamingRequest {
        max_tokens: request.max_tokens,
        reasoning_effort: request.reasoning_effort,
        reasoning_summary: request.reasoning_summary,
        tools: request.tool_schemas,
        continuation: request.continuation,
    };
    let serialized_body = request
        .provider
        .serialized_request_body(request.model, request.messages, &provider_request)
        .map_err(ProviderAttemptError::Request)?;
    let mut accounting = ProviderRequestAccounting::from_serialized_request(
        request.turn_id,
        format!("{}:request:{iteration}", request.turn_id),
        attempt,
        request.provider.name(),
        request.model,
        &serialized_body,
        request.context.to_vec(),
    )
    .with_reduction_receipts(request.reduction_receipts.to_vec())
    .with_model_projection(
        request
            .messages
            .iter()
            .map(|message| ModelProjectionMessage {
                role: message.role.clone(),
                content: match &message.content {
                    crate::providers::ProviderMessageContent::Text(content) => content.clone(),
                    crate::providers::ProviderMessageContent::Blocks(blocks) => {
                        serde_json::to_string(blocks).unwrap_or_else(|_| String::from("[unserializable blocks]"))
                    }
                },
            })
            .collect(),
    );
    if send(tx, AgentEvent::RequestStarted(Box::new(accounting.clone())), cancel).is_none() {
        return Err(ProviderAttemptError::Stream("cancelled".to_string()));
    }
    match request
        .provider
        .send_streaming_request(request.model, request.messages, &provider_request)
    {
        Ok(response) => {
            if codex::is_model_id(request.model)
                && let Some(usage) = codex::CodexUsageStatus::from_response_headers(response.headers())
                && send(tx, AgentEvent::CodexUsage(usage), cancel).is_none()
            {
                return Err(ProviderAttemptError::Stream("cancelled".to_string()));
            }
            match send(
                tx,
                AgentEvent::Status(format!("provider: connected HTTP {}", response.status().as_u16())),
                cancel,
            ) {
                None => Err(ProviderAttemptError::Stream("cancelled".to_string())),
                Some(_) => stream_provider_response(
                    request.provider,
                    request.model,
                    response,
                    tx,
                    cancel,
                    request.max_tokens,
                )
                .map_err(ProviderAttemptError::Stream)
                .and_then(|mut turn| {
                    let stream_format = request
                        .provider
                        .stream_format(request.model)
                        .map_err(ProviderAttemptError::Request)?;
                    let provider_usage = turn.usage.take().and_then(|components| {
                        if components.input_tokens.is_none()
                            && components.output_tokens.is_none()
                            && components.cache_read_input_tokens.is_none()
                            && components.cache_creation_input_tokens.is_none()
                            && components.reasoning_tokens.is_none()
                        {
                            None
                        } else {
                            Some(
                                components
                                    .normalize(request.provider.name(), usage_rule_for_stream_format(stream_format)),
                            )
                        }
                    });
                    accounting.provider_usage = provider_usage;
                    accounting.tool_count = Some(turn.tool_requests.len() as u64);
                    if send(tx, AgentEvent::RequestAccounting(Box::new(accounting)), cancel).is_none() {
                        return Err(ProviderAttemptError::Stream("cancelled".to_string()));
                    }
                    Ok(turn)
                }),
            }
        }
        Err(e) => Err(ProviderAttemptError::Request(e)),
    }
}

fn usage_rule_for_stream_format(format: StreamFormat) -> ProviderUsageRule {
    match format {
        StreamFormat::AnthropicMessages => ProviderUsageRule::AnthropicMessages,
        StreamFormat::OpenAiChat => ProviderUsageRule::OpenAiChat,
        StreamFormat::ChatGptCodexResponses => ProviderUsageRule::OpenAiResponses,
    }
}

pub(crate) fn send_retry_event<P>(
    provider: &P, message: String, retry_attempt: u32, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> bool
where
    P: StreamingProvider,
{
    let delay = PROVIDER_RETRY_POLICY.delay_for_attempt(retry_attempt);
    tracing::warn!(
        provider = provider.name(),
        attempt = retry_attempt,
        max_retries = PROVIDER_RETRY_POLICY.max_retries,
        delay_ms = delay.as_millis(),
        error = %message,
        "retrying provider attempt"
    );
    match send(
        tx,
        AgentEvent::Retrying {
            attempt: retry_attempt,
            max_attempts: PROVIDER_RETRY_POLICY.max_retries,
            delay_ms: delay.as_millis() as u64,
            error: message,
        },
        cancel,
    ) {
        None => false,
        Some(_) => sleep_with_cancel(delay, tx, cancel),
    }
}

pub(crate) fn append_steering_messages(messages: &mut Vec<ProviderMessage>, handle: &RunHandle) -> bool {
    match handle.steering.as_ref() {
        Some(rx) => {
            let mut appended = false;
            while let Ok(text) = rx.try_recv() {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                messages.push(ProviderMessage::user(&format!("[steering]\n{trimmed}")));
                appended = true;
            }
            if appended {
                tracing::debug!(messages = messages.len(), "appended steering messages");
            }
            appended
        }
        None => false,
    }
}

pub(crate) fn stopped_without_expected_write(assistant_text: &str, expects_write: bool, wrote_file: bool) -> bool {
    assistant_text.is_empty() && expects_write && !wrote_file
}
