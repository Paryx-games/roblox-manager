//! Roblox authentication — CSRF token management & auth ticket generation.
//!
//! The [`RobloxClient`] wraps a `reqwest::Client` and transparently handles
//! CSRF token rotation: if a request returns `403` with a new token in the
//! `x-csrf-token` header, the client updates its state and retries.
//! Exponential backoff is applied for `429 Too Many Requests`.
//!
//! Roblox binds a CSRF token to the session that requested it, so tokens are
//! cached **per cookie**, not globally. One shared slot meant every account
//! switch sent the previous account's token and ate a guaranteed 403, and
//! concurrent tasks overwrote each other's token continuously.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE, REFERER};
use reqwest::{Client, Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::CoreError;

const USER_AGENT: &str = "RM-Rust/0.1";
const MAX_RETRIES: u32 = 4;
const BASE_BACKOFF_MS: u64 = 500;
/// How many times a single request will re-send after a CSRF token rotation.
/// Tracked separately from the rate-limit attempt count: a prior 429 must not
/// consume the retry that a freshly-rotated token has earned.
const MAX_CSRF_RETRIES: u32 = 2;

/// Cache key for a cookie's CSRF token. Hashed so the secret itself isn't
/// duplicated into a long-lived map key.
fn cookie_key(cookie: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    cookie.hash(&mut hasher);
    hasher.finish()
}

/// Body variants the shared retry loop understands.
///
/// `Copy`, holding only borrows, because the loop re-reads it on every attempt.
/// This is the whole reason asset uploads hand-roll their multipart body into a
/// `Vec<u8>` (see [`crate::multipart`]): `reqwest::multipart::Form` is `!Clone`
/// and is consumed by `send()`, so it could not survive a CSRF rotation or a
/// 429 backoff.
#[derive(Clone, Copy)]
enum ReqBody<'a> {
    None,
    Json(&'a serde_json::Value),
    /// Pre-encoded bytes plus the exact `Content-Type` to send with them.
    Raw {
        content_type: &'a str,
        bytes: &'a [u8],
    },
}

/// A stateful HTTP client that manages `.ROBLOSECURITY` cookies and CSRF tokens.
#[derive(Clone)]
pub struct RobloxClient {
    inner: Client,
    /// Current CSRF token per cookie (shared across clones via Arc<RwLock>).
    csrf_tokens: Arc<RwLock<HashMap<u64, String>>>,
}

impl RobloxClient {
    /// Create a new client. Does NOT set a cookie yet — call [`set_cookie`] before
    /// making authenticated requests.
    pub fn new() -> Result<Self, CoreError> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            inner: client,
            csrf_tokens: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    // ------------------------------------------------------------------
    // Core request helpers
    // ------------------------------------------------------------------

    /// Low-level request with automatic CSRF retry + exponential backoff.
    pub async fn request(
        &self,
        method: Method,
        url: &str,
        cookie: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<Response, CoreError> {
        let body = match body {
            Some(value) => ReqBody::Json(value),
            None => ReqBody::None,
        };
        self.request_inner(method, url, cookie, body).await
    }

    /// Send a pre-encoded body (e.g. `multipart/form-data` from
    /// [`crate::multipart`]) through the same CSRF-rotation and rate-limit
    /// machinery as [`RobloxClient::request`]. For multipart, `content_type`
    /// must carry the boundary.
    pub async fn request_raw(
        &self,
        method: Method,
        url: &str,
        cookie: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<Response, CoreError> {
        self.request_inner(
            method,
            url,
            cookie,
            ReqBody::Raw {
                content_type,
                bytes,
            },
        )
        .await
    }

    async fn request_inner(
        &self,
        method: Method,
        url: &str,
        cookie: &str,
        body: ReqBody<'_>,
    ) -> Result<Response, CoreError> {
        // Two independent counters. Sharing one made any request that had been
        // rate-limited skip its CSRF retry, returning an auth error while
        // holding the very token that would have worked.
        let mut attempt = 0u32;
        let mut csrf_attempt = 0u32;
        let key = cookie_key(cookie);

        loop {
            let mut headers = HeaderMap::new();
            // Attach cookie. An empty cookie means the caller wants an
            // anonymous request (public endpoints: thumbnails, game icons),
            // so send no header at all rather than a bare `.ROBLOSECURITY=`.
            if !cookie.is_empty() {
                let cookie_val = format!(".ROBLOSECURITY={cookie}");
                headers.insert(
                    COOKIE,
                    HeaderValue::from_str(&cookie_val)
                        .map_err(|e| CoreError::AuthFailed(e.to_string()))?,
                );
            }

            // Attach this cookie's CSRF token if we have one
            {
                let tokens = self.csrf_tokens.read().await;
                if let Some(t) = tokens.get(&key) {
                    headers.insert(
                        "x-csrf-token",
                        HeaderValue::from_str(t)
                            .map_err(|e| CoreError::AuthFailed(e.to_string()))?,
                    );
                }
            }

            // Always send Referer + an empty x-bound-auth-token so requests
            // line up with what the browser sends. The moderation endpoint
            // (and a few others) intermittently rejects requests missing
            // these even when the cookie itself is fine, which made
            // periodic revalidation overwrite specific ban reasons with the
            // generic fallback.
            headers.insert(REFERER, HeaderValue::from_static("https://www.roblox.com/"));
            headers.insert(
                "x-bound-auth-token",
                HeaderValue::from_static(""),
            );

            let mut req = self.inner.request(method.clone(), url).headers(headers);
            req = match body {
                ReqBody::Json(b) => req.json(b),
                // A fresh owned copy per attempt: `send()` consumes the body, so
                // a rotated CSRF token or a 429 backoff has to be able to hand
                // the same bytes over again.
                ReqBody::Raw {
                    content_type,
                    bytes,
                } => req.header(CONTENT_TYPE, content_type).body(bytes.to_vec()),
                // Roblox POST endpoints require application/json even with no body
                ReqBody::None if method == Method::POST => {
                    req.header(CONTENT_TYPE, "application/json")
                }
                ReqBody::None => req,
            };

            let resp = req.send().await?;

            match resp.status() {
                // Token rotation: update this cookie's token and retry
                StatusCode::FORBIDDEN => {
                    let rotated = resp
                        .headers()
                        .get("x-csrf-token")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());

                    let Some(new_token) = rotated else {
                        // No challenge header, so this was never about CSRF.
                        // The cookie is revoked or Roblox wants a challenge
                        // solved. Reporting it as a CSRF failure sent users
                        // chasing the wrong bug.
                        return Err(CoreError::CookieRejected);
                    };

                    {
                        let mut tokens = self.csrf_tokens.write().await;
                        tokens.insert(key, new_token);
                    }

                    if csrf_attempt < MAX_CSRF_RETRIES {
                        csrf_attempt += 1;
                        debug!("CSRF token rotated, retrying (attempt {csrf_attempt})");
                        continue;
                    }
                    return Err(CoreError::AuthFailed(format!(
                        "403 Forbidden after {csrf_attempt} CSRF retries"
                    )));
                }
                // Rate-limit: exponential backoff
                StatusCode::TOO_MANY_REQUESTS => {
                    if attempt >= MAX_RETRIES {
                        return Err(CoreError::RateLimited);
                    }
                    let wait = Duration::from_millis(BASE_BACKOFF_MS * 2u64.pow(attempt));
                    warn!("Rate limited, backing off {wait:?} (attempt {attempt})");
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                    continue;
                }
                _ => return Ok(resp),
            }
        }
    }

    /// Perform a GET and return raw response bytes.
    pub async fn get_bytes(
        &self,
        url: &str,
        cookie: &str,
    ) -> Result<Vec<u8>, CoreError> {
        let resp = self.request(Method::GET, url, cookie, None).await?;
        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(CoreError::RobloxApi {
                status: status.as_u16(),
                message: msg,
            });
        }
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Convenience: perform a GET and return the response body as a string.
    pub async fn get_text(
        &self,
        url: &str,
        cookie: &str,
    ) -> Result<String, CoreError> {
        let resp = self.request(Method::GET, url, cookie, None).await?;
        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(CoreError::RobloxApi {
                status: status.as_u16(),
                message: msg,
            });
        }
        let text = resp.text().await?;
        Ok(text)
    }

    /// Convenience: perform a GET and deserialize JSON.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        cookie: &str,
    ) -> Result<T, CoreError> {
        let resp = self.request(Method::GET, url, cookie, None).await?;
        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(CoreError::RobloxApi {
                status: status.as_u16(),
                message: msg,
            });
        }
        let data = resp.json::<T>().await?;
        Ok(data)
    }

    /// Convenience: perform a POST and deserialize JSON.
    pub async fn post_json<T: DeserializeOwned>(
        &self,
        url: &str,
        cookie: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, CoreError> {
        let resp = self.request(Method::POST, url, cookie, body).await?;
        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(CoreError::RobloxApi {
                status: status.as_u16(),
                message: msg,
            });
        }
        let data = resp.json::<T>().await?;
        Ok(data)
    }

    // ------------------------------------------------------------------
    // Auth-ticket generation
    // ------------------------------------------------------------------

    /// Request an authentication ticket from Roblox for game launch.
    /// Returns the ticket string on success.
    pub async fn generate_auth_ticket(&self, cookie: &str) -> Result<String, CoreError> {
        let resp = self
            .request(
                Method::POST,
                "https://auth.roblox.com/v1/authentication-ticket",
                cookie,
                None,
            )
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(CoreError::AuthFailed(format!(
                "ticket request failed ({status}): {msg}"
            )));
        }

        resp.headers()
            .get("rbx-authentication-ticket")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or(CoreError::AuthFailed(
                "no rbx-authentication-ticket header in response".into(),
            ))
    }

    // ------------------------------------------------------------------
    // Validation
    // ------------------------------------------------------------------

    /// Validate a cookie by fetching the authenticated user info.
    /// Returns `(user_id, username, display_name)` on success.
    pub async fn validate_cookie(
        &self,
        cookie: &str,
    ) -> Result<(u64, String, String), CoreError> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct AuthUser {
            id: u64,
            name: String,
            display_name: String,
        }
        let user: AuthUser = self
            .get_json("https://users.roblox.com/v1/users/authenticated", cookie)
            .await?;
        Ok((user.id, user.name, user.display_name))
    }
}

impl Default for RobloxClient {
    fn default() -> Self {
        Self::new().expect("failed to build reqwest client")
    }
}
