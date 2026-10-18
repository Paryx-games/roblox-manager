# Account export format

Account exports are separate from the live `RAMSTORE` envelope. They do not migrate or replace `accounts.dat`. JSON, CSV and TXT metadata exports can be unencrypted; including the cookie field requires an encrypted container. The save command refuses destinations inside RM's data directory or over the live account store and its backup.

Encrypted exports use a JSON envelope with `header` and Base64 `ciphertext`. The current header contains these fields, in this order:

| Field | Value |
| --- | --- |
| `kind` | `rm.account-export` |
| `version` | `1` |
| `format` | `json`, `csv` or `txt` |
| `encryption` | `aes256Gcm` or `aes128Gcm` |
| `kdf` | `argon2id` |
| `memoryKiB` | `19456` |
| `iterations` | `3` |
| `parallelism` | `1` |
| `derivedKeyBytes` | `32` |
| `salt` | Base64 of 16 random bytes |
| `nonce` | Base64 of 12 random bytes |

Argon2id version 0x13 derives 32 bytes from the UTF-8 export password and salt. AES-256-GCM uses the full derived key; AES-128-GCM uses its first 16 bytes. The compact UTF-8 JSON serialization of the header in the order above is authenticated additional data. The ciphertext includes the 16-byte GCM authentication tag. Every export generates a fresh salt and nonce. The password and plaintext account identifiers are absent from the envelope header.

`ram_core::crypto::export::open(bytes, password)` verifies the header, fixed KDF bounds and authentication tag, then returns the selected format and plaintext bytes in memory. It does not write plaintext, log contents or expose them through IPC. Unknown versions, modified metadata, invalid lengths and wrong passwords return safe errors. The current cookie-import UI does not consume this container; a future import flow must keep decrypted cookies in Rust and preserve the same credential boundaries.

Export payloads contain only the selected fields and accounts. JSON uses an array of objects with string values. CSV uses a header row and quoted values, neutralizing spreadsheet formula prefixes. TXT uses labelled fields and blank lines between accounts, with embedded line breaks escaped. These exports preserve selected data rather than the full internal account-store model.

The default extension is `.<format>.rmexport`. Treat that as an encrypted container regardless of the selected inner format. A future incompatible container change needs its own version decision and decoder compatibility handling, independently of the live store format.
