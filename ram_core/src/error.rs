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

    /// The store is wrapped by a device key that this machine's credential
    /// store does not have: the OS profile was reset, the credential was
    /// cleared, or the file was copied from another machine. Distinct from a
    /// decrypt failure because no password will ever open it and the UI needs
    /// to say so plainly.
    #[error(
        "This account store is bound to a different device, or its key was removed \
         from the credential store. It cannot be decrypted here."
    )]
    DeviceKeyMissing,

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Process error: {0}")]
    Process(String),

    #[error("Roblox API error ({status}): {message}")]
    RobloxApi { status: u16, message: String },
}
