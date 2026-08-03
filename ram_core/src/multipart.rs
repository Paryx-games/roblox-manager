//! Minimal `multipart/form-data` encoder.
//!
//! Deliberately hand-rolled rather than using `reqwest::multipart`.
//! [`crate::auth::RobloxClient`] retries a request on CSRF rotation and on 429
//! backoff by looping and rebuilding the request, but `reqwest::multipart::Form`
//! is `!Clone` and is consumed by `send()`, so a `Form` cannot survive a second
//! attempt. Encoding to a plain `Vec<u8>` up front makes the body trivially
//! re-sendable, keeps the retry loop intact, and adds no dependencies.
//!
//! This module is standalone (not part of `assets`) because the transport layer
//! consumes it, and transport must not depend on a feature module.

use uuid::Uuid;

/// How many distinct boundaries [`encode_with_fresh_boundary`] will try before
/// giving up. A v4 UUID colliding with file content even once is already
/// vanishingly unlikely; the loop exists so a deliberately crafted file cannot
/// silently corrupt the request.
const MAX_BOUNDARY_ATTEMPTS: usize = 8;

/// One part of a `multipart/form-data` body.
pub struct Part<'a> {
    /// The form field name, e.g. `request` or `fileContent`.
    pub name: &'a str,
    /// Present only for file parts.
    pub filename: Option<&'a str>,
    /// Omitted entirely when `None`, which is what a plain text part wants.
    pub content_type: Option<&'a str>,
    pub bytes: &'a [u8],
}

/// The `Content-Type` header value that must accompany a body from [`encode`].
pub fn content_type_header(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// Encode `parts` into a complete `multipart/form-data` body.
///
/// The caller is responsible for `boundary` not occurring inside any part's
/// bytes; [`encode_with_fresh_boundary`] does that for you.
pub fn encode(boundary: &str, parts: &[Part<'_>]) -> Vec<u8> {
    // Headers are small and bounded; the file part dominates. One allocation up
    // front avoids repeatedly regrowing a buffer that can be tens of megabytes.
    let payload: usize = parts.iter().map(|p| p.bytes.len()).sum();
    let mut out = Vec::with_capacity(payload + parts.len() * 256 + 64);

    for part in parts {
        out.extend_from_slice(b"--");
        out.extend_from_slice(boundary.as_bytes());
        out.extend_from_slice(b"\r\n");

        out.extend_from_slice(b"Content-Disposition: form-data; name=\"");
        out.extend_from_slice(sanitize_header_value(part.name).as_bytes());
        out.extend_from_slice(b"\"");
        if let Some(filename) = part.filename {
            out.extend_from_slice(b"; filename=\"");
            out.extend_from_slice(sanitize_header_value(filename).as_bytes());
            out.extend_from_slice(b"\"");
        }
        out.extend_from_slice(b"\r\n");

        if let Some(content_type) = part.content_type {
            out.extend_from_slice(b"Content-Type: ");
            out.extend_from_slice(sanitize_header_value(content_type).as_bytes());
            out.extend_from_slice(b"\r\n");
        }

        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(part.bytes);
        out.extend_from_slice(b"\r\n");
    }

    out.extend_from_slice(b"--");
    out.extend_from_slice(boundary.as_bytes());
    out.extend_from_slice(b"--\r\n");
    out
}

/// Pick a boundary that appears in none of the parts, then [`encode`] with it.
/// Returns `(boundary, body)`; pass the boundary to [`content_type_header`].
pub fn encode_with_fresh_boundary(parts: &[Part<'_>]) -> (String, Vec<u8>) {
    let mut boundary = fresh_boundary();
    for _ in 0..MAX_BOUNDARY_ATTEMPTS {
        if !parts.iter().any(|p| contains(p.bytes, boundary.as_bytes())) {
            break;
        }
        boundary = fresh_boundary();
    }
    let body = encode(&boundary, parts);
    (boundary, body)
}

/// Strip anything from a filename that could inject a header line or terminate
/// the quoted string early. Never returns an empty string, because a bare
/// `filename=""` reads as "no filename" to some servers.
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '"' | '\\'))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "file".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Same treatment for a header value we control but did not author.
fn sanitize_header_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '"'))
        .collect()
}

fn fresh_boundary() -> String {
    format!("----RMFormBoundary{}", Uuid::new_v4().simple())
}

/// Naive substring search over bytes. Boundaries are ~30 bytes and files are at
/// most 20 MB, so this is a few milliseconds at worst and runs once per upload.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_str(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn encodes_two_parts_exactly() {
        let json = r#"{"assetType":"Decal"}"#;
        let parts = [
            Part {
                name: "request",
                filename: None,
                content_type: None,
                bytes: json.as_bytes(),
            },
            Part {
                name: "fileContent",
                filename: Some("logo.png"),
                content_type: Some("image/png"),
                bytes: b"PNGDATA",
            },
        ];
        let body = encode("BOUND", &parts);
        let expected = concat!(
            "--BOUND\r\n",
            "Content-Disposition: form-data; name=\"request\"\r\n",
            "\r\n",
            "{\"assetType\":\"Decal\"}\r\n",
            "--BOUND\r\n",
            "Content-Disposition: form-data; name=\"fileContent\"; filename=\"logo.png\"\r\n",
            "Content-Type: image/png\r\n",
            "\r\n",
            "PNGDATA\r\n",
            "--BOUND--\r\n",
        );
        assert_eq!(as_str(&body), expected);
    }

    #[test]
    fn binary_bytes_survive_verbatim() {
        let raw: &[u8] = &[0x00, 0x0d, 0x0a, 0xff, 0x1b];
        let parts = [Part {
            name: "fileContent",
            filename: Some("a.bin"),
            content_type: Some("application/octet-stream"),
            bytes: raw,
        }];
        let body = encode("B", &parts);
        // The payload sits between the blank line after the headers and the
        // CRLF that precedes the closing boundary.
        let start = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&body[start..start + raw.len()], raw);
    }

    #[test]
    fn omits_content_type_when_none() {
        let parts = [Part {
            name: "request",
            filename: None,
            content_type: None,
            bytes: b"x",
        }];
        let body = as_str(&encode("B", &parts));
        assert!(!body.contains("Content-Type"), "got: {body}");
    }

    #[test]
    fn terminates_with_closing_boundary() {
        let parts = [Part {
            name: "a",
            filename: None,
            content_type: None,
            bytes: b"1",
        }];
        assert!(as_str(&encode("XYZ", &parts)).ends_with("--XYZ--\r\n"));
    }

    #[test]
    fn empty_file_still_encodes() {
        let parts = [Part {
            name: "fileContent",
            filename: Some("empty.png"),
            content_type: Some("image/png"),
            bytes: b"",
        }];
        let body = as_str(&encode("B", &parts));
        assert!(body.contains("filename=\"empty.png\""));
        assert!(body.ends_with("\r\n\r\n\r\n--B--\r\n"), "got: {body:?}");
    }

    #[test]
    fn boundary_never_appears_in_payload() {
        // Force a hit on the first candidate by embedding the fixed prefix that
        // every generated boundary starts with, then a lot of plausible tails.
        let payload = b"----RMFormBoundary".repeat(64);
        let parts = [Part {
            name: "fileContent",
            filename: Some("evil.bin"),
            content_type: None,
            bytes: &payload,
        }];
        let (boundary, _) = encode_with_fresh_boundary(&parts);
        assert!(!contains(&payload, boundary.as_bytes()));
    }

    #[test]
    fn content_type_header_carries_the_boundary() {
        assert_eq!(
            content_type_header("ABC"),
            "multipart/form-data; boundary=ABC"
        );
    }

    #[test]
    fn sanitize_filename_strips_injection_characters() {
        assert_eq!(sanitize_filename("a\r\nb\"c\\d.png"), "abcd.png");
    }

    #[test]
    fn sanitize_filename_preserves_unicode() {
        assert_eq!(sanitize_filename("日本語.png"), "日本語.png");
    }

    #[test]
    fn sanitize_filename_never_returns_empty() {
        assert_eq!(sanitize_filename(""), "file");
        assert_eq!(sanitize_filename("   "), "file");
        assert_eq!(sanitize_filename("\"\""), "file");
    }

    #[test]
    fn a_filename_cannot_inject_a_header_line() {
        let parts = [Part {
            name: "fileContent",
            filename: Some("x\r\nX-Evil: 1"),
            content_type: None,
            bytes: b"d",
        }];
        let body = as_str(&encode("B", &parts));
        assert!(!body.contains("X-Evil: 1\r\n"), "got: {body}");
        assert!(body.contains("filename=\"xX-Evil: 1\""), "got: {body}");
    }
}
