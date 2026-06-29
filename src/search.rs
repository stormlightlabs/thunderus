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

#![allow(dead_code)]

use scraper::{Html, Selector};

/// Maximum number of local search results returned by default.
pub const DEFAULT_SEARCH_LIMIT: usize = 5;

/// DuckDuckGo's form-backed HTML search endpoint.
pub const DUCKDUCKGO_HTML_URL: &str = "https://html.duckduckgo.com/html/";

/// Maximum article content length before truncation.
const MAX_ARTICLE_CONTENT_LEN: usize = 65_536;

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

    if status >= 400 {
        Err(SearchError::HttpStatus { status, body: body.chars().take(500).collect() })
    } else {
        parse_duckduckgo_html(&body, limit)
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
        let results = vec![
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
        ];
        let lines = format_search_results(&results);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Rust Async"));
        assert!(lines[0].contains("tokio.rs"));
        assert!(lines[1].contains("Async Book"));
    }

    #[test]
    fn extract_article_returns_none_for_empty_html() {
        let result = extract_article("<html><body></body></html>", None).expect("should not error");
        assert!(result.is_none());
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
}
