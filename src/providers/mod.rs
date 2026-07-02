//! Provider implementations.

use serde::{Deserialize, Serialize};

use crate::tools::ToolUseRequest;

pub mod umans;

/// A structured content block in the provider-neutral Anthropic-style message
/// format.
///
/// New provider routes can either use this directly or convert from it at their
/// boundary. Umans currently sends it to `/v1/messages`; a future OpenAI
/// compatible route should convert this shape into chat-completions messages
/// instead of mixing both wire formats in the agent loop.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderContentBlock {
    /// A plain text block.
    Text { text: String },
    /// A tool-use request emitted by the assistant.
    ToolUse {
        /// Provider-assigned id (e.g. `toolu_01`), echoed back in `tool_result`.
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool result returned to the model in a `user`-role message.
    ToolResult {
        /// Must match the `id` of the originating `tool_use` block.
        tool_use_id: String,
        content: String,
        /// Umans/Anthropic accept `is_error` as a bool.
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Message content: either a plain string or structured content blocks.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderMessageContent {
    /// Plain string content.
    Text(String),
    /// Structured content blocks.
    Blocks(Vec<ProviderContentBlock>),
}

impl Serialize for ProviderMessageContent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ProviderMessageContent::Text(s) => serializer.serialize_str(s),
            ProviderMessageContent::Blocks(blocks) => blocks.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ProviderMessageContent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::String(s) => Ok(ProviderMessageContent::Text(s)),
            serde_json::Value::Array(arr) => {
                let blocks: Vec<ProviderContentBlock> =
                    serde_json::from_value(serde_json::Value::Array(arr)).map_err(serde::de::Error::custom)?;
                Ok(ProviderMessageContent::Blocks(blocks))
            }
            _ => Err(serde::de::Error::custom("expected string or array for message content")),
        }
    }
}

impl ProviderMessageContent {
    /// Return the concatenated text of all `Text` blocks, or the plain string.
    pub fn as_text(&self) -> String {
        match self {
            ProviderMessageContent::Text(s) => s.clone(),
            ProviderMessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ProviderContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

/// Provider-neutral conversation message used by the agent loop.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderMessage {
    pub role: String,
    pub content: ProviderMessageContent,
}

impl ProviderMessage {
    pub fn user(content: &str) -> Self {
        ProviderMessage { role: "user".to_string(), content: ProviderMessageContent::Text(content.to_string()) }
    }

    pub fn assistant(content: &str) -> Self {
        ProviderMessage { role: "assistant".to_string(), content: ProviderMessageContent::Text(content.to_string()) }
    }

    /// Create a `user`-role message containing one `tool_result` block.
    pub fn tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        ProviderMessage {
            role: "user".to_string(),
            content: ProviderMessageContent::Blocks(vec![ProviderContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: Some(is_error),
            }]),
        }
    }

    /// Create an `assistant`-role message from content blocks.
    pub fn assistant_blocks(blocks: Vec<ProviderContentBlock>) -> Self {
        ProviderMessage { role: "assistant".to_string(), content: ProviderMessageContent::Blocks(blocks) }
    }

    /// Return the concatenated text content of this message.
    pub fn as_text(&self) -> String {
        self.content.as_text()
    }
}

/// Provider-neutral result of one streamed model turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderTurn {
    pub tool_requests: Vec<ToolUseRequest>,
    pub assistant_text: String,
    pub stop_reason: Option<String>,
}
