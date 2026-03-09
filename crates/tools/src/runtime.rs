//! Tool execution runtime with sandboxing
//!
//! Provides safe execution of tools within a sandboxed environment.

use similar::TextDiff;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thndrs_mem::{MemoryDatabase, MemoryKind, MemoryStore};
use tokio::process::Command;
use tokio::time::{Duration, timeout};

/// Global memory store for tool execution
static MEMORY_STORE: std::sync::OnceLock<std::sync::Mutex<Option<MemoryStore>>> = std::sync::OnceLock::new();

/// Initialize the global memory store
pub fn init_memory_store(workspace_path: &Path) -> Result<(), String> {
    let db =
        MemoryDatabase::for_workspace(workspace_path).map_err(|e| format!("Failed to open memory database: {}", e))?;
    let store = MemoryStore::new(db);

    let _ = MEMORY_STORE.set(std::sync::Mutex::new(Some(store)));
    Ok(())
}

/// Get the global memory store
fn get_memory_store() -> Result<std::sync::MutexGuard<'static, Option<MemoryStore>>, String> {
    MEMORY_STORE
        .get()
        .ok_or_else(|| "Memory store not initialized".to_string())
        .map(|m| m.lock().map_err(|e| format!("Lock error: {}", e)))?
}

/// Result of a tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub status: ToolStatus,
    pub content: String,
    pub tool_use_id: Option<String>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(content: impl Into<String>) -> Self {
        Self { status: ToolStatus::Success, content: content.into(), tool_use_id: None }
    }

    /// Create an error result
    pub fn error(content: impl Into<String>) -> Self {
        Self { status: ToolStatus::Error, content: content.into(), tool_use_id: None }
    }

    /// Set the tool use ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.tool_use_id = Some(id.into());
        self
    }

    /// Convert to JSON format for provider consumption
    pub fn to_provider_format(&self) -> serde_json::Value {
        let mut result = serde_json::json!({
            "status": match self.status {
                ToolStatus::Success => "success",
                ToolStatus::Error => "error",
            },
            "content": &self.content
        });
        if let Some(id) = &self.tool_use_id {
            result["tool_use_id"] = serde_json::json!(id);
        }
        result
    }
}

/// Status of a tool execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Success,
    Error,
}

/// Tool execution context with sandbox path
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub sandbox_path: PathBuf,
}

impl ToolContext {
    /// Create a new context with sandbox path
    pub fn new(sandbox_path: impl Into<PathBuf>) -> Self {
        Self { sandbox_path: sandbox_path.into() }
    }

    /// Resolve a path relative to the sandbox
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> Result<PathBuf, String> {
        let path = path.as_ref();
        let resolved = if path.is_absolute() { path.to_path_buf() } else { self.sandbox_path.join(path) };

        let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
        let canonical_sandbox = self
            .sandbox_path
            .canonicalize()
            .unwrap_or_else(|_| self.sandbox_path.clone());

        if !canonical.starts_with(&canonical_sandbox) {
            return Err(format!(
                "Path traversal detected: {} is outside sandbox",
                path.display()
            ));
        }

        Ok(resolved)
    }
}

/// Trait for tool executors
#[async_trait::async_trait]
pub trait ToolExecutor {
    async fn execute(&self, arguments: &HashMap<String, serde_json::Value>, context: &ToolContext) -> ToolResult;
}

/// Execute the read tool
pub async fn execute_read(arguments: &HashMap<String, serde_json::Value>, sandbox_path: &Path) -> ToolResult {
    let context = ToolContext::new(sandbox_path);

    let path_str = match arguments.get("path") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: path"),
    };

    let path = match context.resolve_path(path_str) {
        Ok(p) => p,
        Err(e) => return ToolResult::error(e),
    };

    if !path.exists() {
        return ToolResult::error(format!("File not found: {}", path.display()));
    }

    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) => return ToolResult::error(format!("Cannot access file: {e}")),
    };

    if metadata.file_type().is_symlink() {
        match path.canonicalize() {
            Ok(p) => {
                if !p.exists() {
                    return ToolResult::error(format!("Broken symlink: {}", path.display()));
                }
            }
            Err(e) => return ToolResult::error(format!("Cannot resolve symlink: {e}")),
        }
    }

    if path.is_dir() {
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let mut items = Vec::new();
                for entry in entries.flatten() {
                    let file_name = entry.file_name();
                    let name = file_name.to_string_lossy();
                    let file_type = entry.file_type().ok();
                    let icon = if file_type.as_ref().map(|ft| ft.is_dir()).unwrap_or(false) { "▶" } else { "⌸" };
                    items.push(format!("{} {}", icon, name));
                }
                items.sort();
                return ToolResult::success(items.join("\n"));
            }
            Err(e) => return ToolResult::error(format!("Cannot read directory: {e}")),
        }
    }

    let is_image = match infer::get_from_path(&path) {
        Ok(Some(kind)) => kind.mime_type().starts_with("image/"),
        _ => {
            matches!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .as_deref(),
                Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp")
            )
        }
    };

    if is_image {
        match std::fs::read(&path) {
            Ok(bytes) => {
                use base64::Engine;
                let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let mime_type = infer::get_from_path(&path)
                    .ok()
                    .flatten()
                    .map(|k| k.mime_type().to_string())
                    .unwrap_or_else(|| "image/png".to_string());
                ToolResult::success(format!(
                    "[Image: {} bytes, mime: {}]\nbase64:{}",
                    bytes.len(),
                    mime_type,
                    base64_data
                ))
            }
            Err(e) => ToolResult::error(format!("Cannot read image: {e}")),
        }
    } else {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();

                if lines.is_empty() {
                    return ToolResult::success(format!("File is empty: {}", path.display()));
                }

                let offset = arguments.get("offset").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
                let limit = arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2000)
                    .min(2000) as usize;

                let start = offset.saturating_sub(1);
                let end = (start + limit).min(lines.len());

                if start >= lines.len() {
                    return ToolResult::error(format!(
                        "Offset {} is beyond file length of {} lines",
                        offset,
                        lines.len()
                    ));
                }

                let max_line_num = end;
                let num_width = max_line_num.to_string().len();

                let mut result = String::new();
                for (idx, line) in lines[start..end].iter().enumerate() {
                    let line_num = start + idx + 1;
                    result.push_str(&format!("{: >num_width$}\t{}\n", line_num, line));
                }

                if end < lines.len() {
                    result.push_str(&format!("\n[... {} more lines]", lines.len() - end));
                }

                ToolResult::success(result)
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::InvalidData => {
                    ToolResult::error(format!("Binary file, cannot display: {}", path.display()))
                }
                _ => ToolResult::error(format!("Cannot read file: {e}")),
            },
        }
    }
}

/// Execute the write tool
pub async fn execute_write(arguments: &HashMap<String, serde_json::Value>, sandbox_path: &Path) -> ToolResult {
    let context = ToolContext::new(sandbox_path);

    let path_str = match arguments.get("path") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: path"),
    };

    let content = match arguments.get("content") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: content"),
    };

    let path = match context.resolve_path(path_str) {
        Ok(p) => p,
        Err(e) => return ToolResult::error(e),
    };

    if path.is_dir() {
        return ToolResult::error(format!("Cannot write to directory: {}", path.display()));
    }

    let content_bytes = content.as_bytes();
    if content_bytes.len() > 10 * 1024 * 1024 {
        return ToolResult::error(format!("Content too large: {} bytes (max 10MB)", content_bytes.len()));
    }

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ToolResult::error(format!("Cannot create parent directories: {e}"));
    }

    match std::fs::write(&path, content) {
        Ok(_) => ToolResult::success(format!("Wrote {} bytes to {}", content_bytes.len(), path.display())),
        Err(e) => ToolResult::error(format!("Failed to write file: {e}")),
    }
}

/// Execute the edit tool
pub async fn execute_edit(arguments: &HashMap<String, serde_json::Value>, sandbox_path: &Path) -> ToolResult {
    let context = ToolContext::new(sandbox_path);

    let path_str = match arguments.get("path") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: path"),
    };

    let diff = match arguments.get("diff") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: diff"),
    };

    let path = match context.resolve_path(path_str) {
        Ok(p) => p,
        Err(e) => return ToolResult::error(e),
    };

    if !path.exists() {
        return ToolResult::error(format!("File not found: {}", path.display()));
    }

    if path.is_dir() {
        return ToolResult::error(format!("Cannot edit directory: {}", path.display()));
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return ToolResult::error(format!("Cannot read file: {e}")),
    };

    match apply_diff(&content, diff) {
        Ok(new_content) => match std::fs::write(&path, &new_content) {
            Ok(_) => {
                let excerpt = build_edit_readback(&new_content, diff);
                if excerpt.is_empty() {
                    ToolResult::success(format!(
                        "Successfully edited {} ({} bytes)",
                        path.display(),
                        new_content.len()
                    ))
                } else {
                    ToolResult::success(format!(
                        "Successfully edited {} ({} bytes)\n\n{}",
                        path.display(),
                        new_content.len(),
                        excerpt
                    ))
                }
            }
            Err(e) => ToolResult::error(format!("Failed to write file: {e}")),
        },
        Err(e) => ToolResult::error(format!("Patch failed: {e}")),
    }
}

fn build_edit_readback(content: &str, diff: &str) -> String {
    let Some((start, end)) = parse_changed_new_range(diff) else {
        return String::new();
    };

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return "File is now empty.".to_string();
    }

    if start > lines.len() {
        return String::new();
    }

    let max_lines = 120usize;
    let bounded_end = end.min(lines.len());
    let excerpt_end = bounded_end.min(start + max_lines - 1);
    let line_num_width = excerpt_end.to_string().len().max(2);

    let mut out = format!("Updated lines {}-{}:\n", start, excerpt_end);
    for line_num in start..=excerpt_end {
        let idx = line_num - 1;
        out.push_str(&format!("{: >line_num_width$}\t{}\n", line_num, lines[idx]));
    }

    if excerpt_end < bounded_end {
        out.push_str(&format!(
            "[... {} more changed lines omitted]",
            bounded_end - excerpt_end
        ));
    }

    out
}

fn parse_changed_new_range(diff: &str) -> Option<(usize, usize)> {
    let mut min_start = usize::MAX;
    let mut max_end = 0usize;

    for line in diff.lines() {
        if !line.starts_with("@@") {
            continue;
        }

        let parts = line.split_whitespace().collect::<Vec<_>>();
        let Some(new_range) = parts.get(2) else {
            continue;
        };

        let range = new_range.trim_start_matches('+');
        let mut segments = range.split(',');
        let Some(start) = segments.next().and_then(|v| v.parse::<usize>().ok()) else {
            continue;
        };
        let count = segments
            .next()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1);
        let end = start.saturating_add(count.saturating_sub(1));

        min_start = min_start.min(start);
        max_end = max_end.max(end);
    }

    if min_start == usize::MAX { None } else { Some((min_start.max(1), max_end.max(min_start))) }
}

/// Apply a unified diff to content
///
/// Parses the unified diff format and applies it to the old content.
/// Returns the new content if successful, or an error message if the diff is invalid.
fn apply_diff(old_content: &str, diff: &str) -> Result<String, String> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let mut result = old_lines.clone();
    let mut current_line = 0;
    let mut in_hunk = false;

    for line in diff.lines() {
        if line.starts_with("@@") {
            in_hunk = true;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2
                && let Some(old_range) = parts.get(1)
            {
                let range_str = old_range.trim_start_matches('-');
                let range_parts: Vec<&str> = range_str.split(',').collect();
                if let Some(start_str) = range_parts.first()
                    && let Ok(start) = start_str.parse::<i64>()
                {
                    current_line = if start >= 0 { (start - 1) as usize } else { 0 };
                }
            }
        } else if in_hunk && !line.is_empty() {
            match line.chars().next() {
                Some(' ') => {
                    let content = &line[1..];
                    if current_line < result.len() && result[current_line] != content {
                        return Err(format!(
                            "Context mismatch at line {}: expected '{}', found '{}'",
                            current_line + 1,
                            content,
                            result[current_line]
                        ));
                    }
                    current_line += 1;
                }
                Some('-') => {
                    let content = &line[1..];
                    if current_line >= result.len() || result[current_line] != content {
                        return Err(format!("Cannot remove line {}: context mismatch", current_line + 1));
                    }
                    result.remove(current_line);
                }
                Some('+') => {
                    let content = &line[1..];
                    result.insert(current_line, content);
                    current_line += 1;
                }
                _ => {}
            }
        }
    }

    Ok(result.join("\n"))
}

/// Generate a unified diff between old and new content using similar crate
pub fn generate_diff(old_content: &str, new_content: &str, old_path: &str, new_path: &str) -> String {
    let diff = TextDiff::from_lines(old_content, new_content);
    let mut output = String::new();

    output.push_str(&format!("--- {}\n", old_path));
    output.push_str(&format!("+++ {}\n", new_path));

    for group in diff.grouped_ops(3) {
        let (mut old_start, mut old_count) = (usize::MAX, 0);
        let (mut new_start, mut new_count) = (usize::MAX, 0);

        for op in &group {
            for change in diff.iter_inline_changes(op) {
                if let Some(idx) = change.old_index() {
                    old_start = old_start.min(idx);
                    old_count += 1;
                }
                if let Some(idx) = change.new_index() {
                    new_start = new_start.min(idx);
                    new_count += 1;
                }
            }
        }

        let old_start = if old_start == usize::MAX { 0 } else { old_start };
        let new_start = if new_start == usize::MAX { 0 } else { new_start };

        output.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start + 1,
            old_count,
            new_start + 1,
            new_count
        ));

        for op in group {
            for change in diff.iter_changes(&op) {
                match change.tag() {
                    similar::ChangeTag::Delete => {
                        output.push_str(&format!("-{}", change.value()));
                    }
                    similar::ChangeTag::Insert => {
                        output.push_str(&format!("+{}", change.value()));
                    }
                    similar::ChangeTag::Equal => {
                        output.push_str(&format!(" {}", change.value()));
                    }
                }
            }
        }
    }

    output
}

/// Execute the bash tool
pub async fn execute_bash(arguments: &HashMap<String, serde_json::Value>, sandbox_path: &Path) -> ToolResult {
    let command = match arguments.get("command") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: command"),
    };

    if command.is_empty() {
        return ToolResult::error("Command cannot be empty");
    }

    let result = timeout(Duration::from_secs(120), async {
        Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(sandbox_path)
            .output()
            .await
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut combined = String::new();
            if !stdout.is_empty() {
                combined.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&stderr);
            }

            let max_output = 100 * 1024;
            let was_truncated = combined.len() > max_output;
            let mut content =
                if combined.len() > max_output { combined[..max_output].to_string() } else { combined.to_string() };

            if was_truncated {
                content.push_str("\n[output truncated at 100KB]");
            }

            if exit_code == 0 {
                ToolResult::success(content)
            } else {
                ToolResult::error(format!("Command exited with code {}\n{}", exit_code, content))
            }
        }
        Ok(Err(e)) => ToolResult::error(format!("Failed to execute command: {e}")),
        Err(_) => ToolResult::error("Command timed out after 120 seconds"),
    }
}

/// Execute the research tool
pub async fn execute_research(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let url = match arguments.get("url") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: url"),
    };

    if url.is_empty() {
        return ToolResult::error("URL cannot be empty");
    }

    let url_parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(e) => return ToolResult::error(format!("Invalid URL: {e}")),
    };

    if url_parsed.scheme() != "https" {
        return ToolResult::error("Only HTTPS URLs are allowed");
    }

    if let Some(host) = url_parsed.host_str()
        && is_private_host(host)
    {
        return ToolResult::error(format!("Cannot fetch from private/internal network: {}", host));
    }

    use reqwest;
    use tokio::time::{Duration, timeout};

    let client = match reqwest::Client::builder().timeout(Duration::from_secs(30)).build() {
        Ok(c) => c,
        Err(e) => return ToolResult::error(format!("Failed to create HTTP client: {e}")),
    };

    let result = timeout(Duration::from_secs(30), async {
        client
            .get(url)
            .header("Accept", "text/html,application/json,text/plain")
            .header("User-Agent", "Thunderus/1.0")
            .send()
            .await
    })
    .await;

    match result {
        Ok(Ok(response)) => {
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("text/plain")
                .to_string();

            if content_type.starts_with("image/")
                || content_type.starts_with("video/")
                || content_type.starts_with("audio/")
                || content_type.contains("application/octet-stream")
            {
                return ToolResult::error(format!("Binary content type not supported: {}", content_type));
            }

            let text = match response.text().await {
                Ok(t) => t,
                Err(e) => return ToolResult::error(format!("Failed to read response: {e}")),
            };

            let processed = match content_type {
                ct if ct.contains("application/json") => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(v) => serde_json::to_string_pretty(&v).unwrap_or(text),
                    Err(_) => text,
                },
                ct if ct.contains("text/html") => extract_article_content(&text, url),
                _ => text,
            };

            let max_size = 50 * 1024;
            let was_truncated = processed.len() > max_size;
            let mut result = if was_truncated { processed[..max_size].to_string() } else { processed };

            if was_truncated {
                result.push_str("\n[content truncated at 50KB]");
            }

            ToolResult::success(result)
        }
        Ok(Err(e)) => {
            if e.is_timeout() {
                ToolResult::error("Request timed out after 30 seconds")
            } else {
                ToolResult::error(format!("Request failed: {e}"))
            }
        }
        Err(_) => ToolResult::error("Request timed out after 30 seconds"),
    }
}

/// Check if a host is in a private/internal network
fn is_private_host(host: &str) -> bool {
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return true;
    }

    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        match addr {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();

                if octets[0] == 10 {
                    return true;
                }

                if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                    return true;
                }

                if octets[0] == 192 && octets[1] == 168 {
                    return true;
                }
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() {
                    return true;
                }
            }
        }
    }

    false
}

/// Extract article content from HTML using Lectito's readability engine.
/// Falls back to naive regex extraction on parse failure.
fn extract_article_content(html: &str, url: &str) -> String {
    let result = lectito_core::parse_with_url(html, url);

    match result {
        Ok(article) => {
            let mut output = String::new();

            if let Some(title) = &article.metadata.title {
                let title = title.trim();
                if !title.is_empty() {
                    output.push_str(&format!("# {}\n\n", title));
                }
            }

            let mut meta_parts = Vec::new();
            if let Some(author) = &article.metadata.author {
                let author = author.trim();
                if !author.is_empty() {
                    meta_parts.push(format!("Author: {}", author));
                }
            }
            if let Some(date) = &article.metadata.date {
                let date = date.trim();
                if !date.is_empty() {
                    meta_parts.push(format!("Date: {}", date));
                }
            }
            if let Some(excerpt) = &article.metadata.excerpt {
                let excerpt = excerpt.trim();
                if !excerpt.is_empty() {
                    meta_parts.push(format!("Excerpt: {}", excerpt));
                }
            }

            if !meta_parts.is_empty() {
                output.push_str(&meta_parts.join("\n"));
                output.push_str("\n\n---\n\n");
            }

            let content = article.text_content.trim();
            if content.is_empty() {
                extract_text_from_html(html)
            } else {
                output.push_str(content);
                output
            }
        }
        Err(_) => extract_text_from_html(html),
    }
}

/// Extract text content from HTML (naive regex fallback)
fn extract_text_from_html(html: &str) -> String {
    let mut text = html.to_string();

    let script_regex = regex::Regex::new(r"<script[^\u003e]*>.*?\u003c/script>").unwrap();
    text = script_regex.replace_all(&text, "").to_string();

    let style_regex = regex::Regex::new(r"<style[^\u003e]*>.*?\u003c/style>").unwrap();
    text = style_regex.replace_all(&text, "").to_string();

    text = text.replace("</p>", "\n\n");
    text = text.replace("<br>", "\n");
    text = text.replace("<br/>", "\n");
    text = text.replace("</div>", "\n");
    text = text.replace("</li>", "\n");

    let tag_regex = regex::Regex::new(r"<[^\u003e]+>").unwrap();
    text = tag_regex.replace_all(&text, "").to_string();

    text = text.replace("&amp;", "&");
    text = text.replace("&lt;", "<");
    text = text.replace("&gt;", ">");
    text = text.replace("&quot;", "\"");
    text = text.replace("&#39;", "'");
    text = text.replace("&nbsp;", " ");

    let newline_regex = regex::Regex::new(r"\n{3,}").unwrap();
    text = newline_regex.replace_all(&text, "\n\n").to_string();

    text.trim().to_string()
}

/// Execute the memory_store tool
#[allow(clippy::await_holding_lock)]
pub async fn execute_memory_store(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let content = match arguments.get("content") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: content"),
    };

    if content.is_empty() {
        return ToolResult::error("Content cannot be empty");
    }

    let kind_str = arguments.get("kind").and_then(|v| v.as_str()).unwrap_or("fact");
    let kind = match kind_str {
        "fact" => MemoryKind::Fact,
        "preference" => MemoryKind::Preference,
        "procedure" => MemoryKind::Procedure,
        "context" => MemoryKind::Context,
        _ => MemoryKind::Fact,
    };

    let tags: Vec<String> = arguments
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let mut store_guard = match get_memory_store() {
        Ok(guard) => guard,
        Err(e) => return ToolResult::error(format!("Memory store error: {}", e)),
    };

    let store = match store_guard.as_mut() {
        Some(s) => s,
        None => return ToolResult::error("Memory store not available"),
    };

    match store.store(content, kind, Some("tool"), tags).await {
        Ok(id) => ToolResult::success(format!("Memory stored with ID: {}", id)),
        Err(e) => ToolResult::error(format!("Failed to store memory: {}", e)),
    }
}

/// Execute the memory_recall tool
#[allow(clippy::await_holding_lock)]
pub async fn execute_memory_recall(arguments: &HashMap<String, serde_json::Value>) -> ToolResult {
    let query = match arguments.get("query") {
        Some(v) => v.as_str().unwrap_or_default(),
        None => return ToolResult::error("Missing required argument: query"),
    };

    if query.is_empty() {
        return ToolResult::error("Query cannot be empty");
    }

    let kind = arguments.get("kind").and_then(|v| v.as_str()).and_then(|s| match s {
        "fact" => Some(MemoryKind::Fact),
        "preference" => Some(MemoryKind::Preference),
        "procedure" => Some(MemoryKind::Procedure),
        "context" => Some(MemoryKind::Context),
        _ => None,
    });

    let count = arguments
        .get("count")
        .and_then(|v| v.as_u64())
        .map(|c| c.clamp(1, 20) as usize)
        .unwrap_or(5);

    let mut store_guard = match get_memory_store() {
        Ok(guard) => guard,
        Err(e) => return ToolResult::error(format!("Memory store error: {}", e)),
    };

    let store = match store_guard.as_mut() {
        Some(s) => s,
        None => return ToolResult::error("Memory store not available"),
    };

    match store.recall(query, count, kind, None, 0.3).await {
        Ok(memories) => {
            if memories.is_empty() {
                ToolResult::success("No relevant memories found.")
            } else {
                let results: Vec<String> = memories
                    .iter()
                    .map(|m| {
                        let sim = m
                            .similarity
                            .map(|s| format!(" (similarity: {:.2})", s))
                            .unwrap_or_default();
                        format!("- [{}] {}{}", m.kind.as_str(), m.content, sim)
                    })
                    .collect();
                ToolResult::success(format!("Found {} memories:\n{}", results.len(), results.join("\n")))
            }
        }
        Err(e) => ToolResult::error(format!("Failed to recall memories: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("Hello");
        assert!(matches!(result.status, ToolStatus::Success));
        assert_eq!(result.content, "Hello");
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("Failed");
        assert!(matches!(result.status, ToolStatus::Error));
        assert_eq!(result.content, "Failed");
    }

    #[test]
    fn test_tool_context_resolve_absolute_path() {
        let ctx = ToolContext::new("/home/user/repo");
        let result = ctx.resolve_path("/home/user/repo/src/main.rs");
        assert!(result.is_ok());
    }

    #[test]
    fn test_tool_context_resolve_relative_path() {
        let ctx = ToolContext::new("/home/user/repo");
        let result = ctx.resolve_path("src/main.rs");
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_private_host() {
        assert!(is_private_host("localhost"));
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("10.0.0.1"));
        assert!(is_private_host("172.16.0.1"));
        assert!(is_private_host("192.168.1.1"));
        assert!(!is_private_host("example.com"));
        assert!(!is_private_host("8.8.8.8"));
    }

    #[test]
    fn test_extract_text_from_html() {
        let html = "<html><body><p>Hello <b>world</b>!</p></body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn test_apply_diff_add() {
        let content = "line 1\nline 2\nline 3";
        let diff = "@@ -2,1 +2,2 @@\n line 2\n+new line";
        let result = apply_diff(content, diff).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[1], "line 2");
        assert_eq!(lines[2], "new line");
    }

    #[test]
    fn test_apply_diff_remove() {
        let content = "line 1\nline 2\nline 3";
        let diff = "@@ -2,2 +2,1 @@\n-line 2\n line 3";
        let result = apply_diff(content, diff).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "line 1");
        assert_eq!(lines[1], "line 3");
    }

    #[test]
    fn test_generate_diff() {
        let old_content = "Hello\nWorld\n";
        let new_content = "Hello\nRust\nWorld\n";
        let diff = generate_diff(old_content, new_content, "old.txt", "new.txt");

        assert!(diff.contains("--- old.txt"));
        assert!(diff.contains("+++ new.txt"));
        assert!(diff.contains("@@"));
        assert!(diff.contains("Hello"));
        assert!(diff.contains("Rust"));
        assert!(diff.contains("World"));
    }

    #[test]
    fn test_parse_changed_new_range() {
        let diff = "@@ -3,2 +4,3 @@\n old\n+new\n@@ -12,1 +20,2 @@\n-old\n+new";
        let range = parse_changed_new_range(diff).unwrap();
        assert_eq!(range, (4, 21));
    }

    #[test]
    fn test_build_edit_readback() {
        let content = "a\nb\nc\nd\ne\nf";
        let diff = "@@ -2,1 +2,2 @@\n-b\n+b\n+bb";
        let readback = build_edit_readback(content, diff);
        assert!(readback.contains("Updated lines 2-3:"));
        assert!(readback.contains("2\tb"));
        assert!(readback.contains("3\tc"));
    }
}
