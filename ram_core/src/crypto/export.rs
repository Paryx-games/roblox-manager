//! Password-protected export format, separate from the live account-store envelope.
use super::{
    derive_wrapping_key, fill_random, nonce_from_slice, ARGON_ITERATIONS, ARGON_MEMORY_KIB,
    ARGON_PARALLELISM, B64, NONCE_LEN, SALT_LEN,
};
use crate::{
    account_export::{Encryption, ExportFormat, MAX_EXPORT_BYTES},
    error::CoreError,
};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes128Gcm,
};
use base64::Engine;
use rand::rngs::SysRng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    kind: String,
    version: u8,
    format: ExportFormat,
    encryption: Encryption,
    kdf: String,
    #[serde(rename = "memoryKiB")]
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    derived_key_bytes: u32,
    salt: String,
    nonce: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    header: Header,
    ciphertext: String,
}

fn failed() -> CoreError {
    CoreError::Export(
        "The encrypted export could not be read. Check its password and file integrity.".into(),
    )
}

pub fn seal(
    bytes: &[u8],
    password: &str,
    encryption: Encryption,
    format: ExportFormat,
) -> Result<Vec<u8>, CoreError> {
    if !encryption.recommended()
        || password.chars().count() < 12
        || password.len() > 1024
        || password.trim().is_empty()
        || bytes.len() > MAX_EXPORT_BYTES
    {
        return Err(failed());
    }
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut SysRng, &mut salt)?;
    fill_random(&mut SysRng, &mut nonce)?;
    let header = Header {
        kind: "rm.account-export".into(),
        version: 1,
        format,
        encryption,
        kdf: "argon2id".into(),
        memory_kib: ARGON_MEMORY_KIB,
        iterations: ARGON_ITERATIONS,
        parallelism: ARGON_PARALLELISM,
        derived_key_bytes: 32,
        salt: B64.encode(salt),
        nonce: B64.encode(nonce),
    };
    let aad = serde_json::to_vec(&header).map_err(|_| failed())?;
    let key = derive_wrapping_key(password, &salt)?;
    let payload = Payload {
        msg: bytes,
        aad: &aad,
    };
    let ciphertext = match encryption {
        Encryption::Aes256Gcm => key.cipher()?.encrypt(&nonce_from_slice(&nonce)?, payload),
        Encryption::Aes128Gcm => Aes128Gcm::new_from_slice(&key.0[..16])
            .map_err(|_| failed())?
            .encrypt(&nonce_from_slice(&nonce)?, payload),
        Encryption::None => return Err(failed()),
    }
    .map_err(|_| failed())?;
    serde_json::to_vec_pretty(&Envelope {
        header,
        ciphertext: B64.encode(ciphertext),
    })
    .map_err(|_| failed())
}

/// Recover an export without writing plaintext to disk or returning it through IPC.
pub fn open(bytes: &[u8], password: &str) -> Result<(ExportFormat, Vec<u8>), CoreError> {
    if bytes.len() > MAX_EXPORT_BYTES * 2 || password.len() > 1024 {
        return Err(failed());
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| failed())?;
    let h = &envelope.header;
    if h.kind != "rm.account-export"
        || h.version != 1
        || h.kdf != "argon2id"
        || h.memory_kib != ARGON_MEMORY_KIB
        || h.iterations != ARGON_ITERATIONS
        || h.parallelism != ARGON_PARALLELISM
        || h.derived_key_bytes != 32
        || !h.encryption.recommended()
    {
        return Err(failed());
    }
    let salt = B64.decode(&h.salt).map_err(|_| failed())?;
    let nonce = B64.decode(&h.nonce).map_err(|_| failed())?;
    if salt.len() != SALT_LEN || nonce.len() != NONCE_LEN {
        return Err(failed());
    }
    let ciphertext = B64.decode(&envelope.ciphertext).map_err(|_| failed())?;
    if ciphertext.len() > MAX_EXPORT_BYTES + 16 {
        return Err(failed());
    }
    let aad = serde_json::to_vec(h).map_err(|_| failed())?;
    let key = derive_wrapping_key(password, &salt)?;
    let payload = Payload {
        msg: &ciphertext,
        aad: &aad,
    };
    let plaintext = match h.encryption {
        Encryption::Aes256Gcm => key.cipher()?.decrypt(&nonce_from_slice(&nonce)?, payload),
        Encryption::Aes128Gcm => Aes128Gcm::new_from_slice(&key.0[..16])
            .map_err(|_| failed())?
            .decrypt(&nonce_from_slice(&nonce)?, payload),
        Encryption::None => return Err(failed()),
    }
    .map_err(|_| failed())?;
    Ok((h.format, plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;
    const PASSWORD: &str = "synthetic export test password";
    #[test]
    fn export_metadata_and_ciphertext_are_authenticated_and_kdf_costs_are_bounded() {
        let sealed = seal(
            b"synthetic secret",
            PASSWORD,
            Encryption::Aes256Gcm,
            ExportFormat::Json,
        )
        .unwrap();
        let original: serde_json::Value = serde_json::from_slice(&sealed).unwrap();
        for (field, value) in [
            ("format", serde_json::json!("csv")),
            ("encryption", serde_json::json!("aes128Gcm")),
            ("memoryKiB", serde_json::json!(u32::MAX)),
            ("version", serde_json::json!(2)),
            ("nonce", serde_json::json!("invalid")),
        ] {
            let mut changed = original.clone();
            changed["header"][field] = value;
            assert!(open(&serde_json::to_vec(&changed).unwrap(), PASSWORD).is_err());
        }
        let mut changed = original;
        changed["ciphertext"] = serde_json::json!(B64.encode([0u8; 32]));
        assert!(open(&serde_json::to_vec(&changed).unwrap(), PASSWORD).is_err());
        assert!(open(b"{bad JSON", PASSWORD).is_err());
    }
    #[test]
    fn independent_exports_use_fresh_salts_and_nonces() {
        let first = seal(
            b"same data",
            PASSWORD,
            Encryption::Aes128Gcm,
            ExportFormat::Txt,
        )
        .unwrap();
        let second = seal(
            b"same data",
            PASSWORD,
            Encryption::Aes128Gcm,
            ExportFormat::Txt,
        )
        .unwrap();
        assert_ne!(first, second);
        let a: Envelope = serde_json::from_slice(&first).unwrap();
        let b: Envelope = serde_json::from_slice(&second).unwrap();
        assert_ne!(a.header.salt, b.header.salt);
        assert_ne!(a.header.nonce, b.header.nonce);
        assert_eq!(open(&first, PASSWORD).unwrap().1, b"same data");
    }
}
