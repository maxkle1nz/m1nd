//! Loopback HTTP authentication and browser-origin boundary.
//!
//! This is a local transport guard, not sovereign identity. The bearer/session
//! token is readable by the same OS user and therefore does not satisfy the
//! same-UID authority threat model on its own.

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const HTTP_AUTH_TOKEN_FILE_NAME: &str = "http-auth-token-v1";
const SESSION_COOKIE_NAME: &str = "m1nd_local_session";
const TOKEN_BYTES: usize = 32;
const DEFAULT_REQUESTS_PER_WINDOW: u64 = 4_096;
const DEFAULT_WINDOW_MS: u64 = 60_000;

/// Read and validate an already-created owner bearer token without ever
/// provisioning one. Used by the local `--attach` bridge so authentication and
/// the owner's token-file safety checks cannot drift apart.
pub fn read_existing_bearer_token(path: &Path) -> Result<String, HttpSecurityError> {
    refuse_symlink(path)?;
    read_token(path)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpSecurityRejectCode {
    InvalidHost,
    InvalidOrigin,
    CrossSiteRequest,
    AuthenticationRequired,
    AuthenticationInvalid,
    UnsafeOriginRequired,
    RateLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpSecurityDecision {
    Allow {
        issue_session_cookie: bool,
    },
    Reject {
        status: StatusCode,
        code: HttpSecurityRejectCode,
    },
}

#[derive(Debug)]
struct RateWindow {
    started_at_ms: u64,
    requests: u64,
}

pub struct LocalHttpSecurity {
    port: u16,
    token: String,
    token_path: PathBuf,
    browser_bootstrap_nonce: Mutex<Option<String>>,
    rate_limit: u64,
    window_ms: u64,
    rate_window: Mutex<RateWindow>,
}

impl fmt::Debug for LocalHttpSecurity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalHttpSecurity")
            .field("port", &self.port)
            .field("token_path", &self.token_path)
            .field("rate_limit", &self.rate_limit)
            .field("window_ms", &self.window_ms)
            .finish_non_exhaustive()
    }
}

impl LocalHttpSecurity {
    pub fn load_or_create(root: &Path, port: u16) -> Result<Self, HttpSecurityError> {
        fs::create_dir_all(root).map_err(|source| HttpSecurityError::Io {
            operation: "create_http_security_root",
            source,
        })?;
        let canonical_root = fs::canonicalize(root).map_err(|source| HttpSecurityError::Io {
            operation: "canonicalize_http_security_root",
            source,
        })?;
        let token_path = canonical_root.join(HTTP_AUTH_TOKEN_FILE_NAME);
        refuse_symlink(&token_path)?;
        let token = if token_path.exists() {
            read_token(&token_path)?
        } else {
            create_token(&token_path)?
        };
        Ok(Self {
            port,
            token,
            token_path,
            browser_bootstrap_nonce: Mutex::new(Some(random_token()?)),
            rate_limit: DEFAULT_REQUESTS_PER_WINDOW,
            window_ms: DEFAULT_WINDOW_MS,
            rate_window: Mutex::new(RateWindow {
                started_at_ms: 0,
                requests: 0,
            }),
        })
    }

    #[cfg(test)]
    fn deterministic(port: u16, token: &str) -> Self {
        Self {
            port,
            token: token.to_string(),
            token_path: PathBuf::from("test-only"),
            browser_bootstrap_nonce: Mutex::new(Some("f".repeat(TOKEN_BYTES * 2))),
            rate_limit: DEFAULT_REQUESTS_PER_WINDOW,
            window_ms: DEFAULT_WINDOW_MS,
            rate_window: Mutex::new(RateWindow {
                started_at_ms: 0,
                requests: 0,
            }),
        }
    }

    pub fn token_path(&self) -> &Path {
        &self.token_path
    }

    /// URL used only by the owner-launched browser.  The one-shot nonce is
    /// carried in the initial navigation, exchanged for the HttpOnly session
    /// cookie by the middleware, and never persisted as the bearer token.
    pub fn browser_bootstrap_url(&self) -> String {
        let nonce = self
            .browser_bootstrap_nonce
            .lock()
            .clone()
            .unwrap_or_default();
        format!("http://localhost:{}/?m1nd-bootstrap={nonce}", self.port)
    }

    pub fn evaluate(
        &self,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
    ) -> HttpSecurityDecision {
        let Some(host) = single_header(headers, header::HOST) else {
            return reject(StatusCode::FORBIDDEN, HttpSecurityRejectCode::InvalidHost);
        };
        let normalized_host = host.to_ascii_lowercase();
        if !self
            .allowed_hosts()
            .iter()
            .any(|allowed| allowed == &normalized_host)
        {
            return reject(StatusCode::FORBIDDEN, HttpSecurityRejectCode::InvalidHost);
        }

        let expected_origin = format!("http://{normalized_host}");
        let origin = single_header(headers, header::ORIGIN);
        if origin.is_some_and(|value| !value.eq_ignore_ascii_case(&expected_origin)) {
            return reject(StatusCode::FORBIDDEN, HttpSecurityRejectCode::InvalidOrigin);
        }
        let fetch_site =
            single_header_name(headers, "sec-fetch-site").map(|value| value.to_ascii_lowercase());
        if fetch_site.as_deref() == Some("cross-site") || fetch_site.as_deref() == Some("same-site")
        {
            return reject(
                StatusCode::FORBIDDEN,
                HttpSecurityRejectCode::CrossSiteRequest,
            );
        }

        let bearer = bearer_token(headers);
        let cookie = cookie_token(headers);
        let bearer_valid = bearer.as_deref().is_some_and(|value| self.matches(value));
        let cookie_valid = cookie.as_deref().is_some_and(|value| self.matches(value));
        let credential_present = bearer.is_some() || cookie.is_some();
        let safe = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);

        if !bearer_valid && !cookie_valid {
            if !credential_present && self.is_safe_document_bootstrap(method, uri, headers, origin)
            {
                let issue_session_cookie = self.consume_browser_bootstrap_nonce(uri);
                return HttpSecurityDecision::Allow {
                    issue_session_cookie,
                };
            }
            return reject(
                StatusCode::UNAUTHORIZED,
                if credential_present {
                    HttpSecurityRejectCode::AuthenticationInvalid
                } else {
                    HttpSecurityRejectCode::AuthenticationRequired
                },
            );
        }

        if !safe && cookie_valid && !bearer_valid && origin.is_none() {
            return reject(
                StatusCode::FORBIDDEN,
                HttpSecurityRejectCode::UnsafeOriginRequired,
            );
        }
        if !safe && origin.is_some() && fetch_site.as_deref() != Some("same-origin") {
            return reject(
                StatusCode::FORBIDDEN,
                HttpSecurityRejectCode::CrossSiteRequest,
            );
        }
        HttpSecurityDecision::Allow {
            issue_session_cookie: false,
        }
    }

    fn allowed_hosts(&self) -> [String; 3] {
        [
            format!("localhost:{}", self.port),
            format!("127.0.0.1:{}", self.port),
            format!("[::1]:{}", self.port),
        ]
    }

    fn matches(&self, candidate: &str) -> bool {
        constant_time_digest_equal(candidate.as_bytes(), self.token.as_bytes())
    }

    fn is_safe_document_bootstrap(
        &self,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        origin: Option<&str>,
    ) -> bool {
        if !matches!(*method, Method::GET | Method::HEAD)
            || uri.path().starts_with("/api/")
            || uri.path() == "/mcp"
        {
            return false;
        }
        if origin.is_some() {
            return false;
        }
        let fetch_site =
            single_header_name(headers, "sec-fetch-site").map(|value| value.to_ascii_lowercase());
        if !matches!(
            fetch_site.as_deref(),
            None | Some("none") | Some("same-origin")
        ) {
            return false;
        }
        let fetch_dest =
            single_header_name(headers, "sec-fetch-dest").map(|value| value.to_ascii_lowercase());
        let accepts_html = single_header(headers, header::ACCEPT)
            .is_some_and(|value| value.to_ascii_lowercase().contains("text/html"));
        matches!(fetch_dest.as_deref(), Some("document")) || accepts_html
    }

    fn consume_browser_bootstrap_nonce(&self, uri: &Uri) -> bool {
        let Some(candidate) = uri
            .query()
            .and_then(|query| query.strip_prefix("m1nd-bootstrap="))
            .filter(|candidate| !candidate.is_empty() && !candidate.contains('&'))
        else {
            return false;
        };
        let mut nonce = self.browser_bootstrap_nonce.lock();
        let matches = nonce.as_deref().is_some_and(|expected| {
            constant_time_digest_equal(candidate.as_bytes(), expected.as_bytes())
        });
        if matches {
            *nonce = None;
        }
        matches
    }

    fn rate_allows(&self, now_ms: u64) -> bool {
        let mut window = self.rate_window.lock();
        if window.started_at_ms == 0
            || now_ms < window.started_at_ms
            || now_ms.saturating_sub(window.started_at_ms) >= self.window_ms
        {
            window.started_at_ms = now_ms;
            window.requests = 0;
        }
        if window.requests >= self.rate_limit {
            return false;
        }
        window.requests = window.requests.saturating_add(1);
        true
    }

    fn session_cookie(&self) -> String {
        format!(
            "{SESSION_COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Strict",
            self.token
        )
    }
}

pub fn secure_local_router(router: Router, security: Arc<LocalHttpSecurity>) -> Router {
    router
        .route(
            "/api/security/scope",
            get(|| async {
                Json(serde_json::json!({
                    "scope": "local_http_transport",
                    "sovereign_identity": false,
                    "same_uid_isolation": false,
                }))
            }),
        )
        .layer(middleware::from_fn_with_state(
            security,
            enforce_local_http_security,
        ))
}

async fn enforce_local_http_security(
    State(security): State<Arc<LocalHttpSecurity>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let now_ms = unix_time_ms();
    if !security.rate_allows(now_ms) {
        return rejection_response(
            HttpSecurityRejectCode::RateLimited,
            StatusCode::TOO_MANY_REQUESTS,
        );
    }
    let decision = security.evaluate(request.method(), request.uri(), request.headers());
    match decision {
        HttpSecurityDecision::Reject { status, code } => rejection_response(code, status),
        HttpSecurityDecision::Allow {
            issue_session_cookie,
        } => {
            let mut response = next.run(request).await;
            apply_security_headers(response.headers_mut());
            if issue_session_cookie {
                if let Ok(value) = HeaderValue::from_str(&security.session_cookie()) {
                    response.headers_mut().append(header::SET_COOKIE, value);
                }
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            }
            response
        }
    }
}

fn rejection_response(code: HttpSecurityRejectCode, status: StatusCode) -> Response {
    let mut response = (
        status,
        Json(serde_json::json!({
            "error": "local_http_boundary_rejected",
            "code": code,
        })),
    )
        .into_response();
    apply_security_headers(response.headers_mut());
    response
}

fn apply_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'",
        ),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

fn reject(status: StatusCode, code: HttpSecurityRejectCode) -> HttpSecurityDecision {
    HttpSecurityDecision::Reject { status, code }
}

fn single_header(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    if headers.get_all(name.clone()).iter().count() != 1 {
        return None;
    }
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn single_header_name<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    if headers.get_all(name).iter().count() != 1 {
        return None;
    }
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = single_header(headers, header::AUTHORIZATION)?;
    let mut parts = value.split_ascii_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;
    if !scheme.eq_ignore_ascii_case("bearer") || parts.next().is_some() {
        return Some(String::new());
    }
    Some(token.to_string())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let value = single_header(headers, header::COOKIE)?;
    let mut matches = value.split(';').filter_map(|field| {
        let (name, value) = field.trim().split_once('=')?;
        (name == SESSION_COOKIE_NAME).then(|| value.to_string())
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        return Some(String::new());
    }
    Some(first)
}

fn constant_time_digest_equal(left: &[u8], right: &[u8]) -> bool {
    let left = Sha256::digest(left);
    let right = Sha256::digest(right);
    left.iter()
        .zip(right.iter())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn create_token(path: &Path) -> Result<String, HttpSecurityError> {
    refuse_symlink(path)?;
    let token = random_token()?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| HttpSecurityError::Io {
        operation: "create_http_auth_token",
        source,
    })?;
    file.write_all(token.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|source| HttpSecurityError::Io {
            operation: "persist_http_auth_token",
            source,
        })?;
    sync_parent(path)?;
    Ok(token)
}

fn random_token() -> Result<String, HttpSecurityError> {
    let mut random = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut random).map_err(|error| HttpSecurityError::Random {
        detail: error.to_string(),
    })?;
    Ok(hex(&random))
}

fn read_token(path: &Path) -> Result<String, HttpSecurityError> {
    refuse_symlink(path)?;
    let metadata = fs::metadata(path).map_err(|source| HttpSecurityError::Io {
        operation: "metadata_http_auth_token",
        source,
    })?;
    if !metadata.is_file() {
        return Err(HttpSecurityError::InvalidTokenFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(HttpSecurityError::InsecurePermissions);
        }
    }
    let mut file =
        OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|source| HttpSecurityError::Io {
                operation: "open_http_auth_token",
                source,
            })?;
    let mut token = String::new();
    file.read_to_string(&mut token)
        .map_err(|source| HttpSecurityError::Io {
            operation: "read_http_auth_token",
            source,
        })?;
    let token = token.trim().to_string();
    if token.len() != TOKEN_BYTES * 2 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(HttpSecurityError::InvalidTokenFile);
    }
    Ok(token)
}

fn refuse_symlink(path: &Path) -> Result<(), HttpSecurityError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(HttpSecurityError::SymlinkRefused),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(HttpSecurityError::Io {
            operation: "inspect_http_auth_token",
            source,
        }),
    }
}

fn sync_parent(path: &Path) -> Result<(), HttpSecurityError> {
    let parent = path.parent().ok_or(HttpSecurityError::InvalidTokenFile)?;
    // Windows refuses fsync on directory handles; write-through covers renames.
    #[cfg(windows)]
    {
        let _ = parent;
        Ok(())
    }
    #[cfg(not(windows))]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| HttpSecurityError::Io {
            operation: "sync_http_auth_token_parent",
            source,
        })
}

fn unix_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(ALPHABET[(byte >> 4) as usize] as char);
        output.push(ALPHABET[(byte & 0x0f) as usize] as char);
    }
    output
}

#[derive(Debug)]
pub enum HttpSecurityError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Random {
        detail: String,
    },
    InvalidTokenFile,
    InsecurePermissions,
    SymlinkRefused,
}

impl fmt::Display for HttpSecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Random { detail } => {
                write!(formatter, "secure random generation failed: {detail}")
            }
            Self::InvalidTokenFile => formatter.write_str("invalid HTTP authentication token file"),
            Self::InsecurePermissions => {
                formatter.write_str("HTTP authentication token file is group/world accessible")
            }
            Self::SymlinkRefused => {
                formatter.write_str("HTTP authentication token symlink refused")
            }
        }
    }
}

impl Error for HttpSecurityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, Request as HttpRequest};
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn headers(host: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        headers
    }

    #[test]
    fn safe_document_navigation_needs_the_one_shot_nonce_to_issue_a_cookie() {
        let security = LocalHttpSecurity::deterministic(1338, TOKEN);
        let mut document = headers("localhost:1338");
        document.insert(header::ACCEPT, HeaderValue::from_static("text/html"));
        assert_eq!(
            security.evaluate(&Method::GET, &Uri::from_static("/"), &document),
            HttpSecurityDecision::Allow {
                issue_session_cookie: false
            }
        );
        let bootstrap_uri: Uri = security.browser_bootstrap_url().parse::<Uri>().unwrap();
        assert_eq!(
            security.evaluate(&Method::GET, &bootstrap_uri, &document),
            HttpSecurityDecision::Allow {
                issue_session_cookie: true
            }
        );
        assert_eq!(
            security.evaluate(&Method::GET, &bootstrap_uri, &document),
            HttpSecurityDecision::Allow {
                issue_session_cookie: false
            }
        );
        assert_eq!(
            security.evaluate(&Method::GET, &Uri::from_static("/api/manifest"), &document),
            reject(
                StatusCode::UNAUTHORIZED,
                HttpSecurityRejectCode::AuthenticationRequired
            )
        );
    }

    #[test]
    fn cookie_auth_requires_same_origin_for_writes_and_refuses_cross_site_reads() {
        let security = LocalHttpSecurity::deterministic(1338, TOKEN);
        let mut valid = headers("127.0.0.1:1338");
        valid.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE_NAME}={TOKEN}")).unwrap(),
        );
        valid.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:1338"),
        );
        valid.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert_eq!(
            security.evaluate(
                &Method::POST,
                &Uri::from_static("/api/tools/ingest"),
                &valid
            ),
            HttpSecurityDecision::Allow {
                issue_session_cookie: false
            }
        );

        let mut cross_site = valid.clone();
        cross_site.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.test"),
        );
        cross_site.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert_eq!(
            security.evaluate(
                &Method::GET,
                &Uri::from_static("/api/manifest"),
                &cross_site
            ),
            reject(StatusCode::FORBIDDEN, HttpSecurityRejectCode::InvalidOrigin)
        );
    }

    #[test]
    fn bearer_allows_non_browser_clients_but_host_and_origin_stay_pinned() {
        let security = LocalHttpSecurity::deterministic(1338, TOKEN);
        let mut valid = headers("[::1]:1338");
        valid.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {TOKEN}")).unwrap(),
        );
        assert_eq!(
            security.evaluate(&Method::POST, &Uri::from_static("/mcp"), &valid),
            HttpSecurityDecision::Allow {
                issue_session_cookie: false
            }
        );
        valid.insert(header::HOST, HeaderValue::from_static("m1nd.lan:1338"));
        assert_eq!(
            security.evaluate(&Method::POST, &Uri::from_static("/mcp"), &valid),
            reject(StatusCode::FORBIDDEN, HttpSecurityRejectCode::InvalidHost)
        );
    }

    #[test]
    fn duplicate_or_stale_credentials_fail_closed() {
        let security = LocalHttpSecurity::deterministic(1338, TOKEN);
        let mut stale = headers("localhost:1338");
        stale.insert(
            header::COOKIE,
            HeaderValue::from_static("m1nd_local_session=stale"),
        );
        assert_eq!(
            security.evaluate(&Method::GET, &Uri::from_static("/api/health"), &stale),
            reject(
                StatusCode::UNAUTHORIZED,
                HttpSecurityRejectCode::AuthenticationInvalid
            )
        );
        stale.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE_NAME}={TOKEN}")).unwrap(),
        );
        assert_eq!(
            security.evaluate(&Method::GET, &Uri::from_static("/api/health"), &stale),
            reject(
                StatusCode::UNAUTHORIZED,
                HttpSecurityRejectCode::AuthenticationRequired
            )
        );
    }

    #[test]
    fn token_is_persistent_private_and_symlink_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let first = LocalHttpSecurity::load_or_create(directory.path(), 1338).unwrap();
        let token_path = first.token_path().to_path_buf();
        let second = LocalHttpSecurity::load_or_create(directory.path(), 1338).unwrap();
        assert!(first.matches(&second.token));
        assert_eq!(fs::read_to_string(&token_path).unwrap().trim().len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            assert_eq!(
                fs::metadata(&token_path).unwrap().permissions().mode() & 0o077,
                0
            );
            let other = tempfile::tempdir().unwrap();
            let target = other.path().join(HTTP_AUTH_TOKEN_FILE_NAME);
            symlink(&token_path, &target).unwrap();
            assert!(matches!(
                LocalHttpSecurity::load_or_create(other.path(), 1338),
                Err(HttpSecurityError::SymlinkRefused)
            ));
        }
    }

    #[tokio::test]
    async fn router_layer_authenticates_every_api_surface_and_bootstraps_once() {
        let security = Arc::new(LocalHttpSecurity::deterministic(1338, TOKEN));
        let bootstrap_url = security.browser_bootstrap_url();
        let router =
            secure_local_router(Router::new().route("/", get(|| async { "m1nd" })), security);

        let unauthenticated = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/security/scope")
                    .header(header::HOST, "localhost:1338")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let anonymous_document = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/")
                    .header(header::HOST, "localhost:1338")
                    .header(header::ACCEPT, "text/html")
                    .header("sec-fetch-dest", "document")
                    .header("sec-fetch-site", "none")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous_document.status(), StatusCode::OK);
        assert!(anonymous_document
            .headers()
            .get(header::SET_COOKIE)
            .is_none());

        let bootstrap_path = bootstrap_url.strip_prefix("http://localhost:1338").unwrap();
        let bootstrap = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri(bootstrap_path)
                    .header(header::HOST, "localhost:1338")
                    .header(header::ACCEPT, "text/html")
                    .header("sec-fetch-dest", "document")
                    .header("sec-fetch-site", "none")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bootstrap.status(), StatusCode::OK);
        let cookie = bootstrap
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));

        let replay = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri(bootstrap_path)
                    .header(header::HOST, "localhost:1338")
                    .header(header::ACCEPT, "text/html")
                    .header("sec-fetch-dest", "document")
                    .header("sec-fetch-site", "none")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(replay.headers().get(header::SET_COOKIE).is_none());

        let authenticated = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/security/scope")
                    .header(header::HOST, "localhost:1338")
                    .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated.status(), StatusCode::OK);
        assert_eq!(
            authenticated
                .headers()
                .get(header::X_FRAME_OPTIONS)
                .unwrap(),
            "DENY"
        );
    }
}
