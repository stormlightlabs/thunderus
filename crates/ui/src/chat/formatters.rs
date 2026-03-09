use super::extractors;
use chrono::{DateTime, Local, Utc};
use serde_json::Value;

pub fn write_success_ln(output: &str) -> Option<String> {
    let line = output.lines().next()?.trim();
    let rest = line.strip_prefix("Wrote ")?;
    let (byte_part, path) = rest.split_once(" to ")?;
    let bytes = byte_part.strip_suffix(" bytes")?;
    Some(format!("Wrote {bytes} bytes -> {path}"))
}

pub fn tool_args(name: &str, raw_arguments: &str, terminal_width: u16) -> String {
    let parsed = serde_json::from_str::<Value>(raw_arguments);
    let Ok(value) = parsed else {
        return raw_arguments.to_string();
    };

    let Some(obj) = value.as_object() else {
        return json_compact(&value);
    };

    match name {
        "read" => {
            let path = obj.get("path").and_then(Value::as_str).unwrap_or("<path>");
            let offset = obj.get("offset").and_then(Value::as_u64).unwrap_or(1);
            if let Some(limit) = obj.get("limit").and_then(Value::as_u64) {
                return format!("{path}  ({offset}..{})", offset + limit);
            }
            if obj.contains_key("offset") {
                return format!("{path}  (from {offset})");
            }
            path.to_string()
        }
        "write" => obj.get("path").and_then(Value::as_str).unwrap_or("<path>").to_string(),
        "edit" => obj
            .get("diff")
            .and_then(Value::as_str)
            .and_then(extractors::diff_path)
            .or_else(|| obj.get("path").and_then(Value::as_str).map(ToString::to_string))
            .unwrap_or_else(|| "<path>".to_string()),
        "bash" => {
            let command = obj.get("command").and_then(Value::as_str).unwrap_or("bash");
            truncate_for_width(command, terminal_width.saturating_sub(18))
        }
        "research" => obj.get("url").and_then(Value::as_str).unwrap_or("<url>").to_string(),
        "memory_store" => {
            let kind = obj.get("kind").and_then(Value::as_str).unwrap_or("fact");
            let content = obj.get("content").and_then(Value::as_str).unwrap_or("");
            let snippet_width = terminal_width.saturating_sub(24).clamp(24, 64) as usize;
            let snippet = truncate_text(content, snippet_width);
            format!("{kind}: {snippet}")
        }
        "memory_recall" => {
            let query = obj.get("query").and_then(Value::as_str).unwrap_or_default();
            let count = obj.get("count").and_then(Value::as_u64).unwrap_or(5);
            let kind = obj.get("kind").and_then(Value::as_str).unwrap_or("any");
            format!("query: {query}  |  count: {count}  |  kind: {kind}")
        }
        _ => {
            let mut parts = Vec::new();
            for (idx, (key, value)) in obj.iter().enumerate() {
                if idx >= 4 {
                    parts.push(format!("+{} more", obj.len().saturating_sub(idx)));
                    break;
                }
                parts.push(format!("{key}: {}", json_compact(value)));
            }
            format!("Args: {}", parts.join("  |  "))
        }
    }
}

pub fn tool_output(name: &str, raw_output: &str) -> String {
    let normalized = normalize_display_content(raw_output);
    if name == "memory_recall" {
        return normalized.replacen(":\n- ", ":\n\n- ", 1);
    }
    normalized
}

pub fn json_compact(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Array(values) => {
            let preview = values.iter().take(3).map(json_compact).collect::<Vec<_>>().join(", ");
            if values.len() > 3 {
                format!("[{}, +{} more]", preview, values.len() - 3)
            } else {
                format!("[{}]", preview)
            }
        }
        Value::Object(map) => {
            let preview = map
                .iter()
                .take(3)
                .map(|(key, value)| format!("{key}: {}", json_compact(value)))
                .collect::<Vec<_>>()
                .join(", ");
            if map.len() > 3 {
                format!("{{{}, +{} more}}", preview, map.len() - 3)
            } else {
                format!("{{{}}}", preview)
            }
        }
    }
}

pub fn normalize_display_content(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.replace(":- ", ":\n- ");
    let normalized = normalized.replace(":-\n", ":\n- ");
    let normalized = fix_missing_bullet_breaks(&normalized);
    collapse_blank_lines(normalized.trim())
}

pub fn truncate_for_width(value: &str, width: u16) -> String {
    truncate_text(value, width.max(8) as usize)
}

pub fn truncate_text(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let mut truncated = chars.into_iter().take(keep).collect::<String>();
    truncated.push('…');
    truncated
}

pub fn msg_timestamp(created_at: DateTime<Utc>) -> String {
    let local_time = created_at.with_timezone(&Local);
    let now_local = Local::now();
    if local_time.date_naive() == now_local.date_naive() {
        local_time.format("%H:%M").to_string()
    } else {
        local_time.format("%Y-%m-%d %H:%M").to_string()
    }
}

pub fn u32_with_grouping(value: u32) -> String {
    let mut out = String::new();
    let digits = value.to_string();

    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }

    out.chars().rev().collect()
}

pub fn usd(value: f64) -> String {
    if value < 0.0001 { format!("${value:.6}") } else { format!("${value:.4}") }
}

fn fix_missing_bullet_breaks(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut output = String::with_capacity(content.len() + 8);
    for idx in 0..chars.len() {
        let current = chars[idx];
        let next = chars.get(idx + 1).copied();
        let previous = if idx > 0 { Some(chars[idx - 1]) } else { None };

        if current == '-'
            && next == Some(' ')
            && let Some(prev) = previous
            && prev != '\n'
            && prev != '\r'
            && (prev == ':' || prev == ';' || prev == ')' || prev == ']' || prev == '"' || prev.is_alphanumeric())
        {
            output.push('\n');
        }

        output.push(current);
    }

    output
}

fn collapse_blank_lines(content: &str) -> String {
    let mut lines = Vec::new();
    let mut blank_run = 0usize;

    for line in content.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
            lines.push(String::new());
            continue;
        }

        blank_run = 0;
        lines.push(line.to_string());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assistant_display_content_inserts_missing_bullet_breaks() {
        let formatted = normalize_display_content("I can help with:- one- two");
        assert_eq!(formatted, "I can help with:\n- one\n- two");
    }

    #[test]
    fn test_format_tool_arguments_memory_recall() {
        let formatted = tool_args("memory_recall", r#"{"query":"style","count":5}"#, 120);
        assert!(formatted.contains("query: style"));
        assert!(formatted.contains("count: 5"));
    }

    #[test]
    fn test_format_tool_arguments_read_includes_range() {
        let formatted = tool_args("read", r#"{"path":"src/lib.rs","offset":10,"limit":5}"#, 120);
        assert_eq!(formatted, "src/lib.rs  (10..15)");
    }

    #[test]
    fn test_format_tool_arguments_edit_prefers_diff_path() {
        let formatted = tool_args(
            "edit",
            r#"{"path":"src/old.rs","diff":"--- a/src/old.rs\n+++ b/src/new.rs\n@@ -1 +1 @@\n-old\n+new"}"#,
            120,
        );
        assert_eq!(formatted, "src/new.rs");
    }

    #[test]
    fn test_format_write_success_line() {
        let formatted = write_success_ln("Wrote 42 bytes to src/main.rs").expect("line should parse");
        assert_eq!(formatted, "Wrote 42 bytes -> src/main.rs");
    }
}
