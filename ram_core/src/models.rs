use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A single Roblox account managed by RM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Roblox user ID.
    pub user_id: u64,
    /// Display name on Roblox.
    pub display_name: String,
    /// Roblox username.
    pub username: String,
    /// The encrypted `.ROBLOSECURITY` cookie (never stored in plaintext).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_cookie: Option<String>,
    /// Optional alias set by the user for quick identification.
    #[serde(default)]
    pub alias: String,
    /// Optional group/tag for organizing accounts.
    #[serde(default)]
    pub group: String,
    /// Cached avatar thumbnail URL.
    #[serde(default)]
    pub avatar_url: String,
    /// Last known online presence.
    #[serde(default)]
    pub last_presence: Presence,
    /// Timestamp of the last successful login/validation.
    pub last_validated: Option<DateTime<Utc>>,
    /// Public Roblox account creation timestamp, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// True if the last automatic revalidation found the cookie expired.
    #[serde(default)]
    pub cookie_expired: bool,
    /// Latest moderation state for the account (None = not moderated as of the
    /// last check, or never checked). Refreshed during periodic revalidation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ModerationInfo>,
    /// Manual sort position (used in Custom sort mode). `u32::MAX` = not yet positioned.
    #[serde(default = "default_sort_order")]
    pub sort_order: u32,
    /// Whether this account is pinned (always shows at the top).
    #[serde(default)]
    pub is_pinned: bool,
}

impl Account {
    pub fn new(user_id: u64, username: String, display_name: String) -> Self {
        Self {
            user_id,
            display_name,
            username,
            encrypted_cookie: None,
            alias: String::new(),
            group: String::new(),
            avatar_url: String::new(),
            last_presence: Presence::default(),
            last_validated: None,
            created_at: None,
            cookie_expired: false,
            moderation: None,
            sort_order: u32::MAX,
            is_pinned: false,
        }
    }

    pub fn can_launch(&self) -> bool {
        !self
            .moderation
            .as_ref()
            .is_some_and(ModerationInfo::is_active)
    }

    /// Returns the label shown in the sidebar (alias if set, otherwise username).
    pub fn label(&self) -> &str {
        if self.alias.is_empty() {
            &self.username
        } else {
            &self.alias
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accounts_allow_launching() {
        let account = Account::new(1, "username".to_string(), "Display name".to_string());
        assert!(account.can_launch());
    }

    #[test]
    fn restricted_accounts_cannot_launch() {
        let mut account = Account::new(1, "username".to_string(), "Display name".to_string());
        account.moderation = Some(ModerationInfo {
            reason: Some("restricted".to_string()),
            ..Default::default()
        });
        assert!(!account.can_launch());
    }
}

/// Moderation / enforcement state on a Roblox account.
///
/// Populated by the periodic revalidation scan. `is_banned` reflects the
/// public `isBanned` flag from `users.roblox.com/v1/users/{userId}` (permanent
/// terminations). `reason` / `expires_at` are best-effort: scraped from the
/// `/notapproved` page when the account is signed in, so they may be missing
/// even when the account is moderated (Roblox UI changes, network errors,
/// etc.). `last_checked` lets the UI age-gate the warning if the scan failed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModerationInfo {
    /// True if the public profile reports the account as banned (terminated).
    #[serde(default)]
    pub is_banned: bool,
    /// Human-readable reason from the moderation page, if we could scrape it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When the moderation expires (None = permanent, or unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Timestamp of the last moderation check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<DateTime<Utc>>,
}

impl ModerationInfo {
    /// True when the account is currently restricted in some way (perma or temp).
    pub fn is_active(&self) -> bool {
        self.is_banned || self.reason.is_some()
    }
}

/// Roblox user presence information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Presence {
    /// 0 = Offline, 1 = Online, 2 = InGame, 3 = InStudio
    pub user_presence_type: u8,
    /// Place ID the user is currently in (if in-game).
    pub place_id: Option<u64>,
    /// Job/server ID (if in-game).
    pub game_id: Option<String>,
    /// Human-readable status text from Roblox.
    pub last_location: String,
}

impl Presence {
    pub fn status_text(&self) -> &str {
        match self.user_presence_type {
            0 => "Offline",
            1 => "Online",
            2 => "In Game",
            3 => "In Studio",
            _ => "Unknown",
        }
    }

    pub fn is_online(&self) -> bool {
        self.user_presence_type > 0
    }
}

/// The persistent store of all accounts, serialized to disk as encrypted JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
}

impl AccountStore {
    pub fn find_by_id(&self, user_id: u64) -> Option<&Account> {
        self.accounts.iter().find(|a| a.user_id == user_id)
    }

    pub fn find_by_id_mut(&mut self, user_id: u64) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.user_id == user_id)
    }

    pub fn remove_by_id(&mut self, user_id: u64) -> bool {
        let before = self.accounts.len();
        self.accounts.retain(|a| a.user_id != user_id);
        self.accounts.len() < before
    }
}

/// Global application configuration persisted to `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Path to the encrypted accounts file.
    pub accounts_path: PathBuf,
    /// Whether to use Windows Credential Manager instead of file-based encryption.
    pub use_credential_manager: bool,
    /// Enable multi-instance mutex patching (risky — user opt-in).
    pub multi_instance_enabled: bool,
    /// Automatically kill Roblox background tray processes (`--launch-to-tray`).
    /// Always active when multi-instance is enabled; can also be used standalone.
    #[serde(default)]
    pub kill_background_roblox: bool,
    /// Minimum seconds between successive account launches. Applied to both
    /// single launches (UI gates the click) and bulk launches (sleeps between
    /// iterations). 0 = no throttling. Roblox rate-limits aggressive launching
    /// from some IPs, so users on those IPs set this to a safe interval.
    #[serde(default)]
    pub launch_delay_secs: u32,
    /// Custom Roblox player install path override.
    pub roblox_player_path: Option<PathBuf>,
    /// Per-account Roblox player install path overrides, stored in plain config.
    #[serde(default)]
    pub custom_player_paths: HashMap<u64, PathBuf>,
    /// Saved window dimensions.
    pub window_width: f32,
    pub window_height: f32,
    /// Per-group color/tag metadata.
    #[serde(default)]
    pub groups: HashMap<String, GroupMeta>,
    /// Saved favorite places for quick launching.
    #[serde(default)]
    pub favorite_places: Vec<FavoritePlace>,
    /// Clear RobloxCookies.dat before each launch to prevent account association.
    #[serde(default = "default_true")]
    pub privacy_mode: bool,
    /// Automatically arrange Roblox windows in a grid after launching.
    #[serde(default)]
    pub auto_arrange_windows: bool,
    /// Target monitor for window tiling.
    #[serde(default)]
    pub tiling_target_monitor: MonitorTarget,
    /// Layout mode for window tiling.
    #[serde(default)]
    pub tiling_layout_mode: TilingLayoutMode,
    /// Custom column count when using fixed columns or custom grid.
    #[serde(default = "default_custom_cols")]
    pub tiling_custom_cols: u32,
    /// Custom row count when using fixed rows or custom grid.
    #[serde(default = "default_custom_rows")]
    pub tiling_custom_rows: u32,
    /// Padding/margin in pixels between tiled windows.
    #[serde(default)]
    pub tiling_padding: u32,
    /// Rename each attributed Roblox window after its account, so tiled clients
    /// are tellable apart. Off by default: it is the only feature that writes to
    /// a Roblox window rather than only reading or moving it, and Hyperion's
    /// tolerance for that is not something we can promise. It also changes what
    /// title-based capture (OBS game capture, for one) will match.
    #[serde(default)]
    pub rename_roblox_windows: bool,
    /// Replace usernames/display names with generic "Account 1", "Account 2", etc.
    #[serde(default)]
    pub anonymize_names: bool,
    /// Last version the user has seen — used to detect first launch after update.
    #[serde(default)]
    pub last_seen_version: Option<String>,
    /// Persisted sidebar sort mode: "Custom", "Name", or "Status".
    #[serde(default = "default_sort_mode")]
    pub sort_mode: String,
    /// Persisted direction for sortable sidebar modes: "Ascending" or "Descending".
    #[serde(default = "default_sort_direction")]
    pub sort_direction: String,
    /// Saved private servers for quick launching.
    #[serde(default)]
    pub private_servers: Vec<PrivateServer>,
    /// Show the Asset Manager tab. Off by default: uploading creates permanent,
    /// publicly moderated assets under a real account, which most users of this
    /// app never need.
    #[serde(default)]
    pub developer_options: bool,
    /// Whether the one-time "stop asking for a password on this PC?" prompt has
    /// been shown. Set once the user answers either way, so declining is
    /// remembered and the prompt never nags. Defaults to false, which is
    /// correct for users upgrading from a release that predates it.
    #[serde(default)]
    pub offered_passwordless: bool,
}

/// Target monitor(s) for window arrangement / tiling.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum MonitorTarget {
    /// Tile on the primary monitor only (default).
    #[default]
    Primary,
    /// Distribute and tile windows across all detected monitors.
    All,
    /// Tile on a specific monitor index (0-indexed).
    Index(usize),
}

/// Layout mode for tiling Roblox client windows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum TilingLayoutMode {
    /// Auto grid (1 -> 1x1, 2 -> 1x2, 3 -> 2 top + 1 bottom centered, 4 -> 2x2, etc.).
    #[default]
    Auto,
    /// Fixed number of columns (rows computed automatically to fit count).
    FixedColumns(u32),
    /// Fixed number of rows (columns computed automatically to fit count).
    FixedRows(u32),
    /// Explicit grid dimensions (Columns x Rows).
    CustomGrid { cols: u32, rows: u32 },
    /// Side-by-side single row (1xN).
    SideBySide,
    /// Stacked single column (Nx1).
    Stacked,
}

/// Information about a connected display monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorGeometry {
    pub index: usize,
    pub name: String,
    pub is_primary: bool,
    pub total_x: i32,
    pub total_y: i32,
    pub total_w: i32,
    pub total_h: i32,
    pub work_x: i32,
    pub work_y: i32,
    pub work_w: i32,
    pub work_h: i32,
}

/// A target rectangle for a window placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// User options for window arrangement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilingOptions {
    pub target_monitor: MonitorTarget,
    pub layout_mode: TilingLayoutMode,
    pub custom_cols: u32,
    pub custom_rows: u32,
    pub padding: u32,
}

impl Default for TilingOptions {
    fn default() -> Self {
        Self {
            target_monitor: MonitorTarget::Primary,
            layout_mode: TilingLayoutMode::Auto,
            custom_cols: 2,
            custom_rows: 2,
            padding: 0,
        }
    }
}

fn default_custom_cols() -> u32 {
    2
}

fn default_custom_rows() -> u32 {
    2
}

fn default_sort_mode() -> String {
    "Custom".to_string()
}

fn default_sort_direction() -> String {
    "Ascending".to_string()
}

fn default_true() -> bool {
    true
}

fn default_sort_order() -> u32 {
    u32::MAX
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = std::env::var("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        Self {
            accounts_path: data_dir.join("RM").join("accounts.dat"),
            use_credential_manager: false,
            multi_instance_enabled: false,
            kill_background_roblox: false,
            launch_delay_secs: 0,
            roblox_player_path: None,
            custom_player_paths: HashMap::new(),
            window_width: 960.0,
            window_height: 640.0,
            groups: HashMap::new(),
            favorite_places: Vec::new(),
            privacy_mode: true,
            auto_arrange_windows: false,
            tiling_target_monitor: MonitorTarget::Primary,
            tiling_layout_mode: TilingLayoutMode::Auto,
            tiling_custom_cols: 2,
            tiling_custom_rows: 2,
            tiling_padding: 0,
            rename_roblox_windows: false,
            anonymize_names: false,
            last_seen_version: None,
            sort_mode: "Custom".to_string(),
            sort_direction: "Ascending".to_string(),
            private_servers: Vec::new(),
            developer_options: false,
            // A fresh install is passwordless from the start, so there is
            // nothing to offer to switch away from.
            offered_passwordless: true,
        }
    }
}

impl AppConfig {
    /// Retrieve the current tiling options configuration.
    pub fn tiling_options(&self) -> TilingOptions {
        TilingOptions {
            target_monitor: self.tiling_target_monitor.clone(),
            layout_mode: self.tiling_layout_mode.clone(),
            custom_cols: self.tiling_custom_cols,
            custom_rows: self.tiling_custom_rows,
            padding: self.tiling_padding,
        }
    }

    /// Load from a JSON file, falling back to defaults.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Persist to a JSON file via an atomic write (temp + fsync + rename) so a
    /// concurrent read or a crash never sees a half-written config.
    pub fn save(&self, path: &std::path::Path) -> Result<(), crate::CoreError> {
        let json = serde_json::to_string_pretty(self)?;
        crate::storage::atomic_write(path, json.as_bytes())?;
        Ok(())
    }
}

/// Optional metadata for account groupings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMeta {
    pub color: [u8; 3],
    pub description: String,
    /// Manual sort position for group ordering. `u32::MAX` = not yet positioned.
    #[serde(default = "default_sort_order")]
    pub sort_order: u32,
}

/// A saved favorite place for quick launching.
///
/// Superseded by [`LaunchPreset`] (stored as standalone JSON files under
/// `presets/`). Kept on `AppConfig` only for backwards-compat migration on
/// first launch after upgrade; new code should use `LaunchPreset`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoritePlace {
    pub name: String,
    pub place_id: u64,
}

/// A user-defined launch preset — name + Place ID + optional Job ID.
/// Persisted as individual JSON files under `<data_dir>/presets/<slug>.json`
/// so users can hand-edit, share, or back them up directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchPreset {
    pub name: String,
    pub place_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

/// A saved private server for quick launching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateServer {
    /// User-assigned name for this private server.
    pub name: String,
    /// The Roblox place ID.
    pub place_id: u64,
    /// The universe (experience) ID — used to resolve game name and icon without auth.
    #[serde(default)]
    pub universe_id: Option<u64>,
    /// The private server link code (from the URL parameter `privateServerLinkCode`).
    pub link_code: String,
    /// The UUID access code needed for launching (scraped from game page).
    #[serde(default)]
    pub access_code: String,
    /// Resolved place name from Roblox API (cached).
    #[serde(default)]
    pub place_name: String,
}
