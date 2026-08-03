use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization/deserialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("CSRF token missing from response headers")]
    CsrfTokenMissing,

    /// A 403 that carried no `x-csrf-token` header, so it was never a CSRF
    /// problem: the cookie is revoked, or Roblox wants a challenge solved.
    /// Kept distinct from [`CoreError::AuthFailed`] so callers can tell
    /// "this session is dead" apart from "the token round-trip failed".
    #[error("Cookie rejected by Roblox (403, no CSRF challenge)")]
    CookieRejected,

    #[error("Rate limited by Roblox. Retry after backoff.")]
    RateLimited,

    #[error("Encryption/Decryption error: {0}")]
    Crypto(String),

    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Process error: {0}")]
    Process(String),

    #[error("Roblox API error ({status}): {message}")]
    RobloxApi { status: u16, message: String },
}
