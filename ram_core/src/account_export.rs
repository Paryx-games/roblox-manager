use crate::{crypto, error::CoreError, models::Account};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Json,
    Csv,
    Txt,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Txt => "txt",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encryption {
    None,
    Aes256Gcm,
    Aes128Gcm,
}

impl Encryption {
    pub fn recommended(self) -> bool {
        matches!(self, Self::Aes256Gcm | Self::Aes128Gcm)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportField {
    Username,
    AccountName,
    DisplayName,
    UserId,
    Alias,
    Group,
    CreatedAt,
    AccountAge,
    LastUsed,
    Cookie,
}

impl ExportField {
    fn header(self) -> &'static str {
        match self {
            Self::Username => "username",
            Self::AccountName => "account_name",
            Self::DisplayName => "display_name",
            Self::UserId => "user_id",
            Self::Alias => "alias",
            Self::Group => "group",
            Self::CreatedAt => "created_at",
            Self::AccountAge => "account_age_hours",
            Self::LastUsed => "last_used",
            Self::Cookie => "cookie",
        }
    }
}

// Passwords and account rows deliberately do not implement Debug or Serialize.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    pub account_ids: Vec<u64>,
    pub fields: Vec<ExportField>,
    pub format: ExportFormat,
    pub encryption: Encryption,
    pub password: String,
}

pub struct ExportAccount {
    pub account: Account,
    pub cookie: Option<String>,
}

fn invalid(message: &str) -> CoreError {
    CoreError::Export(message.into())
}

impl ExportOptions {
    pub fn includes_cookies(&self) -> bool {
        self.fields.contains(&ExportField::Cookie)
    }
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.account_ids.is_empty()
            || self.account_ids.len() > 500
            || self.account_ids.contains(&0)
            || self.account_ids.iter().collect::<HashSet<_>>().len() != self.account_ids.len()
        {
            return Err(invalid(
                "Select between 1 and 500 distinct accounts to export.",
            ));
        }
        if self.fields.is_empty()
            || self.fields.len() > 10
            || self.fields.iter().collect::<HashSet<_>>().len() != self.fields.len()
        {
            return Err(invalid(
                "Select at least one export field, without duplicates.",
            ));
        }
        if self.includes_cookies() && !self.encryption.recommended() {
            return Err(invalid(
                "Cookie exports require a recommended encryption method.",
            ));
        }
        if self.encryption.recommended() {
            if self.password.chars().count() < 12
                || self.password.len() > 1024
                || self.password.trim().is_empty()
            {
                return Err(invalid(
                    "Use an export password of at least 12 characters and at most 1024 bytes.",
                ));
            }
        } else if !self.password.is_empty() {
            return Err(invalid(
                "Choose an encryption method to use an export password.",
            ));
        }
        Ok(())
    }
    pub fn filename(&self) -> String {
        format!(
            "roblox-accounts.{}{}",
            self.format.extension(),
            if self.encryption.recommended() {
                ".rmexport"
            } else {
                ""
            }
        )
    }
}

fn field_value(row: &ExportAccount, field: ExportField) -> Result<String, CoreError> {
    let account = &row.account;
    Ok(match field {
        ExportField::Username => account.username.clone(),
        ExportField::AccountName => account.label().to_string(),
        ExportField::DisplayName => account.display_name.clone(),
        ExportField::UserId => account.user_id.to_string(),
        ExportField::Alias => account.alias.clone(),
        ExportField::Group => account.group.clone(),
        ExportField::CreatedAt => account
            .created_at
            .map(|date| date.to_rfc3339())
            .unwrap_or_default(),
        ExportField::AccountAge => account
            .created_at
            .map(|date| {
                chrono::Utc::now()
                    .signed_duration_since(date)
                    .num_hours()
                    .max(0)
                    .to_string()
            })
            .unwrap_or_default(),
        ExportField::LastUsed => account
            .last_used
            .map(|date| date.to_rfc3339())
            .unwrap_or_default(),
        ExportField::Cookie => row
            .cookie
            .as_ref()
            .filter(|cookie| !cookie.is_empty())
            .ok_or_else(|| invalid("An account credential is unavailable. No export was written."))?
            .clone(),
    })
}

fn csv_cell(value: &str) -> String {
    let dangerous = value.trim_start().starts_with(['=', '+', '-', '@'])
        || value.starts_with(['\t', '\r', '\n']);
    format!(
        "\"{}{}\"",
        if dangerous { "'" } else { "" },
        value.replace('"', "\"\"")
    )
}

pub fn encode(options: &ExportOptions, rows: &[ExportAccount]) -> Result<Vec<u8>, CoreError> {
    options.validate()?;
    if rows.len() != options.account_ids.len()
        || rows
            .iter()
            .zip(&options.account_ids)
            .any(|(row, id)| row.account.user_id != *id)
    {
        return Err(invalid(
            "The selected accounts changed. Refresh before exporting.",
        ));
    }
    let values = rows
        .iter()
        .map(|row| {
            options
                .fields
                .iter()
                .map(|field| field_value(row, *field))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let headers: Vec<_> = options.fields.iter().map(|field| field.header()).collect();
    let plaintext = match options.format {
        ExportFormat::Json => {
            let objects: Vec<serde_json::Map<String, serde_json::Value>> = values
                .iter()
                .map(|row| {
                    headers
                        .iter()
                        .zip(row)
                        .map(|(header, value)| {
                            (header.to_string(), serde_json::Value::String(value.clone()))
                        })
                        .collect()
                })
                .collect();
            serde_json::to_vec_pretty(&objects)
                .map_err(|_| invalid("The export could not be encoded."))?
        }
        ExportFormat::Csv => {
            let mut lines = vec![headers.join(",")];
            lines.extend(values.iter().map(|row| {
                row.iter()
                    .map(|value| csv_cell(value))
                    .collect::<Vec<_>>()
                    .join(",")
            }));
            lines.join("\r\n").into_bytes()
        }
        ExportFormat::Txt => values
            .iter()
            .map(|row| {
                headers
                    .iter()
                    .zip(row)
                    .map(|(header, value)| {
                        format!(
                            "{header}: {}",
                            value.replace('\r', "\\r").replace('\n', "\\n")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n\n")
            .into_bytes(),
    };
    if plaintext.len() > MAX_EXPORT_BYTES {
        return Err(invalid("The selected export is too large."));
    }
    if options.encryption.recommended() {
        crypto::export::seal(
            &plaintext,
            &options.password,
            options.encryption,
            options.format,
        )
    } else {
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> ExportOptions {
        ExportOptions {
            account_ids: vec![1],
            fields: vec![ExportField::Username],
            format: ExportFormat::Json,
            encryption: Encryption::None,
            password: String::new(),
        }
    }
    fn rows() -> Vec<ExportAccount> {
        let mut account = Account::new(1, "synthetic_user".into(), "Display".into());
        account.alias = "Alias".into();
        vec![ExportAccount {
            account,
            cookie: Some("SYNTHETIC-EXPORT-COOKIE-NOT-REAL".into()),
        }]
    }

    #[test]
    fn cookie_policy_rejects_plaintext_missing_passwords_and_invalid_selection() {
        let mut request = options();
        request.fields.push(ExportField::Cookie);
        assert!(request.validate().is_err());
        for method in [Encryption::Aes256Gcm, Encryption::Aes128Gcm] {
            request.encryption = method;
            assert!(request.validate().is_err());
            request.password = "a sufficient synthetic password".into();
            assert!(request.validate().is_ok());
            request.password.clear();
        }
        request.password = "a sufficient synthetic password".into();
        request.account_ids = vec![1, 1];
        assert!(request.validate().is_err());
        request.account_ids = vec![0];
        assert!(request.validate().is_err());
        request.account_ids = vec![1];
        request.fields.clear();
        assert!(request.validate().is_err());
        request.fields = vec![ExportField::Username, ExportField::Username];
        assert!(request.validate().is_err());
    }

    #[test]
    fn all_plain_formats_include_only_explicit_fields_and_selected_accounts() {
        for format in [ExportFormat::Json, ExportFormat::Csv, ExportFormat::Txt] {
            let mut request = options();
            request.format = format;
            let text = String::from_utf8(encode(&request, &rows()).unwrap()).unwrap();
            assert!(text.contains("synthetic_user"));
            assert!(!text.contains("SYNTHETIC-EXPORT-COOKIE"));
            assert!(!text.contains("Display"));
            assert!(!text.contains("Alias"));
            if format == ExportFormat::Json {
                let payload: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(payload.as_array().unwrap().len(), 1);
                assert_eq!(payload[0].as_object().unwrap().len(), 1);
                assert_eq!(payload[0]["username"], "synthetic_user");
            }
            request.account_ids = vec![2];
            assert!(encode(&request, &rows()).is_err());
        }
    }

    #[test]
    fn spreadsheet_formulas_and_multiline_text_cannot_inject_extra_rows() {
        for value in ["=1+2", " +SUM(1,2)", "-1", "@command", "\tvalue"] {
            assert!(csv_cell(value).starts_with("\"'"));
        }
        assert_eq!(csv_cell("a,\"b\""), "\"a,\"\"b\"\"\"");
        let mut request = options();
        request.format = ExportFormat::Txt;
        let mut accounts = rows();
        accounts[0].account.username = "name\nforged: value\r".into();
        let text = String::from_utf8(encode(&request, &accounts).unwrap()).unwrap();
        assert_eq!(text, "username: name\\nforged: value\\r");
    }

    #[test]
    fn both_recommended_methods_recover_all_formats_without_plaintext_on_disk() {
        for method in [Encryption::Aes256Gcm, Encryption::Aes128Gcm] {
            for format in [ExportFormat::Json, ExportFormat::Csv, ExportFormat::Txt] {
                let mut request = options();
                request.format = format;
                request.encryption = method;
                request.fields = vec![ExportField::Username, ExportField::Cookie];
                request.password = "synthetic password for export".into();
                let encrypted = encode(&request, &rows()).unwrap();
                let directory =
                    std::env::temp_dir().join(format!("rm-export-test-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir(&directory).unwrap();
                let path = directory.join(request.filename());
                crate::storage::atomic_swap(&path, &encrypted).unwrap();
                assert_eq!(std::fs::read(&path).unwrap(), encrypted);
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                let output = std::str::from_utf8(&encrypted).unwrap();
                assert!(!output.contains("SYNTHETIC-EXPORT-COOKIE"));
                assert!(!output.contains("synthetic_user"));
                assert!(!output.contains(&request.password));
                let (decoded_format, plaintext) =
                    crypto::export::open(&encrypted, &request.password).unwrap();
                assert_eq!(decoded_format, format);
                assert!(String::from_utf8(plaintext)
                    .unwrap()
                    .contains("SYNTHETIC-EXPORT-COOKIE-NOT-REAL"));
                assert!(crypto::export::open(&encrypted, "wrong password").is_err());
                std::fs::remove_file(path).unwrap();
                std::fs::remove_dir(directory).unwrap();
            }
        }
    }
}
