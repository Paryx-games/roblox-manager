//! Secret scrubbing for anything on its way to a log file.
//!
//! RM logs to `%APPDATA%\RM\rm.log`, and users hand that file over when they
//! report a bug. Anything session-grade that reaches it is effectively
//! published: a `.ROBLOSECURITY` cookie is a full account takeover, and a
//! `gameinfo:` authentication ticket is a short-lived but real credential.
//!
//! Two layers use this:
//!
//! * Call sites that knowingly handle a secret and want a *useful* log line
//!   anyway (see [`crate::process::launch_game`], which logs the launch URI
//!   with only the ticket removed).
//! * A blanket writer wrapper in `ram_ui` that runs every formatted event
//!   through [`scrub`] before it reaches the file. That is the backstop: a new
//!   `debug!` that interpolates a cookie cannot silently leak, because the
//!   scrub happens below every call site rather than at each one.
//!
//! Scrubbing at write time rather than read time is deliberate. A filter
//! applied when reading the log would still leave the plaintext on disk, which
//! is precisely the thing that must not exist.
//!
//! This is a safety net, not a licence to log secrets. The patterns below only
//! catch shapes we know about.

use regex::{Captures, Regex};
use std::borrow::Cow;
use std::sync::OnceLock;

/// Stand-in written in place of a removed secret.
const REDACTED: &str = "<redacted>";

type Replacer = fn(&Captures<'_>) -> String;

struct Rule {
    re: Regex,
    repl: Replacer,
}

/// Keep capture group 1 (the label that makes the line worth reading) and
/// replace group 2 (the secret) with the placeholder.
fn keep_label(caps: &Captures<'_>) -> String {
    format!("{}{REDACTED}", &caps[1])
}

/// Replace the entire match. Used where the secret is self-identifying and has
/// no label worth keeping.
fn whole_match(_caps: &Captures<'_>) -> String {
    REDACTED.to_string()
}

/// `C:\Users\<name>\...` keeps the prefix and loses the account name. Shared
/// profile directories are left alone: they name no one, and blanking them
/// would make a genuinely useful path unreadable.
fn user_path(caps: &Captures<'_>) -> String {
    const SHARED: [&str; 4] = ["public", "default", "default user", "all users"];
    let name = &caps[2];
    if SHARED.contains(&name.to_ascii_lowercase().as_str()) {
        return caps[0].to_string();
    }
    format!("{}{REDACTED}", &caps[1])
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mk = |pattern: &str, repl: Replacer| Rule {
            // Every pattern here is a literal in this file, so a compile
            // failure is a bug in this module and nowhere else.
            re: Regex::new(pattern).expect("redaction pattern must compile"),
            repl,
        };
        vec![
            // A bare cookie value. Roblox prefixes every `.ROBLOSECURITY` with
            // this exact banner, which makes the value self-identifying even
            // when it appears with no surrounding context (an error message
            // echoing a request body, say).
            mk(
                r#"_\|WARNING:-DO-NOT-SHARE-THIS![^\s;,"'\)\]}]*"#,
                whole_match,
            ),
            // `.ROBLOSECURITY=<value>` as it appears in a Cookie header, a
            // Set-Cookie, or a serialized request.
            mk(
                r#"(?i)(\.ROBLOSECURITY\s*[=:]\s*"?)([^\s;,"'\)\]}]+)"#,
                keep_label,
            ),
            // The launch URI's authentication ticket. `+` terminates the field
            // in `roblox-player:` URIs, so it must not be consumed.
            mk(r#"(?i)(gameinfo:)([^+\s&"'\)\]}]+)"#, keep_label),
            // The same ticket as it comes off the wire, and the CSRF token that
            // travels with it.
            mk(
                r#"(?i)(rbx-authentication-ticket\s*[=:]\s*"?)([^\s;,"'\)\]}]+)"#,
                keep_label,
            ),
            mk(
                r#"(?i)(x-csrf-token\s*[=:]\s*"?)([^\s;,"'\)\]}]+)"#,
                keep_label,
            ),
            // The Windows account name embedded in every local path we log.
            mk(r#"(?i)([a-z]:[\\/]users[\\/])([^\\/\s"']+)"#, user_path),
        ]
    })
}

/// Remove every secret shape this module knows about from `input`.
///
/// Returns the input untouched (and unallocated) when there is nothing to
/// redact, which is the overwhelmingly common case for a log line.
pub fn scrub(input: &str) -> Cow<'_, str> {
    let mut owned: Option<String> = None;
    for rule in rules() {
        let next = {
            let src: &str = owned.as_deref().unwrap_or(input);
            match rule.re.replace_all(src, rule.repl) {
                Cow::Owned(s) => Some(s),
                Cow::Borrowed(_) => None,
            }
        };
        if let Some(s) = next {
            owned = Some(s);
        }
    }
    match owned {
        Some(s) => Cow::Owned(s),
        None => Cow::Borrowed(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistically shaped cookie: the banner, then base64-ish payload.
    const COOKIE: &str = "_|WARNING:-DO-NOT-SHARE-THIS!--.ABCDEF0123456789abcdef_-";
    const TICKET: &str = "T1cKeT-abcDEF0123456789_ghijk";

    #[test]
    fn leaves_an_ordinary_line_alone_without_allocating() {
        let line = "Launching game - place 606849621";
        assert!(matches!(scrub(line), Cow::Borrowed(_)));
    }

    #[test]
    fn removes_a_bare_cookie_value() {
        let line = format!("cookie is {COOKIE} ok");
        let out = scrub(&line);
        assert!(!out.contains("ABCDEF0123456789"), "{out}");
        assert!(out.contains(REDACTED), "{out}");
        assert!(out.ends_with(" ok"), "swallowed trailing context: {out}");
    }

    #[test]
    fn removes_a_labelled_cookie_but_keeps_the_label() {
        let line = format!("Cookie: .ROBLOSECURITY={COOKIE}; path=/");
        let out = scrub(&line);
        assert!(out.contains(".ROBLOSECURITY="), "{out}");
        assert!(!out.contains("ABCDEF"), "{out}");
        assert!(out.contains("path=/"), "the `;` must terminate the value: {out}");
    }

    #[test]
    fn removes_the_launch_ticket_and_keeps_the_rest_of_the_uri() {
        let uri = format!(
            "roblox-player:1+launchmode:play+gameinfo:{TICKET}+launchtime:1754438400000\
             +placelauncherurl:https%3A%2F%2Fassetgame.roblox.com"
        );
        let out = scrub(&uri);
        assert!(!out.contains("T1cKeT"), "{out}");
        assert!(out.contains("gameinfo:<redacted>"), "{out}");
        // The fields either side of the ticket are what makes the line worth
        // logging at all.
        assert!(out.contains("launchmode:play"), "{out}");
        assert!(out.contains("launchtime:1754438400000"), "{out}");
        assert!(out.contains("placelauncherurl:"), "{out}");
    }

    #[test]
    fn removes_the_ticket_and_csrf_headers() {
        let line =
            format!("headers: rbx-authentication-ticket: {TICKET}, x-csrf-token: aBcD1234");
        let out = scrub(&line);
        assert!(!out.contains("T1cKeT"), "{out}");
        assert!(!out.contains("aBcD1234"), "{out}");
    }

    #[test]
    fn removes_the_windows_username_from_paths() {
        let out = scrub(r"reading C:\Users\Keeper\AppData\Roaming\RM\accounts.dat");
        assert_eq!(
            out,
            r"reading C:\Users\<redacted>\AppData\Roaming\RM\accounts.dat"
        );
    }

    #[test]
    fn handles_forward_slashes_and_a_lowercase_drive() {
        let out = scrub("profile at d:/users/sasha/rm/profile");
        assert_eq!(out, "profile at d:/users/<redacted>/rm/profile");
    }

    #[test]
    fn keeps_shared_profile_directories_readable() {
        let line = r"C:\Users\Public\Documents\thing";
        assert_eq!(scrub(line), line);
    }

    #[test]
    fn scrubs_every_secret_in_one_line() {
        let line = format!(r"C:\Users\Keeper\rm.log: launch gameinfo:{TICKET} for {COOKIE}");
        let out = scrub(&line);
        assert!(!out.contains("Keeper"), "{out}");
        assert!(!out.contains("T1cKeT"), "{out}");
        assert!(!out.contains("ABCDEF"), "{out}");
    }

    #[test]
    fn is_idempotent() {
        let once = scrub(&format!("gameinfo:{TICKET}")).into_owned();
        assert_eq!(scrub(&once), once);
    }
}
