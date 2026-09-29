use crate::{
    error::CoreError,
    models::{Account, ModerationInfo},
};

fn credential_was_rejected(error: &CoreError) -> bool {
    // forbidden responses can request a challenge without invalidating the session
    matches!(error, CoreError::RobloxApi { status: 401, .. })
}

pub fn apply_validation(
    account: &mut Account,
    validation: Result<(u64, String, String), CoreError>,
) -> bool {
    match validation {
        Ok((user_id, username, display_name)) if user_id == account.user_id => {
            account.username = username;
            account.display_name = display_name;
            account.cookie_expired = false;
            account.last_validated = Some(chrono::Utc::now());
        }
        Ok(_) => account.cookie_expired = true,
        Err(ref error) if credential_was_rejected(error) => account.cookie_expired = true,
        Err(_) => return false,
    }
    true
}

pub fn merge_moderation(
    previous: Option<ModerationInfo>,
    current: Option<ModerationInfo>,
) -> Option<ModerationInfo> {
    current.map(|mut current| {
        if let Some(previous) = previous {
            if current.reason.is_none() {
                current.reason = previous.reason;
            }
            if current.expires_at.is_none() {
                current.expires_at = previous.expires_at;
            }
        }
        current
    })
}

pub fn merge_replacement(existing: &Account, account: Account) -> Account {
    let mut updated = existing.clone();
    updated.username = account.username;
    updated.display_name = account.display_name;
    updated.encrypted_cookie = account.encrypted_cookie;
    updated.cookie_expired = account.cookie_expired;
    updated.last_validated = account.last_validated;
    updated.moderation = account.moderation;
    if account.created_at.is_some() {
        updated.created_at = account.created_at;
    }
    if !account.avatar_url.is_empty() {
        updated.avatar_url = account.avatar_url;
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_refresh_preserves_all_cached_account_data() {
        let mut account = Account::new(1, "Cached".into(), "Cached name".into());
        account.last_validated = Some(chrono::Utc::now());
        account.cookie_expired = true;
        account.moderation = Some(ModerationInfo {
            reason: Some("Synthetic restriction".into()),
            ..Default::default()
        });
        let previous = serde_json::to_value(&account).unwrap();
        assert!(!apply_validation(&mut account, Err(CoreError::RateLimited)));
        assert_eq!(serde_json::to_value(&account).unwrap(), previous);
    }

    #[test]
    fn moderation_refresh_keeps_missing_details_but_can_clear_a_restriction() {
        let previous = ModerationInfo {
            reason: Some("Synthetic restriction".into()),
            expires_at: Some(chrono::Utc::now()),
            ..Default::default()
        };
        let refreshed =
            merge_moderation(Some(previous.clone()), Some(ModerationInfo::default())).unwrap();
        assert_eq!(refreshed.reason, previous.reason);
        assert_eq!(refreshed.expires_at, previous.expires_at);
        assert!(merge_moderation(Some(previous.clone()), None).is_none());
        let current = ModerationInfo {
            reason: Some("New restriction".into()),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            ..Default::default()
        };
        let refreshed = merge_moderation(Some(previous), Some(current.clone())).unwrap();
        assert_eq!(refreshed.reason, current.reason);
        assert_eq!(refreshed.expires_at, current.expires_at);
    }

    #[test]
    fn replacement_preserves_cached_metadata_until_new_values_are_available() {
        let mut existing = Account::new(1, "Old".into(), "Old".into());
        existing.avatar_url = "https://example.invalid/cached.png".into();
        existing.created_at = Some(chrono::Utc::now());
        existing.friends_cache.insert(2);
        let replacement = Account::new(1, "New".into(), "New".into());
        let updated = merge_replacement(&existing, replacement.clone());
        assert_eq!(updated.avatar_url, existing.avatar_url);
        assert_eq!(updated.created_at, existing.created_at);
        assert_eq!(updated.friends_cache, existing.friends_cache);
        let replacement = Account {
            avatar_url: "https://example.invalid/new.png".into(),
            created_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            ..replacement
        };
        let updated = merge_replacement(&existing, replacement.clone());
        assert_eq!(updated.avatar_url, replacement.avatar_url);
        assert_eq!(updated.created_at, replacement.created_at);
    }

    #[test]
    fn replacing_credentials_keeps_account_organisation() {
        let mut existing = Account::new(1, "old".into(), "Old".into());
        existing.alias = "Main".into();
        existing.group = "Farm".into();
        existing.is_pinned = true;
        existing.sort_order = 3;
        existing.last_used = Some(chrono::Utc::now());
        let updated = merge_replacement(&existing, Account::new(1, "new".into(), "New".into()));
        assert_eq!(updated.username, "new");
        assert_eq!(updated.alias, existing.alias);
        assert_eq!(updated.group, existing.group);
        assert!(updated.is_pinned);
        assert_eq!(updated.sort_order, 3);
        assert_eq!(updated.last_used, existing.last_used);
    }

    #[test]
    fn transient_errors_do_not_expire_credentials() {
        assert!(!credential_was_rejected(&CoreError::RateLimited));
        assert!(!credential_was_rejected(&CoreError::RobloxApi {
            status: 503,
            message: String::new()
        }));
        assert!(credential_was_rejected(&CoreError::RobloxApi {
            status: 401,
            message: String::new()
        }));
    }

    #[test]
    fn forbidden_and_csrf_failures_do_not_expire_credentials() {
        let failures = [
            CoreError::CookieRejected,
            CoreError::CookieRejectedWithReason("Synthetic challenge required".into()),
            CoreError::AuthFailed("403 Forbidden after CSRF retries".into()),
            CoreError::RobloxApi {
                status: 403,
                message: String::new(),
            },
        ];
        for failure in failures {
            assert!(!credential_was_rejected(&failure));
        }
    }

    #[test]
    fn validation_changes_credential_status_only_with_an_authentication_verdict() {
        let mut account = Account::new(1, "Original".into(), "Original".into());
        assert!(!apply_validation(
            &mut account,
            Err(CoreError::CookieRejected)
        ));
        assert!(!account.cookie_expired);
        assert!(account.last_validated.is_none());
        assert!(apply_validation(
            &mut account,
            Err(CoreError::RobloxApi {
                status: 401,
                message: String::new(),
            })
        ));
        assert!(account.cookie_expired);
        assert!(!apply_validation(
            &mut account,
            Err(CoreError::CookieRejected)
        ));
        assert!(account.cookie_expired);
        assert!(apply_validation(
            &mut account,
            Ok((1, "Updated".into(), "Updated name".into()))
        ));
        assert!(!account.cookie_expired);
        assert_eq!(account.username, "Updated");
        assert!(account.last_validated.is_some());
        assert!(apply_validation(
            &mut account,
            Ok((2, "Other user".into(), "Other".into()))
        ));
        assert!(account.cookie_expired);
        assert_eq!(account.user_id, 1);
        assert_eq!(account.username, "Updated");
    }
}
