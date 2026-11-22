//! Provides a Spotify access token using the OAuth authorization code flow
//! with PKCE.
//!
//! Assuming sufficient scopes, the returned access token may be used with Spotify's
//! Web API, and/or to establish a new Session with [`librespot_core`].
//!
//! The authorization code flow is an interactive process which requires a web browser
//! to complete. The resulting code must then be provided back from the browser to this
//! library for exchange into an access token. Providing the code can be automatic via
//! a spawned http server (mimicking Spotify's client), or manually via stdin. The latter
//! is appropriate for headless systems.

use crate::credentials::Credentials;

use log::{error, info, trace, warn};
use oauth2::{
    basic::BasicClient, basic::BasicErrorResponseType, AuthUrl, AuthorizationCode, ClientId,
    CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use oauth2::{EndpointNotSet, EndpointSet, PkceCodeVerifier, RefreshToken, RequestTokenError};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::task::JoinHandle;
use url::Url;

use crate::session_tokens::SessionTokens;
use crate::token_store::TokenStore;

pub const CLIENT_ID: &str = "782ae96ea60f4cdf986a766049607005";
pub const ENV_CLIENT_ID: &str = "CANTO_SPOTIFY_CLIENT_ID";
pub const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
pub const SCOPES: &str = "user-read-private,\
playlist-read-private,\
playlist-read-collaborative,\
user-library-read,\
user-library-modify,\
user-follow-read,\
user-follow-modify,\
user-top-read,\
user-read-recently-played,\
user-read-playback-state,\
playlist-modify-public,\
playlist-modify-private,\
user-modify-playback-state,\
streaming,\
ugc-image-upload";

/// Resolves the Spotify Web API Client ID using precedence:
/// 1. `CANTO_SPOTIFY_CLIENT_ID` environment variable (when non-empty),
/// 2. Persisted user setting (when non-empty),
/// 3. Built-in [`CLIENT_ID`].
pub fn resolve_client_id(env_override: Option<&str>, configured: Option<&str>) -> String {
    for candidate in [env_override, configured] {
        if let Some(id) = candidate.map(str::trim).filter(|s| !s.is_empty()) {
            return id.to_string();
        }
    }
    CLIENT_ID.to_string()
}

/// Resolves the active Spotify Web API Client ID using [`ENV_CLIENT_ID`] from
/// the environment, falling back to `configured`, then [`CLIENT_ID`].
pub fn effective_client_id(configured: Option<&str>) -> String {
    let env_override = std::env::var(ENV_CLIENT_ID).ok();
    resolve_client_id(env_override.as_deref(), configured)
}

pub const SESSION_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
// The playlist scopes are for the internal API, which makes a playlist private
// or public.
pub const SESSION_SCOPES: &str = "streaming,\
playlist-read-private,\
playlist-modify-public,\
playlist-modify-private";

// Interval between background refresh retries. The refresh token stays valid
// far longer than the access token, so on a transient failure we keep polling
// rather than giving up.
const REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(10);

// How long before expiry we start trying to refresh. This must be larger than
// REFRESH_POLL_INTERVAL so that transient failures get retried while the
// current token is still valid, leaving no window where playback runs on an
// expired token.
const REFRESH_LEAD_TIME: Duration = Duration::from_secs(60);

// How long a connection to the login listener has to send its request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) type OauthClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build OAuth HTTP client")
    })
}

pub struct CantoOauthClient {
    client: RwLock<(String, OauthClient)>,
    token_store: TokenStore,
    session_tokens: SessionTokens,
}

pub struct AuthcodeChallenge {
    client: OauthClient,
    pkce_verifier: PkceCodeVerifier,
    session_pkce_verifier: PkceCodeVerifier,
    pub auth_url: Url,
    listener: JoinHandle<Result<AuthCodes, OAuthError>>,
}

impl AuthcodeChallenge {
    /// Cancels a pending login listener and waits for its TCP socket to close.
    pub async fn cancel(self) {
        self.listener.abort();
        let _ = self.listener.await;
    }
}

pub struct LoginCredentials {
    pub web_api: Credentials,
    pub session: Credentials,
}

struct AuthCodes {
    code: AuthorizationCode,
    session_code: AuthorizationCode,
}

fn authorization(client: &OauthClient, scopes: &str) -> (Url, CsrfToken, PkceCodeVerifier) {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Generate the full authorization URL.
    // Some of these scopes are unavailable for custom client IDs. Which?
    let request_scopes: Vec<oauth2::Scope> =
        scopes.split(",").map(|s| Scope::new(s.into())).collect();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(request_scopes)
        .set_pkce_challenge(pkce_challenge)
        .url();

    (auth_url, csrf_token, pkce_verifier)
}

impl CantoOauthClient {
    pub fn new(token_store: TokenStore, session_tokens: SessionTokens) -> Self {
        Self::with_client_id(token_store, session_tokens, None)
    }

    pub fn with_client_id(
        token_store: TokenStore,
        session_tokens: SessionTokens,
        configured_client_id: Option<&str>,
    ) -> Self {
        let client_id = effective_client_id(configured_client_id);
        let client = Self::build_client(&client_id);
        Self {
            client: RwLock::new((client_id, client)),
            token_store,
            session_tokens,
        }
    }

    /// Returns the currently active Spotify Web API Client ID.
    pub fn client_id(&self) -> String {
        self.client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .clone()
    }

    /// Re-resolves the active Spotify Web API Client ID (respecting
    /// `CANTO_SPOTIFY_CLIENT_ID` over `configured_client_id`). Returns `true`
    /// if the active Client ID changed.
    pub fn set_client_id(&self, configured_client_id: Option<&str>) -> bool {
        let next = effective_client_id(configured_client_id);
        let mut guard = self.client.write().unwrap_or_else(|e| e.into_inner());
        if guard.0 == next {
            return false;
        }
        info!("Switching Spotify OAuth Client ID");
        guard.1 = Self::build_client(&next);
        guard.0 = next;
        true
    }

    fn active_client(&self) -> (String, OauthClient) {
        let guard = self.client.read().unwrap_or_else(|e| e.into_inner());
        (guard.0.clone(), guard.1.clone())
    }

    fn oauth_client(&self) -> OauthClient {
        self.active_client().1
    }

    pub(crate) fn build_client(client_id: &str) -> OauthClient {
        let auth_url = AuthUrl::new("https://accounts.spotify.com/authorize".to_string())
            .expect("Malformed URL");
        let token_url = TokenUrl::new("https://accounts.spotify.com/api/token".to_string())
            .expect("Malformed URL");
        let redirect_url = RedirectUrl::new(REDIRECT_URI.to_string()).expect("Malformed URL");
        BasicClient::new(ClientId::new(client_id.to_string()))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(redirect_url)
    }

    pub async fn spawn_authcode_listener(
        &self,
        notify_complete: impl FnOnce() + Send + 'static,
    ) -> Result<AuthcodeChallenge, OAuthError> {
        let addr = get_socket_address(REDIRECT_URI).expect("Invalid redirect uri");
        self.spawn_authcode_listener_on(addr, notify_complete).await
    }

    async fn spawn_authcode_listener_on(
        &self,
        addr: SocketAddr,
        notify_complete: impl FnOnce() + Send + 'static,
    ) -> Result<AuthcodeChallenge, OAuthError> {
        let client = self.oauth_client();
        let (auth_url, state, pkce_verifier) = authorization(&client, SCOPES);
        let (session_auth_url, session_state, session_pkce_verifier) =
            authorization(self.session_tokens.client(), SESSION_SCOPES);

        // Bind before spawning so a port conflict fails immediately (before
        // opening the browser) and so `accept_authcodes` directly owns the
        // listener socket, letting `AuthcodeChallenge::cancel` release it
        // deterministically before returning.
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| OAuthError::AuthCodeListenerBind { addr, e })?;

        Ok(AuthcodeChallenge {
            client,
            pkce_verifier,
            session_pkce_verifier,
            auth_url,
            listener: tokio::task::spawn(async move {
                let result =
                    accept_authcodes(listener, state, session_state, session_auth_url).await;
                notify_complete();
                result
            }),
        })
    }

    /// Obtain the access tokens of a login, using the authorization
    /// code with PKCE OAuth flow.
    /// The redirect_uri must match what is registered to the client ID.
    pub async fn exchange_authcode(
        &self,
        challenge: AuthcodeChallenge,
    ) -> Result<LoginCredentials, OAuthError> {
        let codes = challenge
            .listener
            .await
            .map_err(|_| OAuthError::AuthCodeListenerTerminated)??;

        let web_api =
            Self::exchange(&challenge.client, codes.code, challenge.pkce_verifier).await?;
        let session = Self::exchange(
            self.session_tokens.client(),
            codes.session_code,
            challenge.session_pkce_verifier,
        )
        .await?;

        Ok(LoginCredentials { web_api, session })
    }

    async fn exchange(
        client: &OauthClient,
        code: AuthorizationCode,
        pkce_verifier: PkceCodeVerifier,
    ) -> Result<Credentials, OAuthError> {
        let token = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(http_client())
            .await
            .map_err(|e| match e {
                RequestTokenError::ServerResponse(res) => {
                    error!(
                        "An error occured while exchange a code: {}",
                        res.to_string()
                    );
                    OAuthError::ExchangeCode { e: res.to_string() }
                }
                e => OAuthError::ExchangeCode { e: e.to_string() },
            })?;

        trace!("Obtained new access token: {token:?}");

        let refresh_token = token
            .refresh_token()
            .ok_or(OAuthError::NoRefreshToken)?
            .secret()
            .to_string();

        let token = Credentials {
            access_token: token.access_token().secret().to_string(),
            refresh_token,
            token_expiry_time: Some(
                SystemTime::now()
                    + token
                        .expires_in()
                        .unwrap_or_else(|| Duration::from_secs(3600)),
            ),
            client_id: Some(client.client_id().as_str().to_string()),
        };

        Ok(token)
    }

    pub async fn clear_credentials(&self) {
        self.token_store.clear().await;
        self.session_tokens.clear().await;
    }

    pub async fn save_credentials(&self, creds: &Credentials) {
        self.token_store.set(creds.clone()).await;
    }

    pub async fn save_session_credentials(&self, creds: &Credentials) {
        self.session_tokens.save(creds).await;
    }

    pub async fn restore_session_credentials(&self) -> bool {
        self.session_tokens.restore().await
    }

    pub async fn get_valid_token(&self) -> Result<Credentials, OAuthError> {
        let (active_client_id, client) = self.active_client();
        let token = self.token_store.get().await.ok_or(OAuthError::LoggedOut)?;
        let minted_by = token.client_id.as_deref().unwrap_or(CLIENT_ID);
        if minted_by != active_client_id {
            info!(
                "Stored OAuth token was minted for a different Client ID; re-authentication required"
            );
            return Err(OAuthError::LoggedOut);
        }
        if token.token_expired() {
            Self::refresh(&client, &self.token_store, token).await
        } else {
            Ok(token)
        }
    }

    pub(crate) async fn valid_token(
        client: &OauthClient,
        token_store: &TokenStore,
    ) -> Result<Credentials, OAuthError> {
        let token = token_store.get().await.ok_or(OAuthError::LoggedOut)?;
        if token.token_expired() {
            Self::refresh(client, token_store, token).await
        } else {
            Ok(token)
        }
    }

    /// Dev tools only: backdate the cached token so it looks expired, then run
    /// the normal valid-token path, which will refresh it against Spotify.
    /// Exercises the full token-refresh flow on demand.
    #[cfg(debug_assertions)]
    pub async fn dev_expire_and_refresh(&self) -> Result<Credentials, OAuthError> {
        self.token_store.dev_expire_cached_token();
        self.get_valid_token().await
    }

    /// Perform a single refresh-token exchange.
    ///
    /// This does not retry. A rejected refresh token (`invalid_grant`) is
    /// fatal: the stored credentials are cleared and [`OAuthError::RefreshFailed`]
    /// is returned. Any other failure is treated as transient
    /// ([`OAuthError::RefreshTransient`]) and the credentials are left intact so
    /// a caller can poll and try again later.
    pub async fn refresh_token(&self, old_token: Credentials) -> Result<Credentials, OAuthError> {
        let client = self.oauth_client();
        Self::refresh(&client, &self.token_store, old_token).await
    }

    async fn refresh(
        client: &OauthClient,
        token_store: &TokenStore,
        old_token: Credentials,
    ) -> Result<Credentials, OAuthError> {
        let refresh_token = old_token.refresh_token.clone();

        match client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.clone()))
            .request_async(http_client())
            .await
        {
            Ok(token) => {
                // Spotify does not always return a new refresh token. When it
                // is omitted, reuse the old one so we stay logged in.
                let new_refresh_token = token
                    .refresh_token()
                    .map(|t| t.secret().to_string())
                    .unwrap_or_else(|| refresh_token.clone());

                let new_token = Credentials {
                    access_token: token.access_token().secret().to_string(),
                    refresh_token: new_refresh_token,
                    token_expiry_time: Some(
                        SystemTime::now()
                            + token
                                .expires_in()
                                .unwrap_or_else(|| Duration::from_secs(3600)),
                    ),
                    client_id: Some(client.client_id().as_str().to_string()),
                };

                token_store.set(new_token.clone()).await;
                Ok(new_token)
            }
            Err(e) => {
                // A rejected refresh token (`invalid_grant`) or mismatched/invalid
                // Client ID (`invalid_client`) will never succeed on retry, so
                // treat both as fatal and clear the stored credentials. Anything
                // else (network errors, 5xx, parse failures) is transient and
                // must not discard the still-valid refresh token.
                let fatal = matches!(
                    &e,
                    RequestTokenError::ServerResponse(res)
                        if is_fatal_refresh_error(res.error())
                );

                if fatal {
                    error!("Refresh token rejected by Spotify: {e}");
                    token_store.clear().await;
                    Err(OAuthError::RefreshFailed { e: e.to_string() })
                } else {
                    warn!("Transient error while refreshing token: {e}");
                    Err(OAuthError::RefreshTransient { e: e.to_string() })
                }
            }
        }
    }

    pub async fn refresh_token_at_expiry(&self) -> Result<Credentials, OAuthError> {
        let client = self.oauth_client();
        Self::refresh_at_expiry(&client, &self.token_store).await
    }

    pub(crate) async fn refresh_at_expiry(
        client: &OauthClient,
        token_store: &TokenStore,
    ) -> Result<Credentials, OAuthError> {
        let Some(old_token) = token_store.get_cached().await.take() else {
            return Err(OAuthError::NoRefreshToken);
        };

        let refresh_at = old_token
            .token_expiry_time
            .unwrap_or_else(|| SystemTime::now() + Duration::from_secs(120))
            .checked_sub(REFRESH_LEAD_TIME)
            .unwrap_or_else(SystemTime::now);

        info!(
            "Refreshing token in approx {}min",
            refresh_at
                .duration_since(SystemTime::now())
                .unwrap_or_default()
                .as_secs()
                .div_euclid(60)
        );
        wait_until(refresh_at).await;

        // Long-poll the refresh: retry transient failures at a generous
        // interval until we succeed. Only a fatal error (rejected refresh
        // token) stops the poll and propagates to the caller.
        loop {
            info!("Refreshing token...");
            match Self::refresh(client, token_store, old_token.clone()).await {
                Ok(new_token) => return Ok(new_token),
                Err(OAuthError::RefreshTransient { e }) => {
                    warn!(
                        "Token refresh failed transiently ({e}); retrying in {}s",
                        REFRESH_POLL_INTERVAL.as_secs()
                    );
                    tokio::time::sleep(REFRESH_POLL_INTERVAL).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

fn is_fatal_refresh_error(err: &BasicErrorResponseType) -> bool {
    matches!(
        err,
        BasicErrorResponseType::InvalidGrant | BasicErrorResponseType::InvalidClient
    )
}

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Auth code param not found in URI")]
    AuthCodeNotFound,

    #[error("CSRF token param not found in URI")]
    CsrfTokenNotFound,

    #[error("Failed to bind server to {addr} ({e})")]
    AuthCodeListenerBind { addr: SocketAddr, e: io::Error },

    #[error("Listener terminated without accepting a connection")]
    AuthCodeListenerTerminated,

    #[error("Failed to parse redirect URI from HTTP request")]
    AuthCodeListenerParse,

    #[error("Failed to write HTTP response")]
    AuthCodeListenerWrite,

    #[error("Failed to exchange code for access token ({e})")]
    ExchangeCode { e: String },

    #[error("Spotify did not provide a refresh token")]
    NoRefreshToken,

    #[error("Failed to refresh access token ({e})")]
    RefreshFailed { e: String },

    #[error("Transient error while refreshing access token ({e})")]
    RefreshTransient { e: String },

    #[error("No saved token")]
    LoggedOut,
}

async fn accept_authcodes(
    listener: tokio::net::TcpListener,
    state: CsrfToken,
    session_state: CsrfToken,
    session_auth_url: Url,
) -> Result<AuthCodes, OAuthError> {
    let (requests_tx, mut requests) = tokio::sync::mpsc::unbounded_channel();

    let mut first_code = None;
    loop {
        let (mut stream, request_line) = tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((mut stream, _)) => {
                        let requests_tx = requests_tx.clone();
                        tokio::task::spawn(async move {
                            let request = tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut stream));
                            if let Ok(Ok(request_line)) = request.await {
                                let _ = requests_tx.send((stream, request_line));
                            }
                        });
                    }
                    Err(_) => return Err(OAuthError::AuthCodeListenerTerminated),
                }
                continue;
            }
            req = requests.recv() => {
                match req {
                    Some(req) => req,
                    None => return Err(OAuthError::AuthCodeListenerTerminated),
                }
            }
        };

        let Ok((callback_state, code)) = parse_query(&request_line) else {
            turn_away(&mut stream).await;
            continue;
        };
        let awaited = if first_code.is_none() {
            &state
        } else {
            &session_state
        };
        if callback_state.secret() != awaited.secret() {
            turn_away(&mut stream).await;
            continue;
        }
        let Some(code) = code else {
            let _ = respond(&mut stream, "200 OK", HTML, &refused_page()).await;
            return Err(OAuthError::AuthCodeNotFound);
        };

        match first_code.take() {
            None => {
                first_code = Some(code);
                let location = format!("location: {session_auth_url}\r\n");
                respond(&mut stream, "302 Found", &location, "").await?;
            }
            Some(first_code) => {
                let _ = respond(&mut stream, "200 OK", HTML, &success_page()).await;
                return Ok(AuthCodes {
                    code: first_code,
                    session_code: code,
                });
            }
        }
    }
}

const HTML: &str = "content-type: text/html; charset=utf-8\r\n";
const REFUSED_MESSAGE: &str = "Login cancelled. You can close this tab.";

fn page(title: &str, message: &str) -> String {
    include_str!("./page.html")
        .replace("{{title}}", title)
        .replace("{{message}}", message)
}

fn success_page() -> String {
    page(
        "Canto Connected",
        "Logged in successfully! You may close this window.",
    )
}

fn refused_page() -> String {
    page("Login Cancelled", REFUSED_MESSAGE)
}

async fn turn_away(stream: &mut tokio::net::TcpStream) {
    let _ = respond(stream, "404 Not Found", "", "").await;
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Result<String, OAuthError> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|_| OAuthError::AuthCodeListenerParse)?;
    let mut header = String::new();
    loop {
        header.clear();
        match reader.read_line(&mut header).await {
            Ok(0) => break,
            Ok(_) if header.trim_end().is_empty() => break,
            Ok(_) => {}
            Err(_) => return Err(OAuthError::AuthCodeListenerParse),
        }
    }
    Ok(request_line)
}

async fn respond(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    headers: &str,
    body: &str,
) -> Result<(), OAuthError> {
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len(),
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| OAuthError::AuthCodeListenerWrite)
}

fn parse_query(request_line: &str) -> Result<(CsrfToken, Option<AuthorizationCode>), OAuthError> {
    let query = request_line
        .split_whitespace()
        .nth(1)
        .ok_or(OAuthError::AuthCodeListenerParse)?
        .split("?")
        .nth(1)
        .ok_or(OAuthError::AuthCodeListenerParse)?;

    let mut query_params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let csrf_token = query_params
        .remove("state")
        .map(CsrfToken::new)
        .ok_or(OAuthError::CsrfTokenNotFound)?;
    let code = query_params.remove("code").map(AuthorizationCode::new);

    Ok((csrf_token, code))
}

// If the specified `redirect_uri` is HTTP, loopback, and contains a port,
// then the corresponding socket address is returned.
fn get_socket_address(redirect_uri: &str) -> Option<SocketAddr> {
    let url = match Url::parse(redirect_uri) {
        Ok(u) if u.scheme() == "http" && u.port().is_some() => u,
        _ => return None,
    };
    let socket_addr = match url.socket_addrs(|| None) {
        Ok(mut addrs) => addrs.pop(),
        _ => None,
    };
    if let Some(s) = socket_addr {
        if s.ip().is_loopback() {
            return socket_addr;
        }
    }
    None
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    use super::*;

    #[test]
    fn get_socket_address_none() {
        // No port
        assert_eq!(get_socket_address("http://127.0.0.1/foo"), None);
        assert_eq!(get_socket_address("http://127.0.0.1:/foo"), None);
        assert_eq!(get_socket_address("http://[::1]/foo"), None);
        // Not localhost
        assert_eq!(get_socket_address("http://56.0.0.1:1234/foo"), None);
        assert_eq!(
            get_socket_address("http://[3ffe:2a00:100:7031::1]:1234/foo"),
            None
        );
        // Not http
        assert_eq!(get_socket_address("https://127.0.0.1/foo"), None);
    }

    #[test]
    fn get_socket_address_localhost() {
        let localhost_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234);
        let localhost_v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 8888);

        assert_eq!(
            get_socket_address("http://127.0.0.1:1234/foo"),
            Some(localhost_v4)
        );
        assert_eq!(
            get_socket_address("http://[0:0:0:0:0:0:0:1]:8888/foo"),
            Some(localhost_v6)
        );
        assert_eq!(
            get_socket_address("http://[::1]:8888/foo"),
            Some(localhost_v6)
        );
    }

    async fn get(addr: SocketAddr, target: &str) -> String {
        use tokio::io::AsyncReadExt;

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!("GET {target} HTTP/1.1\r\nhost: localhost\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    async fn listen() -> (SocketAddr, JoinHandle<Result<AuthCodes, OAuthError>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let codes = tokio::task::spawn(accept_authcodes(
            listener,
            CsrfToken::new("first".to_string()),
            CsrfToken::new("second".to_string()),
            Url::parse("https://accounts.example/authorize?client_id=session").unwrap(),
        ));
        (addr, codes)
    }

    #[tokio::test]
    async fn login_goes_through_both_authorizations() {
        let (addr, codes) = listen().await;

        assert!(get(addr, "/favicon.ico").await.starts_with("HTTP/1.1 404"));
        let stale = get(addr, "/login?code=x&state=stale").await;
        assert!(stale.starts_with("HTTP/1.1 404"));
        let refused = get(addr, "/login?error=access_denied&state=stale").await;
        assert!(refused.starts_with("HTTP/1.1 404"));
        let early = get(addr, "/login?code=x&state=second").await;
        assert!(early.starts_with("HTTP/1.1 404"));

        let _idle = tokio::net::TcpStream::connect(addr).await.unwrap();

        let first = get(addr, "/login?code=one&state=first").await;
        assert!(first.starts_with("HTTP/1.1 302"));
        assert!(
            first.contains("location: https://accounts.example/authorize?client_id=session\r\n")
        );

        let second = get(addr, "/login?code=two&state=second").await;
        assert!(second.starts_with("HTTP/1.1 200"));

        let codes = codes.await.unwrap().unwrap();
        assert_eq!(codes.code.secret(), "one");
        assert_eq!(codes.session_code.secret(), "two");
    }

    #[tokio::test]
    async fn login_ends_when_refused() {
        let (addr, codes) = listen().await;
        let refused = get(addr, "/login?error=access_denied&state=first").await;
        assert!(refused.starts_with("HTTP/1.1 200"));
        assert!(refused.contains("content-type: text/html"));
        assert!(refused.contains(REFUSED_MESSAGE));
        assert!(matches!(
            codes.await.unwrap(),
            Err(OAuthError::AuthCodeNotFound)
        ));
    }

    #[test]
    fn scopes_have_no_duplicates() {
        for (name, raw) in [("SCOPES", SCOPES), ("SESSION_SCOPES", SESSION_SCOPES)] {
            let mut seen = std::collections::HashSet::new();
            for scope in raw.split(',') {
                assert!(!scope.is_empty(), "{name} contains an empty scope");
                assert!(
                    seen.insert(scope),
                    "{name} contains duplicate scope {scope:?}"
                );
            }
        }
        assert!(SCOPES.split(',').any(|s| s == "playlist-modify-public"));
    }

    #[test]
    fn resolve_client_id_follows_env_then_setting_then_default() {
        assert_eq!(
            resolve_client_id(Some("  env-id  "), Some("saved-id")),
            "env-id"
        );
        assert_eq!(
            resolve_client_id(Some("   "), Some("  saved-id  ")),
            "saved-id"
        );
        assert_eq!(resolve_client_id(None, Some("saved-id")), "saved-id");
        assert_eq!(resolve_client_id(None, Some("")), CLIENT_ID);
        assert_eq!(resolve_client_id(Some(""), Some("   ")), CLIENT_ID);
        assert_eq!(resolve_client_id(None, None), CLIENT_ID);
    }

    #[test]
    fn set_client_id_updates_authorization_url() {
        if std::env::var(ENV_CLIENT_ID)
            .ok()
            .is_some_and(|v| !v.trim().is_empty())
        {
            return;
        }
        let client = CantoOauthClient::with_client_id(
            TokenStore::new(),
            SessionTokens::new(),
            Some("custom-id-1"),
        );
        assert_eq!(client.client_id(), "custom-id-1");
        assert!(!client.set_client_id(Some("  custom-id-1  ")));
        assert!(client.set_client_id(Some("custom-id-2")));
        assert_eq!(client.client_id(), "custom-id-2");

        let (url, _, _) = authorization(&client.oauth_client(), SCOPES);
        let client_id_param = url
            .query_pairs()
            .find(|(k, _)| k == "client_id")
            .map(|(_, v)| v.into_owned());
        assert_eq!(client_id_param.as_deref(), Some("custom-id-2"));

        assert!(client.set_client_id(None));
        assert_eq!(client.client_id(), CLIENT_ID);
    }

    #[tokio::test]
    async fn cancel_releases_socket_before_returning() {
        // Reserve a free loopback port, then immediately release it to test
        // deterministic cancel-and-rebind on the exact same SocketAddr.
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = probe.local_addr().unwrap();
        drop(probe);

        let client = CantoOauthClient::with_client_id(
            TokenStore::new(),
            SessionTokens::new(),
            Some("custom-id-1"),
        );

        for i in 0..20 {
            let challenge = client
                .spawn_authcode_listener_on(addr, || {})
                .await
                .expect("listener should bind cleanly after previous cancel");
            // While active, a second bind to the same port fails immediately.
            let conflict = client.spawn_authcode_listener_on(addr, || {}).await;
            assert!(
                matches!(conflict, Err(OAuthError::AuthCodeListenerBind { .. })),
                "expected bind failure while active (iter {i})"
            );
            challenge.cancel().await;
            let _ = client.set_client_id(Some(&format!("custom-id-{}", i + 2)));
        }
    }

    #[tokio::test]
    async fn get_valid_token_requires_relogin_when_client_id_changes() {
        if std::env::var(ENV_CLIENT_ID)
            .ok()
            .is_some_and(|v| !v.trim().is_empty())
        {
            return;
        }
        let future = Some(SystemTime::now() + Duration::from_secs(3600));

        // Legacy token (no stored client_id) matches the built-in CLIENT_ID
        // so existing logins are preserved, but triggers re-login under a custom ID.
        let store = TokenStore::new();
        store.set_cached_for_tests(Credentials {
            access_token: "tok-default".to_string(),
            refresh_token: "ref-default".to_string(),
            token_expiry_time: future,
            client_id: None,
        });
        let client = CantoOauthClient::with_client_id(store.clone(), SessionTokens::new(), None);
        assert_eq!(
            client.get_valid_token().await.unwrap().access_token,
            "tok-default"
        );
        assert!(client.set_client_id(Some("custom-app")));
        assert!(matches!(
            client.get_valid_token().await,
            Err(OAuthError::LoggedOut)
        ));

        // Token minted for "custom-app" works with "custom-app", not with another ID.
        store.set_cached_for_tests(Credentials {
            access_token: "tok-custom".to_string(),
            refresh_token: "ref-custom".to_string(),
            token_expiry_time: future,
            client_id: Some("custom-app".to_string()),
        });
        assert_eq!(
            client.get_valid_token().await.unwrap().access_token,
            "tok-custom"
        );
        assert!(client.set_client_id(Some("other-app")));
        assert!(matches!(
            client.get_valid_token().await,
            Err(OAuthError::LoggedOut)
        ));
        assert!(client.set_client_id(None));
        assert!(matches!(
            client.get_valid_token().await,
            Err(OAuthError::LoggedOut)
        ));
    }

    #[test]
    fn invalid_client_is_fatal_refresh_error() {
        assert!(is_fatal_refresh_error(
            &BasicErrorResponseType::InvalidGrant
        ));
        assert!(is_fatal_refresh_error(
            &BasicErrorResponseType::InvalidClient
        ));
        assert!(!is_fatal_refresh_error(
            &BasicErrorResponseType::InvalidRequest
        ));
    }

    #[test]
    fn credentials_deserialize_without_client_id() {
        let json = r#"{"access_token":"a","refresh_token":"r","token_expiry_time":null}"#;
        let creds: Credentials = serde_json::from_str(json).unwrap();
        assert_eq!(creds.access_token, "a");
        assert_eq!(creds.refresh_token, "r");
        assert_eq!(creds.client_id, None);
    }
}

/// Longest single sleep while waiting for a wall-clock time.
const WALL_CLOCK_CHECK: Duration = Duration::from_secs(30);

/// Waits until the wall clock reaches `when`. Tokio's timers stop while the
/// computer is suspended, so one long sleep would wake far past the token's
/// expiry after a suspend (every request then failing with 401); short sleeps
/// re-read the wall clock and catch up within `WALL_CLOCK_CHECK` of resuming.
async fn wait_until(when: SystemTime) {
    while let Ok(left) = when.duration_since(SystemTime::now()) {
        if left.is_zero() {
            break;
        }
        tokio::time::sleep(left.min(WALL_CLOCK_CHECK)).await;
    }
}
