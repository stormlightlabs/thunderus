//! Provider wire-stream decoders and event projection.

use super::*;

#[derive(Default)]
pub(crate) struct AnthropicStreamState {
    pub(crate) tool_blocks: HashMap<usize, ToolUseBuilder>,
    pub(crate) tool_requests: Vec<ToolUseRequest>,
    pub(crate) assistant_text: String,
    pub(crate) stop_reason: Option<String>,
    pub(crate) provider_content_blocks: Vec<String>,
    pub(crate) usage: ProviderUsageComponents,
}

#[cfg(test)]
impl AnthropicStreamState {
    pub(crate) fn collect(
        &mut self, event_type: &str, data: &str, tx: &Sender<AgentEvent>, cancel: &CancelToken,
    ) -> Result<(), String> {
        collect_anthropic_event(event_type, data, self, tx, cancel)
    }
}

/// Read an Anthropic-compatible SSE streaming response, converting events to [`AgentEvent`]
/// instances and collecting any tool-use requests plus the assistant text.
///
/// Returns a [`TurnOutput`] with the tool-use requests and the accumulated
/// assistant text, or an error message if the stream failed.
fn stream_anthropic_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken, max_tokens: u32,
) -> Result<ProviderTurn, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut buffer = String::new();
    let mut state = AnthropicStreamState::default();
    let mut event_count = 0usize;
    let mut saw_response = false;
    tracing::info!("reading Anthropic-compatible SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading Anthropic-compatible SSE stream");
            return Err("cancelled".to_string());
        }

        match line_result {
            Ok(line) => {
                buffer.push_str(&line);
                buffer.push('\n');

                if line.is_empty() {
                    let events = anthropic::parse_sse_chunk(&buffer);
                    buffer.clear();

                    for (event_type, data) in events {
                        event_count += 1;
                        log_sse_event(event_count, &event_type, &data);
                        if !saw_response {
                            saw_response = true;
                            if send(tx, AgentEvent::Status(String::from("provider: receiving SSE")), cancel).is_none() {
                                return Err("cancelled".to_string());
                            }
                        }
                        collect_anthropic_event(&event_type, &data, &mut state, tx, cancel)?;
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "failed reading Anthropic-compatible SSE stream");
                return Err(format!("stream read error: {e}"));
            }
        }
    }

    if !buffer.is_empty() {
        let events = anthropic::parse_sse_chunk(&buffer);
        for (event_type, data) in events {
            event_count += 1;
            log_sse_event(event_count, &event_type, &data);
            if !saw_response {
                saw_response = true;
                if send(tx, AgentEvent::Status(String::from("provider: receiving SSE")), cancel).is_none() {
                    return Err("cancelled".to_string());
                }
            }
            collect_anthropic_event(&event_type, &data, &mut state, tx, cancel)?;
        }
    }

    for (_, block) in state.tool_blocks {
        if let Some(req) = block.finish() {
            state.tool_requests.push(req);
        }
    }

    if state.assistant_text.is_empty() && state.tool_requests.is_empty() {
        tracing::error!(
            event_count,
            "Anthropic-compatible stream ended without assistant text or tool calls"
        );
        if state.stop_reason.as_deref() == Some("max_tokens") {
            return Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            ));
        }
        if !state.provider_content_blocks.is_empty() {
            let blocks = state.provider_content_blocks.join(", ");
            return Err(format!(
                "provider returned only provider-side content blocks ({blocks}) and no assistant text or tool calls"
            ));
        }
        return Err(format!(
            "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
        ));
    }

    tracing::info!(
        event_count,
        text_chars = state.assistant_text.chars().count(),
        tool_calls = state.tool_requests.len(),
        "finished reading Anthropic-compatible SSE stream"
    );
    let _ = send(
        tx,
        AgentEvent::Status(format!(
            "provider: stream ended ({event_count} SSE events, {} text chars, {} tool calls)",
            state.assistant_text.chars().count(),
            state.tool_requests.len()
        )),
        cancel,
    );

    Ok(ProviderTurn {
        tool_requests: state.tool_requests,
        assistant_text: state.assistant_text,
        stop_reason: state.stop_reason,
        response_items: Vec::new(),
        usage: Some(state.usage),
    })
}

pub(crate) fn stream_provider_response<P: StreamingProvider>(
    provider: &P, model: &str, resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken,
    max_tokens: u32,
) -> Result<ProviderTurn, String> {
    match provider
        .stream_format(model)
        .map_err(|e| P::request_error_message(&e))?
    {
        StreamFormat::OpenAiChat => stream_openai_chat_response(resp, tx, cancel, max_tokens),
        StreamFormat::AnthropicMessages => stream_anthropic_response(resp, tx, cancel, max_tokens),
        StreamFormat::ChatGptCodexResponses => stream_chatgpt_codex_response(resp, tx, cancel, max_tokens),
    }
}

fn stream_openai_chat_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken, max_tokens: u32,
) -> Result<ProviderTurn, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut assistant_text = String::new();
    let mut tool_blocks: HashMap<usize, ChatToolCallBuilder> = HashMap::new();
    let mut tool_requests = Vec::new();
    let mut event_count = 0usize;
    let mut saw_response = false;
    let mut stop_reason = None;
    let mut usage = ProviderUsageComponents::default();
    tracing::info!("reading OpenAI-compatible chat-completions SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading OpenAI-compatible SSE stream");
            return Err("cancelled".to_string());
        }

        let line = line_result.map_err(|e| {
            tracing::error!(error = %e, "failed reading OpenAI-compatible SSE stream");
            format!("stream read error: {e}")
        })?;
        if !line.starts_with("data:") {
            continue;
        }

        if !saw_response {
            saw_response = true;
            if send(tx, AgentEvent::Status(String::from("provider: receiving SSE")), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }

        for data in openai::parse_chat_sse_chunk(&(line + "\n")) {
            event_count += 1;
            let events = openai::parse_chat_sse_event(&data);
            for event in events {
                match &event {
                    openai::ChatSseEvent::Usage { input_tokens, output_tokens } => {
                        usage.merge_snapshot(&ProviderUsageComponents::new(*input_tokens, *output_tokens));
                    }
                    openai::ChatSseEvent::UsageComponents(components) => usage.merge_snapshot(components),
                    _ => {}
                }
                collect_openai_chat_event(
                    event,
                    &mut tool_blocks,
                    &mut tool_requests,
                    &mut assistant_text,
                    &mut stop_reason,
                    tx,
                    cancel,
                )?;
            }
        }
    }

    for (_, block) in tool_blocks {
        if let Some(req) = block.finish() {
            tool_requests.push(req);
        }
    }

    if assistant_text.is_empty() && tool_requests.is_empty() {
        tracing::error!(
            event_count,
            "OpenAI-compatible stream ended without assistant text or tool calls"
        );
        return match stop_reason.as_deref() {
            Some("length") => Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            )),
            _ => Err(format!(
                "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
            )),
        };
    }

    tracing::info!(
        event_count,
        text_chars = assistant_text.chars().count(),
        tool_calls = tool_requests.len(),
        "finished reading OpenAI-compatible SSE stream"
    );
    let _ = send(
        tx,
        AgentEvent::Status(format!(
            "provider: stream ended ({event_count} SSE events, {} text chars, {} tool calls)",
            assistant_text.chars().count(),
            tool_requests.len()
        )),
        cancel,
    );

    Ok(ProviderTurn { tool_requests, assistant_text, stop_reason, response_items: Vec::new(), usage: Some(usage) })
}

pub(crate) fn collect_openai_chat_event(
    event: openai::ChatSseEvent, tool_blocks: &mut HashMap<usize, ChatToolCallBuilder>,
    tool_requests: &mut Vec<ToolUseRequest>, assistant_text: &mut String, stop_reason: &mut Option<String>,
    tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<(), String> {
    match event {
        openai::ChatSseEvent::TextDelta(text) => {
            assistant_text.push_str(&text);
            if send(tx, AgentEvent::AssistantDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::ReasoningDelta(text) => {
            if send(tx, AgentEvent::ReasoningDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::ToolCallStart { index, id, name } => {
            tool_blocks.insert(index, ChatToolCallBuilder { id, name, arguments_json: String::new() });
        }
        openai::ChatSseEvent::ToolCallArgumentsDelta { index, arguments } => {
            let block = tool_blocks.entry(index).or_insert_with(|| ChatToolCallBuilder {
                id: format!("call_{index}"),
                name: String::new(),
                arguments_json: String::new(),
            });
            block.arguments_json.push_str(&arguments);
        }
        openai::ChatSseEvent::FinishReason(reason) => {
            *stop_reason = Some(reason.clone());
            if reason == "tool_calls" {
                let finished = std::mem::take(tool_blocks);
                for (_, block) in finished {
                    if let Some(req) = block.finish() {
                        tool_requests.push(req);
                    }
                }
            }
        }
        openai::ChatSseEvent::ResponseStatus(status) => {
            if matches!(status.as_str(), "failed" | "cancelled" | "canceled") {
                return Err(format!("provider stream status: {status}"));
            }
            if send(tx, AgentEvent::Status(format!("provider: status {status}")), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        openai::ChatSseEvent::Error(message) => {
            tracing::error!(error = %message, "provider emitted SSE error");
            return Err(format!("provider error: {message}"));
        }
        openai::ChatSseEvent::Usage { .. } => {}
        openai::ChatSseEvent::UsageComponents(_) => {}
        openai::ChatSseEvent::Malformed(payload) => {
            return Err(format!("malformed provider stream payload: {payload}"));
        }
        openai::ChatSseEvent::Done | openai::ChatSseEvent::Other => {}
    }

    Ok(())
}

pub(crate) fn stream_chatgpt_codex_response(
    resp: Response<ureq::Body>, tx: &Sender<AgentEvent>, cancel: &CancelToken, max_tokens: u32,
) -> Result<ProviderTurn, String> {
    let reader = BufReader::new(resp.into_body().into_reader());
    let mut assistant_text = String::new();
    let mut tool_blocks: HashMap<String, ChatToolCallBuilder> = HashMap::new();
    let mut tool_requests = Vec::new();
    let mut event_count = 0usize;
    let mut saw_response = false;
    let mut stop_reason = None;
    let mut response_items = Vec::new();
    let mut usage = ProviderUsageComponents::default();
    tracing::info!("reading ChatGPT Codex Responses SSE stream");

    for line_result in reader.lines() {
        if cancel.is_cancelled() {
            tracing::warn!("cancelled while reading ChatGPT Codex SSE stream");
            return Err("cancelled".to_string());
        }

        let line = line_result.map_err(|e| {
            tracing::error!(error = %e, "failed reading ChatGPT Codex SSE stream");
            format!("stream read error: {e}")
        })?;
        if !line.starts_with("data:") {
            continue;
        }

        if !saw_response {
            saw_response = true;
            if send(
                tx,
                AgentEvent::Status(String::from("provider: receiving ChatGPT Codex SSE")),
                cancel,
            )
            .is_none()
            {
                return Err("cancelled".to_string());
            }
        }

        for data in codex::parse_responses_sse_chunk(&(line + "\n")) {
            event_count += 1;
            for event in codex::parse_responses_sse_event(&data) {
                match &event {
                    codex::ResponsesSseEvent::Usage { input_tokens, output_tokens } => {
                        usage.merge_snapshot(&ProviderUsageComponents::new(*input_tokens, *output_tokens));
                    }
                    codex::ResponsesSseEvent::UsageComponents(components) => usage.merge_snapshot(components),
                    _ => {}
                }
                if let codex::ResponsesSseEvent::OutputItem(item) = &event {
                    response_items.push(item.clone());
                }
                collect_chatgpt_codex_event(
                    event,
                    &mut tool_blocks,
                    &mut tool_requests,
                    &mut assistant_text,
                    &mut stop_reason,
                    tx,
                    cancel,
                )?;
            }
        }
    }

    for (_, block) in tool_blocks {
        if let Some(req) = block.finish() {
            tool_requests.push(req);
        }
    }

    if assistant_text.is_empty() && tool_requests.is_empty() {
        tracing::error!(
            event_count,
            "ChatGPT Codex stream ended without assistant text or tool calls"
        );
        return match stop_reason.as_deref() {
            Some("incomplete" | "length") => Err(format!(
                "provider stopped at max_tokens ({max_tokens}) before producing assistant text"
            )),
            _ => Err(format!(
                "provider stream ended without assistant text or tool calls ({event_count} SSE events)"
            )),
        };
    }

    tracing::info!(
        event_count,
        text_chars = assistant_text.chars().count(),
        tool_calls = tool_requests.len(),
        "finished reading ChatGPT Codex SSE stream"
    );
    let _ = send(
        tx,
        AgentEvent::Status(format!(
            "provider: stream ended ({event_count} SSE events, {} text chars, {} tool calls)",
            assistant_text.chars().count(),
            tool_requests.len()
        )),
        cancel,
    );

    Ok(ProviderTurn { tool_requests, assistant_text, stop_reason, response_items, usage: Some(usage) })
}

pub(crate) fn collect_chatgpt_codex_event(
    event: codex::ResponsesSseEvent, tool_blocks: &mut HashMap<String, ChatToolCallBuilder>,
    tool_requests: &mut Vec<ToolUseRequest>, assistant_text: &mut String, stop_reason: &mut Option<String>,
    tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<(), String> {
    match event {
        codex::ResponsesSseEvent::TextDelta(text) => {
            assistant_text.push_str(&text);
            if send(tx, AgentEvent::AssistantDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        codex::ResponsesSseEvent::ReasoningDelta(text) => {
            if send(tx, AgentEvent::ReasoningDelta(text), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        codex::ResponsesSseEvent::ToolCallStart { id, call_id, name } => {
            tool_blocks.insert(
                id,
                ChatToolCallBuilder { id: call_id, name, arguments_json: String::new() },
            );
        }
        codex::ResponsesSseEvent::ToolCallArgumentsDelta { id, arguments } => {
            let block = tool_blocks.entry(id.clone()).or_insert_with(|| ChatToolCallBuilder {
                id,
                name: String::new(),
                arguments_json: String::new(),
            });
            block.arguments_json.push_str(&arguments);
        }
        codex::ResponsesSseEvent::ToolCallDone { id, call_id, name, arguments } => {
            let remove_id = id.clone();
            let block = tool_blocks.entry(id.clone()).or_insert_with(|| ChatToolCallBuilder {
                id: id.clone(),
                name: String::new(),
                arguments_json: String::new(),
            });
            if let Some(call_id) = call_id {
                block.id = call_id;
            }
            if !name.is_empty() {
                block.name = name;
            }
            if !arguments.is_empty() {
                block.arguments_json = arguments;
            }
            if let Some(block) = tool_blocks.remove(&remove_id)
                && let Some(req) = block.finish()
            {
                tool_requests.push(req);
            }
        }
        codex::ResponsesSseEvent::ResponseStatus(status) => {
            match status.as_str() {
                "completed" => *stop_reason = Some(status.clone()),
                "failed" | "incomplete" | "cancelled" | "canceled" => {
                    return Err(format!("provider stream status: {status}"));
                }
                _ => {}
            }
            if send(tx, AgentEvent::Status(format!("provider: status {status}")), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        codex::ResponsesSseEvent::Error(message) => {
            tracing::error!(error = %message, "ChatGPT Codex emitted SSE error");
            return Err(format!("provider error: {message}"));
        }
        codex::ResponsesSseEvent::Usage { .. } => {}
        codex::ResponsesSseEvent::UsageComponents(_) => {}
        codex::ResponsesSseEvent::OutputItem(_) => {}
        codex::ResponsesSseEvent::Malformed(payload) => {
            return Err(format!("malformed provider stream payload: {payload}"));
        }
        codex::ResponsesSseEvent::Done | codex::ResponsesSseEvent::Other => {}
    }

    Ok(())
}

fn log_sse_event(seq: usize, event_type: &str, data: &str) {
    let (content_type, delta_type, stop_reason) = summarize_sse_data(data);
    tracing::info!(
        seq,
        event_type,
        content_type = content_type.as_deref().unwrap_or(""),
        delta_type = delta_type.as_deref().unwrap_or(""),
        stop_reason = stop_reason.as_deref().unwrap_or(""),
        "received SSE event"
    );
}

fn summarize_sse_data(data: &str) -> (Option<String>, Option<String>, Option<String>) {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
    let content_type = v
        .get("content_block")
        .and_then(|cb| cb.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let delta_type = v
        .get("delta")
        .and_then(|d| d.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let stop_reason = v
        .get("delta")
        .and_then(|d| d.get("stop_reason"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    (content_type, delta_type, stop_reason)
}

pub(crate) fn collect_anthropic_event(
    event_type: &str, data: &str, state: &mut AnthropicStreamState, tx: &Sender<AgentEvent>, cancel: &CancelToken,
) -> Result<(), String> {
    let sse_event = anthropic::parse_sse_event(event_type, data);

    if let Some(usage) = extract_usage(data) {
        state.usage.merge_snapshot(&usage);
    }

    if event_type == "content_block_start"
        && let Some((index, block)) = extract_tool_use_start(data)
    {
        state.tool_blocks.insert(index, block);
    }

    if event_type == "content_block_start" {
        collect_content_block_start_text(
            data,
            &mut state.assistant_text,
            &mut state.provider_content_blocks,
            tx,
            cancel,
        )?;
    }

    match &sse_event {
        anthropic::SseEvent::TextDelta(text) => state.assistant_text.push_str(text),
        anthropic::SseEvent::InputJsonDelta { index, partial_json } => {
            if let Some(block) = state.tool_blocks.get_mut(index) {
                block.input_json.push_str(partial_json);
            }
        }
        anthropic::SseEvent::ContentBlockStop { index } => {
            if let Some(index) = index
                && let Some(block) = state.tool_blocks.remove(index)
                && let Some(req) = block.finish()
            {
                state.tool_requests.push(req);
            }
        }
        anthropic::SseEvent::MessageDelta { stop_reason: Some(reason) } => {
            state.stop_reason = Some(reason.clone());
            tracing::info!(stop_reason = %reason, "provider message stop reason");
        }
        anthropic::SseEvent::Error(msg) => {
            tracing::error!(error = %msg, "provider emitted SSE error");
            return Err(format!("provider error: {msg}"));
        }
        _ => {}
    }

    if let Some(agent_event) = anthropic::sse_to_agent_event(&sse_event)
        && send(tx, agent_event, cancel).is_none()
    {
        return Err("cancelled".to_string());
    }

    Ok(())
}

fn extract_usage(data: &str) -> Option<ProviderUsageComponents> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let usage = v
        .get("usage")
        .or_else(|| v.get("message").and_then(|m| m.get("usage")))
        .or_else(|| v.get("delta").and_then(|d| d.get("usage")))?;
    let input_tokens = usage.get("input_tokens").and_then(|t| t.as_u64());
    let output_tokens = usage.get("output_tokens").and_then(|t| t.as_u64());
    let cache_read_input_tokens = usage.get("cache_read_input_tokens").and_then(|value| value.as_u64());
    let cache_creation_input_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|value| value.as_u64());
    if input_tokens.is_none()
        && output_tokens.is_none()
        && cache_read_input_tokens.is_none()
        && cache_creation_input_tokens.is_none()
    {
        None
    } else {
        Some(ProviderUsageComponents {
            input_tokens,
            output_tokens,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            reasoning_tokens: None,
        })
    }
}

fn collect_content_block_start_text(
    data: &str, assistant_text: &mut String, provider_content_blocks: &mut Vec<String>, tx: &Sender<AgentEvent>,
    cancel: &CancelToken,
) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
    let Some(block) = v.get("content_block") else {
        return Ok(());
    };

    match block.get("type").and_then(|t| t.as_str()) {
        Some("text") => {
            let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
            if !text.is_empty() {
                assistant_text.push_str(text);
                if send(tx, AgentEvent::AssistantDelta(text.to_string()), cancel).is_none() {
                    return Err("cancelled".to_string());
                }
            }
        }
        Some("thinking") => {
            let thinking = block.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
            if !thinking.is_empty() && send(tx, AgentEvent::ReasoningDelta(thinking.to_string()), cancel).is_none() {
                return Err("cancelled".to_string());
            }
        }
        Some(other) => {
            provider_content_blocks.push(other.to_string());
            tracing::info!(content_type = other, "unhandled content block start");
        }
        None => {}
    }

    Ok(())
}

pub(crate) fn extract_tool_use_start(data: &str) -> Option<(usize, ToolUseBuilder)> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
    let cb = v.get("content_block")?;
    let block_type = cb.get("type").and_then(|t| t.as_str())?;
    if block_type != "tool_use" {
        return None;
    }
    let name = cb.get("name").and_then(|n| n.as_str())?.to_string();
    let id = cb.get("id").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let initial_input = cb.get("input").cloned().unwrap_or(serde_json::Value::Null);
    Some((
        index,
        ToolUseBuilder { id, name, initial_input, input_json: String::new() },
    ))
}
