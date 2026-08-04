//! Secure storage for `.ROBLOSECURITY` cookies.
//!
//! # Envelope encryption
//!
//! The account store is always encrypted with a random 256-bit **data key**.
//! That data key never leaves memory in the clear; it is wrapped (encrypted)
//! into the store file's header by a **wrapping key** that comes from one of
//! two places:
//!
//! * [`StoreMode::Device`] (the default) — a random 256-bit key held in the OS
//!   credential store (Windows Credential Manager, libsecret, Keychain). No
//!   user interaction, nothing to forget. The file on disk is still ciphertext,
//!   so anything that scrapes files for plaintext cookies comes up empty.
//! * [`StoreMode::Password`] — Argon2id over a user-supplied master password
//!   with a random per-file salt. Opt-in, for people who want the store to
//!   survive their OS account being compromised.
//!
//! Wrapping the data key rather than deriving it directly is what makes
//! "change password" and "switch modes" cheap and total: both rewrite 32 bytes
//! of header and leave every encrypted cookie in the store untouched. The
//! previous scheme re-encrypted each cookie individually with a
//! password-derived key, so a single decrypt failure mid-loop silently stranded
//! that account on the old password forever.
//!
//! # On-disk format
//!
//! ```text
//! off  len  field
//!   0    8  magic  b"RAMSTORE"
//!   8    1  version = 2
//!   9    1  mode    0 = device, 1 = password
//!  10   16  salt    (password mode; zeroed in device mode)
//!  26   12  wrap nonce
//!  38   48  wrapped data key (32-byte key + 16-byte GCM tag)
//!  86   12  body nonce
//!  98   ..  AES-256-GCM(data key, AccountStore JSON)
//! ```
//!
//! The wrapped key authenticates bytes `0..26` as AAD and the body
//! authenticates the whole `0..86` header, so flipping the mode byte or
//! swapping in another file's salt fails to decrypt rather than silently
//! downgrading anything.
//!
//! Files that do not start with the magic are **v1**: a bare
//! `nonce(12) || ciphertext` blob keyed by 100k unsalted SHA-256 iterations
//! over the master password. They still load ([`unlock_with_password`]), and
//! [`upgrade_v1`] rewrites them into the format above with a fresh random data
//! key.

use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::error::CoreError;
use crate::models::AccountStore;

// ---------------------------------------------------------------------------
// Format constants
// ---------------------------------------------------------------------------

const MAGIC: &[u8; 8] = b"RAMSTORE";
const VERSION: u8 = 2;

const MODE_DEVICE: u8 = 0;
const MODE_PASSWORD: u8 = 1;

const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const WRAPPED_KEY_LEN: usize = KEY_LEN + TAG_LEN;

const OFF_VERSION: usize = 8;
const OFF_MODE: usize = 9;
const OFF_SALT: usize = 10;
const OFF_WRAP_NONCE: usize = OFF_SALT + SALT_LEN;
const OFF_WRAPPED_KEY: usize = OFF_WRAP_NONCE + NONCE_LEN;
/// Bytes `0..HEADER_LEN` are the header; the body nonce follows immediately.
const HEADER_LEN: usize = OFF_WRAPPED_KEY + WRAPPED_KEY_LEN;
/// The prefix bound as AAD when wrapping the data key: magic, version, mode, salt.
const WRAP_AAD_LEN: usize = OFF_WRAP_NONCE;

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// A 256-bit symmetric key held in memory. Zeroed on drop, and deliberately
/// neither `Debug` nor `Display` so it cannot be logged by accident.
#[derive(Clone)]
pub struct StoreKey([u8; KEY_LEN]);

impl StoreKey {
    fn random() -> Self {
        let mut k = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut k);
        Self(k)
    }

    fn cipher(&self) -> Result<Aes256Gcm, CoreError> {
        Aes256Gcm::new_from_slice(&self.0).map_err(|e| CoreError::Crypto(e.to_string()))
    }
}

impl Drop for StoreKey {
    fn drop(&mut self) {
        // Overwrite through a volatile write so the compiler cannot elide it as
        // a dead store to a value that is about to go out of scope.
        for b in self.0.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

/// How the store's data key is wrapped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreMode {
    /// Wrapped by a device key in the OS credential store. No prompt.
    Device,
    /// Wrapped by Argon2id over a master password.
    Password,
}

/// An unlocked store: the data key plus the header that wraps it.
///
/// Held by the UI for the lifetime of the session and passed to every
/// operation that touches a cookie. Saving reuses `header` verbatim, so the
/// wrapped key is rewritten only when the user actually changes how the store
/// is locked (see [`rewrap`]).
#[derive(Clone)]
pub struct StoreSession {
    key: StoreKey,
    header: Vec<u8>,
}

/// Prints the mode and nothing else. Hand-written rather than derived so the
/// data key can never reach a log line or a panic message.
impl std::fmt::Debug for StoreSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreSession")
            .field("mode", &self.mode())
            .field("key", &"<redacted>")
            .finish()
    }
}

impl StoreSession {
    pub fn mode(&self) -> StoreMode {
        // A legacy session has no header and is always password-locked; any
        // other header was built by `build_header`, which writes a valid mode.
        if self.is_legacy() || self.header[OFF_MODE] == MODE_PASSWORD {
            StoreMode::Password
        } else {
            StoreMode::Device
        }
    }

    /// True when unlocking this store requires the user to type something.
    pub fn needs_password(&self) -> bool {
        self.mode() == StoreMode::Password
    }

    /// True for a session opened from a pre-envelope file. Such a session can
    /// read the store but cannot be saved until [`upgrade_v1`] rotates it.
    pub fn is_legacy(&self) -> bool {
        self.header.len() != HEADER_LEN
    }
}

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------

/// Argon2id over the master password. Runs once per unlock (not once per
/// cookie, as the v1 scheme did), so the cost parameters can be honest.
fn derive_wrapping_key(password: &str, salt: &[u8]) -> Result<StoreKey, CoreError> {
    use argon2::{Algorithm, Argon2, Params, Version};

    // OWASP's second-choice profile: 19 MiB, t=3, p=1. Comfortably sub-second
    // on the low-end Windows laptops this app targets.
    let params = Params::new(19 * 1024, 3, 1, Some(KEY_LEN))
        .map_err(|e| CoreError::Crypto(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| CoreError::Crypto(format!("argon2: {e}")))?;
    Ok(StoreKey(out))
}

/// The v1 KDF: 100k unsalted SHA-256 iterations. Kept solely to read stores
/// written before the envelope format, and to decrypt the cookies inside them.
/// Never used for new data.
fn derive_key_v1(password: &str) -> StoreKey {
    let mut hash = Sha256::digest(password.as_bytes());
    for _ in 0..100_000 {
        hash = Sha256::digest(hash);
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&hash);
    StoreKey(key)
}

// ---------------------------------------------------------------------------
// Device key (OS credential store)
// ---------------------------------------------------------------------------

const SERVICE_NAME: &str = "RM-Rust";
/// Credential entry holding the device wrapping key. Cannot collide with the
/// per-account entries, which are named by numeric Roblox user ID.
const DEVICE_KEY_ENTRY: &str = "device-key";

fn device_key_entry() -> Result<keyring::Entry, CoreError> {
    keyring::Entry::new(SERVICE_NAME, DEVICE_KEY_ENTRY)
        .map_err(|e| CoreError::Keyring(e.to_string()))
}

/// Read the device wrapping key, or `Ok(None)` if this device has never had
/// one.
///
/// Deliberately does *not* create a key on miss: a store in device mode whose
/// key has vanished (OS profile reset, credential store cleared, file copied
/// from another machine) must report [`CoreError::DeviceKeyMissing`] rather
/// than mint a fresh key and then fail with a generic "wrong password".
pub fn device_key() -> Result<Option<StoreKey>, CoreError> {
    let entry = device_key_entry()?;
    match entry.get_password() {
        Ok(encoded) => {
            let raw = B64
                .decode(encoded.trim())
                .map_err(|e| CoreError::Crypto(format!("device key is not valid base64: {e}")))?;
            if raw.len() != KEY_LEN {
                return Err(CoreError::Crypto(format!(
                    "device key is {} bytes, expected {KEY_LEN}",
                    raw.len()
                )));
            }
            let mut k = [0u8; KEY_LEN];
            k.copy_from_slice(&raw);
            Ok(Some(StoreKey(k)))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(CoreError::Keyring(e.to_string())),
    }
}

/// Read the device wrapping key, generating and persisting one if this device
/// does not have it yet. Used when *creating* or re-keying a store, never when
/// opening an existing one.
///
/// The generated key is deliberately **read back** rather than returned
/// directly. Two processes reaching a fresh credential store at the same time
/// both generate a key and both write; only the last write survives, so a
/// caller that trusted its own copy would wrap its store in a key the
/// credential store no longer holds and no later run could open it. Reading
/// back makes both callers converge on whichever key actually landed.
///
/// `keyring` offers no atomic create-if-absent, so a narrow window remains: a
/// second writer can still land between our write and our read-back. That
/// leaves the store openable (the read-back key is the live one) and only
/// costs the losing writer its own generated key, which nothing has used yet.
pub fn device_key_or_create() -> Result<StoreKey, CoreError> {
    if let Some(k) = device_key()? {
        return Ok(k);
    }
    let key = StoreKey::random();
    device_key_entry()?
        .set_password(&B64.encode(key.0))
        .map_err(|e| CoreError::Keyring(e.to_string()))?;

    match device_key()? {
        Some(k) => {
            tracing::info!("Generated a new device key in the OS credential store");
            Ok(k)
        }
        // Written, then absent. Something else is clearing the entry, and
        // wrapping a store under a key that is already gone would produce a
        // file nothing can open. Fail loudly instead.
        None => Err(CoreError::Keyring(
            "the device key vanished immediately after being written".into(),
        )),
    }
}

/// Remove the device key from the credential store. Only meaningful when the
/// store it protected is being destroyed too.
pub fn delete_device_key() -> Result<(), CoreError> {
    match device_key_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CoreError::Keyring(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Header construction / parsing
// ---------------------------------------------------------------------------

/// Wrap `data_key` under `wrapping_key` and lay out the full header.
fn build_header(
    mode: u8,
    salt: &[u8; SALT_LEN],
    wrapping_key: &StoreKey,
    data_key: &StoreKey,
) -> Result<Vec<u8>, CoreError> {
    let mut header = vec![0u8; HEADER_LEN];
    header[..MAGIC.len()].copy_from_slice(MAGIC);
    header[OFF_VERSION] = VERSION;
    header[OFF_MODE] = mode;
    header[OFF_SALT..OFF_SALT + SALT_LEN].copy_from_slice(salt);

    let mut wrap_nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut wrap_nonce);
    header[OFF_WRAP_NONCE..OFF_WRAP_NONCE + NONCE_LEN].copy_from_slice(&wrap_nonce);

    // AAD is everything decided before the wrap: magic, version, mode, salt.
    let aad = header[..WRAP_AAD_LEN].to_vec();
    let wrapped = wrapping_key.cipher()?.encrypt(
        Nonce::from_slice(&wrap_nonce),
        Payload {
            msg: &data_key.0,
            aad: &aad,
        },
    )
    .map_err(|e| CoreError::Crypto(format!("wrapping the data key failed: {e}")))?;

    if wrapped.len() != WRAPPED_KEY_LEN {
        return Err(CoreError::Crypto("unexpected wrapped key length".into()));
    }
    header[OFF_WRAPPED_KEY..].copy_from_slice(&wrapped);
    Ok(header)
}

/// Recover the data key from a header using `wrapping_key`.
fn unwrap_data_key(header: &[u8], wrapping_key: &StoreKey) -> Result<StoreKey, CoreError> {
    let nonce = &header[OFF_WRAP_NONCE..OFF_WRAP_NONCE + NONCE_LEN];
    let wrapped = &header[OFF_WRAPPED_KEY..HEADER_LEN];
    let aad = &header[..WRAP_AAD_LEN];

    let plain = wrapping_key
        .cipher()?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: wrapped,
                aad,
            },
        )
        .map_err(|_| CoreError::Crypto("could not unwrap the store's data key".into()))?;

    if plain.len() != KEY_LEN {
        return Err(CoreError::Crypto("unwrapped data key has the wrong length".into()));
    }
    let mut k = [0u8; KEY_LEN];
    k.copy_from_slice(&plain);
    Ok(StoreKey(k))
}

/// True when `data` carries a v2 envelope header. A v1 file starts with a
/// random 12-byte nonce, so a false positive costs 2^-64.
fn is_v2(data: &[u8]) -> bool {
    data.len() >= HEADER_LEN + NONCE_LEN && &data[..MAGIC.len()] == MAGIC
}

/// How a store file on disk is locked, without unlocking it.
///
/// Returns `Ok(None)` when no store exists yet. Used at startup to decide
/// between opening silently and showing the unlock prompt.
pub fn peek_mode(path: &Path) -> Result<Option<StoreMode>, CoreError> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(Some(mode_of(&data)))
}

fn mode_of(data: &[u8]) -> StoreMode {
    if !is_v2(data) {
        // v1 files are always password-derived.
        return StoreMode::Password;
    }
    if data[OFF_MODE] == MODE_DEVICE {
        StoreMode::Device
    } else {
        StoreMode::Password
    }
}

/// True when the file at `path` predates the envelope format and should be
/// upgraded once unlocked.
pub fn is_legacy(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(data) => !is_v2(&data),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Body encryption
// ---------------------------------------------------------------------------

fn encrypt_body(header: &[u8], store: &AccountStore, key: &StoreKey) -> Result<Vec<u8>, CoreError> {
    let plaintext = serde_json::to_vec(store)?;

    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);

    let ciphertext = key
        .cipher()?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: header,
            },
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut out = Vec::with_capacity(header.len() + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(header);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_body(data: &[u8], key: &StoreKey) -> Result<AccountStore, CoreError> {
    let header = &data[..HEADER_LEN];
    let nonce = &data[HEADER_LEN..HEADER_LEN + NONCE_LEN];
    let ciphertext = &data[HEADER_LEN + NONCE_LEN..];

    let plaintext = key
        .cipher()?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: header,
            },
        )
        .map_err(|_| {
            // The data key already authenticated against the header, so reaching
            // here means the body itself is damaged — not a wrong password.
            CoreError::Crypto("the account store is damaged and could not be read".into())
        })?;

    Ok(serde_json::from_slice(&plaintext)?)
}

// ---------------------------------------------------------------------------
// Creating, unlocking, saving
// ---------------------------------------------------------------------------

/// Start a brand-new device-mode store session. Generates the data key and,
/// if needed, the device key.
pub fn create_device_session() -> Result<StoreSession, CoreError> {
    let wrapping = device_key_or_create()?;
    let data_key = StoreKey::random();
    let header = build_header(MODE_DEVICE, &[0u8; SALT_LEN], &wrapping, &data_key)?;
    Ok(StoreSession {
        key: data_key,
        header,
    })
}

/// Start a brand-new password-mode store session.
pub fn create_password_session(password: &str) -> Result<StoreSession, CoreError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let wrapping = derive_wrapping_key(password, &salt)?;
    let data_key = StoreKey::random();
    let header = build_header(MODE_PASSWORD, &salt, &wrapping, &data_key)?;
    Ok(StoreSession {
        key: data_key,
        header,
    })
}

/// Re-lock an already-unlocked session under a different mode, keeping the same
/// data key so no cookie needs re-encrypting.
///
/// The caller must [`save_store`] afterwards for the change to reach disk.
pub fn rewrap(session: &StoreSession, password: Option<&str>) -> Result<StoreSession, CoreError> {
    let header = match password {
        Some(pw) => {
            let mut salt = [0u8; SALT_LEN];
            OsRng.fill_bytes(&mut salt);
            let wrapping = derive_wrapping_key(pw, &salt)?;
            build_header(MODE_PASSWORD, &salt, &wrapping, &session.key)?
        }
        None => {
            let wrapping = device_key_or_create()?;
            build_header(MODE_DEVICE, &[0u8; SALT_LEN], &wrapping, &session.key)?
        }
    };
    Ok(StoreSession {
        key: session.key.clone(),
        header,
    })
}

/// Open a device-mode store with no user interaction.
pub fn unlock_with_device(path: &Path) -> Result<(AccountStore, StoreSession), CoreError> {
    let data = std::fs::read(path)?;
    let wrapping = device_key()?.ok_or(CoreError::DeviceKeyMissing)?;
    with_backup_recovery(path, &data, |bytes| open_device(bytes, &wrapping))
}

/// Open a password-mode store, transparently handling both the envelope format
/// and pre-envelope v1 files.
pub fn unlock_with_password(
    path: &Path,
    password: &str,
) -> Result<(AccountStore, StoreSession), CoreError> {
    let data = std::fs::read(path)?;
    with_backup_recovery(path, &data, |bytes| open_password(bytes, password))
}

/// Try `open` on the primary bytes, then on the sibling backup, and self-heal
/// the primary when the backup is the copy that works.
///
/// Dispatch happens per-candidate rather than once up front: a primary damaged
/// badly enough to lose its magic bytes must not drag the backup down the
/// legacy code path with it. A wrong password fails on both copies, so falling
/// through to the backup only ever helps when the primary is truly damaged.
fn with_backup_recovery<F>(
    path: &Path,
    primary: &[u8],
    open: F,
) -> Result<(AccountStore, StoreSession), CoreError>
where
    F: Fn(&[u8]) -> Result<(AccountStore, StoreSession), CoreError>,
{
    let primary_err = match open(primary) {
        Ok(ok) => return Ok(ok),
        Err(e) => e,
    };

    let backup = crate::storage::backup_path(path);
    if let Ok(backup_bytes) = std::fs::read(&backup) {
        if let Ok(ok) = open(&backup_bytes) {
            tracing::warn!(
                "Primary account store at {} failed to open; recovered from backup {}",
                path.display(),
                backup.display()
            );
            let _ = crate::storage::atomic_swap(path, &backup_bytes);
            return Ok(ok);
        }
    }
    Err(primary_err)
}

/// Open one candidate file in device mode.
fn open_device(
    data: &[u8],
    wrapping: &StoreKey,
) -> Result<(AccountStore, StoreSession), CoreError> {
    if is_v2(data) && data[OFF_MODE] == MODE_PASSWORD {
        return Err(CoreError::Crypto(
            "this account store is locked with a master password".into(),
        ));
    }
    if !is_v2(data) {
        return Err(CoreError::Crypto(
            "this account store predates automatic unlocking and needs its master password".into(),
        ));
    }
    try_open_v2(data, wrapping)
}

/// Open one candidate file with a password, dispatching on that candidate's own
/// format so v1 and v2 copies can coexist during an interrupted upgrade.
fn open_password(
    data: &[u8],
    password: &str,
) -> Result<(AccountStore, StoreSession), CoreError> {
    if !is_v2(data) {
        // Pre-envelope file: the data key *is* the v1 password-derived key,
        // which is what keeps the `encrypted_cookie` blobs inside readable.
        let key = derive_key_v1(password);
        let store = decrypt_v1(data, &key)?;
        return Ok((store, v1_session(key)));
    }
    if data[OFF_MODE] == MODE_DEVICE {
        return Err(CoreError::Crypto(
            "this account store unlocks automatically and takes no password".into(),
        ));
    }
    let salt: [u8; SALT_LEN] = data[OFF_SALT..OFF_SALT + SALT_LEN]
        .try_into()
        .map_err(|_| CoreError::Crypto("malformed store header".into()))?;
    let wrapping = derive_wrapping_key(password, &salt)?;
    try_open_v2(data, &wrapping)
}

fn try_open_v2(data: &[u8], wrapping: &StoreKey) -> Result<(AccountStore, StoreSession), CoreError> {
    if !is_v2(data) {
        return Err(CoreError::Crypto("not an envelope-format store".into()));
    }
    if data[OFF_VERSION] != VERSION {
        return Err(CoreError::Crypto(format!(
            "account store was written by a newer version of RM (format {}), \
             update RM to open it",
            data[OFF_VERSION]
        )));
    }
    let header = &data[..HEADER_LEN];
    let data_key = unwrap_data_key(header, wrapping)?;
    let store = decrypt_body(data, &data_key)?;
    Ok((
        store,
        StoreSession {
            key: data_key,
            header: header.to_vec(),
        },
    ))
}

/// A session standing in for a just-opened v1 file.
///
/// It carries the legacy password-derived key — enough to read the cookies
/// already in the store — but no header, so it is deliberately unsaveable:
/// [`save_store`] rejects it. The caller must run [`upgrade_v1`] first, which
/// rotates onto a fresh random data key and produces a real session. That
/// ordering is what stops a half-migrated store from ever reaching disk.
fn v1_session(key: StoreKey) -> StoreSession {
    StoreSession {
        key,
        header: Vec::new(),
    }
}

fn decrypt_v1(data: &[u8], key: &StoreKey) -> Result<AccountStore, CoreError> {
    if data.len() < NONCE_LEN + 1 {
        return Err(CoreError::Crypto("encrypted data too short".into()));
    }
    let (nonce, ciphertext) = data.split_at(NONCE_LEN);
    let plaintext = key
        .cipher()?
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| {
            CoreError::Crypto(
                "could not decrypt the account store: wrong master password, or the file is corrupted"
                    .into(),
            )
        })?;
    Ok(serde_json::from_slice(&plaintext)?)
}

/// Rotate a just-opened v1 store onto a fresh random data key, re-encrypting
/// every stored cookie under it.
///
/// Returns the rewritten store and its new session. Unlike the old
/// `ChangePassword` path this is **all or nothing**: if any cookie fails to
/// re-encrypt the whole upgrade is abandoned and the caller keeps using the
/// legacy session, rather than half-migrating and stranding accounts.
pub fn upgrade_v1(
    store: &AccountStore,
    legacy: &StoreSession,
    password: Option<&str>,
) -> Result<(AccountStore, StoreSession), CoreError> {
    let session = match password {
        Some(pw) => create_password_session(pw)?,
        None => create_device_session()?,
    };

    let mut upgraded = store.clone();
    for account in &mut upgraded.accounts {
        let Some(ref enc) = account.encrypted_cookie else {
            continue;
        };
        let plain = decrypt_cookie(enc, legacy).map_err(|e| {
            CoreError::Crypto(format!(
                "could not re-encrypt the cookie for user {}: {e}",
                account.user_id
            ))
        })?;
        account.encrypted_cookie = Some(encrypt_cookie(&plain, &session)?);
    }
    Ok((upgraded, session))
}

/// Encrypt and atomically write the store, preserving the previous copy as
/// `<path>.bak`.
pub fn save_store(
    path: &Path,
    store: &AccountStore,
    session: &StoreSession,
) -> Result<(), CoreError> {
    if session.header.len() != HEADER_LEN {
        return Err(CoreError::Crypto(
            "cannot save: this session has no valid store header".into(),
        ));
    }
    let bytes = encrypt_body(&session.header, store, &session.key)?;
    crate::storage::atomic_write(path, &bytes)?;
    Ok(())
}

/// Save after the store's key material changed (password change, mode switch,
/// v1 upgrade), leaving no copy readable under the retired key.
///
/// [`save_store`] alone is not enough: [`crate::storage::atomic_write`]
/// preserves the outgoing primary as `<path>.bak`, so a plain save after a
/// password change leaves a backup that the *old* password still opens — and
/// because [`with_backup_recovery`] self-heals from the backup, entering the
/// old password would silently roll the store back. The primary is written
/// first and the backup replaced second, so at no point is there zero readable
/// copies on disk.
pub fn save_rekeyed(
    path: &Path,
    store: &AccountStore,
    session: &StoreSession,
) -> Result<(), CoreError> {
    save_store(path, store, session)?;

    let backup = crate::storage::backup_path(path);
    let bytes = encrypt_body(&session.header, store, &session.key)?;
    crate::storage::atomic_swap(&backup, &bytes)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Windows Credential Manager backend (per-account cookies)
// ---------------------------------------------------------------------------

/// Store a single cookie in the OS credential store.
pub fn credential_store(user_id: u64, cookie: &str) -> Result<(), CoreError> {
    let entry = keyring::Entry::new(SERVICE_NAME, &user_id.to_string())
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    entry
        .set_password(cookie)
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    Ok(())
}

/// Retrieve a cookie from the OS credential store.
pub fn credential_load(user_id: u64) -> Result<String, CoreError> {
    let entry = keyring::Entry::new(SERVICE_NAME, &user_id.to_string())
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    entry
        .get_password()
        .map_err(|e| CoreError::Keyring(e.to_string()))
}

/// Delete a cookie from the OS credential store. Absent entries are not an
/// error: this runs during cleanup, where the goal is "gone", not "was there".
pub fn credential_delete(user_id: u64) -> Result<(), CoreError> {
    let entry = keyring::Entry::new(SERVICE_NAME, &user_id.to_string())
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CoreError::Keyring(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Per-cookie encryption
// ---------------------------------------------------------------------------

/// Encrypt a single cookie under the session's data key, returning a Base64
/// blob. No key derivation happens here — the v1 scheme ran 100k SHA-256
/// rounds per cookie, on every launch of every account.
pub fn encrypt_cookie(cookie: &str, session: &StoreSession) -> Result<String, CoreError> {
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);

    let ciphertext = session
        .key
        .cipher()?
        .encrypt(Nonce::from_slice(&nonce), cookie.as_bytes())
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypt a Base64 cookie blob produced by [`encrypt_cookie`].
pub fn decrypt_cookie(encoded: &str, session: &StoreSession) -> Result<String, CoreError> {
    let data = B64
        .decode(encoded)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    if data.len() < NONCE_LEN + TAG_LEN {
        return Err(CoreError::Crypto("encrypted cookie too short".into()));
    }
    let (nonce, ciphertext) = data.split_at(NONCE_LEN);

    let plaintext = session
        .key
        .cipher()?
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| CoreError::Crypto("cookie decryption failed".into()))?;

    String::from_utf8(plaintext).map_err(|e| CoreError::Crypto(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Account;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ram_crypto_{}_{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("accounts.dat")
    }

    fn cleanup(p: &Path) {
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    fn store_with(n: u64) -> AccountStore {
        let mut s = AccountStore::default();
        for i in 0..n {
            s.accounts
                .push(Account::new(i, format!("user{i}"), format!("User {i}")));
        }
        s
    }

    /// A password session built without touching the OS credential store, so
    /// the suite runs on headless CI.
    fn pw_session(password: &str) -> StoreSession {
        create_password_session(password).unwrap()
    }

    // ---- v1 compatibility ----

    /// Write a file in the pre-envelope format, exactly as v1.7.0 did.
    fn write_v1(path: &Path, store: &AccountStore, password: &str) {
        let key = derive_key_v1(password);
        let plaintext = serde_json::to_vec(store).unwrap();
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let ct = key
            .cipher()
            .unwrap()
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
            .unwrap();
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        crate::storage::atomic_write(path, &out).unwrap();
    }

    #[test]
    fn opens_a_v1_file_written_by_the_previous_release() {
        let p = scratch("v1open");
        write_v1(&p, &store_with(3), "hunter2");
        let (store, session) = unlock_with_password(&p, "hunter2").unwrap();
        assert_eq!(store.accounts.len(), 3);
        assert!(session.is_legacy());
        assert_eq!(session.mode(), StoreMode::Password);
        // Unsaveable until upgraded, so a half-migrated store cannot reach disk.
        assert!(save_store(&p, &store, &session).is_err());
        cleanup(&p);
    }

    #[test]
    fn v1_file_is_reported_as_legacy_and_needing_a_password() {
        let p = scratch("v1legacy");
        write_v1(&p, &store_with(1), "pw");
        assert!(is_legacy(&p));
        assert_eq!(peek_mode(&p).unwrap(), Some(StoreMode::Password));
        cleanup(&p);
    }

    #[test]
    fn v1_cookies_survive_the_upgrade_to_v2() {
        let p = scratch("v1cookies");

        // Build a v1 store whose cookies are encrypted with the v1 KDF.
        let legacy_key = derive_key_v1("pw");
        let legacy = StoreSession {
            key: legacy_key,
            header: Vec::new(),
        };
        let mut store = store_with(2);
        store.accounts[0].encrypted_cookie =
            Some(encrypt_cookie("COOKIE_ZERO", &legacy).unwrap());
        store.accounts[1].encrypted_cookie = Some(encrypt_cookie("COOKIE_ONE", &legacy).unwrap());
        write_v1(&p, &store, "pw");

        let (opened, legacy_session) = unlock_with_password(&p, "pw").unwrap();
        let (upgraded, session) = upgrade_v1(&opened, &legacy_session, Some("pw")).unwrap();

        // Same plaintext, different ciphertext, readable under the new session.
        assert_eq!(
            decrypt_cookie(upgraded.accounts[0].encrypted_cookie.as_ref().unwrap(), &session)
                .unwrap(),
            "COOKIE_ZERO"
        );
        assert_eq!(
            decrypt_cookie(upgraded.accounts[1].encrypted_cookie.as_ref().unwrap(), &session)
                .unwrap(),
            "COOKIE_ONE"
        );

        save_store(&p, &upgraded, &session).unwrap();
        assert!(!is_legacy(&p), "upgrade should have written v2");
        cleanup(&p);
    }

    #[test]
    fn a_failed_cookie_re_encryption_abandons_the_whole_upgrade() {
        let legacy = pw_session("a");
        let mut store = store_with(2);
        store.accounts[0].encrypted_cookie = Some(encrypt_cookie("good", &legacy).unwrap());
        // Not decryptable under `legacy` — simulates a cookie written under a
        // different key. The old ChangePassword silently skipped these.
        store.accounts[1].encrypted_cookie =
            Some(encrypt_cookie("orphan", &pw_session("b")).unwrap());

        let err = upgrade_v1(&store, &legacy, Some("a"));
        assert!(err.is_err(), "upgrade must fail rather than half-migrate");
        cleanup(&scratch("noop"));
    }

    // ---- v2 round trips ----

    #[test]
    fn password_store_round_trips_through_disk() {
        let p = scratch("pwroundtrip");
        let session = pw_session("hunter2");
        save_store(&p, &store_with(3), &session).unwrap();

        let (loaded, reopened) = unlock_with_password(&p, "hunter2").unwrap();
        assert_eq!(loaded.accounts.len(), 3);
        assert_eq!(reopened.mode(), StoreMode::Password);
        assert!(reopened.needs_password());
        cleanup(&p);
    }

    #[test]
    fn wrong_password_is_rejected() {
        let p = scratch("wrongpw");
        save_store(&p, &store_with(1), &pw_session("correct")).unwrap();
        assert!(unlock_with_password(&p, "incorrect").is_err());
        cleanup(&p);
    }

    #[test]
    fn peek_reports_the_mode_without_unlocking() {
        let p = scratch("peek");
        assert_eq!(peek_mode(&p).unwrap(), None, "no file yet");
        save_store(&p, &store_with(1), &pw_session("pw")).unwrap();
        assert_eq!(peek_mode(&p).unwrap(), Some(StoreMode::Password));
        assert!(!is_legacy(&p));
        cleanup(&p);
    }

    #[test]
    fn a_password_store_opened_as_device_mode_says_so() {
        let p = scratch("modemismatch");
        save_store(&p, &store_with(1), &pw_session("pw")).unwrap();
        let bytes = std::fs::read(&p).unwrap();

        // Goes through `open_device` with a dummy wrapping key rather than
        // `unlock_with_device`, so the assertion is about the mode check and not
        // about whether this machine happens to have a credential store.
        let err = open_device(&bytes, &StoreKey::random()).unwrap_err().to_string();
        assert!(err.contains("master password"), "unhelpful error: {err}");
        cleanup(&p);
    }

    #[test]
    fn a_v1_file_opened_as_device_mode_asks_for_the_password() {
        let p = scratch("v1asdevice");
        write_v1(&p, &store_with(1), "pw");
        let bytes = std::fs::read(&p).unwrap();
        let err = open_device(&bytes, &StoreKey::random()).unwrap_err().to_string();
        assert!(err.contains("master password"), "unhelpful error: {err}");
        cleanup(&p);
    }

    // ---- rewrapping ----

    #[test]
    fn changing_the_password_leaves_every_cookie_untouched() {
        let p = scratch("rewrap");
        let session = pw_session("old");
        let mut store = store_with(1);
        let cookie_blob = encrypt_cookie("SECRET", &session).unwrap();
        store.accounts[0].encrypted_cookie = Some(cookie_blob.clone());
        save_store(&p, &store, &session).unwrap();

        let (opened, opened_session) = unlock_with_password(&p, "old").unwrap();
        let rewrapped = rewrap(&opened_session, Some("new")).unwrap();
        save_rekeyed(&p, &opened, &rewrapped).unwrap();

        let (reloaded, reloaded_session) = unlock_with_password(&p, "new").unwrap();
        // The stored blob is byte-identical: only the header changed.
        assert_eq!(
            reloaded.accounts[0].encrypted_cookie.as_deref(),
            Some(cookie_blob.as_str())
        );
        assert_eq!(
            decrypt_cookie(&cookie_blob, &reloaded_session).unwrap(),
            "SECRET"
        );
        cleanup(&p);
    }

    #[test]
    fn the_old_password_cannot_reopen_the_store_through_the_backup() {
        let p = scratch("oldpwbackup");
        let session = pw_session("old");
        // Two saves first, so a genuine backup exists before the re-key.
        save_store(&p, &store_with(1), &session).unwrap();
        save_store(&p, &store_with(1), &session).unwrap();
        assert!(crate::storage::backup_path(&p).exists());

        let (opened, opened_session) = unlock_with_password(&p, "old").unwrap();
        let rewrapped = rewrap(&opened_session, Some("new")).unwrap();
        save_rekeyed(&p, &opened, &rewrapped).unwrap();

        // A plain save_store here would leave `.bak` under the old key, and
        // backup recovery would open it *and* roll the primary back to it.
        assert!(
            unlock_with_password(&p, "old").is_err(),
            "the old password still opens the store"
        );
        assert!(unlock_with_password(&p, "new").is_ok());

        // The backup must still be usable — invalidating it must not mean
        // leaving the store with no recovery copy at all.
        let backup_bytes = std::fs::read(crate::storage::backup_path(&p)).unwrap();
        assert!(
            open_password(&backup_bytes, "new").is_ok(),
            "re-key left an unreadable backup"
        );
        cleanup(&p);
    }

    /// The full path an existing user walks: a v1 file on disk, unlocked with
    /// their old password, upgraded, then re-locked under different key
    /// material. Covers the joint between `upgrade_v1` and `rewrap`, which the
    /// individual tests above each exercise only one side of.
    #[test]
    fn a_v1_store_survives_upgrade_then_a_change_of_key() {
        let p = scratch("v1chain");
        let legacy_key = derive_key_v1("old");
        let legacy = StoreSession {
            key: legacy_key,
            header: Vec::new(),
        };
        let mut store = store_with(1);
        store.accounts[0].encrypted_cookie = Some(encrypt_cookie("COOKIE", &legacy).unwrap());
        write_v1(&p, &store, "old");

        // Unlock as the app does at startup.
        let (opened, legacy_session) = unlock_with_password(&p, "old").unwrap();
        assert!(legacy_session.is_legacy());

        // Upgrade, keeping the password they already had.
        let (upgraded, session) = upgrade_v1(&opened, &legacy_session, Some("old")).unwrap();
        save_rekeyed(&p, &upgraded, &session).unwrap();
        assert!(!is_legacy(&p));

        // Then change it, the way the settings panel does.
        let (reopened, reopened_session) = unlock_with_password(&p, "old").unwrap();
        let rewrapped = rewrap(&reopened_session, Some("new")).unwrap();
        save_rekeyed(&p, &reopened, &rewrapped).unwrap();

        let (finally, final_session) = unlock_with_password(&p, "new").unwrap();
        assert_eq!(
            decrypt_cookie(
                finally.accounts[0].encrypted_cookie.as_ref().unwrap(),
                &final_session
            )
            .unwrap(),
            "COOKIE",
            "the cookie must survive both re-keys"
        );
        assert!(unlock_with_password(&p, "old").is_err());
        cleanup(&p);
    }

    #[test]
    fn upgrading_a_v1_store_retires_the_v1_backup() {
        let p = scratch("v1backup");
        write_v1(&p, &store_with(1), "pw");
        write_v1(&p, &store_with(1), "pw"); // second write creates the .bak

        let (opened, legacy) = unlock_with_password(&p, "pw").unwrap();
        // Password mode keeps this test off the OS credential store; the backup
        // handling under test is identical either way.
        let (upgraded, session) = upgrade_v1(&opened, &legacy, Some("pw")).unwrap();
        save_rekeyed(&p, &upgraded, &session).unwrap();

        // No v1 copy may survive: it is readable under the old unsalted KDF.
        assert!(!is_legacy(&p));
        let backup_bytes = std::fs::read(crate::storage::backup_path(&p)).unwrap();
        assert!(is_v2(&backup_bytes), "a v1 backup survived the upgrade");
        cleanup(&p);
    }

    // ---- tamper resistance ----

    #[test]
    fn flipping_the_mode_byte_does_not_downgrade_the_store() {
        let p = scratch("tampermode");
        save_store(&p, &store_with(1), &pw_session("pw")).unwrap();

        let mut bytes = std::fs::read(&p).unwrap();
        bytes[OFF_MODE] = MODE_DEVICE;
        std::fs::write(&p, &bytes).unwrap();

        // Claims to be device mode, but the wrapped key authenticates the mode
        // byte as AAD, so the unwrap fails instead of opening.
        assert!(unlock_with_device(&p).is_err());
        cleanup(&p);
    }

    #[test]
    fn a_future_format_version_is_named_in_the_error() {
        let p = scratch("future");
        save_store(&p, &store_with(1), &pw_session("pw")).unwrap();
        let mut bytes = std::fs::read(&p).unwrap();
        bytes[OFF_VERSION] = 99;
        std::fs::write(&p, &bytes).unwrap();

        let err = unlock_with_password(&p, "pw").unwrap_err().to_string();
        assert!(err.contains("newer version"), "unhelpful error: {err}");
        cleanup(&p);
    }

    // ---- durability ----

    #[test]
    fn recovers_and_self_heals_from_backup_when_primary_corrupt() {
        let p = scratch("recover");
        let session = pw_session("pw");
        save_store(&p, &store_with(1), &session).unwrap();
        save_store(&p, &store_with(2), &session).unwrap();

        std::fs::write(&p, b"this is not a valid envelope at all").unwrap();

        let (recovered, _) = unlock_with_password(&p, "pw").unwrap();
        assert_eq!(recovered.accounts.len(), 1, "should be the backup's contents");

        let (healed, _) = unlock_with_password(&p, "pw").unwrap();
        assert_eq!(healed.accounts.len(), 1, "primary should have been healed");
        cleanup(&p);
    }

    #[test]
    fn a_damaged_body_is_not_blamed_on_the_password() {
        let p = scratch("damaged");
        let session = pw_session("pw");
        save_store(&p, &store_with(1), &session).unwrap();
        // No backup exists yet, so recovery cannot mask the damage.
        assert!(!crate::storage::backup_path(&p).exists());

        // Corrupt the body, leaving the header (and so the key unwrap) intact.
        let mut bytes = std::fs::read(&p).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&p, &bytes).unwrap();

        let err = unlock_with_password(&p, "pw").unwrap_err().to_string();
        assert!(err.contains("damaged"), "should not blame the password: {err}");
        assert!(!err.contains("password"), "should not blame the password: {err}");
        cleanup(&p);
    }

    #[test]
    fn concurrent_writers_never_corrupt_the_file() {
        use std::sync::Arc;
        use std::thread;

        let p = Arc::new(scratch("concurrent"));
        let session = Arc::new(pw_session("pw"));
        save_store(&p, &store_with(1), &session).unwrap();

        // Derive the wrapping key once and reopen through it directly. Going via
        // `unlock_with_password` would run Argon2 on every one of the 200 reads
        // below and turn a race check into a 90-second KDF benchmark.
        let salt: [u8; SALT_LEN] = std::fs::read(&*p).unwrap()[OFF_SALT..OFF_SALT + SALT_LEN]
            .try_into()
            .unwrap();
        let wrapping = derive_wrapping_key("pw", &salt).unwrap();
        let reopen = || {
            let bytes = std::fs::read(&*p).unwrap();
            with_backup_recovery(&p, &bytes, |b| try_open_v2(b, &wrapping))
        };

        let mut handles = Vec::new();
        for w in 0..8u64 {
            let p = Arc::clone(&p);
            let session = Arc::clone(&session);
            handles.push(thread::spawn(move || {
                for _ in 0..25 {
                    let _ = save_store(&p, &store_with(w + 1), &session);
                }
            }));
        }

        for _ in 0..200 {
            assert!(
                reopen().is_ok(),
                "a concurrent write produced an unreadable file"
            );
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(reopen().is_ok());
        cleanup(&p);
    }

    // ---- cookies ----

    #[test]
    fn cookies_round_trip_and_reject_a_foreign_session() {
        let a = pw_session("a");
        let b = pw_session("b");
        let blob = encrypt_cookie("_|WARNING:-DO-NOT-SHARE-THIS", &a).unwrap();
        assert_eq!(decrypt_cookie(&blob, &a).unwrap(), "_|WARNING:-DO-NOT-SHARE-THIS");
        assert!(decrypt_cookie(&blob, &b).is_err());
    }

    #[test]
    fn the_same_cookie_encrypts_differently_every_time() {
        let s = pw_session("pw");
        assert_ne!(
            encrypt_cookie("same", &s).unwrap(),
            encrypt_cookie("same", &s).unwrap(),
            "nonce reuse"
        );
    }
}
