//! Public URL retrieval for the built-in `read_url` tool.
//!
//! This module accepts only public HTTP(S) destinations. It validates literal
//! and resolved addresses, validates each redirect, caps response size and
//! total time, and extracts readable HTML with Lectito.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

/// Maximum article content length before truncation.
const MAX_ARTICLE_CONTENT_LEN: usize = 65_536;

/// Maximum response body size for fetched URLs (1 MiB).
const MAX_RESPONSE_BYTES: usize = 1_048_576;

/// Maximum redirects followed for one request.
const MAX_REDIRECTS: u32 = 5;

/// Timeout for DNS, connect, TLS, redirects, and body reading.
const FETCH_TIMEOUT_SECS: u64 = 15;

const PRIVATE_RESOLUTION_ERROR: &str = "resolved host includes a non-public network address";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)  \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

/// Errors from public URL retrieval and readable-content extraction.
#[derive(Debug, thiserror::Error)]
pub enum UrlReadError {
    /// HTTP transport error.
    #[error("http error: {0}")]
    Http(String),
    /// Non-success HTTP status.
    #[error("http {status}: {body}")]
    HttpStatus { status: u16, body: String },
    /// Lectito extraction failed.
    #[error("extraction error: {0}")]
    Extraction(String),
    /// The URL scheme is not `http` or `https`.
    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
    /// The URL points to a private or loopback network address.
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
    /// The response content type is not readable text.
    #[error("unexpected content type: {0}")]
    BadContentType(String),
}

type Result<T> = std::result::Result<T, UrlReadError>;
/// Resolver wrapper that validates the exact addresses handed to the socket
/// connector. Rejecting the whole answer when any address is non-public avoids
/// DNS rebinding and mixed public/private answer ambiguity.
#[derive(Debug)]
struct PublicResolver<R> {
    inner: R,
}

impl<R> PublicResolver<R> {
    fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: Resolver> Resolver for PublicResolver<R> {
    fn resolve(
        &self, uri: &ureq::http::Uri, config: &ureq::config::Config, timeout: NextTimeout,
    ) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = self.inner.resolve(uri, config, timeout)?;
        if addresses.iter().any(|address| !is_public_ip(address.ip())) {
            return Err(ureq::Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                PRIVATE_RESOLUTION_ERROR,
            )));
        }
        Ok(addresses)
    }
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

/// Check whether a URL has a literal non-public network address or a localhost
/// hostname. Domain names are resolved and checked at connection time by the
/// fetcher's public-address resolver.
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

    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("localhost.") {
        return true;
    }

    match parsed.host() {
        Some(url::Host::Ipv4(v4)) => !is_public_ipv4(v4),
        Some(url::Host::Ipv6(v6)) => !is_public_ipv6(v6),
        Some(url::Host::Domain(_)) => false,
        None => true,
    }
}

/// Fetch a public URL and extract readable content.
///
/// ## Safety guards
///
/// - Only `http`/`https` schemes are allowed.
/// - Literal and DNS-resolved non-public addresses are rejected before a
///   connection is opened.
/// - Redirects are followed one at a time and every target is validated before
///   its request is sent.
/// - At most `MAX_REDIRECTS` redirects are followed; the chain errors on excess.
/// - The entire request is bounded by a `FETCH_TIMEOUT_SECS` global timeout.
/// - Response size is capped at `MAX_RESPONSE_BYTES`, enforced *while streaming*
///   so a large body cannot exhaust memory before the cap triggers.
/// - Content type must be on the [`allowed_content_kind`] allow-list: HTML/XHTML
///   is extracted via Lectito; other text types (JSON, XML, plain text, feeds,
///   YAML, CSV, JS) are returned as raw text. Binary types are rejected.
pub fn fetch_url(url_str: &str) -> Result<FetchedContent> {
    fetch_url_with_agent_factory(url_str, public_fetch_agent)
}

fn fetch_url_with_agent_factory(
    url_str: &str, mut agent_factory: impl FnMut(Duration) -> ureq::Agent,
) -> Result<FetchedContent> {
    if !is_public_scheme(url_str) {
        return Err(UrlReadError::UnsupportedScheme(url_str.to_string()));
    }
    if is_private_url(url_str) {
        return Err(UrlReadError::PrivateNetwork(url_str.to_string()));
    }

    let started = Instant::now();
    let timeout = Duration::from_secs(FETCH_TIMEOUT_SECS);
    let mut current = url::Url::parse(url_str).map_err(|error| UrlReadError::Http(error.to_string()))?;
    let mut redirect_count = 0;

    let response = loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(UrlReadError::Timeout { secs: FETCH_TIMEOUT_SECS })?;
        let agent = agent_factory(remaining);
        let response = request_public_url(&agent, current.as_str())?;

        let Some(next) = redirect_target(&current, &response)? else {
            break response;
        };
        if redirect_count >= MAX_REDIRECTS {
            return Err(UrlReadError::TooManyRedirects { max: MAX_REDIRECTS });
        }
        validate_public_url(next.as_str())?;
        current = next;
        redirect_count += 1;
    };

    let final_url = current.to_string();

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let kind = allowed_content_kind(&content_type).ok_or_else(|| UrlReadError::BadContentType(content_type.clone()))?;

    let body_result = response
        .into_body()
        .with_config()
        .limit(MAX_RESPONSE_BYTES as u64)
        .read_to_string();

    let (body, body_truncated) = match body_result {
        Ok(s) => (s, false),
        Err(ureq::Error::BodyExceedsLimit(_)) => (String::new(), true),
        Err(e) => return Err(UrlReadError::Http(e.to_string())),
    };

    let content = process_body(&body, &final_url, kind)?;

    let truncated = body_truncated || content.truncated;

    let mut diagnostics = vec![
        format!("status: {status}"),
        format!("content_type: {content_type}"),
        format!("redirects_followed: {redirect_count}"),
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

fn public_fetch_agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .max_redirects(0)
        // A proxy can resolve the destination itself and bypass the guarded
        // resolver. Public URL fetching therefore always connects directly.
        .proxy(None)
        .timeout_global(Some(timeout))
        .build();
    ureq::Agent::with_parts(
        config,
        DefaultConnector::default(),
        PublicResolver::new(DefaultResolver::default()),
    )
}

fn request_public_url(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    match agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", ALLOWED_ACCEPT_HEADER)
        .call()
    {
        Ok(response) => Ok(response),
        Err(ureq::Error::StatusCode(code)) => Err(UrlReadError::HttpStatus { status: code, body: String::new() }),
        Err(ureq::Error::Timeout(_)) => Err(UrlReadError::Timeout { secs: FETCH_TIMEOUT_SECS }),
        Err(ureq::Error::BodyExceedsLimit(limit)) => Err(UrlReadError::Oversized { max: limit as usize }),
        Err(ureq::Error::Io(error)) if is_private_resolution_error(&error) => {
            Err(UrlReadError::PrivateNetwork(url.to_string()))
        }
        Err(error) => Err(UrlReadError::Http(error.to_string())),
    }
}

fn redirect_target(current: &url::Url, response: &ureq::http::Response<ureq::Body>) -> Result<Option<url::Url>> {
    if !response.status().is_redirection() {
        return Ok(None);
    }
    let Some(location) = response.headers().get("Location") else {
        return Ok(None);
    };
    let location = location
        .to_str()
        .map_err(|error| UrlReadError::Http(format!("invalid redirect location: {error}")))?;
    let next = current
        .join(location)
        .map_err(|error| UrlReadError::Http(format!("invalid redirect location: {error}")))?;
    Ok(Some(next))
}

fn validate_public_url(url_str: &str) -> Result<()> {
    if !is_public_scheme(url_str) {
        return Err(UrlReadError::UnsupportedScheme(url_str.to_string()));
    }
    if is_private_url(url_str) {
        return Err(UrlReadError::PrivateNetwork(url_str.to_string()));
    }
    Ok(())
}

fn is_private_resolution_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied && error.to_string() == PRIVATE_RESOLUTION_ERROR
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
                    markdown: cap_text(&a.markdown, MAX_ARTICLE_CONTENT_LEN),
                    text_content: cap_text(&a.text_content, MAX_ARTICLE_CONTENT_LEN),
                    truncated: a.truncated,
                }),

                None => Ok(ProcessedContent {
                    title: title_from_url(final_url),
                    markdown: cap_text(body, MAX_ARTICLE_CONTENT_LEN),
                    text_content: cap_text(body, MAX_ARTICLE_CONTENT_LEN),
                    truncated: body.len() > MAX_ARTICLE_CONTENT_LEN,
                }),
            }
        }
        ContentKind::Text => Ok(ProcessedContent {
            title: title_from_url(final_url),
            markdown: cap_text(body, MAX_ARTICLE_CONTENT_LEN),
            text_content: cap_text(body, MAX_ARTICLE_CONTENT_LEN),
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

/// Extract readable content from already-fetched HTML using Lectito.
///
/// Returns the article title, Markdown content, and a truncation flag.
/// Returns `None` if the page is not probably readable.
pub fn extract_article(html: &str, base_url: Option<&str>) -> Result<Option<ArticleContent>> {
    let options = lectito::ReadabilityOptions::default();
    let article = lectito::extract(html, base_url, &options).map_err(|e| UrlReadError::Extraction(e.to_string()))?;

    match article {
        Some(a) => Ok(Some(ArticleContent {
            title: a.title.unwrap_or_default(),
            markdown: cap_text(&a.markdown, MAX_ARTICLE_CONTENT_LEN),
            text_content: cap_text(&a.text_content, MAX_ARTICLE_CONTENT_LEN),
            truncated: a.markdown.len() > MAX_ARTICLE_CONTENT_LEN,
        })),
        None => Ok(None),
    }
}

fn cap_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.saturating_sub("…".len());
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}…", &text[..end])
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

/// Return whether an IPv4 address is globally routable. The explicit special
/// ranges keep this compatible with the project's Rust 1.88 MSRV.
fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

/// Return whether an IPv6 address is globally routable. IPv4-mapped addresses
/// inherit the IPv4 classification; local, documentation, benchmarking, and
/// transition ranges are rejected conservatively.
fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return is_public_ipv4(ipv4);
    }

    let segments = ip.segments();
    let first = segments[0];
    let second = segments[1];
    let global_unicast = (first & 0xe000) == 0x2000;
    let teredo = first == 0x2001 && second == 0;
    let benchmarking = first == 0x2001 && second == 2 && segments[2] == 0;
    let orchid = first == 0x2001 && (second & 0xfff0 == 0x0010 || second & 0xfff0 == 0x0020);
    let documentation = (first == 0x2001 && second == 0x0db8) || (first == 0x3fff && second & 0xf000 == 0);

    global_unicast && !(teredo || benchmarking || orchid || documentation || first == 0x2002)
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

fn hex_val(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Clone, Debug)]
    struct FixedResolver {
        address: SocketAddr,
    }

    impl Resolver for FixedResolver {
        fn resolve(
            &self, _uri: &ureq::http::Uri, _config: &ureq::config::Config, _timeout: NextTimeout,
        ) -> std::result::Result<ResolvedSocketAddrs, ureq::Error> {
            let mut addresses = self.empty();
            addresses.push(self.address);
            Ok(addresses)
        }
    }

    fn test_agent(timeout: Duration, resolver: impl Resolver) -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .max_redirects(0)
            .timeout_global(Some(timeout))
            .build();
        ureq::Agent::with_parts(config, DefaultConnector::default(), resolver)
    }

    #[test]
    fn public_url_validation_rejects_private_and_non_http_targets() {
        for url in [
            "file:///etc/passwd",
            "http://127.0.0.1/",
            "http://localhost/",
            "http://[::1]/",
        ] {
            assert!(is_private_url(url));
        }
        assert!(!is_private_url("https://example.com/path"));
    }

    #[test]
    fn fetch_rejects_a_private_dns_result_before_connecting() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
        listener.set_nonblocking(true).expect("nonblocking test server");
        let address = listener.local_addr().expect("test server address");
        let url = format!("http://public.test:{}/secret", address.port());

        let result = fetch_url_with_agent_factory(&url, |timeout| {
            test_agent(timeout, PublicResolver::new(FixedResolver { address }))
        });

        assert!(matches!(result, Err(UrlReadError::PrivateNetwork(target)) if target == url));
        assert_eq!(
            listener.accept().expect_err("must not connect").kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn fetch_rejects_a_private_redirect_before_a_second_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let requests = Arc::new(AtomicUsize::new(0));
        let request_count = Arc::clone(&requests);
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("first request");
            request_count.fetch_add(1, Ordering::SeqCst);
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                address.port()
            );
            stream.write_all(response.as_bytes()).expect("redirect response");
        });
        let url = format!("http://public.test:{}/start", address.port());

        let result = fetch_url_with_agent_factory(&url, |timeout| test_agent(timeout, FixedResolver { address }));

        assert!(matches!(result, Err(UrlReadError::PrivateNetwork(_))));
        handle.join().expect("test server");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn readable_content_types_and_extraction_are_preserved() {
        assert_eq!(
            allowed_content_kind("text/html; charset=utf-8"),
            Some(ContentKind::Html)
        );
        assert_eq!(allowed_content_kind("application/json"), Some(ContentKind::Text));
        assert_eq!(allowed_content_kind("application/pdf"), None);
        let body = r#"{"name":"thndrs"}"#;
        let content = process_body(body, "https://example.com/package.json", ContentKind::Text).expect("text");
        assert_eq!(content.title, "package.json");
        assert_eq!(content.markdown, body);
    }

    #[test]
    fn extraction_errors_are_descriptive() {
        assert!(
            UrlReadError::TooManyRedirects { max: MAX_REDIRECTS }
                .to_string()
                .contains("too many redirects")
        );
        assert!(
            UrlReadError::Timeout { secs: FETCH_TIMEOUT_SECS }
                .to_string()
                .contains("timed out")
        );
        assert!(UrlReadError::Oversized { max: 1024 }.to_string().contains("too large"));
    }
}
