//! End-to-end checks for the passwordless (device-locked) store against the
//! real OS credential store.
//!
//! The unit tests in `crypto` deliberately stay on password mode so they never
//! touch the credential store. These do, because the whole point of device mode
//! is that the key lives there, and a mock would not tell us whether the
//! `keyring` round trip actually works on this platform.
//!
//! Two rules keep that safe:
//!
//! * Nothing here **deletes** the device key. Doing so would make a real
//!   device-locked store on the same machine permanently unopenable.
//! * If the credential store is unavailable (a headless Linux box with no
//!   secret service, say), every test skips rather than fails. Creating a key
//!   that the app would create anyway is harmless; failing a build because the
//!   dev box has no keyring is not.

use ram_core::crypto::{self, StoreMode};
use ram_core::models::{Account, AccountStore};
use std::path::PathBuf;

/// Make sure the device key exists before any test uses it, exactly once per
/// test binary.
///
/// Every test here shares the one machine-wide device key entry, and cargo runs
/// them in parallel. Without this, a run against a credential store that has no
/// key yet has each test generate its own and write it: last write wins, and
/// the tests that lost saved stores wrapped in a key the credential store no
/// longer holds. That is precisely how this suite failed on a fresh CI runner
/// while passing on a developer machine where the key already existed.
///
/// Returns `false` when the credential store is unusable, so tests skip rather
/// than fail on a box that has no keyring at all.
fn keyring_available() -> bool {
    use std::sync::OnceLock;
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| match crypto::device_key_or_create() {
        Ok(_) => true,
        Err(e) => {
            eprintln!("skipping: credential store unavailable ({e})");
            false
        }
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ram_device_{}_{name}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("accounts.dat")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

fn store_with_cookie(session: &crypto::StoreSession, cookie: &str) -> AccountStore {
    let mut store = AccountStore::default();
    let mut account = Account::new(7, "tester".into(), "Tester".into());
    account.encrypted_cookie = Some(crypto::encrypt_cookie(cookie, session).unwrap());
    store.accounts.push(account);
    store
}

#[test]
fn a_device_locked_store_reopens_with_no_password() {
    if !keyring_available() {
        return;
    }
    let p = scratch("roundtrip");

    let session = crypto::create_device_session().unwrap();
    assert_eq!(session.mode(), StoreMode::Device);
    assert!(!session.needs_password());

    let store = store_with_cookie(&session, "COOKIE");
    crypto::save_store(&p, &store, &session).unwrap();

    // The headline behaviour: no user input anywhere in this call.
    let (reopened, reopened_session) = crypto::unlock_with_device(&p).unwrap();
    assert_eq!(reopened.accounts.len(), 1);
    assert_eq!(
        crypto::decrypt_cookie(
            reopened.accounts[0].encrypted_cookie.as_ref().unwrap(),
            &reopened_session
        )
        .unwrap(),
        "COOKIE"
    );

    // And the file itself advertises that it needs nothing typed, which is what
    // the app checks at startup to decide whether to show the unlock screen.
    assert_eq!(crypto::peek_mode(&p).unwrap(), Some(StoreMode::Device));
    assert!(!crypto::is_legacy(&p));
    cleanup(&p);
}

#[test]
fn the_file_alone_reveals_no_cookie() {
    if !keyring_available() {
        return;
    }
    let p = scratch("opaque");
    let session = crypto::create_device_session().unwrap();
    let store = store_with_cookie(&session, "_|WARNING:-DO-NOT-SHARE-THIS|_SUPERSECRET");
    crypto::save_store(&p, &store, &session).unwrap();

    // The stated goal of encrypting a passwordless store: an infostealer that
    // scrapes files for raw cookie text finds nothing to take.
    let raw = std::fs::read(&p).unwrap();
    let as_text = String::from_utf8_lossy(&raw);
    assert!(!as_text.contains("SUPERSECRET"));
    assert!(!as_text.contains("WARNING:-DO-NOT-SHARE-THIS"));
    assert!(!as_text.contains("tester"));
    cleanup(&p);
}

#[test]
fn switching_a_password_store_to_passwordless_keeps_the_accounts() {
    if !keyring_available() {
        return;
    }
    let p = scratch("switch");

    // Start where an existing user is: password mode.
    let session = crypto::create_password_session("hunter2").unwrap();
    let store = store_with_cookie(&session, "COOKIE");
    crypto::save_store(&p, &store, &session).unwrap();

    let (opened, opened_session) = crypto::unlock_with_password(&p, "hunter2").unwrap();
    let switched = crypto::rewrap(&opened_session, None).unwrap();
    crypto::save_rekeyed(&p, &opened, &switched).unwrap();

    // Opens with no password now...
    let (reopened, reopened_session) = crypto::unlock_with_device(&p).unwrap();
    assert_eq!(reopened.accounts.len(), 1);
    assert_eq!(
        crypto::decrypt_cookie(
            reopened.accounts[0].encrypted_cookie.as_ref().unwrap(),
            &reopened_session
        )
        .unwrap(),
        "COOKIE"
    );

    // ...and the retired password no longer opens it, including via the backup.
    assert!(crypto::unlock_with_password(&p, "hunter2").is_err());
    cleanup(&p);
}

#[test]
fn switching_back_to_a_password_re_locks_the_store() {
    if !keyring_available() {
        return;
    }
    let p = scratch("relock");

    let session = crypto::create_device_session().unwrap();
    let store = store_with_cookie(&session, "COOKIE");
    crypto::save_store(&p, &store, &session).unwrap();

    let (opened, opened_session) = crypto::unlock_with_device(&p).unwrap();
    let locked = crypto::rewrap(&opened_session, Some("hunter2")).unwrap();
    crypto::save_rekeyed(&p, &opened, &locked).unwrap();

    assert_eq!(crypto::peek_mode(&p).unwrap(), Some(StoreMode::Password));
    assert!(
        crypto::unlock_with_device(&p).is_err(),
        "a password-locked store must not open automatically"
    );
    assert!(crypto::unlock_with_password(&p, "hunter2").is_ok());
    cleanup(&p);
}
