//! Local web search and article extraction.
//!
//! This is the local fallback for when Umans server-side search is disabled
//! (`websearch: none`). It uses DuckDuckGo's HTML endpoint for search and the
//! Lectito crate for article extraction.
//!
//! - Search: sync HTTP via [`ureq`] against `html.duckduckgo.com/html/`.
//! - Parsing: ported from lectito's mcp bin, using [`scraper`] for CSS
//!   selectors.
//! - Extraction: [`lectito::extract`] for HTML → Markdown/text.
//! - Bot-challenge detection: checks for known DDG anomaly markers.
//! - Result limits: kept small (default 5) to avoid runaway output.

use std::time::Duration;

use scraper::{Html, Selector};
use ureq::ResponseExt;

/// Maximum number of local search results returned by default.
pub const DEFAULT_SEARCH_LIMIT: usize = 5;

/// DuckDuckGo's form-backed HTML search endpoint.
pub const DUCKDUCKGO_HTML_URL: &str = "https://html.duckduckgo.com/html/";

/// Maximum article content length before truncation.
const MAX_ARTICLE_CONTENT_LEN: usize = 65_536;

/// Maximum response body size for fetched URLs (1 MiB).
const MAX_RESPONSE_BYTES: usize = 1_048_576;

/// Maximum number of HTTP redirects to follow. ureq's default is 10; we tighten
/// this to keep redirect chains short and bounded.
const MAX_REDIRECTS: u32 = 5;

/// Hard timeout (seconds) for the entire `read_url` fetch: DNS, connect, TLS,
/// redirects, and body read. Prevents a slow or malicious server from hanging
/// the agent loop.
const FETCH_TIMEOUT_SECS: u64 = 15;

/// User agent string for DuckDuckGo requests.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)  \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

type Result<T> = std::result::Result<T, SearchError>;

/// Errors from local search or extraction.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// HTTP transport error.
    #[error("http error: {0}")]
    Http(String),
    /// Non-success HTTP status.
    #[error("http {status}: {body}")]
    HttpStatus { status: u16, body: String },
    /// DuckDuckGo returned an anti-bot page instead of search results.
    #[error("bot challenge: {0}")]
    Blocked(String),
    /// Lectito extraction failed.
    #[error("extraction error: {0}")]
    Extraction(String),
    /// A hard-coded selector failed to parse.
    #[error("invalid CSS selector: {0}")]
    InvalidSelector(&'static str),
    /// The URL scheme is not `http` or `https`.
    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
    /// The URL points to a private/loopback network address.
    #[error("private network target rejected: {0}")]
    PrivateNetwork(String),
    /// The redirect chain exceeded the configured limit.
    #[error("too many redirects (max {max})")]
    TooManyRedirects { max: u32 },
    /// The request did not complete within the timeout.
    #[error("request timed out after {secs}s")]
    Timeout { secs: u64 },
    /// The response exceeded the maximum allowed size.
    #[error("response too large (>{max} bytes)")]
    Oversized { max: usize },
    /// The response content type is not HTML.
    #[error("unexpected content type: {0}")]
    BadContentType(String),
}

/// One result from DuckDuckGo's HTML search page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    /// The result title as displayed by DuckDuckGo.
    pub title: String,
    /// The normalized target URL.
    pub url: String,
    /// DuckDuckGo's result snippet, when present.
    pub snippet: Option<String>,
}

/// Extracted article content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArticleContent {
    /// Article title, if detected.
    pub title: String,
    /// Markdown-formatted content.
    pub markdown: String,
    /// Plain text content.
    pub text_content: String,
    /// Whether the content was truncated to fit the size cap.
    pub truncated: bool,
}

/// Content fetched from a public URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedContent {
    /// The final URL after redirects.
    pub final_url: String,
    /// HTTP status code of the final response.
    pub status: u16,
    /// The page title (from Lectito extraction, if available).
    pub title: String,
    /// Markdown-formatted content.
    pub markdown: String,
    /// Plain text content.
    pub text_content: String,
    /// Whether the content was truncated.
    pub truncated: bool,
    /// Diagnostics: redirects followed, content-type seen, limits applied.
    pub diagnostics: Vec<String>,
}

/// Check whether a URL string uses a public scheme (`http` or `https`).
pub fn is_public_scheme(url_str: &str) -> bool {
    url_str.starts_with("http://") || url_str.starts_with("https://")
}

/// Check whether a URL points to a private or loopback network address.
///
/// Rejects: `localhost`, `127.x.x.x`, `10.x.x.x`, `172.16-31.x.x`,
/// `192.168.x.x`, `169.254.x.x` (link-local), `::1`, `fc00::`/`fd00::`
/// (IPv6 private), and `0.0.0.0`.
pub fn is_private_url(url_str: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url_str) else {
        return true;
    };

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return true;
    }

    let host = match parsed.host_str() {
        Some(h) => h,
        None => return true,
    };

    if host == "localhost" || host == "localhost." {
        return true;
    }

    match parsed.host() {
        Some(url::Host::Ipv4(v4)) => is_private_ipv4(v4),
        Some(url::Host::Ipv6(v6)) => is_private_ipv6(v6),
        Some(url::Host::Domain(_)) => false,
        None => true,
    }
}

/// Fetch a public URL and extract readable content.
///
/// ## Safety guards
///
/// - Only `http`/`https` schemes are allowed.
/// - Private/loopback/link-local addresses are rejected, both for the requested
///   URL and for the final URL after redirects (prevents open-redirect SSRF).
/// - At most [`MAX_REDIRECTS`] redirects are followed; the chain errors on excess.
/// - The entire request is bounded by a [`FETCH_TIMEOUT_SECS`] global timeout.
/// - Response size is capped at [`MAX_RESPONSE_BYTES`], enforced *while streaming*
///   so a large body cannot exhaust memory before the cap triggers.
/// - Content type must be on the [`allowed_content_kind`] allow-list: HTML/XHTML
///   is extracted via Lectito; other text types (JSON, XML, plain text, feeds,
///   YAML, CSV, JS) are returned as raw text. Binary types are rejected.
pub fn fetch_url(url_str: &str) -> Result<FetchedContent> {
    if !is_public_scheme(url_str) {
        return Err(SearchError::UnsupportedScheme(url_str.to_string()));
    }
    if is_private_url(url_str) {
        return Err(SearchError::PrivateNetwork(url_str.to_string()));
    }

    let config = ureq::Agent::config_builder()
        .max_redirects(MAX_REDIRECTS)
        .max_redirects_will_error(true)
        .timeout_global(Some(Duration::from_secs(FETCH_TIMEOUT_SECS)))
        .build();

    let agent = ureq::Agent::new_with_config(config);
    let response = match agent
        .get(url_str)
        .header("User-Agent", USER_AGENT)
        .header("Accept", ALLOWED_ACCEPT_HEADER)
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(SearchError::HttpStatus { status: code, body: String::new() });
        }
        Err(ureq::Error::TooManyRedirects) => return Err(SearchError::TooManyRedirects { max: MAX_REDIRECTS }),
        Err(ureq::Error::Timeout(_)) => return Err(SearchError::Timeout { secs: FETCH_TIMEOUT_SECS }),
        Err(ureq::Error::BodyExceedsLimit(limit)) => return Err(SearchError::Oversized { max: limit as usize }),
        Err(e) => return Err(SearchError::Http(e.to_string())),
    };

    let final_url = response.get_uri().to_string();

    if is_private_url(&final_url) {
        return Err(SearchError::PrivateNetwork(final_url));
    }
    if !is_public_scheme(&final_url) {
        return Err(SearchError::UnsupportedScheme(final_url));
    }

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let kind = allowed_content_kind(&content_type).ok_or_else(|| SearchError::BadContentType(content_type.clone()))?;

    let body_result = response
        .into_body()
        .with_config()
        .limit(MAX_RESPONSE_BYTES as u64)
        .read_to_string();

    let (body, body_truncated) = match body_result {
        Ok(s) => (s, false),
        Err(ureq::Error::BodyExceedsLimit(_)) => (String::new(), true),
        Err(e) => return Err(SearchError::Http(e.to_string())),
    };

    let content = process_body(&body, &final_url, kind)?;

    let truncated = body_truncated || content.truncated;

    let mut diagnostics = vec![
        format!("status: {status}"),
        format!("content_type: {content_type}"),
        format!("max_redirects: {MAX_REDIRECTS}"),
        format!("timeout_secs: {FETCH_TIMEOUT_SECS}"),
        format!("max_bytes: {MAX_RESPONSE_BYTES}"),
    ];
    if truncated {
        diagnostics.push("truncated: true".to_string());
    }
    if url_str != final_url {
        diagnostics.push(format!("redirected: {url_str} -> {final_url}"));
    }

    Ok(FetchedContent {
        final_url,
        status,
        title: content.title,
        markdown: content.markdown,
        text_content: content.text_content,
        truncated,
        diagnostics,
    })
}

/// Classify a `Content-Type` header value into a [`ContentKind`] on the allow-list.
///
/// Returns `None` for binary types (images, audio, video, archives, octet-stream),
/// unrecognized types, and non-text application types not explicitly listed.
///
/// The check is deliberately permissive about parameters (`; charset=utf-8`) and
/// tolerates `+json` / `+xml` suffixes (`application/feed+json`, `application/atom+xml`).
pub fn allowed_content_kind(content_type: &str) -> Option<ContentKind> {
    let essence = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if essence.is_empty() {
        return None;
    }

    if essence.starts_with("text/") {
        return Some(html_kind(&essence));
    }

    if let Some(sub) = essence.strip_prefix("application/") {
        if sub == "html" || sub == "xhtml+xml" {
            return Some(ContentKind::Html);
        }

        if sub == "json" || sub.ends_with("+json") {
            return Some(ContentKind::Text);
        }

        if sub == "xml" || sub.ends_with("+xml") {
            return Some(ContentKind::Text);
        }

        if matches!(
            sub,
            "javascript" | "x-javascript" | "yaml" | "x-yaml" | "x-www-form-urlencoded"
        ) {
            return Some(ContentKind::Text);
        }
    }

    None
}

/// Map a `text/*` essence to the right kind (HTML vs plain text).
fn html_kind(essence: &str) -> ContentKind {
    match essence {
        "text/html" | "text/xhtml" => ContentKind::Html,
        _ => ContentKind::Text,
    }
}

/// Process a fetched body according to its [`ContentKind`].
///
/// HTML/XHTML is run through Lectito readability extraction; other text types
/// are returned as raw text with the title derived from the URL path. This is
/// the no-network, fixture-testable core of [`fetch_url`].
pub fn process_body(body: &str, final_url: &str, kind: ContentKind) -> Result<ProcessedContent> {
    match kind {
        ContentKind::Html => {
            let article = extract_article(body, Some(final_url))?;
            match article {
                Some(a) => Ok(ProcessedContent {
                    title: a.title,
                    markdown: a.markdown,
                    text_content: a.text_content,
                    truncated: a.truncated,
                }),

                None => Ok(ProcessedContent {
                    title: title_from_url(final_url),
                    markdown: body.to_string(),
                    text_content: body.to_string(),
                    truncated: body.len() > MAX_ARTICLE_CONTENT_LEN,
                }),
            }
        }
        ContentKind::Text => Ok(ProcessedContent {
            title: title_from_url(final_url),
            markdown: body.to_string(),
            text_content: body.to_string(),
            truncated: body.len() > MAX_ARTICLE_CONTENT_LEN,
        }),
    }
}

/// Derive a best-effort title from the final URL path.
fn title_from_url(url_str: &str) -> String {
    let Ok(parsed) = url::Url::parse(url_str) else {
        return String::new();
    };
    let path = parsed.path();
    let last = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("");
    percent_decode(last).unwrap_or_default()
}

/// Content category after allow-list classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentKind {
    /// HTML/XHTML — run through Lectito readability extraction.
    Html,
    /// Other text types (JSON, XML, plain text, feeds, YAML, JS) — raw body.
    Text,
}

/// Processed body content, independent of transport details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessedContent {
    /// Derived or extracted title.
    pub title: String,
    /// Markdown-formatted content.
    pub markdown: String,
    /// Plain text content.
    pub text_content: String,
    /// Whether the content was truncated to fit the size cap.
    pub truncated: bool,
}

/// Comma-separated `Accept` header value advertising the allow-listed types.
const ALLOWED_ACCEPT_HEADER: &str = "text/html, application/xhtml+xml, text/plain, \
    text/markdown, text/css, text/csv, text/xml, application/json, application/xml, \
    application/javascript, application/yaml, application/rss+xml, application/atom+xml, \
    application/feed+json, */+json, */+xml;q=0.5";

/// Search DuckDuckGo and return up to `limit` parsed results.
///
/// Uses a sync `ureq` POST to `html.duckduckgo.com/html/`. Empty queries and
/// zero limits return an empty result set without a network request.
pub fn search_duckduckgo(query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", query)
        .append_pair("b", "")
        .append_pair("l", "us-en")
        .finish();

    let agent = ureq::Agent::new_with_defaults();
    let response = agent
        .post(DUCKDUCKGO_HTML_URL)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
        .send(&form);

    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(SearchError::HttpStatus { status: code, body: String::new() });
        }
        Err(e) => return Err(SearchError::Http(e.to_string())),
    };

    let status = response.status().as_u16();
    let body = response.into_body().read_to_string().unwrap_or_default();
    match status >= 400 {
        true => Err(SearchError::HttpStatus { status, body: body.chars().take(500).collect() }),
        false => parse_duckduckgo_html(&body, limit),
    }
}

/// Parse DuckDuckGo HTML search results.
///
/// Detects the common bot-challenge page before returning results. Normalizes
/// DuckDuckGo redirect links (`/l/?uddg=...`) into their target URLs.
pub fn parse_duckduckgo_html(html: &str, limit: usize) -> Result<Vec<SearchResult>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    if is_bot_challenge(html) {
        return Err(SearchError::Blocked(
            "DuckDuckGo returned a bot challenge instead of search results".to_string(),
        ));
    }

    let document = Html::parse_document(html);
    let result_selector = selector(".result")?;
    let title_selector = selector(".result__title a, a.result__a")?;
    let snippet_selector = selector(".result__snippet")?;
    let url_selector = selector(".result__url")?;
    let mut results = Vec::new();

    for result in document.select(&result_selector) {
        let Some(link) = result.select(&title_selector).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };

        let title = clean_text(&link.text().collect::<Vec<_>>().join(" "));
        if title.is_empty() {
            continue;
        }

        let snippet = result
            .select(&snippet_selector)
            .next()
            .map(|node| clean_text(&node.text().collect::<Vec<_>>().join(" ")))
            .filter(|text| !text.is_empty());
        let fallback_url = result
            .select(&url_selector)
            .next()
            .map(|node| clean_text(&node.text().collect::<Vec<_>>().join(" ")))
            .filter(|text| !text.is_empty());

        match normalize_duckduckgo_url(href).or(fallback_url) {
            Some(url) => {
                results.push(SearchResult { title, url, snippet });
                if results.len() >= limit {
                    break;
                }
            }
            None => continue,
        }
    }

    Ok(results)
}

/// Detect DuckDuckGo bot-challenge / anomaly pages.
///
/// Checks for known markers that DDG uses when it suspects automated traffic.
pub fn is_bot_challenge(html: &str) -> bool {
    html.contains("anomaly-modal")
        || html.contains("Unfortunately, bots use DuckDuckGo too")
        || html.contains("/anomaly.js")
}

/// Extract readable content from already-fetched HTML using Lectito.
///
/// Returns the article title, Markdown content, and a truncation flag.
/// Returns `None` if the page is not probably readable.
pub fn extract_article(html: &str, base_url: Option<&str>) -> Result<Option<ArticleContent>> {
    let options = lectito::ReadabilityOptions::default();
    let article = lectito::extract(html, base_url, &options).map_err(|e| SearchError::Extraction(e.to_string()))?;

    match article {
        Some(a) => Ok(Some(ArticleContent {
            title: a.title.unwrap_or_default(),
            markdown: a.markdown.clone(),
            text_content: a.text_content,
            truncated: a.markdown.len() > MAX_ARTICLE_CONTENT_LEN,
        })),
        None => Ok(None),
    }
}

/// Format search results as transcript output lines.
pub fn format_search_results(results: &[SearchResult]) -> Vec<String> {
    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let snippet = r.snippet.as_deref().unwrap_or("");
            format!("{}. {} — {} ({})", i + 1, r.title, snippet, r.url)
        })
        .collect()
}

/// Check if an IPv4 address is private/loopback/link-local.
///
/// In order, check loopback, private 10.0.0.0/8, 0.0.0.0/8
/// private 172.16/12, private 192.168/16, link-local 169.254/16
fn is_private_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 127
        || octets[0] == 10
        || octets[0] == 0
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || (octets[0] == 169 && octets[1] == 254)
}

/// Check if an IPv6 address is private/loopback/link-local.
///
/// In order, check ::1, ::, unique local fc00::/7, link-local fe80::/10
fn is_private_ipv6(ip: std::net::Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        || (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Minimal percent-decoding without an extra dependency.
fn percent_decode(s: &str) -> Option<String> {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            result.push(char::from_u32(hi * 16 + lo).unwrap_or('?'));
            i += 3;
            continue;
        }
        if bytes[i] == b'+' {
            result.push(' ');
        } else {
            result.push(s[i..].chars().next().unwrap_or('?'));
        }
        i += 1;
    }
    Some(result)
}

fn selector(css: &'static str) -> Result<Selector> {
    Selector::parse(css).map_err(|_| SearchError::InvalidSelector(css))
}

fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_duckduckgo_url(href: &str) -> Option<String> {
    let decoded = html_unescape(href);
    if decoded.starts_with("http://") || decoded.starts_with("https://") {
        return Some(decoded);
    }

    if let Some(query_start) = decoded.find("uddg=") {
        let encoded = &decoded[query_start + 5..];
        let end = encoded.find('&').unwrap_or(encoded.len());
        return percent_decode(&encoded[..end]);
    }

    let base = url::Url::parse(DUCKDUCKGO_HTML_URL).ok()?;
    Some(base.join(&decoded).ok()?.to_string())
}

fn hex_val(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duckduckgo_html_extracts_results() {
        let html = r#"
            <div class="result">
              <h2 class="result__title">
                <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpost%3Fx%3D1&amp;rut=abc">
                  Example Result
                </a>
              </h2>
              <a class="result__url">example.com/post</a>
              <a class="result__snippet">A compact result snippet.</a>
            </div>
        "#;

        let results = parse_duckduckgo_html(html, 10).expect("html parses");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example Result");
        assert_eq!(results[0].url, "https://example.com/post?x=1");
        assert_eq!(results[0].snippet.as_deref(), Some("A compact result snippet."));
    }

    #[test]
    fn parse_duckduckgo_html_respects_limit() {
        let html = r#"
            <div class="result"><a class="result__a" href="https://a.test">A</a></div>
            <div class="result"><a class="result__a" href="https://b.test">B</a></div>
        "#;

        let results = parse_duckduckgo_html(html, 1).expect("html parses");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "A");
    }

    #[test]
    fn parse_duckduckgo_html_detects_bot_challenge() {
        let html = r#"
            <form id="challenge-form" action="//duckduckgo.com/anomaly.js">
              <div class="anomaly-modal__title">
                Unfortunately, bots use DuckDuckGo too.
              </div>
            </form>
        "#;

        let error = parse_duckduckgo_html(html, 10).expect_err("challenge is an error");
        assert!(matches!(error, SearchError::Blocked(_)));
    }

    #[test]
    fn is_bot_challenge_detects_anomaly_markers() {
        assert!(is_bot_challenge("anomaly-modal test"));
        assert!(is_bot_challenge("Unfortunately, bots use DuckDuckGo too"));
        assert!(is_bot_challenge("/anomaly.js"));
        assert!(!is_bot_challenge("normal search results"));
    }

    #[test]
    fn is_bot_challenge_false_for_normal_html() {
        let html = r#"<div class="result"><a href="https://example.com">Normal</a></div>"#;
        assert!(!is_bot_challenge(html));
    }

    #[test]
    fn empty_query_returns_empty_results() {
        let results = parse_duckduckgo_html("", 10).expect("empty html");
        assert!(results.is_empty());
    }

    #[test]
    fn zero_limit_returns_empty_results() {
        let html = r#"<div class="result"><a href="https://example.com">Test</a></div>"#;
        let results = parse_duckduckgo_html(html, 0).expect("zero limit");
        assert!(results.is_empty());
    }

    #[test]
    fn normalize_duckduckgo_url_resolves_redirect() {
        let url = normalize_duckduckgo_url("//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpost");
        assert_eq!(url.as_deref(), Some("https://example.com/post"));
    }

    #[test]
    fn normalize_duckduckgo_url_passes_through_absolute() {
        let url = normalize_duckduckgo_url("https://example.com/direct");
        assert_eq!(url.as_deref(), Some("https://example.com/direct"));
    }

    #[test]
    fn default_search_limit_is_small() {
        let limit = DEFAULT_SEARCH_LIMIT;
        assert!(limit <= 10, "search limit should be small");
        assert!(limit >= 3, "search limit should be useful");
    }

    #[test]
    fn format_search_results_produces_lines() {
        let lines = format_search_results(&[
            SearchResult {
                title: "Rust Async".to_string(),
                url: "https://tokio.rs".to_string(),
                snippet: Some("Async runtime".to_string()),
            },
            SearchResult {
                title: "Async Book".to_string(),
                url: "https://rust-lang.org/async".to_string(),
                snippet: None,
            },
        ]);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Rust Async"));
        assert!(lines[0].contains("tokio.rs"));
        assert!(lines[1].contains("Async Book"));
    }

    #[test]
    fn extract_article_returns_none_for_empty_html() {
        assert!(
            extract_article("<html><body></body></html>", None)
                .expect("should not error")
                .is_none()
        );
    }

    #[test]
    fn extract_article_returns_content_for_readable_html() {
        let html = r#"
            <article>
                <h1>Test Article</h1>
                <p>This is a readable article with enough content to pass readability checks.
                It has multiple sentences and proper structure for extraction.</p>
                <p>Second paragraph with more content to ensure the article is detected
                as readable by the Lectito algorithm.</p>
            </article>
        "#;
        let result = extract_article(html, Some("https://example.com/post")).expect("should extract");
        assert!(result.is_some(), "should extract readable article");

        let article = result.unwrap();
        assert!(!article.markdown.is_empty());
        assert!(!article.text_content.is_empty());
    }

    #[test]
    fn is_private_url_table() {
        let rejected: &[(&str, &str)] = &[
            ("localhost", "http://localhost:8080/test"),
            ("localhost dot", "https://localhost./path"),
            ("loopback ipv4", "http://127.0.0.1/test"),
            ("loopback ipv4 high", "http://127.255.255.255/test"),
            ("private 10.x", "http://10.0.0.1/test"),
            ("private 10.x high", "http://10.255.255.255/test"),
            ("private 172.16.x", "http://172.16.0.1/test"),
            ("private 172.31.x high", "http://172.31.255.255/test"),
            ("private 192.168.x", "http://192.168.1.1/test"),
            ("private 192.168.0", "http://192.168.0.0/test"),
            ("link-local", "http://169.254.1.1/test"),
            ("link-local metadata", "http://169.254.169.254/latest/meta-data"),
            ("zero address", "http://0.0.0.0/test"),
            ("ipv6 loopback", "http://[::1]/test"),
            ("file scheme", "file:///etc/passwd"),
            ("ftp scheme", "ftp://example.com/file"),
            ("javascript scheme", "javascript:alert(1)"),
            ("unparseable", "not a url"),
            ("empty", ""),
        ];
        for (label, url) in rejected {
            assert!(
                is_private_url(url),
                "{label}: expected private/rejected, got allowed: {url}"
            );
        }

        let allowed: &[(&str, &str)] = &[
            ("public domain", "https://example.com/article"),
            ("public ipv4", "http://93.184.216.34/test"),
            ("public blog", "https://blog.rust-lang.org/2024/01/01/post"),
        ];
        for (label, url) in allowed {
            assert!(!is_private_url(url), "{label}: expected allowed, got rejected: {url}");
        }
    }

    #[test]
    fn is_public_scheme_checks_prefix() {
        assert!(is_public_scheme("http://example.com"));
        assert!(is_public_scheme("https://example.com"));
        assert!(!is_public_scheme("file:///etc/passwd"));
        assert!(!is_public_scheme("ftp://example.com"));
    }

    #[test]
    fn fetch_url_rejects_non_public_scheme() {
        let result = fetch_url("file:///etc/passwd");
        assert!(matches!(result, Err(SearchError::UnsupportedScheme(_))));
    }

    #[test]
    fn fetch_url_rejects_private_network() {
        let result = fetch_url("http://127.0.0.1:8080/secret");
        assert!(matches!(result, Err(SearchError::PrivateNetwork(_))));

        let result = fetch_url("http://localhost/admin");
        assert!(matches!(result, Err(SearchError::PrivateNetwork(_))));
    }

    #[test]
    fn max_response_bytes_is_reasonable() {
        let max = MAX_RESPONSE_BYTES;
        assert!(max >= 65_536, "should allow at least 64 KiB");
        assert!(max <= 2_097_152, "should cap at 2 MiB");
    }

    #[test]
    fn max_redirects_is_bounded_and_small() {
        let max = MAX_REDIRECTS;
        assert!(max <= 5, "redirect limit should be small");
        assert!(max >= 1, "should follow at least one redirect");
    }

    #[test]
    fn fetch_timeout_is_bounded() {
        let secs = FETCH_TIMEOUT_SECS;
        assert!(secs <= 60, "timeout should be at most 60s");
        assert!(secs >= 5, "timeout should allow at least 5s");
    }

    #[test]
    fn search_error_too_many_redirects_displays_message() {
        let max = MAX_REDIRECTS;
        let err = SearchError::TooManyRedirects { max };
        assert!(err.to_string().contains("too many redirects"));
        assert!(err.to_string().contains(&max.to_string()));
    }

    #[test]
    fn search_error_timeout_displays_message() {
        let secs = FETCH_TIMEOUT_SECS;
        let err = SearchError::Timeout { secs };
        assert!(err.to_string().contains("timed out"));
        assert!(err.to_string().contains(&secs.to_string()));
    }

    #[test]
    fn search_error_oversized_displays_message() {
        let err = SearchError::Oversized { max: 1024 };
        assert!(err.to_string().contains("too large"));
        assert!(err.to_string().contains("1024"));
    }

    #[test]
    fn allowed_content_kind_html() {
        assert_eq!(allowed_content_kind("text/html"), Some(ContentKind::Html));
        assert_eq!(
            allowed_content_kind("text/html; charset=utf-8"),
            Some(ContentKind::Html)
        );
        assert_eq!(allowed_content_kind("application/xhtml+xml"), Some(ContentKind::Html));
        assert_eq!(allowed_content_kind("TEXT/HTML"), Some(ContentKind::Html));
    }

    #[test]
    fn allowed_content_kind_text_family() {
        assert_eq!(allowed_content_kind("text/plain"), Some(ContentKind::Text));
        assert_eq!(
            allowed_content_kind("text/plain; charset=iso-8859-1"),
            Some(ContentKind::Text)
        );
        assert_eq!(allowed_content_kind("text/csv"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("text/markdown"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("text/css"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("text/xml"), Some(ContentKind::Text));
    }

    #[test]
    fn allowed_content_kind_application_text_types() {
        assert_eq!(allowed_content_kind("application/json"), Some(ContentKind::Text));
        assert_eq!(
            allowed_content_kind("application/json; charset=utf-8"),
            Some(ContentKind::Text)
        );
        assert_eq!(allowed_content_kind("application/xml"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("application/javascript"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("application/yaml"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("application/x-yaml"), Some(ContentKind::Text));
    }

    #[test]
    fn allowed_content_kind_suffixes() {
        assert_eq!(allowed_content_kind("application/feed+json"), Some(ContentKind::Text));
        assert_eq!(
            allowed_content_kind("application/vnd.api+json"),
            Some(ContentKind::Text)
        );
        assert_eq!(allowed_content_kind("application/atom+xml"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("application/rss+xml"), Some(ContentKind::Text));
    }

    #[test]
    fn allowed_content_kind_rejects_binary_and_unknown() {
        assert_eq!(allowed_content_kind("image/png"), None);
        assert_eq!(allowed_content_kind("application/octet-stream"), None);
        assert_eq!(allowed_content_kind("application/pdf"), None);
        assert_eq!(allowed_content_kind("application/zip"), None);
        assert_eq!(allowed_content_kind("audio/mpeg"), None);
        assert_eq!(allowed_content_kind("video/mp4"), None);
        assert_eq!(allowed_content_kind("application/octet-stream; charset=binary"), None);
        assert_eq!(allowed_content_kind(""), None);
        assert_eq!(allowed_content_kind("garbage"), None);
    }

    #[test]
    fn process_body_html_uses_lectito_extraction() {
        let html = r#"
            <article>
                <h1>Test Article</h1>
                <p>This is a readable article with enough content to pass readability checks.
                It has multiple sentences and proper structure for extraction.</p>
                <p>Second paragraph with more content to ensure the article is detected
                as readable by the Lectito algorithm.</p>
            </article>
        "#;
        let result = process_body(html, "https://example.com/post", ContentKind::Html).expect("html should process");
        assert!(!result.markdown.is_empty());
        assert!(!result.text_content.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn process_body_html_unreadable_falls_back_to_raw() {
        let html = "<html><body></body></html>";
        let result = process_body(html, "https://example.com/empty", ContentKind::Html)
            .expect("unreadable html should not error");
        assert_eq!(result.markdown, html);
        assert_eq!(result.text_content, html);
    }

    #[test]
    fn process_body_text_returns_raw_with_url_title() {
        let body = r#"{"name": "thndrs", "version": "0.1.0"}"#;
        let result = process_body(body, "https://example.com/package.json", ContentKind::Text)
            .expect("json should process as text");
        assert_eq!(result.markdown, body);
        assert_eq!(result.text_content, body);
        assert_eq!(result.title, "package.json");
        assert!(!result.truncated);
    }

    #[test]
    fn process_body_text_title_from_url_path() {
        let result = process_body("plain", "https://example.com/docs/readme.txt", ContentKind::Text)
            .expect("text should process");
        assert_eq!(result.title, "readme.txt");
    }

    #[test]
    fn process_body_text_truncation_flag() {
        let body = "a".repeat(MAX_ARTICLE_CONTENT_LEN + 100);
        let result =
            process_body(&body, "https://example.com/big.txt", ContentKind::Text).expect("large text should process");
        assert!(result.truncated);
    }

    #[test]
    fn process_body_text_title_percent_decoded() {
        let result = process_body("body", "https://example.com/path/my%20file.txt", ContentKind::Text)
            .expect("text should process");
        assert_eq!(result.title, "my file.txt");
    }
}
