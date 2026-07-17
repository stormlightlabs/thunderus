//! Credential storage for provider API keys.
//!
//! Credentials are stored in simple `.env`-format files, **not** in TOML config.
//! This keeps secrets out of version control, logs, sessions, and diagnostics.
//!
//! ## Storage paths
//!
//! - Global: `~/.thndrs/credentials.env`
//! - Project: `<workspace>/.thndrs/credentials.env`
//!
//! ## Precedence
//!
//! Provider key resolution follows this order (first wins):
//! 1. Process environment variables
//! 2. Global credential store (`~/.thndrs/credentials.env`)
//! 3. Project credential store (`.thndrs/credentials.env`)
//! 4. Workspace `.env` file (legacy compatibility)

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::Digest;

use crate::utils;

/// Environment variable name for the Umans API key.
pub const UMANS_API_KEY_ENV: &str = "UMANS_API_KEY";

/// Environment variable name for the OpenCode Go API key.
pub const OPENCODE_GO_KEY_ENV: &str = "OPENCODE_GO_KEY";

/// Environment variable name for the OpenCode Zen API key.
pub const OPENCODE_ZEN_KEY_ENV: &str = "OPENCODE_ZEN_KEY";

/// Environment variable name for a process-local ChatGPT Codex access token.
pub const CHATGPT_CODEX_ACCESS_TOKEN_ENV: &str = "CHATGPT_CODEX_ACCESS_TOKEN";

/// source: codex-rs/login/src/auth/manager.rs
const CHATGPT_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const DEVICE_CODE_TIMEOUT_SECONDS: u64 = 15 * 60;
const PKCE_CALLBACK_ADDR: &str = "127.0.0.1:1455";
const PKCE_CALLBACK_URL: &str = "http://localhost:1455/auth/callback";
const PKCE_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PKCE_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OAUTH_SCOPE: &str = "openid profile email offline_access";
const OAUTH_ORIGINATOR: &str = "thndrs";

/// Describes where a credential value was found, without leaking the value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    /// Found in a process environment variable.
    Environment,
    /// Found in the global credential store at `~/.thndrs/credentials.env`.
    GlobalStore,
    /// Found in the project credential store at `<workspace>/.thndrs/credentials.env`.
    ProjectStore,
    /// Found in the workspace `.env` file (legacy fallback).
    DotEnvLegacy,
}

impl CredentialSource {
    /// Human-readable label for the source.
    pub fn label(self) -> &'static str {
        match self {
            CredentialSource::Environment => "environment",
            CredentialSource::GlobalStore => "global credentials",
            CredentialSource::ProjectStore => "project credentials",
            CredentialSource::DotEnvLegacy => ".env",
        }
    }
}

impl ChatGptCodexCredentials {
    fn is_expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms <= now_ms.saturating_add(60_000)
    }

    fn is_usable(&self) -> bool {
        !self.access_token.trim().is_empty()
            && !self.refresh_token.trim().is_empty()
            && !self.account_id.trim().is_empty()
    }
}

/// Errors produced by credential store operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Failed to read a credential file.
    #[error("failed to read credentials {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Failed to write a credential file.
    #[error("failed to write credentials {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Malformed line in a credential file.
    #[error("malformed credential file {path}: {message}")]
    Malformed { path: PathBuf, message: String },
    /// Home directory is not available.
    #[error("home directory not found; set $HOME or $USERPROFILE")]
    NoHomeDirectory,
    /// Failed to update `.git/info/exclude`.
    #[error("git exclude failed: {0}")]
    GitExclude(String),
    /// ChatGPT Codex credential data is missing or malformed.
    #[error("chatgpt-codex auth failed: {0}")]
    ChatGptCodex(String),
    /// ChatGPT Codex credential verification could not reach the provider.
    #[error("chatgpt-codex authentication verification unavailable: {0}")]
    ChatGptCodexUnavailable(String),
}

impl AuthError {
    /// Whether retrying later may resolve this without replacing credentials.
    pub fn is_verification_unavailable(&self) -> bool {
        matches!(
            self,
            Self::ChatGptCodexUnavailable(_) | Self::Read { .. } | Self::Write { .. } | Self::NoHomeDirectory
        )
    }
}

/// File-backed ChatGPT Codex credential entry.
#[derive(Clone, serde::Deserialize, serde::Serialize, Eq, PartialEq)]
pub struct ChatGptCodexCredentials {
    /// ChatGPT backend access token.
    pub access_token: String,
    /// Refresh token used to obtain a new access token.
    pub refresh_token: String,
    /// Access-token expiry as Unix epoch milliseconds.
    pub expires_at_ms: u64,
    /// ChatGPT account id required by the Codex backend.
    pub account_id: String,
}

/// Auth material used by the ChatGPT Codex provider.
#[derive(Clone, Eq, PartialEq)]
pub struct ChatGptCodexAuth {
    /// ChatGPT backend access token.
    pub access_token: String,
    /// ChatGPT account id required by the Codex backend.
    pub account_id: String,
}

/// Response from the ChatGPT Codex device-code user-code endpoint.
#[derive(Clone, serde::Deserialize, Eq, PartialEq)]
pub struct ChatGptCodexDeviceCode {
    /// Opaque device-auth id sent only to the device-auth token endpoint.
    pub device_auth_id: String,
    /// User-facing short code entered on the verification page.
    #[serde(alias = "usercode")]
    pub user_code: String,
    /// User-facing verification page.
    #[serde(default = "default_device_verification_url")]
    pub verification_uri: Option<String>,
    /// Optional complete verification link.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    /// Device-auth lifetime in seconds.
    #[serde(default = "default_device_code_expires_in")]
    pub expires_in: Option<u64>,
    /// Recommended polling interval in seconds.
    #[serde(default, deserialize_with = "deserialize_optional_u64_from_string_or_number")]
    pub interval: Option<u64>,
}

/// Result of one ChatGPT Codex device-code token poll.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatGptCodexDevicePoll {
    /// Authorization is not complete yet.
    Pending,
    /// Authorization is pending and the client should slow down polling.
    SlowDown,
    /// Authorization completed and credentials can be stored.
    Authorized(ChatGptCodexCredentials),
}

/// Token response from ChatGPT Codex OAuth endpoints.
#[derive(Clone, serde::Deserialize, Eq, PartialEq)]
pub struct ChatGptCodexTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// A browser PKCE login waiting for its loopback callback.
pub struct ChatGptCodexBrowserLogin {
    listener: Option<TcpListener>,
    verifier: String,
    state: String,
    redirect_uri: String,
    authorization_url: String,
    expires_at: Instant,
}

impl std::fmt::Debug for ChatGptCodexBrowserLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptCodexBrowserLogin")
            .field("listener", &self.listener.as_ref().map_or("[not bound]", |_| "[bound]"))
            .field("redirect_uri", &self.redirect_uri)
            .field("authorization_url", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl ChatGptCodexBrowserLogin {
    /// Copyable authorization URL for display or browser launch.
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }

    /// Registered redirect URI for this login.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Poll the loopback callback once and exchange a valid authorization code.
    pub fn poll(&mut self) -> Result<ChatGptCodexBrowserPoll, AuthError> {
        if Instant::now() >= self.expires_at {
            return Err(AuthError::ChatGptCodex("browser OAuth callback expired".to_string()));
        }

        let Some(listener) = self.listener.as_ref() else {
            return Ok(ChatGptCodexBrowserPoll::Pending);
        };
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(ChatGptCodexBrowserPoll::Pending);
            }
            Err(error) => {
                return Err(AuthError::ChatGptCodex(format!(
                    "failed to accept browser callback: {error}"
                )));
            }
        };

        let redirect = read_browser_callback(&mut stream)?;
        let credentials = self.complete_redirect(&redirect)?;
        Ok(ChatGptCodexBrowserPoll::Authorized(credentials))
    }

    /// Complete this login from a pasted full redirect URL.
    pub fn complete_redirect(&self, redirect: &str) -> Result<ChatGptCodexCredentials, AuthError> {
        if Instant::now() >= self.expires_at {
            return Err(AuthError::ChatGptCodex("browser OAuth callback expired".to_string()));
        }
        let code = parse_chatgpt_codex_redirect(redirect, &self.state)?;
        let token = exchange_chatgpt_codex_authorization_code(&code, &self.verifier, &self.redirect_uri)?;
        credentials_from_token_response(token, None)
    }
}

/// Result of checking a browser PKCE callback without blocking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatGptCodexBrowserPoll {
    /// No callback has arrived yet.
    Pending,
    /// The callback completed and credentials are ready to store.
    Authorized(ChatGptCodexCredentials),
}

impl std::fmt::Debug for ChatGptCodexCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptCodexCredentials")
            .field("access_token", &redact_value(&self.access_token))
            .field("refresh_token", &redact_value(&self.refresh_token))
            .field("expires_at_ms", &self.expires_at_ms)
            .field("account_id", &redact_value(&self.account_id))
            .finish()
    }
}

impl std::fmt::Debug for ChatGptCodexAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptCodexAuth")
            .field("access_token", &redact_value(&self.access_token))
            .field("account_id", &redact_value(&self.account_id))
            .finish()
    }
}

impl std::fmt::Debug for ChatGptCodexDeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptCodexDeviceCode")
            .field("device_auth_id", &redact_value(&self.device_auth_id))
            .field("user_code", &"[redacted]")
            .field(
                "verification_uri",
                &self.verification_uri.as_ref().map(|_| "[redacted]"),
            )
            .field(
                "verification_uri_complete",
                &self.verification_uri_complete.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

impl std::fmt::Debug for ChatGptCodexTokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptCodexTokenResponse")
            .field("access_token", &redact_value(&self.access_token))
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|token| redact_value(token)),
            )
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Default)]
struct AuthJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chatgpt_codex: Option<ChatGptCodexCredentials>,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct ChatGptCodexDeviceAuthResponse {
    authorization_code: String,
    code_verifier: String,
}

static CHATGPT_CODEX_REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Global credential store path: `~/.thndrs/credentials.env`.
pub fn global_credentials_path() -> Result<PathBuf, AuthError> {
    let home = utils::home_dir().ok_or(AuthError::NoHomeDirectory)?;
    Ok(home.join(".thndrs").join("credentials.env"))
}

/// ChatGPT Codex auth store path: `~/.thndrs/auth.json`.
pub fn chatgpt_codex_auth_path() -> Result<PathBuf, AuthError> {
    let home = utils::home_dir().ok_or(AuthError::NoHomeDirectory)?;
    Ok(home.join(".thndrs").join("auth.json"))
}

/// Resolve ChatGPT Codex auth, honoring `CHATGPT_CODEX_ACCESS_TOKEN` as a
/// non-persisted process override before reading `~/.thndrs/auth.json`.
pub fn resolve_chatgpt_codex_auth() -> Result<ChatGptCodexAuth, AuthError> {
    if let Ok(token) = std::env::var(CHATGPT_CODEX_ACCESS_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        let account_id = chatgpt_account_id_from_jwt(&token)?;
        return Ok(ChatGptCodexAuth { access_token: token, account_id });
    }

    let path = chatgpt_codex_auth_path()?;
    resolve_chatgpt_codex_file_auth_at(&path, refresh_chatgpt_codex_credentials)
}

fn resolve_chatgpt_codex_file_auth_at(
    path: &Path, refresh: impl FnOnce(ChatGptCodexCredentials) -> Result<ChatGptCodexCredentials, AuthError>,
) -> Result<ChatGptCodexAuth, AuthError> {
    let _guard = CHATGPT_CODEX_REFRESH_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| AuthError::ChatGptCodex("refresh lock poisoned".to_string()))?;
    let mut credentials = read_chatgpt_codex_credentials_at(path)?
        .ok_or_else(|| AuthError::ChatGptCodex("run `thndrs login chatgpt-codex`".to_string()))?;
    if !credentials.is_usable() {
        return Err(AuthError::ChatGptCodex(
            "stored ChatGPT Codex credential is incomplete; run `thndrs login chatgpt-codex`".to_string(),
        ));
    }
    if credentials.is_expired(now_ms()) {
        credentials = refresh(credentials)?;
        write_chatgpt_codex_credentials_at(path, &credentials)?;
    }
    Ok(ChatGptCodexAuth { access_token: credentials.access_token, account_id: credentials.account_id })
}

/// Read the ChatGPT Codex credential entry from `~/.thndrs/auth.json`.
pub fn read_chatgpt_codex_credentials() -> Result<Option<ChatGptCodexCredentials>, AuthError> {
    read_chatgpt_codex_credentials_at(&chatgpt_codex_auth_path()?)
}

/// Write the ChatGPT Codex credential entry to `~/.thndrs/auth.json`.
pub fn write_chatgpt_codex_credentials(credentials: &ChatGptCodexCredentials) -> Result<(), AuthError> {
    write_chatgpt_codex_credentials_at(&chatgpt_codex_auth_path()?, credentials)
}

/// Delete only the ChatGPT Codex credential entry from `~/.thndrs/auth.json`.
pub fn remove_chatgpt_codex_credentials() -> Result<(), AuthError> {
    let path = chatgpt_codex_auth_path()?;
    remove_chatgpt_codex_credentials_at(&path)
}

fn remove_chatgpt_codex_credentials_at(path: &Path) -> Result<(), AuthError> {
    let mut store = read_auth_json_at(path)?;
    store.chatgpt_codex = None;
    write_auth_json_atomic(path, &store)
}

/// Decode a JWT payload and extract the ChatGPT account id claim.
pub fn chatgpt_account_id_from_jwt(jwt: &str) -> Result<String, AuthError> {
    let payload = jwt
        .split('.')
        .nth(1)
        .ok_or_else(|| AuthError::ChatGptCodex("access token is not a JWT".to_string()))?;
    let decoded = base64_url_decode(payload)?;
    let value: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|_| AuthError::ChatGptCodex("access token payload is not valid JSON".to_string()))?;
    value
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| AuthError::ChatGptCodex("access token is missing chatgpt_account_id".to_string()))
}

/// Request a ChatGPT Codex device code.
pub fn request_chatgpt_codex_device_code() -> Result<ChatGptCodexDeviceCode, AuthError> {
    let mut response = ureq::Agent::new_with_defaults()
        .post(DEVICE_USER_CODE_URL)
        .header("content-type", "application/json")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(serde_json::json!({ "client_id": CHATGPT_CODEX_CLIENT_ID }))
        .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("device-code request failed: {e}")))?;
    read_json_response(&mut response, "device-code request")
}

/// Poll the device-code token endpoint until authorization succeeds.
pub fn poll_chatgpt_codex_device_code(code: &ChatGptCodexDeviceCode) -> Result<ChatGptCodexCredentials, AuthError> {
    let mut interval_seconds = code.interval.unwrap_or(5).max(1);
    let deadline = SystemTime::now() + Duration::from_secs(code.expires_in.unwrap_or(DEVICE_CODE_TIMEOUT_SECONDS));
    loop {
        match poll_chatgpt_codex_device_code_once(code)? {
            ChatGptCodexDevicePoll::Authorized(credentials) => return Ok(credentials),
            ChatGptCodexDevicePoll::Pending => {
                if SystemTime::now() >= deadline {
                    return Err(AuthError::ChatGptCodex("device-code login expired".to_string()));
                }
                std::thread::sleep(Duration::from_secs(interval_seconds));
            }
            ChatGptCodexDevicePoll::SlowDown => {
                interval_seconds = interval_seconds.saturating_add(5);
                if SystemTime::now() >= deadline {
                    return Err(AuthError::ChatGptCodex("device-code login expired".to_string()));
                }
                std::thread::sleep(Duration::from_secs(interval_seconds));
            }
        }
    }
}

/// Poll the device-code token endpoint once without sleeping.
pub fn poll_chatgpt_codex_device_code_once(code: &ChatGptCodexDeviceCode) -> Result<ChatGptCodexDevicePoll, AuthError> {
    let mut response = ureq::Agent::new_with_defaults()
        .post(DEVICE_TOKEN_URL)
        .header("content-type", "application/json")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(serde_json::json!({
            "device_auth_id": code.device_auth_id,
            "user_code": code.user_code,
        }))
        .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("device-code poll failed: {e}")))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("device-code poll body read failed: {e}")))?;

    if (200..=299).contains(&status) {
        let auth_response: ChatGptCodexDeviceAuthResponse = serde_json::from_str(&body)
            .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("device-code poll JSON parse failed: {e}")))?;
        let token = exchange_chatgpt_codex_authorization_code(
            &auth_response.authorization_code,
            &auth_response.code_verifier,
            DEVICE_REDIRECT_URI,
        )?;
        return credentials_from_token_response(token, None).map(ChatGptCodexDevicePoll::Authorized);
    }

    match chatgpt_codex_error_code(&body).as_deref() {
        Some("deviceauth_authorization_pending" | "authorization_pending") => Ok(ChatGptCodexDevicePoll::Pending),
        Some("slow_down") => Ok(ChatGptCodexDevicePoll::SlowDown),
        _ if status == 403 || status == 404 => Ok(ChatGptCodexDevicePoll::Pending),
        _ => {
            let message = chatgpt_codex_error_summary(&body);
            Err(AuthError::ChatGptCodex(format!(
                "device-code poll failed with status {status}: {message}"
            )))
        }
    }
}

/// Start a browser-first ChatGPT Codex PKCE login.
///
/// The listener is bound before the authorization URL is returned, so a fast
/// browser redirect cannot race setup. It is non-blocking and expires after a
/// short interval; callers should check it with
/// [`poll_chatgpt_codex_browser_login_once`] and may use
/// [`ChatGptCodexBrowserLogin::complete_redirect`] when a pasted full redirect
/// is needed.
pub fn start_chatgpt_codex_browser_login() -> Result<ChatGptCodexBrowserLogin, AuthError> {
    let listener = TcpListener::bind(PKCE_CALLBACK_ADDR)
        .map_err(|e| AuthError::ChatGptCodex(format!("failed to bind localhost:1455 callback: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| AuthError::ChatGptCodex(format!("failed to prepare browser callback: {e}")))?;

    let verifier = random_pkce_verifier()?;
    let challenge = base64_url_encode(&sha2::Sha256::digest(verifier.as_bytes()));
    let state = random_pkce_verifier()?;
    let authorization_url = chatgpt_codex_pkce_authorize_url(&challenge, &state);

    Ok(ChatGptCodexBrowserLogin {
        listener: Some(listener),
        verifier,
        state,
        redirect_uri: PKCE_CALLBACK_URL.to_string(),
        authorization_url,
        expires_at: Instant::now() + PKCE_CALLBACK_TIMEOUT,
    })
}

#[cfg(test)]
pub fn test_chatgpt_codex_browser_login() -> ChatGptCodexBrowserLogin {
    ChatGptCodexBrowserLogin {
        listener: None,
        verifier: String::from("test-verifier"),
        state: String::from("test-state"),
        redirect_uri: PKCE_CALLBACK_URL.to_string(),
        authorization_url: String::from("https://auth.example.test/oauth/authorize?state=test-state"),
        expires_at: Instant::now() + PKCE_CALLBACK_TIMEOUT,
    }
}

/// Poll a browser login once without sleeping.
pub fn poll_chatgpt_codex_browser_login_once(
    login: &mut ChatGptCodexBrowserLogin,
) -> Result<ChatGptCodexBrowserPoll, AuthError> {
    login.poll()
}

/// Open a ChatGPT authorization URL using the host's conventional browser launcher.
pub fn open_chatgpt_codex_authorization_url(url: &str) -> Result<(), AuthError> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(AuthError::ChatGptCodex(
        "automatic browser launch is unavailable".to_string(),
    ));

    command
        .arg(url)
        .status()
        .map_err(|_| AuthError::ChatGptCodex("automatic browser launch failed".to_string()))
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(AuthError::ChatGptCodex("automatic browser launch failed".to_string()))
            }
        })
}

/// Browser PKCE login for headless CLI callers that cannot use the TUI recovery surface.
pub fn login_chatgpt_codex_with_browser_pkce() -> Result<ChatGptCodexCredentials, AuthError> {
    let mut login = start_chatgpt_codex_browser_login()?;
    eprintln!(
        "Open this URL to continue ChatGPT Codex browser login (copy/paste the URL if it does not open):\n{}",
        login.authorization_url()
    );
    let _ = open_chatgpt_codex_authorization_url(login.authorization_url());
    loop {
        match login.poll()? {
            ChatGptCodexBrowserPoll::Pending => std::thread::sleep(Duration::from_millis(100)),
            ChatGptCodexBrowserPoll::Authorized(credentials) => return Ok(credentials),
        }
    }
}

/// Convert a token response into persistable ChatGPT Codex credentials.
pub fn credentials_from_token_response(
    response: ChatGptCodexTokenResponse, previous_refresh_token: Option<String>,
) -> Result<ChatGptCodexCredentials, AuthError> {
    let account_id = chatgpt_account_id_from_jwt(&response.access_token)?;
    let refresh_token = response
        .refresh_token
        .or(previous_refresh_token)
        .ok_or_else(|| AuthError::ChatGptCodex("token response is missing refresh_token".to_string()))?;
    Ok(ChatGptCodexCredentials {
        access_token: response.access_token,
        refresh_token,
        expires_at_ms: now_ms() + response.expires_in.unwrap_or(3600).saturating_mul(1000),
        account_id,
    })
}

/// Project credential store path: `<workspace>/.thndrs/credentials.env`.
pub fn project_credentials_path(workspace: &Path) -> PathBuf {
    workspace.join(".thndrs").join("credentials.env")
}

/// Read all credential key-value pairs from a `.env`-format file.
///
/// Blank lines, comment lines (starting with `#`), and `export`-prefixed lines
/// are skipped. The first assignment for each key wins (duplicate keys are
/// ignored).
pub fn read_credentials(path: &Path) -> Result<BTreeMap<String, String>, AuthError> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let file = fs::File::open(path).map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
    let reader = std::io::BufReader::new(file);
    let mut credentials = BTreeMap::new();

    for (i, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
        if let Some((key, value)) = parse_credential_line(&line) {
            credentials.entry(key).or_insert(value);
        } else if !line.trim().is_empty() && !line.trim().starts_with('#') {
            return Err(AuthError::Malformed {
                path: path.to_path_buf(),
                message: format!("line {}: invalid env format", i + 1),
            });
        }
    }

    Ok(credentials)
}

/// Write a single credential key-value pair to the credential file,
/// preserving all unrelated entries.
///
/// If the file does not exist, it is created. If the key already exists in the
/// file, its line is replaced. Otherwise the new assignment is appended.
///
/// The write is atomic: content is written to a temporary file in the same
/// directory, then renamed over the target path.
///
/// On Unix, the file is created with mode `0600`.
pub fn set_credential(path: &Path, key: &str, value: &str) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    }

    let existing = if path.is_file() { read_lines(path)? } else { Vec::new() };

    let new_line = format!("{key}={value}");

    let mut replaced = false;
    let mut lines = existing;

    for line in &mut lines {
        if let Some((line_key, _)) = parse_credential_line(line)
            && line_key == key
            && !replaced
        {
            *line = new_line.clone();
            replaced = true;
        }
    }

    if !replaced {
        lines.push(new_line);
    }

    write_lines_atomic(path, &lines)
}

/// Remove a single credential key from the credential file, preserving all
/// other entries.
///
/// If the key appears multiple times, all matching lines are removed.
pub fn remove_credential(path: &Path, key: &str) -> Result<(), AuthError> {
    if !path.is_file() {
        return Ok(());
    }

    let lines = read_lines(path)?;
    let filtered: Vec<String> = lines
        .into_iter()
        .filter(
            |line| {
                if let Some((line_key, _)) = parse_credential_line(line) { line_key != key } else { true }
            },
        )
        .collect();

    write_lines_atomic(path, &filtered)
}

/// Redact a credential value for display/debug output.
///
/// This returns a fixed sentinel string so callers cannot accidentally leak
/// value prefixes, hashes, suffixes, or lengths.
pub fn redact_value(_value: &str) -> String {
    String::from("[redacted]")
}

/// Resolve a credential by checking all sources in precedence order.
///
/// Order: process environment → global credential store
/// (`~/.thndrs/credentials.env`) → project credential store
/// (`<workspace>/.thndrs/credentials.env`) → workspace `.env` (legacy).
///
/// Returns `None` when the key is not found in any source.
pub fn resolve_credential(key: &str, workspace: &Path) -> Option<(String, CredentialSource)> {
    if let Ok(value) = std::env::var(key)
        && !value.is_empty()
    {
        return Some((value, CredentialSource::Environment));
    }

    if let Ok(global_path) = global_credentials_path()
        && let Ok(creds) = read_credentials(&global_path)
        && let Some(value) = creds.get(key)
    {
        return Some((value.clone(), CredentialSource::GlobalStore));
    }

    let project_path = project_credentials_path(workspace);
    if let Ok(creds) = read_credentials(&project_path)
        && let Some(value) = creds.get(key)
    {
        return Some((value.clone(), CredentialSource::ProjectStore));
    }

    let dotenv_path = workspace.join(".env");
    if let Ok(creds) = read_credentials(&dotenv_path)
        && let Some(value) = creds.get(key)
    {
        return Some((value.clone(), CredentialSource::DotEnvLegacy));
    }

    None
}

/// Return the [`CredentialSource`] for a key without revealing its value.
pub fn credential_source(key: &str, workspace: &Path) -> Option<CredentialSource> {
    resolve_credential(key, workspace).map(|(_, source)| source)
}

/// Ensure `.thndrs/credentials.env` is listed in `.git/info/exclude` so it
/// cannot be accidentally committed.
///
/// If the workspace is not inside a git repository, this is a no-op.
/// Repeated calls are idempotent: the exclude entry is never duplicated.
pub fn ensure_git_exclude(workspace: &Path) -> Result<(), AuthError> {
    let git_dir = workspace.join(".git");
    let exclude_path = git_dir.join("info").join("exclude");
    if !exclude_path.is_file() {
        return Ok(());
    }

    let exclude_entry = ".thndrs/credentials.env";
    let content = fs::read_to_string(&exclude_path)
        .map_err(|source| AuthError::GitExclude(format!("failed to read {}: {source}", exclude_path.display())))?;

    let already_excluded = content.lines().any(|line| line.trim() == exclude_entry);
    if already_excluded {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&exclude_path)
        .map_err(|source| AuthError::GitExclude(format!("failed to open {}: {source}", exclude_path.display())))?;

    writeln!(file, "{exclude_entry}")
        .map_err(|source| AuthError::GitExclude(format!("failed to write {}: {source}", exclude_path.display())))?;

    Ok(())
}

/// Parse a single `.env`-format assignment line.
///
/// Supports `KEY=value`, `export KEY=value`, and quoted values
/// (`KEY="value"`, `KEY='value'`).
fn parse_credential_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, raw_value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let raw_value = raw_value.trim();
    let value = raw_value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| raw_value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(raw_value);
    if value.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

/// Read all lines from a text file.
fn read_lines(path: &Path) -> Result<Vec<String>, AuthError> {
    let file = fs::File::open(path).map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
    let reader = std::io::BufReader::new(file);
    reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| AuthError::Read { path: path.to_path_buf(), source })
}

/// Write lines to a temporary file in the same directory, then atomically
/// rename over the target path.
///
/// On Unix, sets file mode `0600`.
fn write_lines_atomic(path: &Path, lines: &[String]) -> Result<(), AuthError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;

    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;

    for line in lines {
        writeln!(tmp, "{line}").map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    }
    tmp.flush()
        .map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;

    set_unix_permissions(tmp.path());

    tmp.persist(path)
        .map(|_| ())
        .map_err(|e| AuthError::Write { path: path.to_path_buf(), source: e.error })
}

fn read_chatgpt_codex_credentials_at(path: &Path) -> Result<Option<ChatGptCodexCredentials>, AuthError> {
    Ok(read_auth_json_at(path)?.chatgpt_codex)
}

fn write_chatgpt_codex_credentials_at(path: &Path, credentials: &ChatGptCodexCredentials) -> Result<(), AuthError> {
    let mut store = read_auth_json_at(path)?;
    store.chatgpt_codex = Some(credentials.clone());
    write_auth_json_atomic(path, &store)
}

fn read_auth_json_at(path: &Path) -> Result<AuthJson, AuthError> {
    if !path.is_file() {
        return Ok(AuthJson::default());
    }
    let content = fs::read_to_string(path).map_err(|source| AuthError::Read { path: path.to_path_buf(), source })?;
    serde_json::from_str(&content).map_err(|e| AuthError::ChatGptCodex(format!("malformed {}: {e}", path.display())))
}

fn write_auth_json_atomic(path: &Path, store: &AuthJson) -> Result<(), AuthError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    let mut tmp =
        tempfile::NamedTempFile::new_in(dir).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    serde_json::to_writer_pretty(&mut tmp, store)
        .map_err(|e| AuthError::ChatGptCodex(format!("serialize auth store: {e}")))?;
    writeln!(tmp).map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    tmp.flush()
        .map_err(|source| AuthError::Write { path: path.to_path_buf(), source })?;
    set_unix_permissions(tmp.path());
    tmp.persist(path)
        .map(|_| ())
        .map_err(|e| AuthError::Write { path: path.to_path_buf(), source: e.error })
}

fn refresh_chatgpt_codex_credentials(current: ChatGptCodexCredentials) -> Result<ChatGptCodexCredentials, AuthError> {
    let token = exchange_chatgpt_codex_token_form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &current.refresh_token),
        ("client_id", CHATGPT_CODEX_CLIENT_ID),
    ])?;
    credentials_from_token_response(token, Some(current.refresh_token))
}

fn exchange_chatgpt_codex_authorization_code(
    code: &str, verifier: &str, redirect_uri: &str,
) -> Result<ChatGptCodexTokenResponse, AuthError> {
    exchange_chatgpt_codex_token_form(&[
        ("grant_type", "authorization_code"),
        ("client_id", CHATGPT_CODEX_CLIENT_ID),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
    ])
}

fn exchange_chatgpt_codex_token_form(params: &[(&str, &str)]) -> Result<ChatGptCodexTokenResponse, AuthError> {
    let body = form_urlencoded(params);
    let mut response = ureq::Agent::new_with_defaults()
        .post(OAUTH_TOKEN_URL)
        .header("content-type", "application/x-www-form-urlencoded")
        .config()
        .http_status_as_error(false)
        .build()
        .send(&body)
        .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("token request failed: {e}")))?;
    read_json_response(&mut response, "token request")
}

fn form_urlencoded(params: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

fn read_json_response<T>(response: &mut ureq::http::Response<ureq::Body>, context: &str) -> Result<T, AuthError>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("{context} body read failed: {e}")))?;
    if !(200..=299).contains(&status) {
        return Err(chatgpt_codex_response_error(context, status, &body));
    }
    serde_json::from_str(&body)
        .map_err(|e| AuthError::ChatGptCodexUnavailable(format!("{context} JSON parse failed: {e}")))
}

fn chatgpt_codex_response_error(context: &str, status: u16, body: &str) -> AuthError {
    let message = format!(
        "{context} failed with status {status}: {}",
        chatgpt_codex_error_summary(body)
    );
    if status == 408 || status == 429 || (500..=599).contains(&status) {
        AuthError::ChatGptCodexUnavailable(message)
    } else {
        AuthError::ChatGptCodex(message)
    }
}

fn chatgpt_codex_error_code(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let error = value.get("error")?;
    if let Some(code) = error.as_str() {
        return Some(code.to_string());
    }
    error.get("code").and_then(|v| v.as_str()).map(str::to_string)
}

fn chatgpt_codex_error_summary(body: &str) -> &'static str {
    match chatgpt_codex_error_code(body).as_deref() {
        Some("deviceauth_authorization_pending" | "authorization_pending") => "authorization pending",
        Some("slow_down") => "provider requested slower polling",
        Some("invalid_grant" | "invalid_request") => "authorization was rejected or expired",
        Some("access_denied") => "authorization was denied",
        _ => "provider returned an authentication error",
    }
}

fn chatgpt_codex_pkce_authorize_url(challenge: &str, state: &str) -> String {
    let mut url = url::Url::parse(PKCE_AUTHORIZE_URL).expect("valid ChatGPT Codex authorize URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CHATGPT_CODEX_CLIENT_ID)
        .append_pair("redirect_uri", PKCE_CALLBACK_URL)
        .append_pair("scope", OAUTH_SCOPE)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", OAUTH_ORIGINATOR);
    url.to_string()
}

fn read_browser_callback(stream: &mut TcpStream) -> Result<String, AuthError> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| AuthError::ChatGptCodex(format!("failed to prepare browser callback: {e}")))?;
    let mut request_line = String::new();
    std::io::BufReader::new(
        stream
            .try_clone()
            .map_err(|e| AuthError::ChatGptCodex(format!("failed to read browser callback: {e}")))?,
    )
    .read_line(&mut request_line)
    .map_err(|e| AuthError::ChatGptCodex(format!("failed to read browser callback: {e}")))?;

    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| AuthError::ChatGptCodex("browser callback was malformed".to_string()))?;
    let redirect = if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("http://localhost:1455{target}")
    };
    let body = "You can return to thndrs.\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    Ok(redirect)
}

fn parse_chatgpt_codex_redirect(redirect: &str, expected_state: &str) -> Result<String, AuthError> {
    let url = url::Url::parse(redirect)
        .map_err(|_| AuthError::ChatGptCodex("browser callback URL was malformed".to_string()))?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        || url.port() != Some(1455)
        || url.path() != "/auth/callback"
    {
        return Err(AuthError::ChatGptCodex(
            "browser callback URL was unexpected".to_string(),
        ));
    }
    if url
        .query_pairs()
        .any(|(key, _)| key == "error" || key == "error_description")
    {
        return Err(AuthError::ChatGptCodex("ChatGPT authorization was denied".to_string()));
    }
    let state = url
        .query_pairs()
        .find_map(|(key, value)| if key == "state" { Some(value.into_owned()) } else { None })
        .ok_or_else(|| AuthError::ChatGptCodex("browser callback did not include state".to_string()))?;
    if state != expected_state {
        return Err(AuthError::ChatGptCodex(
            "browser callback state did not match".to_string(),
        ));
    }
    url.query_pairs()
        .find_map(|(key, value)| if key == "code" { Some(value.into_owned()) } else { None })
        .ok_or_else(|| AuthError::ChatGptCodex("browser callback did not include code".to_string()))
}

fn default_device_verification_url() -> Option<String> {
    Some(DEVICE_VERIFICATION_URL.to_string())
}

fn default_device_code_expires_in() -> Option<u64> {
    Some(DEVICE_CODE_TIMEOUT_SECONDS)
}

fn deserialize_optional_u64_from_string_or_number<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("expected non-negative integer"))
            .map(Some),
        serde_json::Value::String(value) => value.trim().parse::<u64>().map(Some).map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("expected integer or string integer")),
    }
}

fn random_pkce_verifier() -> Result<String, AuthError> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut bytes))
        .map_err(|e| AuthError::ChatGptCodex(format!("failed to generate PKCE verifier: {e}")))?;
    Ok(base64_url_encode(&bytes))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, AuthError> {
    let mut out = Vec::new();
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    for ch in input.chars().filter(|ch| *ch != '=') {
        let value = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '-' => 62,
            '_' => 63,
            _ => {
                return Err(AuthError::ChatGptCodex(
                    "access token payload is not base64url".to_string(),
                ));
            }
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

fn base64_url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied().unwrap_or(0);
        let b2 = bytes.get(i + 2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(ALPHABET[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        }
        if i + 2 < bytes.len() {
            out.push(ALPHABET[(b2 & 0b11_1111) as usize] as char);
        }
        i += 3;
    }
    out
}

/// Set Unix file mode `0600` (owner read/write only) when supported.
#[cfg(unix)]
fn set_unix_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_unix_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_test_lock() -> crate::test_env::Guard {
        crate::test_env::lock()
    }

    fn temp_cred_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("credentials.env");
        (dir, path)
    }

    fn write_cred_file(path: &Path, content: &str) {
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::write(path, content).unwrap();
    }

    fn jwt_with_account(account_id: &str) -> String {
        let payload = serde_json::json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": account_id,
            }
        });
        format!(
            "header.{}.sig",
            base64_url_encode(serde_json::to_string(&payload).unwrap().as_bytes())
        )
    }

    #[test]
    fn parses_simple_assignment() {
        assert_eq!(
            parse_credential_line("UMANS_API_KEY=sk-abc123"),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn chatgpt_codex_account_id_decodes_from_jwt_without_logging_token() {
        let token = jwt_with_account("acct_test");
        assert_eq!(chatgpt_account_id_from_jwt(&token).unwrap(), "acct_test");
        assert!(matches!(
            chatgpt_account_id_from_jwt("header.e30.sig"),
            Err(AuthError::ChatGptCodex(_))
        ));
    }

    #[test]
    fn chatgpt_codex_account_id_rejects_missing_or_malformed_claims() {
        let missing_claim = format!(
            "header.{}.sig",
            base64_url_encode(br#"{"https://api.openai.com/auth":{}}"#)
        );
        let malformed_claim = format!(
            "header.{}.sig",
            base64_url_encode(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":42}}"#)
        );

        assert!(matches!(
            chatgpt_account_id_from_jwt(&missing_claim),
            Err(AuthError::ChatGptCodex(message)) if message.contains("missing chatgpt_account_id")
        ));
        assert!(matches!(
            chatgpt_account_id_from_jwt(&malformed_claim),
            Err(AuthError::ChatGptCodex(message)) if message.contains("missing chatgpt_account_id")
        ));
        assert!(matches!(
            chatgpt_account_id_from_jwt("not-a-jwt"),
            Err(AuthError::ChatGptCodex(message)) if message.contains("not a JWT")
        ));
    }

    #[test]
    fn chatgpt_token_response_distinguishes_service_unavailability_from_rejection() {
        assert!(matches!(
            chatgpt_codex_response_error("token request", 503, r#"{"error":"server_error"}"#),
            AuthError::ChatGptCodexUnavailable(message) if message.contains("status 503")
        ));
        assert!(matches!(
            chatgpt_codex_response_error("token request", 429, r#"{"error":"rate_limited"}"#),
            AuthError::ChatGptCodexUnavailable(message) if message.contains("status 429")
        ));
        assert!(matches!(
            chatgpt_codex_response_error("token request", 401, r#"{"error":"invalid_grant"}"#),
            AuthError::ChatGptCodex(message) if message.contains("status 401")
        ));
    }

    #[test]
    fn chatgpt_codex_debug_output_redacts_tokens() {
        let credentials = ChatGptCodexCredentials {
            access_token: "access-secret-token".to_string(),
            refresh_token: "refresh-secret-token".to_string(),
            expires_at_ms: 123,
            account_id: "acct_file".to_string(),
        };
        let auth =
            ChatGptCodexAuth { access_token: "auth-secret-token".to_string(), account_id: "acct_file".to_string() };
        let token = ChatGptCodexTokenResponse {
            access_token: "response-access-secret".to_string(),
            refresh_token: Some("response-refresh-secret".to_string()),
            expires_in: Some(3600),
        };

        for debug in [format!("{credentials:?}"), format!("{auth:?}"), format!("{token:?}")] {
            assert!(debug.contains("[redacted]"));
            assert!(!debug.contains("secret-token"));
            assert!(!debug.contains("response-access-secret"));
            assert!(!debug.contains("response-refresh-secret"));
            assert!(!debug.contains("acct_file"));
        }
    }

    #[test]
    fn chatgpt_codex_device_code_accepts_string_interval() {
        let code: ChatGptCodexDeviceCode =
            serde_json::from_str(r#"{"device_auth_id":"device-auth-secret","user_code":"USER-CODE","interval":"5"}"#)
                .expect("parse device-auth response");

        assert_eq!(code.device_auth_id, "device-auth-secret");
        assert_eq!(code.user_code, "USER-CODE");
        assert_eq!(code.interval, Some(5));
        assert_eq!(code.expires_in, Some(DEVICE_CODE_TIMEOUT_SECONDS));
        assert_eq!(code.verification_uri.as_deref(), Some(DEVICE_VERIFICATION_URL));
    }

    #[test]
    fn chatgpt_codex_device_code_debug_redacts_device_auth_id() {
        let code = ChatGptCodexDeviceCode {
            device_auth_id: "device-auth-secret-from-test".to_string(),
            user_code: "USER-CODE".to_string(),
            verification_uri: Some(DEVICE_VERIFICATION_URL.to_string()),
            verification_uri_complete: Some("https://auth.example.test/device?user_code=USER-CODE".to_string()),
            expires_in: Some(DEVICE_CODE_TIMEOUT_SECONDS),
            interval: Some(5),
        };

        let debug = format!("{code:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("device-auth-secret-from-test"));
        assert!(!debug.contains("USER-CODE"));
    }

    #[test]
    fn chatgpt_codex_pkce_authorize_url_uses_codex_app_client() {
        let url = url::Url::parse(&chatgpt_codex_pkce_authorize_url("challenge-test", "state-test")).unwrap();
        let params: BTreeMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(url.as_str().split('?').next(), Some(PKCE_AUTHORIZE_URL));
        assert_eq!(
            params.get("client_id").map(String::as_str),
            Some(CHATGPT_CODEX_CLIENT_ID)
        );
        assert_eq!(params.get("redirect_uri").map(String::as_str), Some(PKCE_CALLBACK_URL));
        assert_eq!(params.get("scope").map(String::as_str), Some(OAUTH_SCOPE));
        assert_eq!(params.get("code_challenge").map(String::as_str), Some("challenge-test"));
        assert_eq!(params.get("code_challenge_method").map(String::as_str), Some("S256"));
        assert_eq!(params.get("state").map(String::as_str), Some("state-test"));
        assert_eq!(
            params.get("id_token_add_organizations").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            params.get("codex_cli_simplified_flow").map(String::as_str),
            Some("true")
        );
        assert_eq!(params.get("originator").map(String::as_str), Some(OAUTH_ORIGINATOR));
    }

    #[test]
    fn chatgpt_codex_browser_redirect_requires_matching_state_without_logging_query() {
        let redirect = "http://localhost:1455/auth/callback?code=authorization-code-secret&state=state-ok";
        assert_eq!(
            parse_chatgpt_codex_redirect(redirect, "state-ok").unwrap(),
            "authorization-code-secret"
        );

        let error = parse_chatgpt_codex_redirect(redirect, "state-wrong").expect_err("state mismatch");
        assert!(error.to_string().contains("state did not match"));
        assert!(!error.to_string().contains("authorization-code-secret"));
        assert!(!format!("{error:?}").contains("authorization-code-secret"));

        let error = parse_chatgpt_codex_redirect(
            "https://localhost:1455/auth/callback?code=authorization-code-secret&state=state-ok",
            "state-ok",
        )
        .expect_err("unexpected callback scheme");
        assert!(error.to_string().contains("URL was unexpected"));
    }

    #[test]
    fn chatgpt_codex_browser_redirect_rejects_denial_without_query_details() {
        let error = parse_chatgpt_codex_redirect(
            "http://localhost:1455/auth/callback?error=access_denied&error_description=secret-account-detail&state=state-ok",
            "state-ok",
        )
        .expect_err("denied callback");

        assert!(error.to_string().contains("authorization was denied"));
        assert!(!error.to_string().contains("secret-account-detail"));
    }

    #[test]
    fn chatgpt_codex_auth_json_preserves_unrelated_entries_and_removes_only_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("auth.json");
        fs::write(&path, r#"{"other":{"keep":true}}"#).unwrap();
        let credentials = ChatGptCodexCredentials {
            access_token: jwt_with_account("acct_file"),
            refresh_token: "refresh-secret".to_string(),
            expires_at_ms: 123,
            account_id: "acct_file".to_string(),
        };

        write_chatgpt_codex_credentials_at(&path, &credentials).expect("write");
        assert_eq!(read_chatgpt_codex_credentials_at(&path).unwrap(), Some(credentials));
        let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["chatgpt_codex"]["access_token"], jwt_with_account("acct_file"));
        assert_eq!(written["chatgpt_codex"]["refresh_token"], "refresh-secret");
        assert_eq!(written["chatgpt_codex"]["expires_at_ms"], 123);
        assert_eq!(written["chatgpt_codex"]["account_id"], "acct_file");
        assert_eq!(written["other"]["keep"], true);

        remove_chatgpt_codex_credentials_at(&path).expect("remove");
        assert_eq!(read_chatgpt_codex_credentials_at(&path).unwrap(), None);
        let removed: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(removed["other"]["keep"], true);
    }

    #[test]
    fn chatgpt_codex_env_token_override_does_not_write_auth_json() {
        let _guard = env_test_lock();
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let old_home = std::env::var_os("HOME");
        let old_token = std::env::var_os(CHATGPT_CODEX_ACCESS_TOKEN_ENV);
        let token = jwt_with_account("acct_env");

        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var(CHATGPT_CODEX_ACCESS_TOKEN_ENV, &token);
        }

        let auth = resolve_chatgpt_codex_auth().expect("resolve env auth");
        let auth_path = home.join(".thndrs").join("auth.json");

        unsafe {
            if let Some(home) = old_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(token) = old_token {
                std::env::set_var(CHATGPT_CODEX_ACCESS_TOKEN_ENV, token);
            } else {
                std::env::remove_var(CHATGPT_CODEX_ACCESS_TOKEN_ENV);
            }
        }

        assert_eq!(auth.access_token, token);
        assert_eq!(auth.account_id, "acct_env");
        assert!(!auth_path.exists());
    }

    #[test]
    fn chatgpt_codex_expired_credentials_refresh_under_locked_resolver() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("auth.json");
        let old_credentials = ChatGptCodexCredentials {
            access_token: jwt_with_account("acct_old"),
            refresh_token: "refresh-old".to_string(),
            expires_at_ms: 1,
            account_id: "acct_old".to_string(),
        };
        write_chatgpt_codex_credentials_at(&path, &old_credentials).expect("write old credentials");

        let refreshed_token = jwt_with_account("acct_new");
        let auth = resolve_chatgpt_codex_file_auth_at(&path, |current| {
            assert_eq!(current.refresh_token, "refresh-old");
            Ok(ChatGptCodexCredentials {
                access_token: refreshed_token.clone(),
                refresh_token: "refresh-new".to_string(),
                expires_at_ms: now_ms() + 3_600_000,
                account_id: "acct_new".to_string(),
            })
        })
        .expect("refresh");

        assert_eq!(auth.access_token, refreshed_token);
        assert_eq!(auth.account_id, "acct_new");
        let stored = read_chatgpt_codex_credentials_at(&path)
            .expect("read refreshed credentials")
            .expect("stored credentials");
        assert_eq!(stored.refresh_token, "refresh-new");
        assert_eq!(stored.account_id, "acct_new");
    }

    #[cfg(unix)]
    #[test]
    fn chatgpt_codex_auth_json_is_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("auth.json");
        let credentials = ChatGptCodexCredentials {
            access_token: jwt_with_account("acct_file"),
            refresh_token: "refresh-secret".to_string(),
            expires_at_ms: 123,
            account_id: "acct_file".to_string(),
        };
        write_chatgpt_codex_credentials_at(&path, &credentials).expect("write");

        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn parses_export_prefix() {
        assert_eq!(
            parse_credential_line("export UMANS_API_KEY=sk-abc123"),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_double_quoted_value() {
        assert_eq!(
            parse_credential_line(r#"UMANS_API_KEY="sk-abc123""#),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_single_quoted_value() {
        assert_eq!(
            parse_credential_line("UMANS_API_KEY='sk-abc123'"),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn parses_export_quoted_value() {
        assert_eq!(
            parse_credential_line(r#"export UMANS_API_KEY="sk-abc123""#),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn skips_blank_lines() {
        assert_eq!(parse_credential_line(""), None);
        assert_eq!(parse_credential_line("   "), None);
    }

    #[test]
    fn skips_comment_lines() {
        assert_eq!(parse_credential_line("# this is a comment"), None);
        assert_eq!(parse_credential_line("  # indented comment"), None);
    }

    #[test]
    fn skips_empty_values() {
        assert_eq!(parse_credential_line("UMANS_API_KEY="), None);
        assert_eq!(parse_credential_line("UMANS_API_KEY=\"\""), None);
    }

    #[test]
    fn rejects_lines_without_equals() {
        assert_eq!(parse_credential_line("just a string"), None);
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(
            parse_credential_line("  UMANS_API_KEY = sk-abc123  "),
            Some(("UMANS_API_KEY".to_string(), "sk-abc123".to_string()))
        );
    }

    #[test]
    fn value_can_contain_equals() {
        assert_eq!(
            parse_credential_line(r#"UMANS_API_KEY="sk-abc=xyz""#),
            Some(("UMANS_API_KEY".to_string(), "sk-abc=xyz".to_string()))
        );
    }

    #[test]
    fn reads_empty_file_as_empty_map() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "");
        let creds = read_credentials(&path).unwrap();
        assert!(creds.is_empty());
    }

    #[test]
    fn reads_multiple_credentials() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans-1\nOPENCODE_GO_KEY=sk-opencode-1\n");
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-umans-1");
        assert_eq!(creds.get("OPENCODE_GO_KEY").unwrap(), "sk-opencode-1");
        assert_eq!(creds.len(), 2);
    }

    #[test]
    fn first_key_wins_on_duplicate() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=first\nUMANS_API_KEY=second\n");
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "first");
    }

    #[test]
    fn reads_with_comments_and_blanks() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "# Umans key\nUMANS_API_KEY=sk-umans-1\n\n# OpenCode key\nOPENCODE_GO_KEY=sk-opencode-1\n",
        );
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.len(), 2);
    }

    #[test]
    fn missing_file_returns_empty() {
        let (_dir, path) = temp_cred_path();
        let creds = read_credentials(&path).unwrap();
        assert!(creds.is_empty());
    }

    #[test]
    fn malformed_file_is_rejected() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-valid\nthis is not valid\n");
        let err = read_credentials(&path).unwrap_err();
        assert!(matches!(err, AuthError::Malformed { .. }));
    }

    #[test]
    fn malformed_error_does_not_print_value() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "totally invalid line\n");
        let err = read_credentials(&path).unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("sk-"), "error should not contain secret-like values");
    }

    #[test]
    fn creates_new_file() {
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-fresh").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-fresh");
    }

    #[test]
    fn appends_new_key_to_existing_file() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans-1\n");
        set_credential(&path, "OPENCODE_GO_KEY", "sk-opencode-1").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-umans-1");
        assert_eq!(creds.get("OPENCODE_GO_KEY").unwrap(), "sk-opencode-1");
    }

    #[test]
    fn replaces_existing_key_in_place() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-old\n");
        set_credential(&path, "UMANS_API_KEY", "sk-new").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-new");
    }

    #[test]
    fn preserves_unrelated_entries() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "UMANS_API_KEY=sk-umans\nOTHER_VAR=keep-me\nOPENCODE_GO_KEY=sk-opencode\n",
        );
        set_credential(&path, "OPENCODE_GO_KEY", "sk-opencode-new").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("OTHER_VAR=keep-me"),
            "unrelated entries must be preserved"
        );
        assert!(
            content.contains("UMANS_API_KEY=sk-umans"),
            "other credentials must be preserved"
        );
    }

    #[test]
    fn preserves_comments_and_blanks() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "# Umans key\nUMANS_API_KEY=sk-umans\n\n# OpenCode key\nOPENCODE_GO_KEY=sk-opencode\n",
        );
        set_credential(&path, "OPENCODE_GO_KEY", "sk-opencode-new").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Umans key"));
        assert!(content.contains("# OpenCode key"));
    }

    #[test]
    fn set_is_idempotent() {
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-val").unwrap();
        set_credential(&path, "UMANS_API_KEY", "sk-val").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-val");
        assert_eq!(creds.len(), 1);
    }

    #[test]
    fn removes_existing_key() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans\nOPENCODE_GO_KEY=sk-opencode\n");
        remove_credential(&path, "UMANS_API_KEY").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert!(!creds.contains_key("UMANS_API_KEY"));
        assert_eq!(creds.get("OPENCODE_GO_KEY").unwrap(), "sk-opencode");
    }

    #[test]
    fn removing_missing_key_is_noop() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(&path, "UMANS_API_KEY=sk-umans\n");
        remove_credential(&path, "OPENCODE_GO_KEY").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-umans");
    }

    #[test]
    fn removing_from_missing_file_is_noop() {
        let (_dir, path) = temp_cred_path();
        remove_credential(&path, "UMANS_API_KEY").unwrap();
    }

    #[test]
    fn remove_preserves_unrelated_entries() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "UMANS_API_KEY=sk-umans\nOTHER_VAR=keep-me\nOPENCODE_GO_KEY=sk-opencode\n",
        );
        remove_credential(&path, "OPENCODE_GO_KEY").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("OTHER_VAR=keep-me"));
        assert!(content.contains("UMANS_API_KEY=sk-umans"));
        assert!(!content.contains("OPENCODE_GO_KEY"));
    }

    #[test]
    fn redact_returns_fixed_string() {
        assert_eq!(redact_value(""), "[redacted]");
        assert_eq!(redact_value("sk-abc123"), "[redacted]");
        assert_eq!(redact_value("my-secret-key-12345"), "[redacted]");
    }

    #[test]
    fn global_path_uses_home_thndrs() {
        let _guard = env_test_lock();
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &home) };
        let path = global_credentials_path().unwrap();
        assert_eq!(path, home.join(".thndrs").join("credentials.env"));
        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn global_path_fails_without_home() {
        let _guard = env_test_lock();
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::remove_var("HOME") };
        let old_profile = std::env::var_os("USERPROFILE");
        unsafe { std::env::remove_var("USERPROFILE") };
        let err = global_credentials_path().unwrap_err();
        assert!(matches!(err, AuthError::NoHomeDirectory));
        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            }
            if let Some(p) = old_profile {
                std::env::set_var("USERPROFILE", p);
            }
        }
    }

    #[test]
    fn project_path_uses_workspace_thndrs() {
        let path = project_credentials_path(Path::new("/repo"));
        assert_eq!(path, PathBuf::from("/repo/.thndrs/credentials.env"));
    }

    #[test]
    fn git_exclude_creates_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git").join("info");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("exclude"), "# git exclude\n").unwrap();

        ensure_git_exclude(tmp.path()).unwrap();
        let content = fs::read_to_string(git_dir.join("exclude")).unwrap();
        assert!(content.contains(".thndrs/credentials.env"));
    }

    #[test]
    fn git_exclude_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git").join("info");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("exclude"), "# git exclude\n").unwrap();

        ensure_git_exclude(tmp.path()).unwrap();
        ensure_git_exclude(tmp.path()).unwrap();
        ensure_git_exclude(tmp.path()).unwrap();

        let content = fs::read_to_string(git_dir.join("exclude")).unwrap();
        let count = content
            .lines()
            .filter(|l| l.trim() == ".thndrs/credentials.env")
            .count();
        assert_eq!(count, 1, "entry should appear exactly once");
    }

    #[test]
    fn git_exclude_noop_without_git_dir() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_git_exclude(tmp.path()).unwrap();
    }

    #[test]
    fn git_exclude_preserves_existing_content() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git").join("info");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("exclude"), "*.log\n.env\n").unwrap();

        ensure_git_exclude(tmp.path()).unwrap();
        let content = fs::read_to_string(git_dir.join("exclude")).unwrap();
        assert!(content.contains("*.log"));
        assert!(content.contains(".env"));
        assert!(content.contains(".thndrs/credentials.env"));
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("dir").join("credentials.env");
        set_credential(&path, "UMANS_API_KEY", "sk-test").unwrap();
        assert!(path.is_file());
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-test");
    }

    #[test]
    fn read_write_round_trip() {
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-round-trip").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.get("UMANS_API_KEY").unwrap(), "sk-round-trip");
    }

    #[test]
    fn source_labels_are_readable() {
        assert_eq!(CredentialSource::Environment.label(), "environment");
        assert_eq!(CredentialSource::GlobalStore.label(), "global credentials");
        assert_eq!(CredentialSource::ProjectStore.label(), "project credentials");
        assert_eq!(CredentialSource::DotEnvLegacy.label(), ".env");
    }

    #[test]
    fn debug_does_not_leak_values() {
        let label = format!("{:?}", CredentialSource::Environment);
        assert!(!label.contains("[redacted]"));
        assert!(label.contains("Environment"));
    }

    #[test]
    fn resolve_picks_env_var_first() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_ENV_FIRST";
        unsafe { std::env::set_var(key, "from-env") };
        let dir = tempfile::tempdir().unwrap();
        let (value, source) = resolve_credential(key, dir.path()).unwrap();
        assert_eq!(value, "from-env");
        assert_eq!(source, CredentialSource::Environment);
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn resolve_falls_through_to_global_store() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_GLOBAL";
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");

        let global_path = home.join(".thndrs").join("credentials.env");
        fs::create_dir_all(global_path.parent().unwrap()).unwrap();
        fs::write(&global_path, format!("{key}=from-global\n")).unwrap();

        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &home) };

        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-global");
        assert_eq!(source, CredentialSource::GlobalStore);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_falls_through_to_project_store() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_PROJECT";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        let project_path = workspace.join(".thndrs").join("credentials.env");
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(&project_path, format!("{key}=from-project\n")).unwrap();

        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-project");
        assert_eq!(source, CredentialSource::ProjectStore);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_falls_through_to_dotenv_legacy() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_DOTENV";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        fs::write(workspace.join(".env"), format!("{key}=from-dotenv\n")).unwrap();

        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-dotenv");
        assert_eq!(source, CredentialSource::DotEnvLegacy);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_returns_none_for_missing_key() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_NONEXISTENT";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        assert!(resolve_credential(key, &workspace).is_none());

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_precedence_env_overrides_all() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_PRECEDENCE";
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let old_home = std::env::var_os("HOME");
        let test_home = tmp.path().join("home");
        fs::create_dir_all(&test_home).unwrap();
        unsafe { std::env::set_var("HOME", &test_home) };

        let global_path = test_home.join(".thndrs").join("credentials.env");
        fs::create_dir_all(global_path.parent().unwrap()).unwrap();
        fs::write(&global_path, format!("{key}=from-global\n")).unwrap();

        let project_path = workspace.join(".thndrs").join("credentials.env");
        fs::create_dir_all(project_path.parent().unwrap()).unwrap();
        fs::write(&project_path, format!("{key}=from-project\n")).unwrap();

        fs::write(workspace.join(".env"), format!("{key}=from-dotenv\n")).unwrap();

        unsafe { std::env::set_var(key, "from-env") };
        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-env");
        assert_eq!(source, CredentialSource::Environment);
        unsafe { std::env::remove_var(key) };

        unsafe { std::env::set_var("HOME", &test_home) };
        let (value, source) = resolve_credential(key, &workspace).unwrap();
        assert_eq!(value, "from-global");
        assert_eq!(source, CredentialSource::GlobalStore);

        unsafe {
            if let Some(h) = old_home {
                std::env::set_var("HOME", h);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn resolve_empty_env_var_is_skipped() {
        let _guard = env_test_lock();
        let key = "RESOLVE_TEST_EMPTY_ENV";
        unsafe { std::env::set_var(key, "") };
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve_credential(key, dir.path()).is_none());
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn credential_source_returns_label_without_value() {
        let _guard = env_test_lock();
        let key = "RESOLVE_SRC_LABEL";
        unsafe { std::env::set_var(key, "some-value") };
        let dir = tempfile::tempdir().unwrap();
        let source = credential_source(key, dir.path());
        assert_eq!(source, Some(CredentialSource::Environment));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn credential_source_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(credential_source("THIS_KEY_DOES_NOT_EXIST", dir.path()), None);
    }

    #[test]
    fn remove_one_preserves_others() {
        let (_dir, path) = temp_cred_path();
        write_cred_file(
            &path,
            "UMANS_API_KEY=sk-umans\nOPENCODE_GO_KEY=sk-opencode\nOTHER_KEY=other-val\n",
        );
        remove_credential(&path, "UMANS_API_KEY").unwrap();
        let creds = read_credentials(&path).unwrap();
        assert_eq!(creds.len(), 2);
        assert!(creds.contains_key("OPENCODE_GO_KEY"));
        assert!(creds.contains_key("OTHER_KEY"));
    }

    #[test]
    #[cfg(unix)]
    fn file_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) = temp_cred_path();
        set_credential(&path, "UMANS_API_KEY", "sk-perm").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "file permissions should be 0600");
    }
}
