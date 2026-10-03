use crate::{Account, AppConfig, SettingsConfig};
use serde_json::{json, Value};

const LOCKED_MESSAGE: &str =
    "Demo mode is read-only. Live accounts, launches and changes are disabled.";

fn accounts() -> Vec<Value> {
    [
        (900_001, "builderman", "builderman"),
        (900_002, "roblox", "Roblox"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (user_id, username, display_name))| {
        let mut account = Account::new(user_id, username.into(), display_name.into());
        account.group = "Demo accounts".into();
        account.avatar_url = format!("/demo-avatars/{username}.png");
        account.sort_order = index as u32;
        let mut summary = crate::account_summary(&account, None, &AppConfig::default());
        summary.can_launch = false;
        summary.created_at = Some("2024-01-01T00:00:00Z".into());
        summary.presence_text = "Demo account".into();
        json!(summary)
    })
    .collect()
}

fn response(command: &str) -> Result<Value, &'static str> {
    Ok(match command {
        "list_accounts" | "revalidate_accounts" => json!(accounts()),
        "store_status" | "unlock_device" => json!({
            "exists": true, "unlocked": true, "needsPassword": false,
            "legacy": false, "accountCount": 2
        }),
        "startup_status" => json!({
            "needsTutorial": false, "changelog": null, "passwordlessOffer": false,
            "legacyMigrationAvailable": false, "migrationNotice": null
        }),
        "acknowledge_startup" | "check_release_update" => Value::Null,
        "get_settings" => {
            let config = AppConfig {
                developer_options: true,
                ..AppConfig::default()
            };
            json!({
                "config": SettingsConfig::from_config(&config),
                "appVersion": concat!(env!("CARGO_PKG_VERSION"), " (Demo)"),
                "systemArchitecture": std::env::consts::ARCH,
                "monitors": [], "hasPassword": false, "hasDiscordWebhook": false,
                "robloxRunning": false, "infoCards": {}
            })
        }
        "list_account_groups" => json!([{
            "name": "Demo accounts", "color": [107, 112, 128], "sortOrder": 0
        }]),
        "list_account_group_colors" => json!({"Demo accounts": [107, 112, 128]}),
        "list_instances" => json!({"instances": [], "runningCount": 0}),
        "get_client_launch_settings" => {
            json!(crate::client_settings::ClientLaunchSettings::default())
        }
        "list_private_servers" => json!([{
            "index": 0, "name": "Demo hangout", "placeId": 123456789,
            "placeName": "Example experience", "iconUrl": "", "url": "https://example.invalid/demo"
        }]),
        "list_launch_presets" => json!([{
            "index": 0, "name": "Demo adventure", "placeId": 123456789,
            "iconUrl": "", "jobId": null, "data": null
        }]),
        "fetch_account_inventory" => json!([{
            "assetId": 900001, "name": "Demo builder's hat", "assetType": "Hat",
            "iconUrl": "/demo-avatars/builderman.png", "priceRobux": 100
        }]),
        "search_connection_users" => json!([]),
        "refresh_account_presence" => json!([
            {"userId": 900001, "presence": "neutral", "presenceText": "Demo account", "location": ""},
            {"userId": 900002, "presence": "neutral", "presenceText": "Demo account", "location": ""}
        ]),
        "search_groups" => json!([{
            "id": 900001, "name": "Demo builders", "description": "An example community.",
            "memberCount": 128, "hasVerifiedBadge": false
        }]),
        "load_group" => json!({
            "group": {
                "id": 900001, "name": "Demo builders", "description": "Build something brilliant together.",
                "memberCount": 128, "publicEntryAllowed": true, "hasVerifiedBadge": false,
                "hasSocialModules": false, "communityTier": null, "created": null,
                "shout": null, "owner": {"id": 900001, "username": "builderman", "displayName": "builderman"}
            },
            "iconDataUrl": null,
            "announcement": {"title": "Welcome, builders", "body": "This community is sample data for screenshots.",
                "created": null, "imageUrl": null, "reactions": []},
            "forums": [], "forumStatus": "Demo mode is read-only.",
            "memberships": [{"userId": 900001, "joined": true, "roleName": "Owner", "roleRank": 255},
                {"userId": 900002, "joined": true, "roleName": "Member", "roleRank": 1}]
        }),
        "list_asset_workspace" => json!({
            "rows": [], "isUploading": false, "isReadOnly": true, "notice": LOCKED_MESSAGE
        }),
        "list_asset_creations" => json!({"rows": [], "nextCursor": null}),
        "list_asset_universes" | "list_asset_creators" => json!([]),
        _ => return Err(LOCKED_MESSAGE),
    })
}

pub fn run() {
    let mut context = tauri::generate_context!();
    for window in &mut context.config_mut().app.windows {
        window.create = false;
    }
    tauri::Builder::default()
        .setup(|app| {
            let configuration = app
                .config()
                .app
                .windows
                .first()
                .ok_or("Main window unavailable")?;
            // a fresh profile prevents the demo from inheriting the regular webview's cache or storage
            let profile = std::env::temp_dir().join(format!("rm-demo-{}", uuid::Uuid::new_v4()));
            tauri::WebviewWindowBuilder::from_config(app.handle(), configuration)?
                .data_directory(profile)
                .incognito(true)
                .title("Roblox Manager - Demo")
                .build()?;
            Ok(())
        })
        .invoke_handler(|invoke| {
            match response(invoke.message.command()) {
                Ok(value) => invoke.resolver.resolve(value),
                Err(message) => invoke.resolver.reject(message),
            }
            true
        })
        .run(context)
        .expect("error while running RM demo");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_accounts_have_only_local_avatars_and_cannot_launch() {
        let accounts = accounts();
        assert_eq!(accounts.len(), 2);
        for account in accounts {
            assert_eq!(account["canLaunch"], false);
            assert_eq!(account["group"], "Demo accounts");
            assert!(account["avatarUrl"]
                .as_str()
                .unwrap()
                .starts_with("/demo-avatars/"));
            assert!(account.get("cookie").is_none());
        }
    }

    #[test]
    fn mutations_and_unknown_commands_are_rejected() {
        for command in [
            "add_account",
            "launch_account",
            "browse_as_account",
            "save_settings",
            "reset_account_store",
            "kill_all_accounts",
            "upload_assets",
            "restart_app",
            "open_data_folder",
            "set_startup_with_windows",
            "future_command",
        ] {
            assert_eq!(response(command), Err(LOCKED_MESSAGE));
        }
    }

    #[test]
    fn startup_does_not_request_storage_or_update_flows() {
        assert_eq!(response("startup_status").unwrap()["needsTutorial"], false);
        assert_eq!(response("check_release_update").unwrap(), Value::Null);
        assert_eq!(response("store_status").unwrap()["unlocked"], true);
        assert_eq!(
            response("list_asset_workspace").unwrap()["isReadOnly"],
            true
        );
    }
}
