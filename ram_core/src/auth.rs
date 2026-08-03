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
const MAX_RETRIES: u32 = 6;
const BASE_BACKOFF_MS: u64 = 1_000;
/// Ceiling on one 429 backoff. Without it the doubling reaches half a minute by
/// attempt 5 and over a minute by attempt 6, which stalls a bulk upload far
/// longer than Roblox's limiter actually holds.
const MAX_BACKOFF_MS: u64 = 20_000;
/// Longest `Retry-After` this client will sit out. Roblox occasionally answers
/// with a value in the hundreds of seconds; obeying that verbatim looks like a
/// hang, so past this the request gives up and the caller retries later.
const MAX_RETRY_AFTER_SECS: u64 = 60;

/// Timeout for an ordinary request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for a request that carries a pre-encoded body, which in this app
/// means an asset upload.
///
/// `reqwest`'s timeout spans the whole round trip, body transfer included, so
/// the 30s that is generous for a JSON GET is a hard ceiling on upload size
/// times uplink speed. A few concurrent multi-megabyte audio files on a normal
/// home connection cross it, and the resulting timeout is indistinguishable
/// from a rejection.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);
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

/// Exponential backoff for a 429, capped, with jitter.
///
/// The jitter is not decoration. Concurrent uploads share one limiter, so
/// without it every task that gets rate limited in the same instant also wakes
/// in the same instant and collides again, burning the whole retry budget on a
/// thundering herd of its own making.
fn backoff_for_attempt(attempt: u32) -> Duration {
    let base = BASE_BACKOFF_MS
        .saturating_mul(2u64.saturating_pow(attempt))
        .min(MAX_BACKOFF_MS);
    let spread = base / 4;
    let jitter = if spread == 0 {
        0
    } else {
        use rand::Rng as _;
        rand::thread_rng().gen_range(0..=spread)
    };
    Duration::from_millis(base.saturating_sub(spread / 2).saturating_add(jitter))
}

/// Roblox's own `Retry-After`, when it sends one. Seconds only: the HTTP-date
/// form is legal but Roblox does not use it, and guessing at clock skew to
/// parse it would be worse than falling back to the local backoff.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let secs: u64 = headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS)))
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
            .timeout(REQUEST_TIMEOUT)
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
                } => req
                    .header(CONTENT_TYPE, content_type)
                    .timeout(UPLOAD_TIMEOUT)
                    .body(bytes.to_vec()),
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
                // Rate-limit: exponential backoff, or whatever Roblox asked for.
                StatusCode::TOO_MANY_REQUESTS => {
                    if attempt >= MAX_RETRIES {
                        return Err(CoreError::RateLimited);
                    }
                    let wait = retry_after(resp.headers())
                        .unwrap_or_else(|| backoff_for_attempt(attempt));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_is_capped() {
        // Jitter is +/-12.5%, so assert on bands rather than exact values.
        let first = backoff_for_attempt(0);
        assert!(first >= Duration::from_millis(875) && first <= Duration::from_millis(1_125));

        let fourth = backoff_for_attempt(3);
        assert!(fourth >= Duration::from_millis(7_000) && fourth <= Duration::from_millis(9_000));

        // Past the cap, and at an attempt count that would overflow a naive
        // `2u64.pow(attempt)`.
        for attempt in [6u32, 40, u32::MAX] {
            let wait = backoff_for_attempt(attempt);
            assert!(
                wait <= Duration::from_millis(MAX_BACKOFF_MS + MAX_BACKOFF_MS / 8),
                "attempt {attempt} gave {wait:?}"
            );
        }
    }

    #[test]
    fn backoff_does_not_return_a_single_fixed_value() {
        // The whole point of the jitter: concurrent uploads that are rate
        // limited together must not all wake together.
        let waits: std::collections::HashSet<Duration> =
            (0..40).map(|_| backoff_for_attempt(3)).collect();
        assert!(waits.len() > 1, "backoff produced no spread");
    }

    fn headers_with(name: &str, value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    #[test]
    fn retry_after_is_honoured_in_seconds() {
        assert_eq!(
            retry_after(&headers_with("retry-after", "7")),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            retry_after(&headers_with("retry-after", " 12 ")),
            Some(Duration::from_secs(12))
        );
    }

    #[test]
    fn an_absurd_retry_after_is_clamped_not_obeyed() {
        // Roblox sometimes answers with hundreds of seconds. Sitting that out
        // is indistinguishable from a hang.
        assert_eq!(
            retry_after(&headers_with("retry-after", "3600")),
            Some(Duration::from_secs(MAX_RETRY_AFTER_SECS))
        );
    }

    #[test]
    fn an_unparseable_retry_after_falls_back_to_local_backoff() {
        // The HTTP-date form is legal but Roblox does not send it, and guessing
        // at clock skew would be worse than the local backoff.
        assert_eq!(retry_after(&HeaderMap::new()), None);
        assert_eq!(
            retry_after(&headers_with("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after(&headers_with("retry-after", "")), None);
    }

    #[test]
    fn an_upload_gets_far_longer_than_an_ordinary_request() {
        // A multi-megabyte body on a normal home uplink crosses 30s, and the
        // resulting timeout is indistinguishable from a rejection.
        assert!(UPLOAD_TIMEOUT >= REQUEST_TIMEOUT * 4);
    }
}
