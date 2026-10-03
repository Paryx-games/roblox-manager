#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(rm_demo, not(debug_assertions)))]
compile_error!("Demo mode is only available in debug builds");

#[cfg(debug_assertions)]
mod demo;

mod accounts;
mod asset_manager;
mod background;
mod benchmark;
mod instances;
mod launcher;
mod lifecycle;
mod login;
mod state;
mod webview_recovery;

#[allow(dead_code)]
mod browser_login;

mod startup;

use base64::Engine;
use ram_core::api::{parse_private_server_url, ParsedPrivateServerUrl};
use ram_core::crypto;
use ram_core::group_api;
use ram_core::models::{
    Account, AccountStore, AppConfig, GroupMeta, LaunchPreset, LogLevel, MonitorGeometry,
    MonitorTarget, Presence, PrivateServer, TilingLayoutMode, TilingOptions,
};
use ram_core::{api, assets_api, auth::RobloxClient, process};
use serde::{Deserialize, Serialize};
use state::AppState;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};
use tracing_subscriber::fmt::writer::{MakeWriter, MakeWriterExt};
use tracing_subscriber::EnvFilter;

const LOG_FILES_KEPT: usize = 7;

fn log_appender(data_dir: &Path) -> Option<tracing_appender::rolling::RollingFileAppender> {
    tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("rm")
        .filename_suffix("log")
        .max_log_files(LOG_FILES_KEPT)
        .build(data_dir)
        .ok()
}

struct Scrubbed<M>(M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Scrubbed<M> {
    type Writer = ScrubbingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        ScrubbingWriter::new(self.0.make_writer())
    }

    fn make_writer_for(&'a self, metadata: &tracing::Metadata<'_>) -> Self::Writer {
        ScrubbingWriter::new(self.0.make_writer_for(metadata))
    }
}

struct ScrubbingWriter<W: Write> {
    inner: W,
    buffer: Vec<u8>,
}

impl<W: Write> ScrubbingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
        }
    }
}

impl<W: Write> Write for ScrubbingWriter<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&self.buffer);
        self.inner
            .write_all(ram_core::redact::scrub(&text).as_bytes())?;
        self.buffer.clear();
        self.inner.flush()
    }
}

impl<W: Write> Drop for ScrubbingWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn init_logging() {
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let data_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RM");

    if std::fs::create_dir_all(&data_dir).is_ok() {
        if let Some(appender) = log_appender(&data_dir) {
            let subscriber = tracing_subscriber::fmt()
                .with_env_filter(filter())
                .with_target(false)
                .with_ansi(false);
            if cfg!(debug_assertions) {
                subscriber
                    .with_writer(Scrubbed(appender.and(std::io::stderr)))
                    .init();
            } else {
                subscriber.with_writer(Scrubbed(appender)).init();
            }
            return;
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_target(false)
        .with_writer(Scrubbed(std::io::stderr))
        .init();
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummary {
    user_id: u64,
    label: String,
    username: String,
    display_name: String,
    alias: String,
    group: String,
    avatar_url: String,
    is_pinned: bool,
    sort_order: u32,
    cookie_expired: bool,
    moderation_active: bool,
    moderation_banned: bool,
    moderation_reason: Option<String>,
    moderation_expires_at: Option<String>,
    player_path: Option<String>,
    created_at: Option<String>,
    presence: &'static str,
    presence_text: String,
    presence_location: String,
    can_launch: bool,
    last_activity: Option<String>,
    last_used: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreStatus {
    exists: bool,
    unlocked: bool,
    needs_password: bool,
    legacy: bool,
    account_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryItem {
    asset_id: u64,
    name: String,
    asset_type: String,
    icon_url: Option<String>,
    price_robux: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InventoryBrowserTarget {
    user_id: u64,
    asset_id: u64,
}

fn inventory_browser_destination(
    target: &InventoryBrowserTarget,
) -> Result<(String, String), String> {
    if target.user_id == 0 || target.asset_id == 0 {
        return Err("Select a valid account and marketplace item".to_string());
    }
    Ok((
        format!(
            "inventory-marketplace-{}-{}",
            target.user_id, target.asset_id
        ),
        format!("https://www.roblox.com/catalog/{}", target.asset_id),
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionSearchResult {
    user_id: u64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresenceUpdate {
    user_id: u64,
    presence: &'static str,
    presence_text: String,
    location: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountGroupSummary {
    name: String,
    color: [u8; 3],
    sort_order: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivateServerSummary {
    index: usize,
    name: String,
    place_id: u64,
    place_name: String,
    icon_url: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchPresetSummary {
    index: usize,
    name: String,
    place_id: u64,
    icon_url: String,
    job_id: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsUpdate {
    use_credential_manager: bool,
    refresh_on_startup: bool,
    auto_launch_on_startup: bool,
    auto_launch_account_id: Option<u64>,
    multi_instance_enabled: bool,
    kill_background_roblox: bool,
    confirm_kill_all: bool,
    launch_delay_secs: u32,
    custom_game_args: String,
    roblox_player_path: Option<String>,
    roblox_fast_flags: std::collections::HashMap<String, String>,
    privacy_mode: bool,
    privacy_clean_cookies: bool,
    privacy_clean_local_storage: bool,
    privacy_clean_full_profile: bool,
    privacy_clean_on_exit: bool,
    privacy_clear_clipboard: bool,
    mac_rotation_enabled: bool,
    mac_preserve_oui: bool,
    mac_alternate_oui: String,
    auto_arrange_windows: bool,
    tiling_target_monitor: MonitorTarget,
    tiling_layout_mode: TilingLayoutMode,
    tiling_custom_cols: u32,
    tiling_custom_rows: u32,
    tiling_padding: u32,
    rename_roblox_windows: bool,
    anonymize_names: bool,
    developer_options: bool,
    utility_enabled: bool,
    log_level: LogLevel,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsConfig {
    use_credential_manager: bool,
    startup_with_windows: bool,
    refresh_on_startup: bool,
    auto_launch_on_startup: bool,
    auto_launch_account_id: Option<u64>,
    multi_instance_enabled: bool,
    kill_background_roblox: bool,
    confirm_kill_all: bool,
    launch_delay_secs: u32,
    custom_game_args: String,
    roblox_player_path: Option<String>,
    roblox_fast_flags: std::collections::HashMap<String, String>,
    privacy_mode: bool,
    privacy_clean_cookies: bool,
    privacy_clean_local_storage: bool,
    privacy_clean_full_profile: bool,
    privacy_clean_on_exit: bool,
    privacy_clear_clipboard: bool,
    mac_rotation_enabled: bool,
    mac_preserve_oui: bool,
    mac_alternate_oui: String,
    auto_arrange_windows: bool,
    tiling_target_monitor: MonitorTarget,
    tiling_layout_mode: TilingLayoutMode,
    tiling_custom_cols: u32,
    tiling_custom_rows: u32,
    tiling_padding: u32,
    rename_roblox_windows: bool,
    anonymize_names: bool,
    developer_options: bool,
    utility_enabled: bool,
    log_level: LogLevel,
}

impl SettingsConfig {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            use_credential_manager: config.use_credential_manager,
            startup_with_windows: config.startup_with_windows,
            refresh_on_startup: config.refresh_on_startup,
            auto_launch_on_startup: config.auto_launch_on_startup,
            auto_launch_account_id: config.auto_launch_account_id,
            multi_instance_enabled: config.multi_instance_enabled,
            kill_background_roblox: config.kill_background_roblox,
            confirm_kill_all: config.confirm_kill_all,
            launch_delay_secs: config.launch_delay_secs,
            custom_game_args: config.custom_game_args.clone(),
            roblox_player_path: config
                .roblox_player_path
                .as_ref()
                .map(|path| path.display().to_string()),
            roblox_fast_flags: config.roblox_fast_flags.clone(),
            privacy_mode: config.privacy_mode,
            privacy_clean_cookies: config.privacy_clean_cookies,
            privacy_clean_local_storage: config.privacy_clean_local_storage,
            privacy_clean_full_profile: config.privacy_clean_full_profile,
            privacy_clean_on_exit: config.privacy_clean_on_exit,
            privacy_clear_clipboard: config.privacy_clear_clipboard,
            mac_rotation_enabled: config.mac_rotation_enabled,
            mac_preserve_oui: config.mac_preserve_oui,
            mac_alternate_oui: config.mac_alternate_oui.clone(),
            auto_arrange_windows: config.auto_arrange_windows,
            tiling_target_monitor: config.tiling_target_monitor.clone(),
            tiling_layout_mode: config.tiling_layout_mode.clone(),
            tiling_custom_cols: config.tiling_custom_cols,
            tiling_custom_rows: config.tiling_custom_rows,
            tiling_padding: config.tiling_padding,
            rename_roblox_windows: config.rename_roblox_windows,
            anonymize_names: config.anonymize_names,
            developer_options: config.developer_options,
            utility_enabled: config.utility_enabled,
            log_level: config.log_level,
        }
    }
}

impl SettingsUpdate {
    fn apply_to_config(self, config: &mut AppConfig) -> Result<(), String> {
        if self.custom_game_args.chars().count() > 4096 || self.custom_game_args.contains('\0') {
            return Err("Custom Roblox arguments must be 4096 characters or fewer".to_string());
        }
        if self
            .roblox_player_path
            .as_deref()
            .is_some_and(|path| path.chars().count() > 32768 || path.contains('\0'))
        {
            return Err(
                "Roblox player path is too long or contains an invalid character".to_string(),
            );
        }
        if self.roblox_fast_flags.len() > 100
            || self.roblox_fast_flags.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 128
                    || !key
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    || value.len() > 4096
                    || value.contains(['\0', '\n', '\r'])
            })
        {
            return Err("Fast flags must have valid names and values".to_string());
        }
        if self.launch_delay_secs > 300 {
            return Err("Launch delay must be between 0 and 300 seconds".to_string());
        }
        if self.tiling_custom_cols == 0 || self.tiling_custom_cols > 12 {
            return Err("Tiling columns must be between 1 and 12".to_string());
        }
        if self.tiling_custom_rows == 0 || self.tiling_custom_rows > 12 {
            return Err("Tiling rows must be between 1 and 12".to_string());
        }
        if self.tiling_padding > 50 {
            return Err("Window padding must be between 0 and 50 pixels".to_string());
        }
        if !self.log_level.allowed_in_profile() {
            return Err("That log level is unavailable in this build".to_string());
        }
        if !valid_mac_oui(&self.mac_alternate_oui) {
            return Err("Alternate OUI must use the format 00:1B:21".to_string());
        }

        config.use_credential_manager = self.use_credential_manager;
        config.refresh_on_startup = self.refresh_on_startup;
        config.auto_launch_on_startup = self.auto_launch_on_startup;
        config.auto_launch_account_id = self.auto_launch_account_id;
        config.multi_instance_enabled = self.multi_instance_enabled;
        config.kill_background_roblox = self.kill_background_roblox;
        config.confirm_kill_all = self.confirm_kill_all;
        config.launch_delay_secs = self.launch_delay_secs;
        config.custom_game_args = self.custom_game_args;
        config.roblox_fast_flags = self.roblox_fast_flags;
        config.roblox_player_path = self
            .roblox_player_path
            .filter(|path| !path.trim().is_empty())
            .map(std::path::PathBuf::from);
        config.privacy_mode = self.privacy_mode;
        config.privacy_clean_cookies = self.privacy_clean_cookies;
        config.privacy_clean_local_storage = self.privacy_clean_local_storage;
        config.privacy_clean_full_profile = self.privacy_clean_full_profile;
        config.privacy_clean_on_exit = self.privacy_clean_on_exit;
        config.privacy_clear_clipboard = self.privacy_clear_clipboard;
        config.mac_rotation_enabled = self.mac_rotation_enabled;
        config.mac_preserve_oui = self.mac_preserve_oui;
        config.mac_alternate_oui = self.mac_alternate_oui;
        config.auto_arrange_windows = self.auto_arrange_windows;
        config.tiling_target_monitor = self.tiling_target_monitor;
        config.tiling_layout_mode = self.tiling_layout_mode;
        config.tiling_custom_cols = self.tiling_custom_cols;
        config.tiling_custom_rows = self.tiling_custom_rows;
        config.tiling_padding = self.tiling_padding;
        config.rename_roblox_windows = self.rename_roblox_windows;
        config.anonymize_names = self.anonymize_names;
        config.developer_options = self.developer_options;
        config.utility_enabled = self.utility_enabled;
        config.log_level = self.log_level;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsInfoCard {
    kind: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSnapshot {
    config: SettingsConfig,
    app_version: &'static str,
    system_architecture: &'static str,
    monitors: Vec<MonitorGeometry>,
    has_password: bool,
    has_discord_webhook: bool,
    roblox_running: bool,
    info_cards: std::collections::HashMap<String, SettingsInfoCard>,
}

#[derive(Debug, Deserialize)]
struct RawSettingsInfoCard {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupSearchResultDto {
    id: u64,
    name: String,
    description: String,
    member_count: u64,
    has_verified_badge: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupPosterDto {
    username: String,
    display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupShoutDto {
    body: String,
    created: Option<String>,
    poster: Option<GroupPosterDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupAnnouncementDto {
    title: String,
    body: String,
    created: Option<String>,
    image_url: Option<String>,
    reactions: Vec<GroupReactionDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupReactionDto {
    label: String,
    count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupForumCategoryDto {
    id: String,
    name: String,
    posts: Vec<GroupForumPostDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupForumPostDto {
    id: String,
    title: String,
    body: String,
    created: Option<String>,
    author: Option<String>,
    comment_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupOwnerDto {
    id: u64,
    username: String,
    display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupInfoDto {
    id: u64,
    name: String,
    description: String,
    member_count: u64,
    public_entry_allowed: bool,
    has_verified_badge: bool,
    has_social_modules: bool,
    community_tier: Option<u8>,
    created: Option<String>,
    shout: Option<GroupShoutDto>,
    owner: Option<GroupOwnerDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupMembershipDto {
    user_id: u64,
    joined: bool,
    role_name: Option<String>,
    role_rank: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupWorkspaceDto {
    group: GroupInfoDto,
    icon_data_url: Option<String>,
    announcement: Option<GroupAnnouncementDto>,
    forums: Vec<GroupForumCategoryDto>,
    forum_status: Option<String>,
    memberships: Vec<GroupMembershipDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupMembershipResultDto {
    user_id: u64,
    join: bool,
    ok: bool,
    challenge: bool,
    message: Option<String>,
}

fn private_server_summary(
    index: usize,
    server: &PrivateServer,
    icon_url: String,
) -> PrivateServerSummary {
    PrivateServerSummary {
        index,
        name: server.name.clone(),
        place_id: server.place_id,
        place_name: server.place_name.clone(),
        icon_url,
        url: format!(
            "https://www.roblox.com/games/{}/game?privateServerLinkCode={}",
            server.place_id, server.link_code
        ),
    }
}

fn group_poster_dto(poster: &group_api::GroupPoster) -> GroupPosterDto {
    GroupPosterDto {
        username: poster.username.clone(),
        display_name: poster.display_name.clone(),
    }
}

fn group_info_dto(group: &group_api::GroupInfo) -> GroupInfoDto {
    GroupInfoDto {
        id: group.id,
        name: group.name.clone(),
        description: group.description.clone(),
        member_count: group.member_count,
        public_entry_allowed: group.public_entry_allowed,
        has_verified_badge: group.has_verified_badge,
        has_social_modules: group.has_social_modules,
        community_tier: group.community_tier,
        created: group.created.map(|date| date.to_rfc3339()),
        shout: group.shout.as_ref().map(|shout| GroupShoutDto {
            body: shout.body.clone(),
            created: shout.created.map(|date| date.to_rfc3339()),
            poster: shout.poster.as_ref().map(group_poster_dto),
        }),
        owner: group.owner.as_ref().map(|owner| GroupOwnerDto {
            id: owner.id,
            username: owner.username.clone(),
            display_name: owner.display_name.clone(),
        }),
    }
}

fn group_announcement_dto(
    announcement: group_api::LatestGroupAnnouncement,
) -> GroupAnnouncementDto {
    GroupAnnouncementDto {
        title: announcement.title,
        body: announcement.body,
        created: announcement.created,
        image_url: announcement.image_url,
        reactions: announcement
            .reactions
            .into_iter()
            .map(|reaction| GroupReactionDto {
                label: reaction.label,
                count: reaction.count,
            })
            .collect(),
    }
}

fn group_forum_category_dto(category: group_api::GroupForumCategory) -> GroupForumCategoryDto {
    GroupForumCategoryDto {
        id: category.id,
        name: category.name,
        posts: category
            .posts
            .into_iter()
            .map(|post| GroupForumPostDto {
                id: post.id,
                title: post.title,
                body: post.body,
                created: post.created,
                author: post.author,
                comment_count: post.comment_count,
            })
            .collect(),
    }
}

fn group_membership_dto(membership: &group_api::GroupMembership) -> GroupMembershipDto {
    GroupMembershipDto {
        user_id: membership.user_id,
        joined: membership.joined,
        role_name: membership.role_name.clone(),
        role_rank: membership.role_rank,
    }
}

async fn enrich_private_server(server: &mut PrivateServer) -> String {
    let client = match RobloxClient::new() {
        Ok(client) => client,
        Err(_) => return String::new(),
    };
    if server.universe_id.is_none() {
        if let Ok(universe_id) =
            assets_api::resolve_place_universe(&client, "", server.place_id).await
        {
            server.universe_id = Some(universe_id);
        }
    }
    let Some(universe_id) = server.universe_id else {
        return String::new();
    };
    if server.place_name.is_empty() {
        if let Ok(place_name) = api::resolve_universe_name(&client, universe_id).await {
            server.place_name = place_name;
        }
    }
    if let Ok(icons) = api::fetch_game_icons(&client, "", &[universe_id]).await {
        if let Some((_, icon_url)) = icons.into_iter().next() {
            return icon_url;
        }
    }
    String::new()
}

fn account_cookie(runtime: &state::RuntimeState, user_id: u64) -> Result<String, String> {
    if !runtime.unlocked {
        return Err("Account store is locked".to_string());
    }
    let account = runtime
        .accounts
        .find_by_id(user_id)
        .ok_or_else(|| "Account not found".to_string())?;
    if runtime.config.use_credential_manager {
        crypto::credential_load(user_id).map_err(|error| error.to_string())
    } else {
        let encrypted = account
            .encrypted_cookie
            .as_deref()
            .ok_or_else(|| "This account has no stored credential".to_string())?;
        let session = runtime
            .session
            .as_ref()
            .ok_or_else(|| "Account store is locked".to_string())?;
        crypto::decrypt_cookie(encrypted, session).map_err(|error| error.to_string())
    }
}

fn account_summary(
    account: &Account,
    player_path: Option<String>,
    config: &AppConfig,
) -> AccountSummary {
    let anonymous_label = format!("Account {}", account.user_id);
    AccountSummary {
        user_id: account.user_id,
        label: if config.anonymize_names {
            anonymous_label.clone()
        } else {
            account.label().to_string()
        },
        username: if config.anonymize_names {
            anonymous_label.clone()
        } else {
            account.username.clone()
        },
        display_name: if config.anonymize_names {
            anonymous_label.clone()
        } else {
            account.display_name.clone()
        },
        alias: if config.anonymize_names {
            String::new()
        } else {
            account.alias.clone()
        },
        group: account.group.clone(),
        avatar_url: if config.anonymize_names {
            String::new()
        } else {
            account.avatar_url.clone()
        },
        is_pinned: account.is_pinned,
        sort_order: account.sort_order,
        cookie_expired: account.cookie_expired,
        moderation_active: account
            .moderation
            .as_ref()
            .is_some_and(|info| info.is_active()),
        moderation_banned: account
            .moderation
            .as_ref()
            .is_some_and(|info| info.is_banned),
        moderation_reason: account.moderation.as_ref().and_then(|info| {
            info.reason
                .as_deref()
                .map(|reason| ram_core::redact::scrub(reason).into_owned())
        }),
        moderation_expires_at: account
            .moderation
            .as_ref()
            .and_then(|info| info.expires_at.map(|value| value.to_rfc3339())),
        player_path,
        created_at: account.created_at.map(|value| value.to_rfc3339()),
        presence: presence_kind(&account.last_presence),
        presence_text: account.last_presence.status_text().to_string(),
        presence_location: account.last_presence.last_location.clone(),
        can_launch: !account.cookie_expired && account.can_launch(),
        last_used: account.last_used.map(|timestamp| timestamp.to_rfc3339()),
        last_activity: account
            .last_used
            .or(account.last_validated)
            .map(|timestamp| timestamp.to_rfc3339()),
    }
}

#[tauri::command]
async fn store_status(state: tauri::State<'_, AppState>) -> Result<StoreStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let exists = runtime.config.accounts_path.is_file();
        let needs_password = if exists {
            crypto::peek_mode(&runtime.config.accounts_path)
                .map_err(|_| "Account store status unavailable".to_string())?
                .is_some_and(|mode| mode == crypto::StoreMode::Password)
        } else {
            false
        };
        Ok(StoreStatus {
            exists,
            unlocked: runtime.unlocked,
            needs_password,
            legacy: runtime.legacy_store,
            account_count: runtime.accounts.accounts.len(),
        })
    })
    .await
    .map_err(|_| "Account status task failed".to_string())?
}

#[tauri::command]
async fn unlock_device(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<StoreStatus, String> {
    let state = state.inner().clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let (path, runtime_state) = {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable".to_string())?;
            (runtime.config.accounts_path.clone(), state.runtime.clone())
        };
        let (accounts, session) =
            crypto::unlock_with_device(&path).map_err(|error| error.to_string())?;
        let mut runtime = runtime_state
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        runtime.accounts = accounts;
        runtime.legacy_store = session.is_legacy();
        runtime.session = Some(session);
        runtime.unlocked = true;
        Ok::<StoreStatus, String>(StoreStatus {
            exists: true,
            unlocked: true,
            needs_password: false,
            legacy: runtime.legacy_store,
            account_count: runtime.accounts.accounts.len(),
        })
    })
    .await
    .map_err(|_| "Device unlock task failed".to_string())??;
    let _ = app.emit("store-unlocked", ());
    Ok(status)
}

#[tauri::command]
async fn create_device_store(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<StoreStatus, String> {
    let state = state.inner().clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        if runtime.config.accounts_path.exists() {
            return Err("Account store already exists".to_string());
        }
        if let Some(parent) = runtime.config.accounts_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let session = crypto::create_device_session().map_err(|error| error.to_string())?;
        crypto::save_store(&runtime.config.accounts_path, &runtime.accounts, &session)
            .map_err(|error| error.to_string())?;
        runtime.session = Some(session);
        runtime.unlocked = true;
        runtime.legacy_store = false;
        Ok(StoreStatus {
            exists: true,
            unlocked: true,
            needs_password: false,
            legacy: false,
            account_count: runtime.accounts.accounts.len(),
        })
    })
    .await
    .map_err(|_| "Device store task failed".to_string())??;
    let _ = app.emit("store-unlocked", ());
    Ok(status)
}

#[tauri::command]
async fn unlock_password(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    password: String,
) -> Result<StoreStatus, String> {
    let (path, runtime_state) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        (runtime.config.accounts_path.clone(), state.runtime.clone())
    };
    let (accounts, session) = tauri::async_runtime::spawn_blocking(move || {
        let (accounts, session) = crypto::unlock_with_password(&path, &password).map_err(|_| {
            "The password was not accepted or the store could not be opened".to_string()
        })?;
        if session.is_legacy() {
            let (upgraded, next_session) = crypto::upgrade_v1(&accounts, &session, Some(&password))
                .map_err(|_| {
                    "Legacy store upgrade failed. The original store is unchanged".to_string()
                })?;
            crypto::save_rekeyed(&path, &upgraded, &next_session).map_err(|_| {
                "The upgraded store could not be saved. Keep your original backup".to_string()
            })?;
            Ok::<_, String>((upgraded, next_session))
        } else {
            Ok((accounts, session))
        }
    })
    .await
    .map_err(|_| "Account unlock task failed".to_string())??;
    let mut runtime = runtime_state
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    runtime.accounts = accounts;
    runtime.legacy_store = session.is_legacy();
    runtime.session = Some(session);
    runtime.unlocked = true;
    let _ = app.emit("store-unlocked", ());
    Ok(StoreStatus {
        exists: true,
        unlocked: true,
        needs_password: true,
        legacy: runtime.legacy_store,
        account_count: runtime.accounts.accounts.len(),
    })
}

#[tauri::command]
fn list_accounts(state: tauri::State<'_, AppState>) -> Result<Vec<AccountSummary>, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    if !runtime.unlocked {
        return Err("Account store is locked".to_string());
    }
    Ok(runtime
        .accounts
        .accounts
        .iter()
        .map(|account| {
            let player_path = runtime
                .config
                .custom_player_paths
                .get(&account.user_id)
                .map(|path| path.display().to_string());
            account_summary(account, player_path, &runtime.config)
        })
        .collect())
}

fn save_runtime(runtime: &state::RuntimeState) -> Result<(), String> {
    let session = runtime
        .session
        .as_ref()
        .ok_or_else(|| "Account store is locked".to_string())?;
    crypto::save_store(&runtime.config.accounts_path, &runtime.accounts, session)
        .map_err(|error| error.to_string())
}

fn configured_player_path(runtime: &state::RuntimeState, user_id: u64) -> Option<String> {
    runtime
        .config
        .custom_player_paths
        .get(&user_id)
        .map(|path| path.display().to_string())
}

#[tauri::command]
async fn update_account_alias(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    alias: String,
) -> Result<AccountSummary, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        if alias.chars().count() > 64 {
            return Err("Alias must be 64 characters or fewer".to_string());
        }
        let player_path = configured_player_path(&runtime, user_id);
        let presentation_config = runtime.config.clone();
        let summary = {
            let account = runtime
                .accounts
                .find_by_id_mut(user_id)
                .ok_or_else(|| "Account not found".to_string())?;
            account.alias = alias.trim().to_string();
            account_summary(account, player_path, &presentation_config)
        };
        save_runtime(&runtime)?;
        Ok(summary)
    })
    .await
    .map_err(|_| "Account alias task failed".to_string())?
}

#[tauri::command]
async fn toggle_account_pin(
    state: tauri::State<'_, AppState>,
    user_id: u64,
) -> Result<AccountSummary, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let player_path = configured_player_path(&runtime, user_id);
        let presentation_config = runtime.config.clone();
        let summary = {
            let account = runtime
                .accounts
                .find_by_id_mut(user_id)
                .ok_or_else(|| "Account not found".to_string())?;
            account.is_pinned = !account.is_pinned;
            account_summary(account, player_path, &presentation_config)
        };
        save_runtime(&runtime)?;
        Ok(summary)
    })
    .await
    .map_err(|_| "Account pin task failed".to_string())?
}

#[tauri::command]
async fn update_account_group(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    group: String,
) -> Result<AccountSummary, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let group = group.trim();
        if group.chars().count() > 64 {
            return Err("Group name must be 64 characters or fewer".to_string());
        }
        let player_path = configured_player_path(&runtime, user_id);
        let presentation_config = runtime.config.clone();
        let summary = {
            let account = runtime
                .accounts
                .find_by_id_mut(user_id)
                .ok_or_else(|| "Account not found".to_string())?;
            account.group = group.to_string();
            account_summary(account, player_path, &presentation_config)
        };
        save_runtime(&runtime)?;
        Ok(summary)
    })
    .await
    .map_err(|_| "Account group task failed".to_string())?
}

#[tauri::command]
async fn reorder_accounts(
    state: tauri::State<'_, AppState>,
    user_ids: Vec<u64>,
) -> Result<Vec<AccountSummary>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        for (sort_order, user_id) in user_ids.iter().enumerate() {
            if let Some(account) = runtime.accounts.find_by_id_mut(*user_id) {
                account.sort_order = sort_order as u32;
            }
        }
        let summaries = runtime
            .accounts
            .accounts
            .iter()
            .map(|account| {
                account_summary(
                    account,
                    configured_player_path(&runtime, account.user_id),
                    &runtime.config,
                )
            })
            .collect();
        save_runtime(&runtime)?;
        Ok(summaries)
    })
    .await
    .map_err(|_| "Account ordering task failed".to_string())?
}

#[tauri::command]
async fn update_player_path(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    path: Option<String>,
) -> Result<AccountSummary, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let path = path.map(std::path::PathBuf::from);
        if let Some(candidate) = path.as_ref() {
            if !candidate.is_dir() && !candidate.is_file() {
                return Err("The Roblox player path does not exist".to_string());
            }
        }
        if let Some(candidate) = path {
            runtime
                .config
                .custom_player_paths
                .insert(user_id, candidate);
        } else {
            runtime.config.custom_player_paths.remove(&user_id);
        }
        runtime
            .config
            .save(&runtime.config_path)
            .map_err(|error| error.to_string())?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        let player_path = configured_player_path(&runtime, user_id);
        Ok(account_summary(account, player_path, &runtime.config))
    })
    .await
    .map_err(|_| "Player path task failed".to_string())?
}

fn save_config(runtime: &state::RuntimeState) -> Result<(), String> {
    runtime
        .config
        .save(&runtime.config_path)
        .map_err(|error| error.to_string())
}

fn save_group_changes(
    runtime: &mut state::RuntimeState,
    accounts: AccountStore,
    config: AppConfig,
) -> Result<(), String> {
    let session = runtime
        .session
        .as_ref()
        .ok_or_else(|| "Account store is locked".to_string())?;
    config
        .save(&runtime.config_path)
        .map_err(|_| "Group settings could not be saved".to_string())?;
    if crypto::save_store(&runtime.config.accounts_path, &accounts, session).is_err() {
        runtime.config.save(&runtime.config_path).map_err(|_| {
            "Account store could not be saved, and group settings could not be restored".to_string()
        })?;
        return Err("Account store could not be saved".to_string());
    }
    runtime.config = config;
    runtime.accounts = accounts;
    Ok(())
}

#[cfg(test)]
mod group_persistence_tests {
    use super::*;

    #[test]
    fn failed_account_save_restores_group_config_and_memory() {
        let directory =
            std::env::temp_dir().join(format!("rm-group-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let config_path = directory.join("config.json");
        let mut config = AppConfig {
            accounts_path: directory.clone(),
            ..AppConfig::default()
        };
        config.groups.insert(
            "Original".to_string(),
            GroupMeta {
                color: [1, 2, 3],
                description: String::new(),
                sort_order: 0,
            },
        );
        config.save(&config_path).unwrap();
        let mut runtime = state::RuntimeState {
            accounts: AccountStore::default(),
            config: config.clone(),
            config_path: config_path.clone(),
            session: Some(
                crypto::create_password_session(&uuid::Uuid::new_v4().to_string()).unwrap(),
            ),
            unlocked: true,
            legacy_store: false,
            is_first_install: false,
            credential_revisions: std::collections::HashMap::new(),
        };
        let mut candidate = config;
        candidate.groups.clear();

        assert!(save_group_changes(&mut runtime, AccountStore::default(), candidate).is_err());
        assert!(runtime.config.groups.contains_key("Original"));
        assert!(AppConfig::load(&config_path)
            .groups
            .contains_key("Original"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

fn settings_info_cards() -> std::collections::HashMap<String, SettingsInfoCard> {
    serde_json::from_str::<std::collections::HashMap<String, RawSettingsInfoCard>>(include_str!(
        "../../../infocards.json"
    ))
    .unwrap_or_default()
    .into_iter()
    .map(|(key, card)| {
        (
            key,
            SettingsInfoCard {
                kind: card.kind,
                text: card.text,
            },
        )
    })
    .collect()
}

fn valid_discord_webhook_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://discord.com/api/webhooks/") else {
        return false;
    };
    let Some((webhook_id, token)) = path.split_once('/') else {
        return false;
    };
    webhook_id.len() >= 18
        && webhook_id
            .chars()
            .all(|character| character.is_ascii_digit())
        && !token.is_empty()
        && token
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn valid_mac_oui(oui: &str) -> bool {
    oui.len() == 8
        && oui.chars().enumerate().all(|(index, character)| {
            (index == 2 || index == 5) && character == ':'
                || (index != 2 && index != 5 && character.is_ascii_hexdigit())
        })
}

fn validate_tiling_options(options: &TilingOptions) -> Result<(), String> {
    if options.custom_cols == 0 || options.custom_cols > 12 {
        return Err("Tiling columns must be between 1 and 12".to_string());
    }
    if options.custom_rows == 0 || options.custom_rows > 12 {
        return Err("Tiling rows must be between 1 and 12".to_string());
    }
    if options.padding > 50 {
        return Err("Window padding must be between 0 and 50 pixels".to_string());
    }
    match &options.layout_mode {
        TilingLayoutMode::FixedColumns(columns) if !(1..=12).contains(columns) => {
            Err("Fixed columns must be between 1 and 12".to_string())
        }
        TilingLayoutMode::FixedRows(rows) if !(1..=12).contains(rows) => {
            Err("Fixed rows must be between 1 and 12".to_string())
        }
        TilingLayoutMode::CustomGrid { cols, rows }
            if !(1..=12).contains(cols) || !(1..=12).contains(rows) =>
        {
            Err("Custom grid dimensions must be between 1 and 12".to_string())
        }
        _ => Ok(()),
    }
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        let has_password = runtime
            .session
            .as_ref()
            .is_some_and(|session| session.needs_password());
        let has_discord_webhook = crypto::discord_webhook()
            .map_err(|error| error.to_string())?
            .is_some();
        Ok(SettingsSnapshot {
            config: SettingsConfig::from_config(&runtime.config),
            app_version: env!("CARGO_PKG_VERSION"),
            system_architecture: std::env::consts::ARCH,
            monitors: process::enumerate_monitors(),
            has_password,
            has_discord_webhook,
            roblox_running: process::is_roblox_running(),
            info_cards: settings_info_cards(),
        })
    })
    .await
    .map_err(|_| "Settings task failed".to_string())?
}

#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: SettingsUpdate,
) -> Result<SettingsConfig, String> {
    let state = state.inner().clone();
    let settings = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        let mut candidate = runtime.config.clone();
        settings.apply_to_config(&mut candidate)?;
        candidate
            .save(&runtime.config_path)
            .map_err(|error| error.to_string())?;
        runtime.config = candidate;
        Ok::<SettingsConfig, String>(SettingsConfig::from_config(&runtime.config))
    })
    .await
    .map_err(|_| "Settings save task failed".to_string())??;
    accounts::publish(&app);
    let _ = app.emit("settings-updated", &settings);
    Ok(settings)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartupChange {
    enabled: bool,
}

#[tauri::command]
async fn set_startup_with_windows(
    state: tauri::State<'_, AppState>,
    change: StartupChange,
) -> Result<(), String> {
    let enabled = change.enabled;
    tauri::async_runtime::spawn_blocking(move || startup::set_enabled(enabled))
        .await
        .map_err(|error| format!("Startup task failed: {error}"))?
        .map_err(|error| error.to_string())?;

    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable".to_string())?;
    let previous = runtime.config.startup_with_windows;
    runtime.config.startup_with_windows = enabled;
    if let Err(error) = save_config(&runtime) {
        runtime.config.startup_with_windows = previous;
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
async fn enable_multi_instance(state: tauri::State<'_, AppState>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        process::kill_tray_roblox();
        std::thread::sleep(std::time::Duration::from_millis(500));
        if process::is_roblox_running() {
            return Err(
                "Close all Roblox instances (including tray) before enabling multi-instance."
                    .to_string(),
            );
        }
        process::enable_multi_instance().map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Multi-instance task failed: {error}"))??;

    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable".to_string())?;
    runtime.config.multi_instance_enabled = true;
    save_config(&runtime)
}

#[tauri::command]
async fn arrange_settings_windows(options: TilingOptions) -> Result<(), String> {
    validate_tiling_options(&options)?;
    tauri::async_runtime::spawn_blocking(move || process::arrange_roblox_windows(&options))
        .await
        .map_err(|error| format!("Window arrangement task failed: {error}"))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacAddressRotation {
    preserve_oui: bool,
    alternate_oui: String,
}

#[tauri::command]
async fn rotate_mac_address(rotation: MacAddressRotation) -> Result<(), String> {
    if !valid_mac_oui(&rotation.alternate_oui) {
        return Err("Alternate OUI must use the format 00:1B:21".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        process::rotate_mac_address(rotation.preserve_oui, &rotation.alternate_oui)
    })
    .await
    .map_err(|error| format!("MAC rotation task failed: {error}"))?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn open_data_folder() -> Result<(), String> {
    let data_folder = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RM");
    tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("explorer.exe")
            .arg(data_folder)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Folder open task failed: {error}"))?
}

#[tauri::command]
async fn open_presets_folder() -> Result<(), String> {
    let presets_folder = ram_core::presets::presets_dir(&preset_data_dir());
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&presets_folder)?;
        std::process::Command::new("explorer.exe")
            .arg(presets_folder)
            .spawn()
            .map(|_| ())
    })
    .await
    .map_err(|_| "Preset folder open task failed".to_string())?
    .map_err(|error: std::io::Error| error.to_string())
}

#[tauri::command]
async fn clean_orphaned_data(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let known_user_ids = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        runtime
            .accounts
            .accounts
            .iter()
            .map(|account| account.user_id)
            .collect::<std::collections::HashSet<_>>()
    };
    let data_folder = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RM");
    tauri::async_runtime::spawn_blocking(move || {
        browser_login::clean_orphaned_browse_as_data(&data_folder, &known_user_ids)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Orphan cleanup task failed: {error}"))?
}

#[tauri::command]
fn clear_application_caches() -> Result<usize, String> {
    // the tauri frontend keeps reloadable data in memory; no account or browser data is cacheable
    Ok(0)
}

#[tauri::command]
fn restart_app() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    std::process::Command::new(executable)
        .spawn()
        .map_err(|error| error.to_string())?;
    std::process::exit(0);
}

#[tauri::command]
async fn save_discord_webhook(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    if !valid_discord_webhook_url(&url) {
        return Err("Enter a valid Discord webhook URL".to_string());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crypto::set_discord_webhook(&url)
            .map_err(|_| "Discord webhook could not be saved".to_string())?;
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        runtime.config.discord_webhook_url.clear();
        Ok(())
    })
    .await
    .map_err(|_| "Webhook save task failed".to_string())?
}

#[tauri::command]
async fn remove_discord_webhook() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        crypto::delete_discord_webhook()
            .map_err(|_| "Discord webhook could not be removed".to_string())
    })
    .await
    .map_err(|_| "Webhook removal task failed".to_string())?
}

#[tauri::command]
async fn test_discord_webhook(url: String) -> Result<(), String> {
    if !valid_discord_webhook_url(&url) {
        return Err("Enter a valid Discord webhook URL".to_string());
    }
    let avatar = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../../../assets/branding/Logo.png"));
    let http = reqwest::Client::new();
    let response = http
        .patch(&url)
        .json(&serde_json::json!({
            "name": "Roblox Manager",
            "avatar": format!("data:image/png;base64,{avatar}"),
        }))
        .send()
        .await
        .map_err(|_| "Discord webhook request failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Discord webhook rejected branding (HTTP {})",
            response.status().as_u16()
        ));
    }
    let response = http
        .post(&url)
        .json(&serde_json::json!({
            "username": "Roblox Manager",
            "content": "Roblox Manager webhook connected successfully.",
        }))
        .send()
        .await
        .map_err(|_| "Discord webhook request failed".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Discord webhook rejected test message (HTTP {})",
            response.status().as_u16()
        ));
    }
    Ok(())
}

async fn rekey_store(
    state: tauri::State<'_, AppState>,
    password: Option<String>,
) -> Result<(), String> {
    let runtime_state = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = runtime_state
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        let session = runtime
            .session
            .as_ref()
            .ok_or("Unlock the account store first")?;
        let (accounts, next_session) = if session.is_legacy() {
            crypto::upgrade_v1(&runtime.accounts, session, password.as_deref())
        } else {
            crypto::rewrap(session, password.as_deref())
                .map(|session| (runtime.accounts.clone(), session))
        }
        .map_err(|_| "The store encryption could not be changed".to_string())?;
        crypto::save_rekeyed(&runtime.config.accounts_path, &accounts, &next_session)
            .map_err(|_| "The rekeyed account store could not be saved".to_string())?;
        runtime.accounts = accounts;
        runtime.session = Some(next_session);
        runtime.legacy_store = false;
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "Encryption task failed".to_string())??;
    Ok(())
}

#[tauri::command]
async fn change_password(
    state: tauri::State<'_, AppState>,
    new_password: String,
) -> Result<(), String> {
    if new_password.is_empty() {
        return Err("Enter a password before saving".to_string());
    }
    if new_password.chars().count() > 1024 {
        return Err("Password must be 1024 characters or fewer".to_string());
    }
    rekey_store(state, Some(new_password)).await
}

#[tauri::command]
async fn clear_password(state: tauri::State<'_, AppState>) -> Result<(), String> {
    rekey_store(state, None).await
}

#[tauri::command]
async fn list_private_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PrivateServerSummary>, String> {
    let original_servers = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        runtime.config.private_servers.clone()
    };
    let mut servers = original_servers.clone();
    let mut icon_urls = Vec::with_capacity(servers.len());
    for server in &mut servers {
        icon_urls.push(enrich_private_server(server).await);
    }
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable".to_string())?;
    if runtime.config.private_servers != original_servers {
        return Ok(runtime
            .config
            .private_servers
            .iter()
            .enumerate()
            .map(|(index, server)| private_server_summary(index, server, String::new()))
            .collect());
    }
    if original_servers != servers {
        let mut candidate = runtime.config.clone();
        candidate.private_servers = servers.clone();
        candidate
            .save(&runtime.config_path)
            .map_err(|_| "Private server details could not be saved".to_string())?;
        runtime.config = candidate;
    }
    Ok(servers
        .iter()
        .enumerate()
        .zip(icon_urls)
        .map(|((index, server), icon_url)| private_server_summary(index, server, icon_url))
        .collect())
}

async fn build_private_server(
    state: &AppState,
    name: &str,
    url: &str,
) -> Result<(PrivateServer, String), String> {
    let mut server = match parse_private_server_url(url).map_err(str::to_string)? {
        ParsedPrivateServerUrl::Direct {
            place_id,
            link_code,
        } => PrivateServer {
            name: name.to_string(),
            place_id,
            universe_id: None,
            link_code,
            access_code: String::new(),
            place_name: String::new(),
        },
        ParsedPrivateServerUrl::Share { share_code } => {
            let (cookie, client) = {
                let runtime = state
                    .runtime
                    .lock()
                    .map_err(|_| "Application state unavailable".to_string())?;
                let user_id = runtime
                    .accounts
                    .accounts
                    .first()
                    .map(|account| account.user_id)
                    .ok_or_else(|| "Add an account before using a share link".to_string())?;
                (
                    account_cookie(&runtime, user_id)?,
                    RobloxClient::new().map_err(|error| error.to_string())?,
                )
            };
            let (place_id, universe_id, link_code, access_code) =
                api::resolve_share_link(&client, &cookie, &share_code)
                    .await
                    .map_err(|_| {
                        "The private server share link could not be resolved".to_string()
                    })?;
            PrivateServer {
                name: name.to_string(),
                place_id,
                universe_id,
                link_code,
                access_code,
                place_name: String::new(),
            }
        }
    };
    let icon_url = enrich_private_server(&mut server).await;
    Ok((server, icon_url))
}

#[tauri::command]
async fn add_private_server(
    state: tauri::State<'_, AppState>,
    name: String,
    url: String,
) -> Result<PrivateServerSummary, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err("Server name must be between 1 and 64 characters".to_string());
    }
    let (server, icon_url) = build_private_server(&state, name, &url).await?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable".to_string())?;
    let index = runtime.config.private_servers.len();
    runtime.config.private_servers.push(server);
    save_config(&runtime)?;
    Ok(private_server_summary(
        index,
        &runtime.config.private_servers[index],
        icon_url,
    ))
}

#[tauri::command]
async fn update_private_server(
    state: tauri::State<'_, AppState>,
    index: usize,
    name: String,
    url: String,
) -> Result<PrivateServerSummary, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err("Server name must be between 1 and 64 characters".to_string());
    }
    let (server, icon_url) = build_private_server(&state, name, &url).await?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "Application state unavailable".to_string())?;
    if index >= runtime.config.private_servers.len() {
        return Err("Private server not found".to_string());
    }
    runtime.config.private_servers[index] = server;
    save_config(&runtime)?;
    Ok(private_server_summary(
        index,
        &runtime.config.private_servers[index],
        icon_url,
    ))
}

#[tauri::command]
async fn remove_private_server(
    state: tauri::State<'_, AppState>,
    index: usize,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        if index >= runtime.config.private_servers.len() {
            return Err("Private server not found".to_string());
        }
        runtime.config.private_servers.remove(index);
        save_config(&runtime)
    })
    .await
    .map_err(|_| "Private server removal task failed".to_string())?
}

#[tauri::command]
async fn rename_private_server(
    state: tauri::State<'_, AppState>,
    index: usize,
    name: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err("Server name must be between 1 and 64 characters".to_string());
        }
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        let server = runtime
            .config
            .private_servers
            .get_mut(index)
            .ok_or_else(|| "Private server not found".to_string())?;
        server.name = name.to_string();
        save_config(&runtime)
    })
    .await
    .map_err(|_| "Private server rename task failed".to_string())?
}

#[tauri::command]
async fn create_account_group(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err("Group name must be between 1 and 64 characters".to_string());
        }
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        if runtime.config.groups.contains_key(name) {
            return Err("A group with that name already exists".to_string());
        }
        runtime.config.groups.insert(
            name.to_string(),
            GroupMeta {
                color: [59, 130, 246],
                description: String::new(),
                sort_order: u32::MAX,
            },
        );
        save_config(&runtime)
    })
    .await
    .map_err(|_| "Group creation task failed".to_string())?
}

#[tauri::command]
async fn delete_account_group(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let mut config = runtime.config.clone();
        let mut accounts = runtime.accounts.clone();
        config.groups.remove(name.trim());
        for account in &mut accounts.accounts {
            if account.group == name.trim() {
                account.group.clear();
            }
        }
        save_group_changes(&mut runtime, accounts, config)
    })
    .await
    .map_err(|_| "Group deletion task failed".to_string())?
}

#[tauri::command]
async fn update_account_group_meta(
    state: tauri::State<'_, AppState>,
    old_name: String,
    new_name: String,
    color: [u8; 3],
) -> Result<Vec<AccountGroupSummary>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let old_name = old_name.trim();
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.chars().count() > 64 {
            return Err("Group name must be between 1 and 64 characters".to_string());
        }
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        if old_name.is_empty() || !runtime.config.groups.contains_key(old_name) {
            return Err("Group not found".to_string());
        }
        if old_name != new_name && runtime.config.groups.contains_key(new_name) {
            return Err("A group with that name already exists".to_string());
        }
        let mut config = runtime.config.clone();
        let mut accounts = runtime.accounts.clone();
        let mut metadata = config
            .groups
            .remove(old_name)
            .ok_or_else(|| "Group not found".to_string())?;
        metadata.color = color;
        config.groups.insert(new_name.to_string(), metadata);
        for account in &mut accounts.accounts {
            if account.group == old_name {
                account.group = new_name.to_string();
            }
        }
        save_group_changes(&mut runtime, accounts, config)?;
        let mut groups = runtime
            .config
            .groups
            .iter()
            .map(|(name, meta)| AccountGroupSummary {
                name: name.clone(),
                color: meta.color,
                sort_order: meta.sort_order,
            })
            .collect::<Vec<_>>();
        groups.sort_by_key(|group| (group.sort_order, group.name.clone()));
        Ok(groups)
    })
    .await
    .map_err(|_| "Group update task failed".to_string())?
}

#[tauri::command]
fn open_account_guide() -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg("https://roblox-manager.gitbook.io/docs/guides/manage-accounts")
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not open the account guide".to_string())
}

#[tauri::command]
fn open_account_url(user_id: u64, inventory: bool) -> Result<(), String> {
    let path = if inventory { "inventory" } else { "profile" };
    let url = format!("https://www.roblox.com/users/{user_id}/{path}");
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|error| format!("Could not open Roblox: {error}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("Opening Roblox is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn open_inventory_assets(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    targets: Vec<InventoryBrowserTarget>,
) -> Result<(), String> {
    if targets.is_empty() || targets.len() > 20 {
        return Err("Select between 1 and 20 inventory browser destinations".to_string());
    }

    let destinations = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        targets
            .iter()
            .map(|target| {
                let account = runtime
                    .accounts
                    .find_by_id(target.user_id)
                    .ok_or_else(|| "Account not found".to_string())?;
                let (label, url) = inventory_browser_destination(target)?;
                Ok((
                    label,
                    url,
                    format!(
                        "Roblox - {}",
                        account_summary(account, None, &runtime.config).label
                    ),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    use tauri::Manager;
    for (label, url, title) in destinations {
        if let Some(window) = app.get_webview_window(&label) {
            window
                .set_focus()
                .map_err(|_| "The Roblox item window could not be focused".to_string())?;
            continue;
        }
        let url = url
            .parse()
            .map_err(|_| "The Roblox item URL is invalid".to_string())?;
        tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::External(url))
            .title(title)
            .inner_size(1000.0, 760.0)
            .incognito(true)
            .build()
            .map_err(|_| "The Roblox item window could not be opened".to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn remove_account(state: tauri::State<'_, AppState>, user_id: u64) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        *runtime.credential_revisions.entry(user_id).or_default() += 1;
        if !runtime.accounts.remove_by_id(user_id) {
            return Err("Account not found".to_string());
        }
        if runtime.config.use_credential_manager {
            crypto::credential_delete(user_id).map_err(|error| error.to_string())?;
        }
        save_runtime(&runtime)
    })
    .await
    .map_err(|_| "Account removal task failed".to_string())?
}

#[tauri::command]
async fn launch_account(
    app: tauri::AppHandle,
    user_id: u64,
    place_id: u64,
    job_id: Option<String>,
    data: Option<String>,
    link_code: Option<String>,
    access_code: Option<String>,
) -> Result<(), String> {
    launcher::launch(
        &app,
        launcher::LaunchRequest {
            user_id,
            place_id,
            job_id,
            data,
            link_code,
            access_code,
        },
    )
    .await
}

#[tauri::command]
async fn launch_private_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    index: usize,
    user_ids: Vec<u64>,
) -> Result<(), String> {
    if user_ids.is_empty() {
        return Err("Select at least one account before launching".to_string());
    }
    let (place_id, link_code, access_code) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Application state unavailable".to_string())?;
        let server = runtime
            .config
            .private_servers
            .get(index)
            .ok_or_else(|| "Private server not found".to_string())?;
        (
            server.place_id,
            server.link_code.clone(),
            (!server.access_code.is_empty()).then(|| server.access_code.clone()),
        )
    };
    for user_id in user_ids {
        launch_account(
            app.clone(),
            user_id,
            place_id,
            None,
            None,
            Some(link_code.clone()),
            access_code.clone(),
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
async fn fetch_account_inventory(
    state: tauri::State<'_, AppState>,
    user_id: u64,
) -> Result<Vec<InventoryItem>, String> {
    let (cookie, client) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        (
            account_cookie(&runtime, user_id)?,
            RobloxClient::new().map_err(|error| error.to_string())?,
        )
    };
    let items = assets_api::fetch_account_inventory(&client, &cookie, user_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(items
        .into_iter()
        .map(|item| InventoryItem {
            asset_id: item.asset_id,
            name: item.name,
            asset_type: item.asset_type,
            icon_url: item.icon_url,
            price_robux: item.price_robux,
        })
        .collect())
}

#[tauri::command]
async fn search_connection_users(keyword: String) -> Result<Vec<ConnectionSearchResult>, String> {
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let users = api::search_users(&client, &keyword)
        .await
        .map_err(|error| error.to_string())?;
    let user_ids: Vec<u64> = users.iter().map(|user| user.user_id).collect();
    let avatars: std::collections::HashMap<u64, String> = api::fetch_avatars(&client, &user_ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
    Ok(users
        .into_iter()
        .map(|user| ConnectionSearchResult {
            user_id: user.user_id,
            username: user.username,
            display_name: user.display_name,
            avatar_url: avatars.get(&user.user_id).cloned(),
        })
        .collect())
}

#[tauri::command]
async fn refresh_account_presence(
    app: tauri::AppHandle,
    user_ids: Vec<u64>,
) -> Result<Vec<PresenceUpdate>, String> {
    background::refresh_presence(&app, user_ids).await
}

#[tauri::command]
async fn revalidate_accounts(
    app: tauri::AppHandle,
    user_ids: Vec<u64>,
) -> Result<Vec<AccountSummary>, String> {
    accounts::refresh(&app, user_ids).await
}

#[tauri::command]
async fn kill_all_accounts() -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(process::kill_all_roblox)
        .await
        .map_err(|_| "Roblox termination task failed".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn arrange_account_windows(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let options = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        runtime.config.tiling_options()
    };
    tauri::async_runtime::spawn_blocking(move || process::arrange_roblox_windows(&options))
        .await
        .map_err(|_| "Window arrangement task failed".to_string())?;
    Ok(())
}

#[tauri::command]
async fn connection_action(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    target_user_id: u64,
    action: String,
) -> Result<(), String> {
    let cookie = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        account_cookie(&runtime, user_id)?
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    match action.as_str() {
        "follow" => api::follow_user(&client, &cookie, target_user_id).await,
        "unfollow" => api::unfollow_user(&client, &cookie, target_user_id).await,
        "friend" => api::send_friend_request(&client, &cookie, target_user_id).await,
        "block" => api::block_user(&client, &cookie, target_user_id).await,
        _ => return Err("Unsupported connection action".to_string()),
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn join_user_game(
    app: tauri::AppHandle,
    user_id: u64,
    target_user_id: u64,
) -> Result<(), String> {
    if target_user_id == 0 || user_id == target_user_id {
        return Err("Choose another player to join".into());
    }
    let state = app.state::<AppState>().inner().clone();
    let cookie = tauri::async_runtime::spawn_blocking(move || {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable")?;
        account_cookie(&runtime, user_id)
    })
    .await
    .map_err(|_| "Join credential task failed")??;
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable")?;
    let presence = api::fetch_presences(&client, &cookie, &[target_user_id])
        .await
        .map_err(|_| "The target player's server could not be checked")?
        .into_iter()
        .find(|(id, _)| *id == target_user_id)
        .map(|(_, presence)| presence)
        .ok_or("The target player is not reporting a live presence")?;
    drop(cookie);
    if presence.user_presence_type != 2 {
        return Err("The target player is not in a game currently".into());
    }
    let place_id = presence
        .place_id
        .ok_or("Roblox is not reporting the target player's Place ID")?;
    let job_id = presence
        .game_id
        .filter(|value| !value.trim().is_empty())
        .ok_or(
        "Roblox is not reporting the target player's server. Their join privacy may be hiding it.",
    )?;
    launcher::launch(
        &app,
        launcher::LaunchRequest {
            user_id,
            place_id,
            job_id: Some(job_id),
            data: None,
            link_code: None,
            access_code: None,
        },
    )
    .await
}

#[tauri::command]
async fn add_account(
    app: tauri::AppHandle,
    cookie: String,
) -> Result<accounts::AdditionOutcome, String> {
    if cookie.trim().is_empty() {
        return Err("Enter a Roblox security cookie".to_string());
    }
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let (user_id, username, display_name) = client
        .validate_cookie(cookie.trim())
        .await
        .map_err(|_| "Roblox rejected this account credential".to_string())?;
    let mut account = Account::new(user_id, username, display_name);
    account.last_validated = Some(chrono::Utc::now());
    accounts::stage_addition(&app, cookie.trim(), account).await
}

#[tauri::command]
async fn add_account_anyway(
    app: tauri::AppHandle,
    cookie: String,
    username: String,
) -> Result<accounts::AdditionOutcome, String> {
    if cookie.trim().is_empty() || username.trim().is_empty() {
        return Err("Enter both the cookie and Roblox username".to_string());
    }
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let candidates = api::search_users(&client, username.trim())
        .await
        .map_err(|_| "Roblox username lookup failed".to_string())?;
    let candidate = candidates
        .into_iter()
        .find(|user| user.username.eq_ignore_ascii_case(username.trim()))
        .ok_or_else(|| "That Roblox username could not be found".to_string())?;
    let mut account = Account::new(
        candidate.user_id,
        candidate.username,
        candidate.display_name,
    );
    account.cookie_expired = true;
    accounts::stage_addition(&app, cookie.trim(), account).await
}

#[tauri::command]
async fn login_and_add_account(
    app: tauri::AppHandle,
) -> Result<Option<accounts::AdditionOutcome>, String> {
    let cookie = login::capture_cookie(&app).await?;
    let Some(cookie) = cookie else {
        return Ok(None);
    };
    let client = RobloxClient::new().map_err(|error| error.to_string())?;
    let (user_id, username, display_name) = client
        .validate_cookie(&cookie)
        .await
        .map_err(|_| "Roblox rejected this account credential".to_string())?;
    let mut account = Account::new(user_id, username, display_name);
    account.last_validated = Some(chrono::Utc::now());
    accounts::stage_addition(&app, &cookie, account)
        .await
        .map(Some)
}

#[tauri::command]
async fn browse_as_account(
    state: tauri::State<'_, AppState>,
    user_id: u64,
    inventory: bool,
) -> Result<(), String> {
    let (cookie, label) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        (
            account_cookie(&runtime, user_id)?,
            account_summary(account, None, &runtime.config).label,
        )
    };
    let profile_dir = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RM")
        .join("webview_browse_as")
        .join(user_id.to_string());
    let destination = if inventory {
        Some(format!("https://www.roblox.com/users/{user_id}/inventory"))
    } else {
        None
    };
    tauri::async_runtime::spawn_blocking(move || {
        browser_login::spawn_browse_as_to(profile_dir, cookie, label, destination)
    })
    .await
    .map_err(|_| "Browser launch task failed".to_string())??;
    Ok(())
}

fn preset_data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RM")
}

fn preset_summary(index: usize, preset: &LaunchPreset, icon_url: String) -> LaunchPresetSummary {
    LaunchPresetSummary {
        index,
        name: preset.name.clone(),
        place_id: preset.place_id,
        icon_url,
        job_id: preset.job_id.clone(),
        data: preset.data.clone(),
    }
}

async fn fetch_preset_icon(place_id: u64) -> String {
    let client = match RobloxClient::new() {
        Ok(client) => client,
        Err(_) => return String::new(),
    };
    let universe_id = match assets_api::resolve_place_universe(&client, "", place_id).await {
        Ok(universe_id) => universe_id,
        Err(_) => return String::new(),
    };
    match api::fetch_game_icons(&client, "", &[universe_id]).await {
        Ok(icons) => icons
            .into_iter()
            .next()
            .map(|(_, icon_url)| icon_url)
            .unwrap_or_default(),
        Err(_) => String::new(),
    }
}

fn load_preset_entries() -> Result<ram_core::presets::LoadedPresets, String> {
    ram_core::presets::load_all(&preset_data_dir()).map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_launch_presets() -> Result<Vec<LaunchPresetSummary>, String> {
    let (presets, _) = load_preset_entries()?;
    let mut summaries = Vec::with_capacity(presets.len());
    for (index, (_, preset)) in presets.iter().enumerate() {
        summaries.push(preset_summary(
            index,
            preset,
            fetch_preset_icon(preset.place_id).await,
        ));
    }
    Ok(summaries)
}

#[tauri::command]
async fn save_launch_preset(
    name: String,
    place_id: u64,
    job_id: Option<String>,
    data: Option<String>,
) -> Result<(), String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() || trimmed_name.chars().count() > 80 {
        return Err("Preset name must be between 1 and 80 characters".to_string());
    }
    if place_id == 0 {
        return Err("Enter a valid Roblox place ID".to_string());
    }
    let preset = LaunchPreset {
        name: trimmed_name.to_string(),
        place_id,
        job_id: job_id.filter(|value| !value.trim().is_empty()),
        data: data.filter(|value| !value.trim().is_empty()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        ram_core::presets::save(&preset_data_dir(), &preset, None)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Preset save task failed".to_string())?
}

#[tauri::command]
async fn update_launch_preset(
    index: usize,
    name: String,
    place_id: u64,
    job_id: Option<String>,
    data: Option<String>,
) -> Result<LaunchPresetSummary, String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() || trimmed_name.chars().count() > 80 {
        return Err("Preset name must be between 1 and 80 characters".to_string());
    }
    if place_id == 0 {
        return Err("Enter a valid Roblox place ID".to_string());
    }
    let (presets, _) = load_preset_entries()?;
    let (path, _) = presets
        .get(index)
        .ok_or_else(|| "Preset not found".to_string())?;
    let preset = LaunchPreset {
        name: trimmed_name.to_string(),
        place_id,
        job_id: job_id.filter(|value| !value.trim().is_empty()),
        data: data.filter(|value| !value.trim().is_empty()),
    };
    ram_core::presets::save(&preset_data_dir(), &preset, Some(path))
        .map_err(|error| error.to_string())?;
    Ok(preset_summary(
        index,
        &preset,
        fetch_preset_icon(preset.place_id).await,
    ))
}

#[tauri::command]
async fn remove_launch_preset(index: usize) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (presets, _) = load_preset_entries()?;
        let (path, _) = presets
            .get(index)
            .ok_or_else(|| "Preset not found".to_string())?;
        ram_core::presets::delete(path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Preset deletion task failed".to_string())?
}

#[tauri::command]
async fn launch_launch_preset(
    app: tauri::AppHandle,
    index: usize,
    user_ids: Vec<u64>,
) -> Result<(), String> {
    if user_ids.is_empty() {
        return Err("Select at least one account before launching".to_string());
    }
    let (presets, _) = load_preset_entries()?;
    let (_, preset) = presets
        .get(index)
        .ok_or_else(|| "Preset not found".to_string())?;
    for user_id in user_ids {
        launch_account(
            app.clone(),
            user_id,
            preset.place_id,
            preset.job_id.clone(),
            preset.data.clone(),
            None,
            None,
        )
        .await?;
    }
    Ok(())
}

fn presence_kind(presence: &Presence) -> &'static str {
    match presence.user_presence_type {
        1..=3 => "online",
        _ => "neutral",
    }
}

#[tauri::command]
fn list_account_group_colors(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, [u8; 3]>, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    Ok(runtime
        .config
        .groups
        .iter()
        .map(|(name, meta)| (name.clone(), meta.color))
        .collect())
}

#[tauri::command]
fn list_account_groups(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AccountGroupSummary>, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "Account state unavailable".to_string())?;
    let mut groups = runtime
        .config
        .groups
        .iter()
        .map(|(name, meta)| AccountGroupSummary {
            name: name.clone(),
            color: meta.color,
            sort_order: meta.sort_order,
        })
        .collect::<Vec<_>>();
    groups.sort_by_key(|group| (group.sort_order, group.name.clone()));
    Ok(groups)
}

#[tauri::command]
async fn reorder_account_groups(
    state: tauri::State<'_, AppState>,
    names: Vec<String>,
) -> Result<Vec<AccountGroupSummary>, String> {
    let state_clone = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state_clone
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        for (sort_order, name) in names.iter().enumerate() {
            if let Some(group) = runtime.config.groups.get_mut(name) {
                group.sort_order = sort_order as u32;
            }
        }
        save_config(&runtime)
    })
    .await
    .map_err(|_| "Group ordering task failed".to_string())??;
    list_account_groups(state)
}

#[tauri::command]
async fn search_groups(keyword: String) -> Result<Vec<GroupSearchResultDto>, String> {
    let keyword = keyword.trim().to_string();
    if keyword.is_empty() {
        return Err("Enter a group name to search".to_string());
    }
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable".to_string())?;
    group_api::search_groups(&client, &keyword)
        .await
        .map(|groups| {
            groups
                .into_iter()
                .map(|group| GroupSearchResultDto {
                    id: group.id,
                    name: group.name,
                    description: group.description,
                    member_count: group.member_count,
                    has_verified_badge: group.has_verified_badge,
                })
                .collect()
        })
        .map_err(|_| "Roblox group search failed".to_string())
}

#[tauri::command]
async fn load_group(
    state: tauri::State<'_, AppState>,
    group_id: u64,
    user_ids: Vec<u64>,
) -> Result<GroupWorkspaceDto, String> {
    if group_id == 0 {
        return Err("Enter a valid Roblox group ID".to_string());
    }
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable".to_string())?;
    let group = group_api::fetch_group(&client, group_id)
        .await
        .map_err(|_| "Roblox group could not be loaded".to_string())?;
    let icon_data_url = group_api::fetch_group_icon(&client, group_id)
        .await
        .ok()
        .flatten()
        .map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        });
    let announcement = group_api::fetch_latest_group_announcement(&client, group_id)
        .await
        .map(group_announcement_dto);
    let forum_cookie = user_ids.first().and_then(|user_id| {
        let runtime = state.runtime.lock().ok()?;
        account_cookie(&runtime, *user_id).ok()
    });
    let (forums, forum_status) = if let Some(cookie) = forum_cookie {
        match group_api::fetch_group_forums(&client, &cookie, group_id).await {
            Some(forums) => (
                forums.into_iter().map(group_forum_category_dto).collect(),
                None,
            ),
            None => (
                Vec::new(),
                Some("Forums could not be loaded for the selected account.".to_string()),
            ),
        }
    } else {
        (
            Vec::new(),
            Some("Select an account to load this group's forums.".to_string()),
        )
    };
    let mut memberships = Vec::new();
    for user_id in user_ids {
        if let Ok(membership) = group_api::fetch_membership(&client, group_id, user_id).await {
            memberships.push(group_membership_dto(&membership));
        }
    }
    Ok(GroupWorkspaceDto {
        group: group_info_dto(&group),
        icon_data_url,
        announcement,
        forums,
        forum_status,
        memberships,
    })
}

#[tauri::command]
async fn change_group_membership(
    state: tauri::State<'_, AppState>,
    group_id: u64,
    join: bool,
    user_ids: Vec<u64>,
) -> Result<Vec<GroupMembershipResultDto>, String> {
    let accounts = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        if !runtime.unlocked {
            return Err("Unlock the account store before managing groups".to_string());
        }
        user_ids
            .into_iter()
            .map(|user_id| {
                runtime
                    .accounts
                    .find_by_id(user_id)
                    .map(|_| user_id)
                    .ok_or_else(|| "Selected account is no longer available".to_string())
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    let client = RobloxClient::new().map_err(|_| "Roblox client unavailable".to_string())?;
    let mut results = Vec::with_capacity(accounts.len());
    for user_id in accounts {
        let cookie = {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "Account state unavailable".to_string())?;
            account_cookie(&runtime, user_id)
        };
        let (ok, challenge) = match cookie {
            Ok(cookie) => {
                match group_api::change_membership(&client, &cookie, group_id, user_id, join).await
                {
                    Ok(()) => (true, false),
                    Err(ram_core::CoreError::RobloxApi { message, .. }) => {
                        (false, message.to_lowercase().contains("challenge"))
                    }
                    Err(_) => (false, false),
                }
            }
            Err(_) => (false, false),
        };
        results.push(GroupMembershipResultDto {
            user_id,
            join,
            ok,
            challenge,
            message: (!ok).then(|| {
                if challenge {
                    "Roblox requires additional verification".to_string()
                } else {
                    "The group membership request failed".to_string()
                }
            }),
        });
    }
    Ok(results)
}

#[tauri::command]
async fn open_group_challenge(
    state: tauri::State<'_, AppState>,
    group_id: u64,
    user_id: u64,
) -> Result<(), String> {
    let (cookie, label) = {
        let runtime = state
            .runtime
            .lock()
            .map_err(|_| "Account state unavailable".to_string())?;
        let account = runtime
            .accounts
            .find_by_id(user_id)
            .ok_or_else(|| "Account not found".to_string())?;
        (
            account_cookie(&runtime, user_id)?,
            account_summary(account, None, &runtime.config).label,
        )
    };
    let profile_dir = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("RM")
        .join("webview_browse_as")
        .join(user_id.to_string());
    let destination = format!("https://www.roblox.com/communities/{group_id}");
    tauri::async_runtime::spawn_blocking(move || {
        browser_login::spawn_browse_as_to(profile_dir, cookie, label, Some(destination))
    })
    .await
    .map_err(|_| "Browser launch task failed".to_string())??;
    Ok(())
}

#[cfg(test)]
mod logging_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct SharedOutput(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("test output lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn console_and_file_receive_the_same_redacted_diagnostics() {
        let console = Arc::new(Mutex::new(Vec::new()));
        let file = Arc::new(Mutex::new(Vec::new()));
        let console_output = Arc::clone(&console);
        let file_output = Arc::clone(&file);
        let writer = (move || SharedOutput(Arc::clone(&file_output)))
            .and(move || SharedOutput(Arc::clone(&console_output)));
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(Scrubbed(writer))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                "Synthetic diagnostic .ROBLOSECURITY={} gameinfo:{} x-csrf-token={}",
                "synthetic-cookie",
                "synthetic-ticket",
                "synthetic-csrf"
            );
        });
        let console = console.lock().expect("test output lock");
        let file = file.lock().expect("test output lock");
        assert_eq!(*console, *file);
        let text = String::from_utf8_lossy(&console);
        assert!(text.contains("Synthetic diagnostic"));
        assert_eq!(text.matches("<redacted>").count(), 3);
        for secret in ["synthetic-cookie", "synthetic-ticket", "synthetic-csrf"] {
            assert!(!text.contains(secret));
        }
    }
}

#[cfg(test)]
mod account_presentation_tests {
    use super::*;

    #[test]
    fn anonymisation_hides_names_and_avatars_without_changing_identity() {
        let mut account = Account::new(42, "SyntheticUser".into(), "Synthetic Name".into());
        account.alias = "Private alias".into();
        account.avatar_url = "https://example.invalid/avatar.png".into();
        let config = AppConfig {
            anonymize_names: true,
            ..AppConfig::default()
        };
        let summary = account_summary(&account, None, &config);
        assert_eq!(summary.user_id, 42);
        assert_eq!(summary.username, "Account 42");
        assert_eq!(summary.display_name, "Account 42");
        assert!(summary.alias.is_empty());
        assert!(summary.avatar_url.is_empty());
        assert_eq!(account.username, "SyntheticUser");
    }
}

fn main() {
    let is_demo_requested = std::env::args().any(|argument| argument == "--demo");
    #[cfg(debug_assertions)]
    if cfg!(rm_demo) || is_demo_requested {
        demo::run();
        return;
    }
    if is_demo_requested {
        eprintln!("Demo mode is only available in debug builds");
        std::process::exit(1);
    }
    init_logging();
    tracing::info!(
        event = "startup",
        version = env!("CARGO_PKG_VERSION"),
        profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        "RM Tauri process started"
    );

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == browser_login::FLAG {
        let code = browser_login::run_child(
            std::path::PathBuf::from(&args[2]),
            std::path::PathBuf::from(&args[3]),
        );
        std::process::exit(code);
    }
    if args.len() >= 4 && args[1] == browser_login::BROWSE_AS_FLAG {
        let profile_dir = std::path::PathBuf::from(&args[2]);
        let cookie_in = std::path::PathBuf::from(&args[3]);
        let label = args.get(4).cloned().unwrap_or_default();
        let code = if let Some(destination_url) = args.get(5) {
            browser_login::run_browse_as_child_to(
                profile_dir,
                cookie_in,
                label,
                destination_url.clone(),
            )
        } else {
            browser_login::run_browse_as_child(profile_dir, cookie_in, label)
        };
        std::process::exit(code);
    }
    let mut context = tauri::generate_context!();
    if let Some(window) = context
        .config_mut()
        .app
        .windows
        .iter_mut()
        .find(|window| window.label == "main")
    {
        window.create = false;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(asset_manager::AssetManager::default())
        .setup(|app| {
            app.manage(webview_recovery::RecoveryState::default());
            let configuration = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or("Main window configuration unavailable")?;
            let profile = app.path().app_local_data_dir()?.join("interface-webview");
            let mut builder =
                tauri::WebviewWindowBuilder::from_config(app.handle(), configuration)?
                    .data_directory(profile);
            if benchmark::event_name().is_some() {
                builder = builder.initialization_script("window.__RM_BENCHMARK__ = true;");
            }
            let window = builder.build()?;
            webview_recovery::monitor(app.handle(), &window)?;
            app.manage(background::start(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            benchmark::benchmark_ready,
            asset_manager::list_asset_workspace,
            asset_manager::add_asset_files,
            asset_manager::upload_assets,
            asset_manager::change_asset_queue,
            asset_manager::list_asset_universes,
            asset_manager::list_asset_creators,
            asset_manager::update_asset_row,
            asset_manager::list_asset_creations,
            asset_manager::grant_asset_access,
            asset_manager::reveal_asset_file,
            instances::list_instances,
            instances::focus_instance,
            instances::kill_instance,
            accounts::confirm_account_addition,
            accounts::cancel_account_addition,
            lifecycle::startup_status,
            lifecycle::acknowledge_startup,
            lifecycle::migrate_legacy_data,
            lifecycle::reset_account_store,
            lifecycle::check_release_update,
            lifecycle::open_release_page,
            list_accounts,
            store_status,
            create_device_store,
            unlock_device,
            unlock_password,
            get_settings,
            save_settings,
            set_startup_with_windows,
            enable_multi_instance,
            arrange_settings_windows,
            rotate_mac_address,
            open_data_folder,
            open_presets_folder,
            clean_orphaned_data,
            clear_application_caches,
            restart_app,
            save_discord_webhook,
            remove_discord_webhook,
            test_discord_webhook,
            change_password,
            clear_password,
            update_account_alias,
            toggle_account_pin,
            update_account_group,
            reorder_accounts,
            update_player_path,
            create_account_group,
            delete_account_group,
            update_account_group_meta,
            open_account_url,
            open_account_guide,
            open_inventory_assets,
            remove_account,
            launch_account,
            list_private_servers,
            add_private_server,
            remove_private_server,
            rename_private_server,
            update_private_server,
            launch_private_server,
            fetch_account_inventory,
            search_connection_users,
            refresh_account_presence,
            revalidate_accounts,
            kill_all_accounts,
            arrange_account_windows,
            connection_action,
            join_user_game,
            add_account,
            add_account_anyway,
            login_and_add_account,
            browse_as_account,
            list_launch_presets,
            save_launch_preset,
            update_launch_preset,
            remove_launch_preset,
            launch_launch_preset,
            list_account_group_colors,
            list_account_groups,
            reorder_account_groups,
            search_groups,
            load_group,
            change_group_membership,
            open_group_challenge
        ])
        .build(context)
        .expect("error while building RM")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } if app
                .state::<webview_recovery::RecoveryState>()
                .is_recovering() =>
            {
                api.prevent_exit();
            }
            tauri::RunEvent::Exit => {
                app.state::<background::BackgroundTasks>().stop();
                instances::shutdown(&app.state::<AppState>());
            }
            _ => {}
        });
}
