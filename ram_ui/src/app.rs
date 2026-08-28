//! Top-level application state and the `eframe::App` implementation that ties
//! the sidebar, main panel, settings, toast system, and backend bridge together.

use eframe::egui;
use ram_core::models::{AccountStore, AppConfig, PrivateServer};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ram_core::assets::{AssetKind, AssetState, ModerationStatus, OperationOutcome};

use crate::bridge::{BackendBridge, BackendCommand, BackendEvent, UploadJob};
use crate::components::{
    asset_manager, group_panel, groups_panel, main_panel, presets_panel, private_servers, settings,
    sidebar, tutorial,
};
use crate::theme::ThemeUi;
use crate::toast::{Toast, Toasts};

/// Wall-clock interval gate for background work. Returns `true` and stamps
/// `slot` when `every` has elapsed since the last fire. Callers must put any
/// cheap guard (empty account list, etc.) *before* this in the condition, since
/// a `true` result consumes the interval.
fn interval_due(slot: &mut Option<Instant>, every: Duration) -> bool {
    let now = Instant::now();
    if slot.is_none_or(|t| now.duration_since(t) >= every) {
        *slot = Some(now);
        true
    } else {
        false
    }
}

/// Produce a blurred PNG of an avatar for anonymize mode. Returns `None` if
/// the input couldn't be decoded or re-encoded. Box blur (`fast_blur`) is
/// chosen over Gaussian because avatars are tiny and the speed difference is
/// imperceptible to the user but matters when toggling anonymize on a store
/// with many accounts.
fn anonymize_avatar(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let blurred = image::imageops::fast_blur(&rgba, 20.0);
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(blurred)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

/// How an account is named in anything RM shows or sets on screen.
///
/// One function so the instance tooltip and the Roblox window title cannot
/// drift apart on the thing that matters: with `anonymize_names` on, neither
/// may contain the username or the alias.
fn account_display_label(
    accounts: &[ram_core::models::Account],
    user_id: u64,
    anonymize_names: bool,
) -> String {
    accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| {
            if anonymize_names {
                format!("Account #{}", sidebar::anon_tag(a.user_id))
            } else {
                a.label().to_string()
            }
        })
        .unwrap_or_else(|| format!("user {user_id}"))
}

/// What each attributed client's window should be called.
///
/// Pure, and split out from [`AppState::retitle_roblox_windows`] for one
/// reason: a window title is world-readable. Any process on the machine can ask
/// for it, and it shows up in screenshots, screen shares, and stream captures.
/// A user who turned on `anonymize_names` asked for their usernames not to be
/// on screen, and a title bar is very much on screen, so the rule that
/// `anonymize_names` blanks the name is worth being able to assert directly.
///
/// Inferred attributions are titled too. Getting it wrong writes a wrong name
/// on a title bar, which is cosmetic, so the bar is lower than it is for kill.
fn instance_window_titles(
    tracked: &[ram_core::instances::TrackedInstance],
    accounts: &[ram_core::models::Account],
    anonymize_names: bool,
) -> Vec<(u32, String)> {
    tracked
        .iter()
        .map(|instance| {
            (
                instance.pid,
                account_display_label(accounts, instance.user_id, anonymize_names),
            )
        })
        .collect()
}

/// The body of [`AppState::instance_attribution_summary`], as a free function
/// over exactly the four things it reads. Split out from the method so it can
/// be exercised without standing up an `AppState`, which owns a live backend
/// bridge and a tokio runtime.
fn attribution_summary(
    tracked: &[ram_core::instances::TrackedInstance],
    accounts: &[ram_core::models::Account],
    anonymize_names: bool,
    roblox_instance_count: usize,
) -> String {
    use ram_core::instances::Attribution;

    if tracked.is_empty() {
        return "None of these were launched by RM, or they have not been \
                matched to an account yet."
            .to_string();
    }

    let mut lines = vec!["Launched by RM:".to_string()];
    for instance in tracked {
        let who = account_display_label(accounts, instance.user_id, anonymize_names);
        // Only the guesses are marked. An unmarked line was read off the
        // client's own command line and is not a guess, so hedging it would
        // make the two indistinguishable again.
        let suffix = match instance.attribution {
            Attribution::Exact => "",
            Attribution::Inferred => "  (best guess)",
        };
        lines.push(format!("  PID {} - {who}{suffix}", instance.pid));
    }

    let untracked = roblox_instance_count.saturating_sub(tracked.len());
    if untracked > 0 {
        lines.push(format!(
            "{untracked} other instance(s) not matched to an account."
        ));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Accounts,
    Groups,
    PrivateServers,
    Presets,
    /// Only reachable while `config.utility_enabled` is on.
    Utility,
    /// Only reachable while `config.utility_enabled` and
    /// `config.developer_options` are on.
    AssetManager,
    Settings,
}

// ---------------------------------------------------------------------------
// Add-account dialog state
// ---------------------------------------------------------------------------

/// Which page of the Add Account dialog the user is currently on.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum AddAccountStep {
    /// Initial method picker: browser login vs. manual cookie paste.
    #[default]
    Choose,
    /// Browser login subprocess is spawned / has completed.
    Browser,
    /// Manual `.ROBLOSECURITY` paste.
    Manual,
    /// Bulk import — paste many cookies at once.
    Bulk,
}

#[derive(Default)]
struct AddAccountDialog {
    open: bool,
    step: AddAccountStep,
    cookie_input: String,
    /// True while we're waiting for the backend to validate.
    loading: bool,
    /// Error message from the last failed attempt.
    last_error: Option<String>,
    /// True while the embedded login window is open and we're waiting for a cookie.
    browser_login_pending: bool,
    /// Receiver for the outcome of the embedded login window, if one is active.
    browser_login_rx: Option<std::sync::mpsc::Receiver<crate::browser_login::LoginOutcome>>,
    /// Set when validation succeeded but the account is currently under
    /// moderation. The store push is deferred until the user explicitly
    /// chooses to add anyway (or cancels). Box keeps the dialog struct small.
    pending_moderated: Option<Box<PendingModeratedAdd>>,
    /// Raw cookie that the backend rejected at the auth layer. Held only
    /// to power the "Open browser as" investigate button next to the error.
    /// Cleared when the user retries, opens the browser, or closes the dialog.
    rejected_cookie: Option<String>,
    /// Whether the inline "add anyway" form (username field) is expanded.
    force_add_form_open: bool,
    /// Username buffer for the "add anyway" form.
    force_add_username: String,
    // --- Bulk-import state ---
    /// Multiline paste buffer for the bulk step.
    bulk_input: String,
    /// Cookies still queued for dispatch. We send them one at a time (each
    /// AccountValidated/Error/AuthFailed advances the queue) to avoid hitting
    /// Roblox rate limits with parallel validate_cookie calls. Stored in
    /// reverse so `pop()` yields paste order.
    bulk_queue: Vec<String>,
    bulk_total: usize,
    bulk_succeeded: usize,
    bulk_failed: usize,
    /// True from "Import" click until the user closes the summary screen.
    bulk_running: bool,
}

/// Parse a bulk-paste buffer into individual cookies. Splits on newlines,
/// commas, semicolons, and tabs so that newline-delimited lists, CSV, and
/// TSV all work without the user having to pick a format up front. Empty
/// tokens are dropped and surrounding quotes/whitespace are trimmed.
fn parse_bulk_cookies(input: &str) -> Vec<String> {
    input
        .split(['\n', '\r', ',', ';', '\t'])
        .map(|s| s.trim().trim_matches('"').trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Snapshot of an about-to-be-added account that the user must confirm because
/// Roblox reports it as moderated.
struct PendingModeratedAdd {
    account: ram_core::models::Account,
    encrypted_cookie: Option<String>,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    config: AppConfig,
    config_path: PathBuf,
    store: AccountStore,
    /// Holds the unlocked store's data key. `None` until the store is opened,
    /// which for a device-mode store happens automatically at startup and for a
    /// password-mode store happens when the user types their password.
    ///
    /// Replaces the old `master_password: String`: the password itself is no
    /// longer kept around after unlock, and an empty string is no longer
    /// overloaded to mean "not unlocked yet".
    store_session: Option<ram_core::crypto::StoreSession>,

    bridge: BackendBridge,
    toasts: Toasts,

    // UI state
    active_tab: Tab,
    selected_ids: HashSet<u64>,
    sidebar_state: sidebar::SidebarState,
    main_panel_state: main_panel::MainPanelState,
    private_servers_state: private_servers::PrivateServerState,
    presets_state: presets_panel::PresetsState,
    asset_manager_state: asset_manager::AssetManagerState,
    groups_state: groups_panel::GroupsPanelState,
    settings_state: settings::SettingsState,
    utility_warning_visible: bool,
    add_dialog: AddAccountDialog,

    /// Cached preset list (loaded from disk on startup + after each edit).
    presets: Vec<(PathBuf, ram_core::models::LaunchPreset)>,
    /// Where preset files live on disk. Resolved once at startup.
    presets_dir: PathBuf,

    /// Downloaded avatar image bytes, keyed by user ID.
    avatar_bytes: HashMap<u64, Vec<u8>>,

    /// Blurred variants of `avatar_bytes` for anonymize mode. Computed lazily
    /// each update() so each avatar is blurred at most once. Invalidated when
    /// the underlying avatar refreshes so the next pass re-blurs from source.
    anonymized_avatar_bytes: HashMap<u64, Vec<u8>>,

    /// Downloaded game icon bytes, keyed by place ID.
    game_icon_bytes: HashMap<u64, Vec<u8>>,

    /// Every asset this app has staged or uploaded. Loaded unconditionally,
    /// even when `developer_options` is off, so toggling the setting can never
    /// strand an upload that is still being moderated.
    asset_index: ram_core::assets::AssetIndex,
    asset_index_path: PathBuf,
    /// Set when the index on disk was written by a newer build or could not be
    /// read. Blocks saving so we cannot destroy what we cannot represent.
    asset_index_read_only: bool,
    /// Index has unsaved changes; flushed by the debounce timer and on exit.
    asset_index_dirty: bool,
    /// Rows waiting on the upload confirmation modal.
    pending_upload_rows: Vec<String>,
    /// Universes the acting account manages, and who they were fetched for.
    /// Possibly empty: the listing endpoint is provisional and the manual ID
    /// field in the grant dialog is the guaranteed path.
    universe_targets: Vec<ram_core::assets_api::UniverseTarget>,
    universe_targets_user: Option<u64>,
    /// Groups the acting account belongs to, as candidate publish targets.
    publish_groups: Vec<ram_core::assets_api::GroupTarget>,
    /// The inventory page currently loaded in the browse pane.
    remote_inventory: asset_manager::RemoteInventory,
    /// Results from the latest common-inventory scan for the selected accounts.
    common_inventory_items: Vec<ram_core::assets_api::UserInventoryItem>,
    account_inventory_user_id: Option<u64>,
    account_inventory_items: Vec<ram_core::assets_api::UserInventoryItem>,
    account_inventory_loading: bool,
    account_inventory_error: Option<String>,
    common_inventory_loading: bool,
    common_inventory_message: Option<String>,
    /// Thumbnail PNGs for the icon views, keyed by asset ID.
    asset_thumbnails: HashMap<u64, Vec<u8>>,
    /// Requests currently in flight, so the same asset is not asked for on
    /// every frame while one is outstanding.
    asset_thumbnails_inflight: HashSet<u64>,
    /// Earliest time to ask again for an asset Roblox has not rendered yet.
    ///
    /// Without this a first miss was permanent: old assets whose thumbnails
    /// have aged out come back as `Pending`, and the tile stayed a placeholder
    /// for the rest of the session even after Roblox caught up.
    asset_thumbnails_retry_at: HashMap<u64, std::time::Instant>,

    /// User IDs currently visible in the sidebar (after search filtering).
    visible_user_ids: Vec<u64>,

    /// Cached flag, refreshed from `BackendEvent::InstancesUpdated`.
    roblox_running: bool,
    /// Every running Roblox client, whoever started it. Cached rather than
    /// counted per frame: `sysinfo` has to walk the whole process table, which
    /// is not something to do on the paint thread at 60fps.
    roblox_instance_count: usize,
    /// Clients RM launched, and the account each belongs to. Mostly read off
    /// the client's own command line and therefore exact; see
    /// [`ram_core::instances`] for when it is not, and check
    /// `TrackedInstance::attribution` before acting destructively.
    tracked_instances: Vec<ram_core::instances::TrackedInstance>,
    /// What each client's window was called before RM renamed it, so switching
    /// the feature off puts things back instead of leaving Roblox wearing our
    /// labels. Refreshed whenever Roblox rewrites its own title, so this holds
    /// Roblox's latest intent and not the "Roblox" placeholder we first
    /// displaced. Keyed by PID; entries are dropped when the client exits.
    original_window_titles: std::collections::HashMap<u32, String>,
    /// Previous frame's value of `config.rename_roblox_windows`, so the moment
    /// the user unticks it can be noticed and acted on.
    renaming_was_enabled: bool,
    /// When RM last asked for a launch. Sweeps run faster for a short while
    /// afterwards, because that is the only window in which the delay between a
    /// client appearing and being named is something the user is watching for.
    last_launch_request: Option<std::time::Instant>,
    /// Debounced auto-arrange timer. Each new launch pushes the deadline back so
    /// tiling runs only after the launch burst settles instead of on the first
    /// Roblox window to appear.
    pending_auto_arrange: Option<std::time::Instant>,
    /// Frame counter to throttle background refreshes.
    frame_count: u64,
    /// Wall-clock timestamp of the last tray-kill sweep. Frame-counter timers
    /// don't fire reliably in eframe's reactive mode (update() only runs on
    /// input), so periodic background work uses real time instead.
    last_tray_kill: Option<std::time::Instant>,
    /// Wall-clock timestamps for the background refresh timers, same reasoning
    /// as `last_tray_kill`. The repaint rate swings between ~0.5fps when idle
    /// and 60fps while anything animates, so a frame count is off by 120x
    /// depending on what the UI happens to be doing.
    last_presence_poll: Option<std::time::Instant>,
    last_avatar_refresh: Option<std::time::Instant>,
    last_revalidation: Option<std::time::Instant>,
    last_instance_sweep: Option<std::time::Instant>,
    /// Poll cadence for in-flight asset uploads. Interval is adaptive, so
    /// unlike the timers above it is recomputed each tick from the age of the
    /// oldest pending operation.
    last_asset_poll: Option<std::time::Instant>,
    /// Poll cadence for assets waiting on a moderation verdict. Its own timer
    /// because reviews outlast operations by orders of magnitude.
    last_moderation_poll: Option<std::time::Instant>,
    /// Ticks the upload pump so a row held back by its retry backoff or by
    /// audio spacing gets picked up without waiting for another event.
    last_upload_pump: Option<std::time::Instant>,
    /// When the last audio upload was dispatched, for
    /// [`RmApp::AUDIO_UPLOAD_SPACING`].
    last_audio_upload: Option<std::time::Instant>,
    /// Debounce for asset index writes. A batch of uploads would otherwise fire
    /// one atomic write per state change.
    last_asset_index_save: Option<std::time::Instant>,
    /// Wall-clock timestamp of the last user-initiated game launch. Used to
    /// enforce `config.launch_delay_secs` so the user can't trigger another
    /// single/quick launch inside the cooldown window.
    last_launch: Option<std::time::Instant>,

    /// Password prompt shown on first launch when the store on disk is
    /// password-locked. A device-mode store opens without ever setting this.
    needs_unlock: bool,
    unlock_password_input: String,
    /// The password that just unlocked the store, held only long enough to
    /// re-encrypt a legacy store under it. Cleared as soon as the upgrade
    /// round-trip returns.
    unlock_password_used: String,
    /// Set while the startup unlock is in flight so the unlock screen shows a
    /// spinner instead of flashing an empty password box.
    unlocking: bool,
    /// Show the recovery dialog offered on the unlock screen, which explains
    /// what is and is not recoverable before offering to wipe the store.
    show_recovery: bool,
    /// Typed confirmation guarding the wipe in the recovery dialog.
    recovery_confirm_input: String,
    /// The store opened, but it predates the envelope format. Set until the
    /// upgrade round-trip finishes.
    pending_legacy_upgrade: bool,
    /// Offer to stop asking for a password, shown once after an existing
    /// password user unlocks. Their answer is recorded in
    /// `config.offered_passwordless` either way.
    show_passwordless_offer: bool,
    /// The store is device-locked but this machine's key is gone, so no
    /// password can open it. Drives a different unlock screen.
    device_key_missing: bool,

    /// When set, shows a confirmation dialog before removing the account.
    confirm_remove: Option<u64>,
    /// Place ID received when a browse-as window's Roblox launch was blocked.
    browse_launch_prompt: Option<u64>,

    /// Available update info: (version, release_url).
    update_available: Option<(String, String)>,
    /// Show the "What's New" changelog window.
    show_changelog: bool,

    /// Interactive first-launch tutorial.
    tutorial: tutorial::TutorialState,
}

impl AppState {
    pub fn new(mut config: AppConfig, config_path: PathBuf) -> Self {
        let bridge = BackendBridge::spawn();

        // How the store on disk is locked decides whether the user sees
        // anything at all before the app opens. A read error here (unreadable
        // file, permissions) is treated as "password-locked": the unlock screen
        // can explain itself, whereas the main UI would just look empty.
        let store_mode = match ram_core::crypto::peek_mode(&config.accounts_path) {
            Ok(mode) => mode,
            Err(e) => {
                tracing::warn!("Could not read the account store header: {e}");
                Some(ram_core::crypto::StoreMode::Password)
            }
        };
        let needs_unlock = store_mode.is_some();
        let unlock_silently = store_mode == Some(ram_core::crypto::StoreMode::Device);

        // If multi-instance was previously enabled, run the same validation as
        // the UI toggle: kill tray processes, wait, then only acquire the mutex
        // if no Roblox instances remain.
        if config.multi_instance_enabled {
            ram_core::process::kill_tray_roblox();
            std::thread::sleep(std::time::Duration::from_millis(500));
            if ram_core::process::is_roblox_running() {
                tracing::warn!(
                    "Roblox is running at startup — cannot acquire singleton mutex. \
                     Disabling multi-instance until manually re-enabled."
                );
                config.multi_instance_enabled = false;
            } else if let Err(e) = ram_core::process::enable_multi_instance() {
                tracing::warn!("Failed to acquire singleton mutex at startup: {e}");
                config.multi_instance_enabled = false;
            }
        }

        // Loaded regardless of `developer_options`: a user who uploads, hides
        // the tab, then reopens the app must not silently lose an upload that
        // was still being moderated.
        let asset_index_path = ram_core::assets::index_path(&crate::data_dir());
        let (asset_index, asset_index_status) =
            ram_core::assets::AssetIndex::load(&asset_index_path);
        let asset_index_read_only = asset_index_status.is_read_only();

        let mut sidebar_state = sidebar::SidebarState::default();
        sidebar_state.sort_order = match config.sort_mode.as_str() {
            "Name" => sidebar::SortOrder::Name,
            "Status" => sidebar::SortOrder::Status,
            "Account Age" => sidebar::SortOrder::AccountAge,
            _ => sidebar::SortOrder::Custom,
        };
        sidebar_state.sort_direction = match config.sort_direction.as_str() {
            "Descending" => sidebar::SortDirection::Descending,
            _ => sidebar::SortDirection::Ascending,
        };

        let renaming_enabled_at_start = config.rename_roblox_windows;

        let mut state = Self {
            config,
            config_path,
            store: AccountStore::default(),
            store_session: None,
            bridge,
            toasts: Toasts::default(),
            active_tab: Tab::Accounts,
            selected_ids: HashSet::new(),
            sidebar_state,
            main_panel_state: main_panel::MainPanelState::default(),
            private_servers_state: private_servers::PrivateServerState::default(),
            presets_state: presets_panel::PresetsState::default(),
            asset_manager_state: asset_manager::AssetManagerState::default(),
            groups_state: groups_panel::GroupsPanelState::default(),
            settings_state: settings::SettingsState::default(),
            utility_warning_visible: true,
            add_dialog: AddAccountDialog::default(),
            presets: Vec::new(),
            presets_dir: ram_core::presets::presets_dir(&crate::data_dir()),
            avatar_bytes: HashMap::new(),
            anonymized_avatar_bytes: HashMap::new(),
            game_icon_bytes: HashMap::new(),
            asset_index,
            asset_index_path,
            asset_index_read_only,
            asset_index_dirty: false,
            pending_upload_rows: Vec::new(),
            universe_targets: Vec::new(),
            universe_targets_user: None,
            publish_groups: Vec::new(),
            remote_inventory: asset_manager::RemoteInventory::default(),
            common_inventory_items: Vec::new(),
            account_inventory_user_id: None,
            account_inventory_items: Vec::new(),
            account_inventory_loading: false,
            account_inventory_error: None,
            common_inventory_loading: false,
            common_inventory_message: None,
            asset_thumbnails: HashMap::new(),
            asset_thumbnails_inflight: HashSet::new(),
            asset_thumbnails_retry_at: HashMap::new(),
            visible_user_ids: Vec::new(),
            roblox_running: false,
            roblox_instance_count: 0,
            tracked_instances: Vec::new(),
            original_window_titles: std::collections::HashMap::new(),
            // Read before `config` is moved into the struct below.
            renaming_was_enabled: renaming_enabled_at_start,
            last_launch_request: None,
            pending_auto_arrange: None,
            frame_count: 0,
            last_tray_kill: None,
            // Seeded to "now" so the first tick lands one full interval in,
            // rather than firing a redundant round at startup (StoreUnlocked
            // already kicks off a refresh and revalidation).
            last_presence_poll: Some(std::time::Instant::now()),
            last_avatar_refresh: Some(std::time::Instant::now()),
            last_revalidation: Some(std::time::Instant::now()),
            // Not seeded: the first frame should establish what is running
            // rather than showing "nothing" for two seconds.
            last_instance_sweep: None,
            // Not seeded: if a previous run left operations pending, they
            // should be polled on the first frame, not one interval later.
            last_asset_poll: None,
            last_moderation_poll: None,
            last_upload_pump: None,
            last_audio_upload: None,
            last_asset_index_save: None,
            last_launch: None,
            needs_unlock,
            unlock_password_input: String::new(),
            unlock_password_used: String::new(),
            unlocking: unlock_silently,
            show_recovery: false,
            recovery_confirm_input: String::new(),
            pending_legacy_upgrade: false,
            show_passwordless_offer: false,
            device_key_missing: false,
            confirm_remove: None,
            browse_launch_prompt: None,
            update_available: None,
            show_changelog: false,
            tutorial: tutorial::TutorialState::default(),
        };

        // A device-locked store needs no interaction: open it now so the first
        // frame the user sees is the account list, not a password box.
        if unlock_silently {
            state.bridge.send(BackendCommand::UnlockWithDevice {
                path: state.config.accounts_path.clone(),
            });
        }

        // Check for updates on startup
        state.bridge.send(BackendCommand::CheckForUpdates {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        // Resolve game icons for saved private servers
        state.resolve_private_server_icons();

        // Initial load of preset files from disk.
        state.reload_presets();

        // Reconcile any uploads left mid-flight by the previous run.
        state.recover_asset_index();

        // Detect first launch after update
        let current = env!("CARGO_PKG_VERSION");
        let is_new_version = state.config.last_seen_version.as_deref() != Some(current);
        if is_new_version && state.config.last_seen_version.is_some() {
            // Upgraded from a previous version — show changelog
            state.show_changelog = true;
        }
        // True first launch — show the tutorial (but not if an accounts file
        // already exists, which means an existing user just lost their config).
        if state.config.last_seen_version.is_none() && !state.needs_unlock {
            state.tutorial = tutorial::TutorialState::start();
        }
        // Always update the stored version
        state.config.last_seen_version = Some(current.to_string());
        let _ = state.config.save(&state.config_path);

        state
    }

    // ---- Event processing ----

    /// Human-readable version of the PID-to-account map, for the hover text on
    /// the running-instance counter.
    ///
    /// Lines that came from reading the client's command line are stated
    /// plainly; the ones that fell back to guessing are marked. See
    /// [`ram_core::instances`] for what separates them.
    /// Name each attributed client's window after its account, so tiled clients
    /// are tellable apart.
    ///
    /// Called from the sweep result rather than from a timer of its own, which
    /// is also the reapply loop: Roblox rewrites its own title as it loads (it
    /// comes up as "Roblox" and becomes the place name once in-game), so a
    /// title set once at launch does not survive. `apply_instance_titles` reads
    /// the current title first and only writes when it differs, so the steady
    /// state costs one `GetWindowTextW` per client every two seconds and no
    /// writes at all.
    fn retitle_roblox_windows(&mut self) {
        // Opt-in. This is the only place RM writes to a Roblox window instead of
        // merely reading or repositioning it, so it stays off until asked for.
        if !self.config.rename_roblox_windows {
            return;
        }
        // Forget clients that have exited, so the restore list cannot grow for
        // the life of the session or name a PID that has since been reused.
        let live: std::collections::HashSet<u32> =
            self.tracked_instances.iter().map(|i| i.pid).collect();
        self.original_window_titles
            .retain(|pid, _| live.contains(pid));

        let titles = instance_window_titles(
            &self.tracked_instances,
            &self.store.accounts,
            self.config.anonymize_names,
        );
        if titles.is_empty() {
            return;
        }
        // Whatever we displaced is what Roblox last wanted the window called.
        // Recording it on every rewrite (not just the first) is what keeps the
        // restore from putting back the "Roblox" placeholder shown while the
        // client was still loading.
        for (pid, previous) in ram_core::process::apply_instance_titles(&titles) {
            self.original_window_titles.insert(pid, previous);
        }
    }

    /// Put every window RM renamed back the way Roblox had it.
    ///
    /// Called when the setting is switched off and again on exit. Leaving
    /// Roblox wearing RM's labels after RM has stopped managing them, or after
    /// it has closed entirely, is a mess the user cannot undo without
    /// restarting the client.
    fn restore_roblox_window_titles(&mut self) {
        if self.original_window_titles.is_empty() {
            return;
        }
        let originals: Vec<(u32, String)> = self
            .original_window_titles
            .iter()
            .map(|(pid, title)| (*pid, title.clone()))
            .collect();
        ram_core::process::restore_instance_titles(&originals);
        self.original_window_titles.clear();
    }

    /// Bring one account's client to the front.
    ///
    /// Run inline on the UI thread rather than queued to the backend on
    /// purpose. `SetForegroundWindow` is granted to the process that already
    /// owns the foreground window, and at this moment that is RM, because the
    /// user just clicked a menu item in it. Handing the call to another thread
    /// via a channel would only widen the gap in which that stops being true.
    fn focus_instance(&mut self, pid: u32) {
        if ram_core::process::focus_instance(pid) {
            return;
        }
        self.toasts.push(Toast::error(format!(
            "Could not bring PID {pid} to the front. Windows refused, or the client has no window yet."
        )));
    }

    /// Kill one account's client, after `ram_core` re-verifies it is really
    /// that client.
    ///
    /// This does not trust `tracked_instances`. It passes the recorded launch
    /// token down, and the kill only happens if the live process still carries
    /// it. Everything this function decides is presentation: which instance the
    /// user meant, and what to say when the verification refuses.
    fn kill_instance(&mut self, pid: u32) {
        let Some(instance) = self
            .tracked_instances
            .iter()
            .find(|i| i.pid == pid)
            .cloned()
        else {
            self.toasts.push(Toast::error(
                "That client is no longer being tracked. Nothing was killed.",
            ));
            return;
        };
        // An inferred mapping has no confirmed token, so verification cannot
        // pass and the kill would fail with a confusing message. Say the real
        // reason instead, and do not pretend the option was live.
        if !instance.attribution.is_exact() {
            self.toasts.push(Toast::error(
                "RM could not read that client's command line, so it cannot \
                 confirm which account it belongs to. Use Kill All instead.",
            ));
            return;
        }
        match ram_core::process::kill_verified_instance(pid, instance.launchtime) {
            Ok(()) => {
                let who = account_display_label(
                    &self.store.accounts,
                    instance.user_id,
                    self.config.anonymize_names,
                );
                self.toasts
                    .push(Toast::info(format!("Closed {who}'s client")));
                // The sweep reaps the mapping within a couple of seconds, but
                // dropping it now keeps the menu from offering a dead PID in
                // the meantime.
                self.tracked_instances.retain(|i| i.pid != pid);
            }
            Err(e) => self.toasts.push(Toast::error(e.to_string())),
        }
    }

    /// Launch `user_id` into whatever server `target_user_id` is in.
    ///
    /// Goes through the ordinary `LaunchGame` command rather than a path of its
    /// own, so multi-instance, privacy mode, the tray kill, and the launch
    /// delay all apply exactly as they do to any other launch. The only thing
    /// this adds is where to land.
    ///
    /// The place and job come from the target's cached presence, which the
    /// sidebar refreshes every ten seconds. That is the weak point and it is
    /// unavoidable: there is no way to ask Roblox "is job X still alive"
    /// without walking the whole server list for the place. If the server has
    /// closed in the meantime, Roblox rejects the join rather than silently
    /// dropping the account somewhere else, so the failure is at least visible
    /// to the user in the client.
    fn join_account_server(&mut self, user_id: u64, target_user_id: u64) {
        let target = self.store.find_by_id(target_user_id).map(|a| {
            (
                account_display_label(
                    &self.store.accounts,
                    target_user_id,
                    self.config.anonymize_names,
                ),
                a.last_presence.clone(),
            )
        });
        let Some((target_name, presence)) = target else {
            self.toasts
                .push(Toast::error("That account is no longer in the list."));
            return;
        };

        // Two distinct reasons the join cannot happen, worth separate wording:
        // the account is not playing, versus it is playing but Roblox is not
        // telling RM where. The second happens when the target's join privacy
        // hides the server, and "not in a game" would be a lie.
        if presence.user_presence_type != 2 {
            self.toasts.push(Toast::error(format!(
                "{target_name} is not in a game right now."
            )));
            return;
        }
        let (Some(place_id), Some(job_id)) = (presence.place_id, presence.game_id.clone()) else {
            self.toasts.push(Toast::error(format!(
                "Roblox is not reporting which server {target_name} is in, so RM \
                 cannot follow. Their join privacy setting may be hiding it."
            )));
            return;
        };

        let Some(account) = self
            .store
            .find_by_id(user_id)
            .filter(|account| account.can_launch())
            .map(|a| (a.user_id, a.encrypted_cookie.clone()))
        else {
            self.toasts
                .push(Toast::error("That account cannot be launched."));
            return;
        };
        // Check the session before spending the launch slot, so a locked store
        // does not eat the user's next launch window.
        let Some(session) = self.session() else {
            self.toasts
                .push(Toast::error("Unlock the store before launching."));
            return;
        };
        if !self.try_consume_launch_slot() {
            return;
        }

        self.bridge.send(BackendCommand::LaunchGame {
            user_id: account.0,
            encrypted_cookie: account.1,
            session,
            use_credential_manager: self.config.use_credential_manager,
            place_id,
            job_id: Some(job_id),
            data: None,
            link_code: None,
            access_code: None,
            player_path: self
                .config
                .custom_player_paths
                .get(&account.0)
                .cloned()
                .or_else(|| self.config.roblox_player_path.clone()),
            multi_instance: self.config.multi_instance_enabled,
            kill_background: self.config.kill_background_roblox,
            privacy: self.config.privacy_cleanup_options(),
        });
        self.toasts
            .push(Toast::info(format!("Joining {target_name}'s server")));
    }

    fn instance_attribution_summary(&self) -> String {
        attribution_summary(
            &self.tracked_instances,
            &self.store.accounts,
            self.config.anonymize_names,
            self.roblox_instance_count,
        )
    }

    fn process_events(&mut self) {
        for event in self.bridge.poll() {
            match event {
                BackendEvent::AccountValidated {
                    account,
                    encrypted_cookie: _encrypted_cookie_bulk,
                } if self.add_dialog.bulk_running => {
                    // Bulk import — skip the moderation confirm prompt. The
                    // user opted into batch processing, so moderated accounts
                    // are added silently and can be reviewed afterward.
                    self.store.remove_by_id(account.user_id);
                    self.store.accounts.push(*account);
                    self.add_dialog.bulk_succeeded += 1;
                    self.dispatch_next_bulk();
                }
                BackendEvent::AccountValidated {
                    account,
                    encrypted_cookie,
                } => {
                    // If the account is moderated, don't add silently — let
                    // the user confirm (or open a browser to investigate).
                    let moderated = account.moderation.as_ref().is_some_and(|m| m.is_active());
                    if moderated {
                        self.add_dialog.loading = false;
                        self.add_dialog.last_error = None;
                        self.add_dialog.pending_moderated = Some(Box::new(PendingModeratedAdd {
                            account: *account,
                            encrypted_cookie,
                        }));
                        // Keep the dialog open so the warning step renders.
                    } else {
                        let name = if self.config.anonymize_names {
                            "Account".to_string()
                        } else {
                            account.username.clone()
                        };
                        // Avoid duplicates
                        self.store.remove_by_id(account.user_id);
                        self.store.accounts.push(*account);
                        self.toasts.push(Toast::success(format!("Added {name}")));
                        // Dismiss the dialog — the user's job is done.
                        self.add_dialog.open = false;
                        self.add_dialog.loading = false;
                        self.add_dialog.last_error = None;
                        self.add_dialog.cookie_input.clear();
                        self.add_dialog.browser_login_pending = false;
                        self.add_dialog.browser_login_rx = None;
                        self.add_dialog.rejected_cookie = None;
                        self.tutorial
                            .advance_from(tutorial::TutorialStep::EnterCookie);
                        self.auto_save();
                    }
                }
                BackendEvent::AccountRemoved { user_id } => {
                    self.store.remove_by_id(user_id);
                    self.selected_ids.remove(&user_id);
                    self.toasts.push(Toast::info("Account removed"));
                    self.auto_save();
                }
                BackendEvent::AvatarsUpdated(avatars) => {
                    for (id, url) in avatars {
                        if let Some(acc) = self.store.find_by_id_mut(id) {
                            acc.avatar_url = url;
                        }
                    }
                }
                BackendEvent::AvatarImagesReady(images) => {
                    for (id, bytes) in images {
                        self.avatar_bytes.insert(id, bytes);
                        // Drop the cached blur so the next update() re-blurs
                        // from the fresh source.
                        self.anonymized_avatar_bytes.remove(&id);
                    }
                }
                BackendEvent::PresencesUpdated(presences) => {
                    for (id, p) in presences {
                        if let Some(acc) = self.store.find_by_id_mut(id) {
                            acc.last_presence = p;
                        }
                    }
                }
                BackendEvent::GameLaunched => {
                    // Start the fast-sweep window here rather than at each of the
                    // six call sites that can ask for a launch. The client does
                    // not exist yet either way, so this is still well ahead of
                    // anything there is to attribute.
                    self.last_launch_request = Some(std::time::Instant::now());
                    self.toasts.push(Toast::success("Game launched"));
                    self.schedule_auto_arrange_after_launch();
                }
                BackendEvent::BulkLaunchProgress { launched, total } => {
                    // Each iteration re-arms the window, so a long bulk launch
                    // stays responsive to its last client rather than to its
                    // first.
                    self.last_launch_request = Some(std::time::Instant::now());
                    self.toasts
                        .push(Toast::info(format!("Launching {launched}/{total}...")));
                }
                BackendEvent::BulkLaunchComplete { launched, failed } => {
                    if failed == 0 {
                        self.toasts.push(Toast::success(format!(
                            "Bulk launch complete: {launched} launched"
                        )));
                    } else {
                        self.toasts.push(Toast::error(format!(
                            "Bulk launch done: {launched} launched, {failed} failed"
                        )));
                    }
                    self.schedule_auto_arrange_after_launch();
                }
                BackendEvent::StoreSaved => {
                    // silent
                }
                BackendEvent::StoreUnlocked {
                    store,
                    session,
                    legacy,
                } => {
                    let was_password = session.needs_password();
                    self.store = *store;
                    self.store_session = Some(*session);
                    self.needs_unlock = false;
                    self.unlocking = false;
                    self.device_key_missing = false;
                    self.unlock_password_input.clear();

                    if legacy {
                        // Pre-envelope file. Rotate it onto a fresh data key
                        // before anything can try to save through the old one;
                        // `auto_save` refuses to write a legacy session for
                        // exactly this window.
                        self.pending_legacy_upgrade = true;
                        self.bridge.send(BackendCommand::RekeyStore {
                            store: self.store.clone(),
                            path: self.config.accounts_path.clone(),
                            session: self.store_session.clone().expect("session was just set"),
                            // Keep the password they already have; the offer to
                            // drop it comes after, as an explicit choice.
                            new_password: Some(self.unlock_password_used.clone()),
                            upgrade_legacy: true,
                        });
                    } else {
                        // Only toast when the user actually did something. A
                        // device-mode store opens before the window is drawn,
                        // and announcing that is noise.
                        if was_password {
                            self.toasts.push(Toast::success("Account store unlocked"));
                        }
                        self.offer_passwordless_if_due();

                        // Held back during an upgrade: the backend re-keys a
                        // clone of the store taken just above, so anything a
                        // refresh wrote into `self.store` in the meantime would
                        // be discarded when the upgraded copy replaces it.
                        self.trigger_refresh();
                        self.trigger_revalidation();
                    }
                }
                BackendEvent::StoreRekeyed { store, session } => {
                    let upgraded = self.pending_legacy_upgrade;
                    self.store_session = Some(*session);
                    self.pending_legacy_upgrade = false;
                    self.unlock_password_used.clear();

                    if upgraded {
                        // Only an upgrade rewrites account data (every cookie
                        // is re-encrypted). A plain rewrap returns the same
                        // store it was given, and adopting that stale clone
                        // would discard anything a refresh landed meanwhile.
                        self.store = *store;
                        tracing::info!("Upgraded the account store to the envelope format");
                        self.offer_passwordless_if_due();
                        // Deferred from StoreUnlocked, above.
                        self.trigger_refresh();
                        self.trigger_revalidation();
                    } else {
                        let msg = match self.store_session.as_ref().map(|s| s.needs_password()) {
                            Some(true) => "Master password set",
                            _ => "This PC now unlocks your accounts automatically",
                        };
                        self.toasts.push(Toast::success(msg));
                        // The backend wrote the clone it was handed. Save again
                        // under the new session so any change that landed while
                        // the re-key was in flight reaches disk too.
                        self.auto_save();
                    }
                }
                BackendEvent::DeviceKeyMissing => {
                    self.unlocking = false;
                    self.device_key_missing = true;
                }
                BackendEvent::Killed(count) => {
                    self.toasts
                        .push(Toast::info(format!("Killed {count} instance(s)")));
                }
                BackendEvent::WindowsArranged => {
                    // silent — arrangement complete
                }
                BackendEvent::InstancesUpdated {
                    instances,
                    running_count,
                } => {
                    self.tracked_instances = instances;
                    self.roblox_instance_count = running_count;
                    self.roblox_running = running_count > 0;
                    self.retitle_roblox_windows();
                }
                BackendEvent::AccountRevalidated {
                    user_id,
                    valid,
                    username,
                    display_name,
                    created_at,
                    moderation,
                } => {
                    // Track transitions so we only toast on state changes
                    // (every revalidation cycle re-emits the current state,
                    // so toasting unconditionally spams the user).
                    let mut newly_moderated = false;
                    let mut newly_expired = false;
                    if let Some(acc) = self.store.find_by_id_mut(user_id) {
                        let was_expired = acc.cookie_expired;
                        if valid {
                            acc.last_validated = Some(chrono::Utc::now());
                            acc.username = username;
                            acc.display_name = display_name;
                            acc.cookie_expired = false;
                        } else {
                            acc.cookie_expired = true;
                            newly_expired = !was_expired;
                        }
                        if created_at.is_some() {
                            acc.created_at = created_at;
                        }
                        let was_active = acc.moderation.as_ref().is_some_and(|m| m.is_active());
                        let now_active = moderation.as_ref().is_some_and(|m| m.is_active());
                        newly_moderated = !was_active && now_active;
                        // Merge instead of clobber: when this scan didn't get
                        // a specific reason / expiry (typically because the
                        // cookie is dead and the auth'd moderation endpoint
                        // can't be reached), preserve whatever we already
                        // knew from a previous successful fetch.
                        //
                        // Generic stand-in strings from previous buggy fetches
                        // ("Account terminated.", "Account moderated.") are
                        // intentionally NOT preserved — better to fall back to
                        // the banner's generic title than to keep displaying a
                        // string that's no more informative than the title.
                        fn is_specific(r: &str) -> bool {
                            !matches!(r.trim(), "Account terminated." | "Account moderated.")
                        }
                        acc.moderation = match (acc.moderation.take(), moderation) {
                            (Some(old), Some(mut new)) => {
                                if new.reason.is_none() {
                                    new.reason = old.reason.filter(|r| is_specific(r));
                                }
                                if new.expires_at.is_none() {
                                    new.expires_at = old.expires_at;
                                }
                                Some(new)
                            }
                            (old, None) => old,
                            (None, new) => new,
                        };
                    }
                    self.auto_save();
                    // Toast on state transitions only, and never duplicate
                    // "cookie expired" with the moderation toast — for a
                    // terminated account the cookie revocation is implied
                    // by the moderation itself, so the moderation toast
                    // alone is correct.
                    if newly_moderated {
                        if let Some(acc) = self.store.find_by_id(user_id) {
                            let label = if self.config.anonymize_names {
                                "An account".to_string()
                            } else {
                                acc.label().to_string()
                            };
                            self.toasts.push(Toast::error(format!(
                                "{label} has been moderated. See the account panel for details."
                            )));
                        }
                    } else if newly_expired {
                        if let Some(acc) = self.store.find_by_id(user_id) {
                            // Skip the "cookie expired" toast entirely for
                            // accounts we know are moderated — Roblox revokes
                            // the cookie as part of the enforcement, so the
                            // "re-add with a fresh cookie" advice is wrong.
                            let is_moderated =
                                acc.moderation.as_ref().is_some_and(|m| m.is_active());
                            if !is_moderated {
                                let label = if self.config.anonymize_names {
                                    "An account".to_string()
                                } else {
                                    acc.label().to_string()
                                };
                                self.toasts.push(Toast::error(format!(
                                    "Cookie expired for {label}. Re-add with a fresh cookie."
                                )));
                            }
                        }
                    }
                }
                BackendEvent::Error(msg) => {
                    // A failed unlock or re-key must clear its in-flight flag,
                    // or the unlock screen sits on a spinner with no way back.
                    self.unlocking = false;
                    self.pending_legacy_upgrade = false;
                    if self.groups_state.loading {
                        self.groups_state.loading = false;
                        self.groups_state.error = Some(msg.clone());
                    }

                    if self.add_dialog.bulk_running {
                        // Don't toast or block the dialog mid-batch — count
                        // the failure and move on. The summary screen reports
                        // the totals.
                        self.add_dialog.bulk_failed += 1;
                        self.dispatch_next_bulk();
                    } else {
                        // If the add dialog is loading, show error there for retry
                        if self.add_dialog.loading {
                            self.add_dialog.loading = false;
                            self.add_dialog.last_error = Some(msg.clone());
                        }
                        self.toasts.push(Toast::error(msg));
                    }
                }
                BackendEvent::UpdateAvailable { version, url } => {
                    self.update_available = Some((version, url));
                }
                BackendEvent::PlaceResolved {
                    index,
                    place_name,
                    place_id,
                    icon_bytes,
                } => {
                    if let Some(server) = self.config.private_servers.get_mut(index) {
                        // Only update place_name if the new one is non-empty
                        // (don't overwrite good cached data on transient failures).
                        if !place_name.is_empty() {
                            server.place_name = place_name;
                            let _ = self.config.save(&self.config_path);
                        }
                    }
                    if let Some(bytes) = icon_bytes {
                        self.game_icon_bytes.insert(place_id, bytes);
                    }
                }
                BackendEvent::ShareLinkResolved {
                    server_name,
                    place_id,
                    universe_id,
                    link_code,
                    access_code,
                } => {
                    let server = PrivateServer {
                        name: server_name,
                        place_id,
                        universe_id,
                        link_code,
                        access_code,
                        place_name: String::new(),
                    };
                    let idx = self.config.private_servers.len();
                    self.config.private_servers.push(server);
                    let _ = self.config.save(&self.config_path);
                    // Auto-resolve the place name and icon
                    self.bridge.send(BackendCommand::ResolvePlace {
                        place_id,
                        universe_id,
                        index: idx,
                    });
                    self.toasts
                        .push(Toast::success("Share link resolved, private server added"));
                }
                BackendEvent::ShareLinkFailed(msg) => {
                    self.toasts
                        .push(Toast::error(format!("Failed to resolve share link: {msg}")));
                }
                BackendEvent::BrowseAsLaunched => {
                    self.toasts.push(Toast::success("Opening browser..."));
                }
                BackendEvent::AccountForceAdded {
                    account,
                    encrypted_cookie: _,
                } => {
                    let name = if self.config.anonymize_names {
                        "Account".to_string()
                    } else {
                        account.username.clone()
                    };
                    self.store.remove_by_id(account.user_id);
                    self.store.accounts.push(*account);
                    self.toasts.push(Toast::success(format!("Added {name}")));
                    // Reset the dialog fully — the user is done with this flow.
                    self.add_dialog.open = false;
                    self.add_dialog.loading = false;
                    self.add_dialog.last_error = None;
                    self.add_dialog.cookie_input.clear();
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                    self.add_dialog.rejected_cookie = None;
                    self.add_dialog.force_add_form_open = false;
                    self.add_dialog.force_add_username.clear();
                    self.tutorial
                        .advance_from(tutorial::TutorialStep::EnterCookie);
                    self.auto_save();
                }
                BackendEvent::AddAccountAuthFailed {
                    cookie,
                    moderation_message,
                } => {
                    if self.add_dialog.bulk_running {
                        // Rejected cookie in a batch — count it and move on.
                        // The user can re-run individual paths for failures.
                        self.add_dialog.bulk_failed += 1;
                        self.dispatch_next_bulk();
                    } else {
                        // The validate step rejected the cookie. Most often this
                        // means the account was terminated (cookie revoked) but
                        // it could also be an expired or malformed cookie. Surface
                        // a clearer message + stash the rejected cookie so the
                        // dialog can offer "Open browser as" to investigate.
                        self.add_dialog.loading = false;
                        let msg = match moderation_message {
                            Some(_) => "Moderated\n\nExploits detected. Login to the browser to agree and reactivate your account".to_string(),
                            None => "Cookie was rejected by Roblox. The account may be terminated, the cookie may be expired, or you may need to log in again.".to_string(),
                        };
                        self.add_dialog.last_error = Some(msg);
                        self.add_dialog.rejected_cookie = Some(cookie);
                    }
                }
                BackendEvent::AssetUploadStarted {
                    row_id,
                    file_sha256,
                    file_bytes,
                } => {
                    if let Some(record) = self.asset_index.get_mut(&row_id) {
                        record.file_sha256 = file_sha256;
                        record.file_bytes = file_bytes;
                    }
                    // Flushed rather than debounced: the hash is what lets a
                    // crash-interrupted upload be recognised as already done
                    // instead of being sent a second time.
                    self.save_asset_index();
                }
                BackendEvent::AssetOperationCreated {
                    row_id,
                    operation,
                    started_at,
                } => {
                    if let Some(record) = self.asset_index.get_mut(&row_id) {
                        record.state = AssetState::Pending {
                            operation,
                            since: started_at,
                        };
                        record.updated_at = Some(started_at);
                    }
                    // Flush immediately rather than waiting for the debounce.
                    // This is the one write that makes an upload survivable
                    // across a crash: without the operation ID on disk there is
                    // nothing to resume.
                    self.save_asset_index();
                }
                BackendEvent::AssetOperationResolved { row_id, outcome } => {
                    self.apply_operation_outcome(&row_id, outcome);
                }
                BackendEvent::AssetModerationResolved { row_id, status } => {
                    self.apply_moderation_status(&row_id, status);
                }
                BackendEvent::AssetUploadFailed {
                    row_id,
                    message,
                    retryable,
                } => {
                    self.fail_upload(&row_id, message, retryable);
                }
                BackendEvent::AssetPollBatchDone => {}
                BackendEvent::AssetPermissionsGranted {
                    universe_id,
                    row_ids,
                    granted,
                    refused,
                } => {
                    for row_id in &row_ids {
                        if let Some(record) = self.asset_index.get_mut(row_id) {
                            if !record.granted_universes.contains(&universe_id) {
                                record.granted_universes.push(universe_id);
                            }
                            // One-shot: an auto-grant that already fired must
                            // not fire again if the row is somehow re-resolved.
                            record.auto_grant_universe = None;
                        }
                    }
                    self.save_asset_index();
                    // A 200 can still carry per-asset refusals, so say what
                    // actually landed rather than reporting the batch size.
                    self.toasts.push(if refused > 0 {
                        Toast::warning(format!(
                            "Granted {granted} asset(s) to universe {universe_id}, {refused} refused. See the log for which."
                        ))
                    } else {
                        Toast::success(format!(
                            "Granted {granted} asset(s) to universe {universe_id}"
                        ))
                    });
                }
                BackendEvent::AssetPermissionsFailed {
                    universe_id,
                    message,
                } => {
                    self.toasts.push(Toast::error(format!(
                        "Could not grant access to universe {universe_id}: {message}"
                    )));
                }
                BackendEvent::UniverseTargetsFetched {
                    user_id,
                    universes,
                    resolved_place,
                } => {
                    self.universe_targets = universes;
                    self.universe_targets_user = Some(user_id);
                    if let Some((place_id, universe_id)) = resolved_place {
                        self.asset_manager_state.grant_universe = Some(universe_id);
                        self.toasts.push(Toast::info(format!(
                            "Place {place_id} is universe {universe_id}"
                        )));
                    }
                }
                BackendEvent::PublishGroupsFetched { user_id, groups } => {
                    // Ignore a late reply for an account the user has since
                    // switched away from.
                    if self.asset_manager_state.acting_user_id == Some(user_id) {
                        self.publish_groups = groups;
                    }
                }
                BackendEvent::GroupFetched {
                    group,
                    icon_bytes,
                    shout,
                    announcements,
                    memberships,
                } => {
                    self.groups_state.loaded_group_id = Some(group.id);
                    self.groups_state.group = Some(*group);
                    self.groups_state.icon_bytes = icon_bytes;
                    self.groups_state.shout = shout;
                    self.groups_state.announcements = announcements;
                    self.groups_state.memberships = memberships;
                    self.groups_state.loading = false;
                    self.groups_state.error = None;
                }
                BackendEvent::GroupsSearched(groups) => {
                    self.groups_state.search_results = groups;
                    self.groups_state.loading = false;
                }
                BackendEvent::GroupMembershipChanged {
                    user_id,
                    join,
                    result,
                } => {
                    let challenge_required = result
                        .as_ref()
                        .err()
                        .is_some_and(|error| error.to_lowercase().contains("challenge"));
                    self.groups_state.pending_actions =
                        self.groups_state.pending_actions.saturating_sub(1);
                    self.groups_state.action_in_flight = self.groups_state.pending_actions > 0;
                    match result {
                        Ok(()) => {
                            self.toasts.push(Toast::success(format!(
                                "Account {} {} the group",
                                user_id,
                                if join { "joined" } else { "left" }
                            )));
                            if let Some(membership) = self
                                .groups_state
                                .memberships
                                .iter_mut()
                                .find(|membership| membership.user_id == user_id)
                            {
                                membership.joined = join;
                                if !join {
                                    membership.role_name = None;
                                    membership.role_rank = 0;
                                }
                            }
                        }
                        Err(error) => self.toasts.push(Toast::error(format!(
                            "Could not {} account {}: {error}",
                            if join { "join" } else { "remove" },
                            user_id
                        ))),
                    }
                    if challenge_required {
                        self.groups_state.challenge_user_id = Some(user_id);
                        self.groups_state.error = Some(
                            "Roblox requires an interactive security challenge for this action. Complete it on the group page, then retry membership changes."
                                .to_string(),
                        );
                    }
                }
                BackendEvent::CreationsFetched {
                    creator,
                    kind,
                    appended: _,
                    page,
                    error,
                } => {
                    let node = asset_manager::TreeNode::Inventory(creator);
                    // A fan-out request for a kind the user has since filtered
                    // away, or for a node they navigated off, is stale.
                    let wanted = self.remote_inventory.node == Some(node)
                        && self
                            .remote_inventory
                            .filter
                            .is_none_or(|selected| selected == kind);
                    if !wanted {
                        continue;
                    }

                    self.remote_inventory.inflight =
                        self.remote_inventory.inflight.saturating_sub(1);
                    match page.next_cursor {
                        Some(cursor) => {
                            self.remote_inventory.cursors.insert(kind, cursor);
                        }
                        None => {
                            self.remote_inventory.cursors.remove(&kind);
                        }
                    }
                    // Always append: with a fan-out, each reply carries one
                    // kind's slice of the whole. Replacing would leave only the
                    // last kind to answer.
                    //
                    // Deduped by asset ID because a filter change mid-fan-out
                    // can let a reply from the superseded request through, and
                    // a doubled row is worse than a slightly late one.
                    for item in page.items {
                        if !self
                            .remote_inventory
                            .items
                            .iter()
                            .any(|existing| existing.asset_id == item.asset_id)
                        {
                            self.remote_inventory.items.push(item);
                        }
                    }

                    // One kind failing must not blank the kinds that worked, so
                    // an error is only surfaced once nothing is still in flight
                    // and nothing at all came back.
                    if let Some(message) = error {
                        if self.remote_inventory.inflight == 0
                            && self.remote_inventory.items.is_empty()
                        {
                            tracing::warn!(
                                ?creator,
                                ?kind,
                                "individual inventory scan finished without results: {message}"
                            );
                            self.remote_inventory.error = Some(message.clone());
                            self.toasts
                                .push(Toast::error(format!("Inventory refresh failed: {message}")));
                        }
                    }
                }
                BackendEvent::BulkCreationsFetched {
                    items,
                    failed_accounts,
                } => {
                    self.common_inventory_items = items;
                    self.common_inventory_loading = false;
                    self.common_inventory_message = if self.common_inventory_items.is_empty() {
                        Some("none found in bulk, try individually".to_string())
                    } else if failed_accounts > 0 {
                        Some(format!(
                            "{} common item(s) found; {failed_accounts} account(s) could not be checked.",
                            self.common_inventory_items.len()
                        ))
                    } else {
                        None
                    };
                }
                BackendEvent::UserInventoryFetched {
                    user_id,
                    items,
                    error,
                } => {
                    if self.account_inventory_user_id != Some(user_id) {
                        continue;
                    }
                    self.account_inventory_loading = false;
                    self.account_inventory_items = items;
                    self.account_inventory_error = error;
                    if let Some(error) = self.account_inventory_error.as_deref() {
                        self.toasts
                            .push(Toast::error(format!("Inventory refresh failed: {error}")));
                    }
                }
                BackendEvent::AssetThumbnailsReady { requested, images } => {
                    let now = std::time::Instant::now();
                    let mut resolved = HashSet::new();
                    for (asset_id, bytes) in images {
                        resolved.insert(asset_id);
                        self.asset_thumbnails.insert(asset_id, bytes);
                    }
                    for asset_id in requested {
                        self.asset_thumbnails_inflight.remove(&asset_id);
                        if resolved.contains(&asset_id) {
                            self.asset_thumbnails_retry_at.remove(&asset_id);
                        } else {
                            // Roblox is still rendering it, so ask again later
                            // rather than leaving a permanent placeholder.
                            self.asset_thumbnails_retry_at
                                .insert(asset_id, now + Self::THUMBNAIL_RETRY);
                        }
                    }
                }
            }
        }
    }

    /// The unlocked store session, if there is one.
    ///
    /// Every command that touches a cookie needs this. It is `None` only before
    /// the store is unlocked or while a legacy store is mid-upgrade, and the UI
    /// paths that could send such a command are gated behind `needs_unlock`.
    fn session(&self) -> Option<ram_core::crypto::StoreSession> {
        self.store_session.clone()
    }

    /// The session to save under, creating a device-mode one on first use.
    ///
    /// This is what makes passwordless the default: a brand-new install gets a
    /// data key wrapped by the OS credential store the moment it has something
    /// to save, with nothing to prompt for. Returns `None` only when the
    /// credential store is unavailable, which callers report rather than
    /// silently writing the store out unencrypted.
    fn ensure_session(&mut self) -> Option<ram_core::crypto::StoreSession> {
        if let Some(s) = &self.store_session {
            return Some(s.clone());
        }
        match ram_core::crypto::create_device_session() {
            Ok(s) => {
                tracing::info!("Created a device-locked account store");
                self.store_session = Some(s.clone());
                Some(s)
            }
            Err(e) => {
                tracing::error!("Could not create a device-locked store: {e}");
                self.toasts.push(Toast::error(format!(
                    "Could not set up encryption: {e}. Set a master password in Settings instead."
                )));
                None
            }
        }
    }

    /// Show the one-time offer to stop requiring a master password.
    ///
    /// Only fires for someone who actually has a password to drop, and only
    /// once: `config.offered_passwordless` is set as soon as they answer, so
    /// declining sticks. New installs are passwordless already and never see it.
    fn offer_passwordless_if_due(&mut self) {
        if self.config.offered_passwordless {
            return;
        }
        let has_password = self
            .store_session
            .as_ref()
            .is_some_and(|s| s.needs_password());
        if !has_password {
            // Nothing to offer, but record that we got here so a user who later
            // sets a password on purpose is not second-guessed about it.
            self.config.offered_passwordless = true;
            let _ = self.config.save(&self.config_path);
            return;
        }
        self.show_passwordless_offer = true;
    }

    /// Record the user's answer to the passwordless offer so it is asked once.
    fn dismiss_passwordless_offer(&mut self) {
        self.show_passwordless_offer = false;
        self.config.offered_passwordless = true;
        if let Err(e) = self.config.save(&self.config_path) {
            tracing::warn!("Could not record the passwordless choice: {e}");
        }
    }

    /// Switch the store between device and password locking. `None` drops the
    /// password; `Some` sets or replaces it.
    fn rekey_store(&mut self, new_password: Option<String>) {
        let Some(session) = self.session() else {
            self.toasts
                .push(Toast::error("Unlock the account store first"));
            return;
        };
        self.bridge.send(BackendCommand::RekeyStore {
            store: self.store.clone(),
            path: self.config.accounts_path.clone(),
            session,
            new_password,
            upgrade_legacy: false,
        });
    }

    /// Persist the store. Silently does nothing before the store is unlocked,
    /// which is when there is no data worth writing anyway.
    ///
    /// Note this no longer keys off "is a password set": that older gate meant
    /// credential-manager users, who never set one, never had their account
    /// roster written to disk at all.
    fn auto_save(&self) {
        let Some(session) = self.session() else {
            return;
        };
        if session.is_legacy() {
            // Mid-upgrade: saving now would write a store still keyed by the
            // old unsalted KDF. The upgrade round-trip saves for us.
            return;
        }
        self.bridge.send(BackendCommand::SaveStore {
            store: self.store.clone(),
            path: self.config.accounts_path.clone(),
            session,
        });
    }

    /// Pop the next queued cookie and dispatch an AddAccount for it. When the
    /// queue is empty the batch is done: save once, refresh avatars/presence
    /// for the newly added accounts, and clear the loading flag so the bulk
    /// summary screen renders.
    fn dispatch_next_bulk(&mut self) {
        match self.add_dialog.bulk_queue.pop() {
            Some(cookie) => {
                // Creates the device-locked store on the first import into a
                // fresh install. Failing here means no encryption is available,
                // so the batch stops rather than proceeding unprotected.
                let Some(session) = self.ensure_session() else {
                    self.add_dialog.bulk_queue.clear();
                    self.add_dialog.bulk_running = false;
                    self.add_dialog.loading = false;
                    return;
                };
                self.add_dialog.loading = true;
                self.bridge.send(BackendCommand::AddAccount {
                    cookie,
                    session: session.clone(),
                    use_credential_manager: self.config.use_credential_manager,
                });
            }
            None => {
                self.add_dialog.loading = false;
                if self.add_dialog.bulk_succeeded > 0 {
                    self.auto_save();
                    self.trigger_refresh();
                }
            }
        }
    }

    /// Get the first available cookie for API calls (decrypted from credential
    /// manager or in-memory encrypted cookie).
    /// The account whose cookie the shared refresh calls (presence, avatars)
    /// borrow. Skips accounts already known to have a dead cookie: those calls
    /// fail on every poll otherwise, and since the polls are on a timer that
    /// produced an endless stream of identical error toasts. Returning `None`
    /// here is what stops the polling entirely once no usable cookie is left.
    fn first_account_with_cookie(&self) -> Option<&ram_core::models::Account> {
        self.store.accounts.iter().find(|a| {
            !a.cookie_expired
                && (self.config.use_credential_manager || a.encrypted_cookie.is_some())
        })
    }

    fn trigger_refresh(&self) {
        let Some(session) = self.session() else {
            return;
        };
        let user_ids: Vec<u64> = self.store.accounts.iter().map(|a| a.user_id).collect();
        if user_ids.is_empty() {
            return;
        }
        if let Some(first) = self.first_account_with_cookie() {
            self.bridge.send(BackendCommand::RefreshAll {
                user_ids,
                first_user_id: first.user_id,
                encrypted_cookie: first.encrypted_cookie.clone(),
                session: session.clone(),
                use_credential_manager: self.config.use_credential_manager,
            });
        }
    }

    /// Lightweight presence-only refresh for the currently visible accounts.
    fn trigger_presence_refresh(&self) {
        let Some(session) = self.session() else {
            return;
        };
        if self.visible_user_ids.is_empty() {
            return;
        }
        if let Some(first) = self.first_account_with_cookie() {
            self.bridge.send(BackendCommand::RefreshPresenceOnly {
                user_ids: self.visible_user_ids.clone(),
                first_user_id: first.user_id,
                encrypted_cookie: first.encrypted_cookie.clone(),
                session: session.clone(),
                use_credential_manager: self.config.use_credential_manager,
            });
        }
    }

    /// Resolve place names and game icons for private servers that are missing them.
    fn resolve_private_server_icons(&self) {
        for (i, server) in self.config.private_servers.iter().enumerate() {
            if server.place_name.is_empty() || !self.game_icon_bytes.contains_key(&server.place_id)
            {
                self.bridge.send(BackendCommand::ResolvePlace {
                    place_id: server.place_id,
                    universe_id: server.universe_id,
                    index: i,
                });
            }
        }
    }

    /// Reload the preset cache from disk. Called on startup and after every
    /// save/delete so the UI stays in sync with what's actually on disk
    /// (users can also hand-edit the JSON files outside the app).
    fn reload_presets(&mut self) {
        let data_dir = crate::data_dir();
        match ram_core::presets::load_all(&data_dir) {
            Ok((list, skipped)) => {
                self.presets = list;
                if !skipped.is_empty() {
                    self.toasts.push(Toast::error(format!(
                        "Skipped {} unreadable preset file(s)",
                        skipped.len()
                    )));
                }
            }
            Err(e) => {
                self.toasts
                    .push(Toast::error(format!("Failed to load presets: {e}")));
            }
        }
    }

    /// Dispatch a "browse as" request: decrypt the cookie on the backend and
    /// spawn a fresh webview window pre-logged-in as the account.
    /// Gate a single user-initiated launch through the configured launch
    /// delay. Returns `true` and updates `last_launch` if the launch may
    /// proceed; returns `false` and shows a "wait Xs" toast otherwise.
    /// Bulk launches don't go through this — the backend handles their
    /// pacing internally so the UI can fire-and-forget the whole batch.
    fn try_consume_launch_slot(&mut self) -> bool {
        let delay = self.config.launch_delay_secs;
        if delay == 0 {
            self.last_launch = Some(std::time::Instant::now());
            return true;
        }
        let now = std::time::Instant::now();
        if let Some(last) = self.last_launch {
            let elapsed = now.duration_since(last);
            let needed = std::time::Duration::from_secs(delay as u64);
            if elapsed < needed {
                let remaining = (needed - elapsed).as_secs() + 1;
                self.toasts
                    .push(Toast::info(format!("Launch cooldown: wait {remaining}s",)));
                return false;
            }
        }
        self.last_launch = Some(now);
        true
    }

    fn open_browser_as(&mut self, user_id: u64) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        if !self.config.use_credential_manager && account.encrypted_cookie.is_none() {
            self.toasts
                .push(Toast::error("No stored cookie for this account"));
            return;
        }
        let label = if self.config.anonymize_names {
            format!("#{user_id}")
        } else {
            account.username.clone()
        };
        // Per-account profile dir so sessions don't bleed between accounts.
        let profile_dir = crate::data_dir()
            .join("webview_browse_as")
            .join(user_id.to_string());
        self.bridge.send(BackendCommand::BrowseAsAccount {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            session: session.clone(),
            use_credential_manager: self.config.use_credential_manager,
            profile_dir,
            label,
            destination_url: None,
        });
    }

    /// Revalidate all account cookies in the background.
    fn trigger_revalidation(&self) {
        let Some(session) = self.session() else {
            return;
        };
        if self.store.accounts.is_empty() {
            return;
        }
        let accounts: Vec<(u64, Option<String>)> = self
            .store
            .accounts
            .iter()
            .map(|a| (a.user_id, a.encrypted_cookie.clone()))
            .collect();
        self.bridge.send(BackendCommand::RevalidateAll {
            accounts,
            session: session.clone(),
            use_credential_manager: self.config.use_credential_manager,
        });
    }

    /// Wait a bit after a launch burst to tile windows only once the final
    /// group of client windows has actually appeared.
    fn schedule_auto_arrange_after_launch(&mut self) {
        if !self.config.auto_arrange_windows {
            return;
        }
        let delay = std::cmp::max(5u64, self.config.launch_delay_secs as u64 + 2);
        self.pending_auto_arrange =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(delay));
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for AppState {
    /// Flush anything the 500 ms index debounce is still holding. Without this,
    /// closing the app within half a second of a state change loses it.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Hand the clients their own titles back. They outlive RM, so a rename
        // left behind sticks until the user restarts the client.
        self.restore_roblox_window_titles();
        if self.config.privacy_clean_on_exit {
            match ram_core::process::prepare_privacy_cleanup(self.config.privacy_cleanup_options())
            {
                Ok(cleanup) => {
                    if let Err(error) = cleanup.commit() {
                        tracing::warn!(%error, "Could not finish privacy cleanup on exit");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Privacy cleanup on exit was skipped");
                }
            }
        }
        if self.asset_index_dirty {
            self.save_asset_index();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        // Schedule a repaint so background timers below tick even when the
        // user isn't interacting. Without this, eframe's reactive mode means
        // update() sleeps indefinitely and periodic work (tray-kill,
        // presence refresh, etc.) never fires.
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        // Ensure the bridge can wake the UI when async events arrive.
        self.bridge.set_repaint_ctx(ctx.clone());
        self.process_events();

        if self.browse_launch_prompt.is_none() {
            if let Some(place_id) =
                crate::browser_login::take_browse_as_launch_request(&crate::data_dir())
            {
                tracing::info!(
                    "Prepopulating Place ID field with {place_id} from blocked browser launch"
                );
                self.browse_launch_prompt = Some(place_id);
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Critical,
                ));
            }
        }

        // Top up the blurred-avatar cache for anonymize mode. No-op once
        // every visible avatar already has its blur computed; on first toggle
        // or after a fresh fetch this fills in the gap.
        if self.config.anonymize_names {
            let needs: Vec<u64> = self
                .avatar_bytes
                .keys()
                .filter(|id| !self.anonymized_avatar_bytes.contains_key(id))
                .copied()
                .collect();
            for id in needs {
                if let Some(orig) = self.avatar_bytes.get(&id) {
                    if let Some(blurred) = anonymize_avatar(orig) {
                        self.anonymized_avatar_bytes.insert(id, blurred);
                    }
                }
            }
        }

        // Reconcile the PID-to-account map, and with it `roblox_running` and
        // the instance count. Every 2 seconds of wall clock, not every 120
        // frames: the frame rate swings by two orders of magnitude between
        // idle and animating, so a frame counter is not a clock. Process
        // enumeration happens on the backend thread.
        // The user just unticked "name Roblox windows". Undo what we did rather
        // than abandoning the clients under RM's labels.
        if self.renaming_was_enabled && !self.config.rename_roblox_windows {
            self.restore_roblox_window_titles();
        }
        self.renaming_was_enabled = self.config.rename_roblox_windows;

        // Sweep hard for a while after a launch, idle the rest of the time. A
        // client takes a few seconds to appear and the 2 second cadence sat on
        // top of that, so naming it visibly lagged the window opening. Outside
        // that window nobody is waiting, and each sweep costs a ReadProcessMemory
        // against every client, so the slow cadence is the one to default to.
        let sweep_every = match self.last_launch_request {
            Some(t) if t.elapsed() < Duration::from_secs(30) => Duration::from_millis(400),
            _ => Duration::from_secs(2),
        };
        if interval_due(&mut self.last_instance_sweep, sweep_every) {
            self.bridge.send(BackendCommand::SweepInstances);
        }

        if let Some(deadline) = self.pending_auto_arrange {
            if std::time::Instant::now() >= deadline {
                self.pending_auto_arrange = None;
                self.bridge.send(BackendCommand::ArrangeWindows {
                    options: self.config.tiling_options(),
                    delay_secs: 0,
                });
            }
        }

        // Periodically kill background tray Roblox processes when enabled.
        // Uses wall-clock time so the cadence is reliable in reactive mode
        // (the frame counter approach we used before only fired when the
        // user happened to interact 600 times).
        if (self.config.kill_background_roblox || self.config.multi_instance_enabled)
            && interval_due(&mut self.last_tray_kill, Duration::from_secs(10))
        {
            ram_core::process::kill_tray_roblox();
        }

        // Periodically refresh presence for visible accounts (every 10s)
        if !self.visible_user_ids.is_empty()
            && interval_due(&mut self.last_presence_poll, Duration::from_secs(10))
        {
            self.trigger_presence_refresh();
        }

        // Periodically refresh avatars for all accounts (every 60s)
        if !self.store.accounts.is_empty()
            && interval_due(&mut self.last_avatar_refresh, Duration::from_secs(60))
        {
            self.trigger_refresh();
        }

        // Periodically revalidate all account cookies (every 5 min). This is
        // also the path that clears `cookie_expired` once a cookie starts
        // working again, which is what lets the refresh timers above pick an
        // account back up, so it must keep ticking while the app sits idle.
        if !self.store.accounts.is_empty()
            && interval_due(&mut self.last_revalidation, Duration::from_secs(300))
        {
            self.trigger_revalidation();
        }

        // Poll uploads still in moderation. Not gated on the active tab: a
        // result must land whether or not the user is looking at the Asset
        // Manager. The interval widens as the oldest upload ages, and the timer
        // stops entirely once nothing is pending, so an idle app makes no asset
        // requests at all.
        if !self.needs_unlock && self.asset_index.pending().next().is_some() {
            // Computed before the call: `interval_due` takes &mut self.
            let every = self.asset_poll_interval();
            if interval_due(&mut self.last_asset_poll, every) {
                self.dispatch_asset_poll();
            }
        }

        // Same shape for moderation, on its own slower clock. Separate rather
        // than folded into the above because the two wait on different things:
        // an operation finishes in seconds, a review can take days, and one
        // timer would have to run at the faster of the two rates.
        if !self.needs_unlock && self.asset_index.in_review().next().is_some() {
            let every = self.moderation_poll_interval();
            if interval_due(&mut self.last_moderation_poll, every) {
                self.dispatch_moderation_poll();
            }
        }

        // Re-send anything whose retry backoff or audio spacing has come due.
        // Cheap: a scan of the index, and only while something is queued.
        if !self.needs_unlock
            && self
                .asset_index
                .records
                .iter()
                .any(|r| matches!(r.state, AssetState::Queued))
            && interval_due(&mut self.last_upload_pump, Duration::from_secs(1))
        {
            self.dispatch_next_uploads();
        }

        // Flush index changes that the debounce has been holding.
        if self.asset_index_dirty
            && interval_due(&mut self.last_asset_index_save, Duration::from_millis(500))
        {
            self.save_asset_index();
        }

        // ---- Unlock screen ----
        if self.needs_unlock {
            let mut submit = false;
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);

                    if self.device_key_missing {
                        // No password exists for this store, so offering a
                        // password box would be a dead end. Say what happened.
                        ui.heading("🔒 RM | Cannot Unlock On This PC");
                        ui.add_space(16.0);
                        ui.label(
                            "This account store unlocks automatically, but the key for it is \
                             missing from this PC's credential store.",
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "That usually means the store was copied from another PC, or \
                                 Windows credentials were reset.",
                            )
                            .small()
                            .weak(),
                        );
                    } else if self.unlocking {
                        ui.heading("🔒 RM | Unlocking");
                        ui.add_space(16.0);
                        ui.spinner();
                    } else {
                        ui.heading("🔒 RM | Unlock Account Store");
                        ui.add_space(16.0);
                        ui.label("Enter your master password to decrypt accounts:");
                        ui.add_space(8.0);

                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.unlock_password_input)
                                .password(true)
                                .hint_text("Master password"),
                        );

                        ui.add_space(8.0);
                        let enter_pressed =
                            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Unlock").clicked() || enter_pressed {
                            submit = true;
                        }
                    }

                    ui.add_space(6.0);
                    let link = if self.device_key_missing {
                        "Recovery options"
                    } else {
                        "Forgot password?"
                    };
                    if ui.link(egui::RichText::new(link).weak().small()).clicked() {
                        self.show_recovery = true;
                    }
                });
            });

            if submit {
                let pw = self.unlock_password_input.clone();
                // Held only until the store opens: a legacy store is re-encrypted
                // under it, and it is cleared as soon as that finishes.
                self.unlock_password_used = pw.clone();
                self.unlocking = true;
                self.bridge.send(BackendCommand::UnlockWithPassword {
                    path: self.config.accounts_path.clone(),
                    password: pw,
                });
            }

            self.show_recovery_dialog(ctx);
            self.toasts.show(ctx);
            return;
        }

        // Turning Developer options off must not strand the user on a tab that
        // is no longer in the bar.
        if !self.config.utility_enabled
            && matches!(self.active_tab, Tab::Utility | Tab::AssetManager)
        {
            self.active_tab = Tab::Accounts;
        }
        if !self.config.developer_options && self.active_tab == Tab::AssetManager {
            self.active_tab = Tab::Utility;
        }

        // ---- Top bar ----
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            ui.set_min_height(30.0);

            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Accounts, "📋 Accounts");
                ui.selectable_value(&mut self.active_tab, Tab::Groups, "👥 Groups");
                ui.selectable_value(&mut self.active_tab, Tab::PrivateServers, "🔒 Servers");
                ui.selectable_value(&mut self.active_tab, Tab::Presets, "⭐ Presets");
                if self.config.utility_enabled {
                    ui.selectable_value(
                        &mut self.active_tab,
                        Tab::Utility,
                        "▦ Utility",
                    );
                }
                if self.config.developer_options {
                    ui.selectable_value(&mut self.active_tab, Tab::AssetManager, "📦 Assets");
                }
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙ Settings");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((ref version, ref url)) = self.update_available {
                        let text = if ui.available_width() < 520.0 {
                            format!("Update v{version}")
                        } else {
                            format!("⬆ Update v{version} available")
                        };
                        if ui.link(text).on_hover_text("Click to open the GitHub release page").clicked() {
                            ui.output_mut(|o| o.open_url = Some(egui::output::OpenUrl::new_tab(url)));
                        }
                        ui.separator();
                    }
                    if !self.store.accounts.is_empty()
                        && ui
                            .button("\u{1f504}")
                            .on_hover_text(
                                "Refresh all accounts: re-validate cookies, fetch moderation status, presence, and avatars",
                            )
                            .clicked()
                    {
                        self.toasts.push(Toast::info("Refreshing all accounts..."));
                        self.trigger_revalidation();
                        self.trigger_refresh();
                    }
                    if self.roblox_running {
                        let count = self.roblox_instance_count;
                        ui.colored_label(
                            ui.theme().info,
                            if count == 1 {
                                "● 1 Roblox".to_string()
                            } else {
                                format!("● {count} Roblox")
                            },
                        )
                        .on_hover_text(self.instance_attribution_summary());
                        ui.separator();
                    }
                    if self.selected_ids.len() > 1 {
                        ui.colored_label(
                            ui.theme().accent_text,
                            format!("{} selected", self.selected_ids.len()),
                        );
                        ui.separator();
                    }
                    ui.label(format!("{} acct{}", self.store.accounts.len(), if self.store.accounts.len() == 1 { "" } else { "s" }));
                });
            });
        });

        match self.active_tab {
            Tab::Accounts => self.show_accounts_tab(ctx),
            Tab::Groups => self.show_groups_tab(ctx),
            Tab::PrivateServers => self.show_private_servers_tab(ctx),
            Tab::Presets => self.show_presets_tab(ctx),
            Tab::Utility => self.show_utility_tab(ctx),
            Tab::AssetManager => self.show_asset_manager_tab(ctx),
            Tab::Settings => self.show_settings_tab(ctx),
        }

        // ---- Floating add-account dialog ----
        self.show_add_dialog(ctx);

        // ---- Confirmation dialog for account removal ----
        self.show_confirm_remove_dialog(ctx);

        // ---- Blocked browser launch prompt ----
        self.show_browse_launch_prompt(ctx);

        // ---- Confirmation before an asset upload batch ----
        self.show_upload_confirm_dialog(ctx);

        // ---- Grant universe access to selected assets ----
        self.show_grant_dialog(ctx);

        // ---- Changelog window ----
        self.show_changelog_window(ctx);

        // ---- One-time offer to stop requiring a master password ----
        self.show_passwordless_offer_dialog(ctx);

        // ---- First-launch tutorial overlay ----
        tutorial::show_overlay(ctx, &mut self.tutorial);

        // ---- Toasts ----
        self.toasts.show(ctx);
    }
}

// ---------------------------------------------------------------------------
// Tab rendering
// ---------------------------------------------------------------------------

impl AppState {
    fn show_groups_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let selected_accounts: Vec<&ram_core::models::Account> = self
                .store
                .accounts
                .iter()
                .filter(|account| self.selected_ids.contains(&account.user_id))
                .collect();
            if let Some(action) = groups_panel::show(ui, &mut self.groups_state, &selected_accounts)
            {
                match action {
                    groups_panel::GroupsPanelAction::Search { keyword } => {
                        self.groups_state.loading = true;
                        self.bridge.send(BackendCommand::SearchGroups { keyword });
                    }
                    groups_panel::GroupsPanelAction::Load { group_id } => {
                        self.groups_state.loading = true;
                        self.groups_state.error = None;
                        self.bridge.send(BackendCommand::FetchGroup {
                            group_id,
                            user_ids: selected_accounts
                                .iter()
                                .map(|account| account.user_id)
                                .collect(),
                        });
                    }
                    groups_panel::GroupsPanelAction::Join
                    | groups_panel::GroupsPanelAction::Leave => {
                        let Some(group_id) = self.groups_state.loaded_group_id else {
                            return;
                        };
                        let join = matches!(action, groups_panel::GroupsPanelAction::Join);
                        let Some(session) = self.session() else {
                            self.toasts
                                .push(Toast::error("Unlock the store before managing groups."));
                            return;
                        };
                        let accounts: Vec<(u64, Option<String>)> = selected_accounts
                            .iter()
                            .filter(|account| {
                                let joined = self
                                    .groups_state
                                    .memberships
                                    .iter()
                                    .find(|membership| membership.user_id == account.user_id)
                                    .is_some_and(|membership| membership.joined);
                                joined != join
                            })
                            .map(|account| (account.user_id, account.encrypted_cookie.clone()))
                            .collect();
                        if accounts.is_empty() {
                            self.toasts.push(Toast::info(if join {
                                "All selected accounts are already in the group."
                            } else {
                                "None of the selected accounts are in the group."
                            }));
                            return;
                        }
                        self.groups_state.action_in_flight = true;
                        self.groups_state.pending_actions = accounts.len();
                        self.bridge.send(BackendCommand::ChangeGroupMembership {
                            group_id,
                            join,
                            accounts,
                            session,
                            use_credential_manager: self.config.use_credential_manager,
                        });
                    }
                    groups_panel::GroupsPanelAction::OpenGroupPage => {
                        if let (Some(group_id), Some(user_id)) = (
                            self.groups_state.loaded_group_id,
                            self.groups_state.challenge_user_id,
                        ) {
                            let account = self.store.find_by_id(user_id).cloned();
                            if let (Some(account), Some(session)) = (account, self.session()) {
                                let profile_dir = crate::data_dir()
                                    .join("webview_browse_as")
                                    .join(user_id.to_string());
                                let label = if self.config.anonymize_names {
                                    format!("#{user_id}")
                                } else {
                                    account.username.clone()
                                };
                                self.bridge.send(BackendCommand::BrowseAsAccount {
                                    user_id,
                                    encrypted_cookie: account.encrypted_cookie,
                                    session,
                                    use_credential_manager: self.config.use_credential_manager,
                                    profile_dir,
                                    label,
                                    destination_url: Some(format!(
                                        "https://www.roblox.com/communities/{group_id}"
                                    )),
                                });
                            }
                        }
                    }
                }
            }
        });
    }

    fn show_accounts_tab(&mut self, ctx: &egui::Context) {
        // Sidebar
        egui::SidePanel::left("sidebar")
            .default_width(220.0)
            .width_range(140.0..=400.0)
            .resizable(true)
            .show(ctx, |ui| {
                let avatars = if self.config.anonymize_names {
                    &self.anonymized_avatar_bytes
                } else {
                    &self.avatar_bytes
                };
                let result = sidebar::show(
                    ui,
                    &mut self.sidebar_state,
                    &self.store.accounts,
                    &self.selected_ids,
                    self.config.anonymize_names,
                    &self.config.groups,
                    avatars,
                    &self.tracked_instances,
                );
                self.visible_user_ids = result.visible_user_ids;
                self.tutorial.add_btn_rect = result.add_btn_rect;
                self.tutorial.sidebar_accounts_rect = result.accounts_rect;
                // Tutorial: advance when the sidebar account list area is known
                if !self.selected_ids.is_empty() {
                    self.tutorial
                        .advance_from(tutorial::TutorialStep::SelectAccount);
                }
                for a in result.actions {
                    match a {
                        sidebar::SidebarAction::Select(id) => {
                            self.selected_ids.clear();
                            self.selected_ids.insert(id);
                        }
                        sidebar::SidebarAction::ToggleSelect(id) => {
                            if self.selected_ids.contains(&id) {
                                self.selected_ids.remove(&id);
                            } else {
                                self.selected_ids.insert(id);
                            }
                        }
                        sidebar::SidebarAction::RangeSelect(ids) => {
                            for id in ids {
                                self.selected_ids.insert(id);
                            }
                        }
                        sidebar::SidebarAction::AddAccountDialog => {
                            self.add_dialog.open = true;
                            self.add_dialog.step = AddAccountStep::Choose;
                            self.add_dialog.cookie_input.clear();
                            self.add_dialog.last_error = None;
                            self.add_dialog.loading = false;
                            self.add_dialog.browser_login_pending = false;
                            self.add_dialog.browser_login_rx = None;
                            self.add_dialog.rejected_cookie = None;
                            self.add_dialog.pending_moderated = None;
                            self.tutorial
                                .advance_from(tutorial::TutorialStep::AddAccount);
                        }
                        sidebar::SidebarAction::CopyJobId(job_id) => {
                            ui.output_mut(|o| o.copied_text = job_id.clone());
                            self.toasts.push(Toast::info("Copied to clipboard"));
                        }
                        sidebar::SidebarAction::CopyAccountInfo(info) => {
                            ui.output_mut(|o| o.copied_text = info);
                            self.toasts.push(Toast::info("Copied to clipboard"));
                        }
                        sidebar::SidebarAction::OpenBrowserAs(user_id) => {
                            self.open_browser_as(user_id);
                        }
                        sidebar::SidebarAction::FocusInstance(pid) => {
                            self.focus_instance(pid);
                        }
                        sidebar::SidebarAction::KillInstance(pid) => {
                            self.kill_instance(pid);
                        }
                        sidebar::SidebarAction::JoinAccountServer {
                            user_id,
                            target_user_id,
                        } => {
                            self.join_account_server(user_id, target_user_id);
                        }
                        sidebar::SidebarAction::QuickLaunch(user_id) => {
                            // Prefer the first saved preset (with its Job ID
                            // if any); otherwise fall back to whatever's in
                            // the launch inputs right now.
                            let (place_id, job_id) = self
                                .presets
                                .first()
                                .map(|(_, p)| (Some(p.place_id), p.job_id.clone()))
                                .unwrap_or_else(|| {
                                    let pid =
                                        self.main_panel_state.place_id_input.parse::<u64>().ok();
                                    let j = {
                                        let t = self.main_panel_state.job_id_input.trim();
                                        if t.is_empty() {
                                            None
                                        } else {
                                            Some(t.to_string())
                                        }
                                    };
                                    (pid, j)
                                });
                            if let Some(place_id) = place_id {
                                let acc_lookup = self
                                    .store
                                    .find_by_id(user_id)
                                    .filter(|account| account.can_launch())
                                    .map(|a| (a.user_id, a.encrypted_cookie.clone()));
                                if let (Some((uid, enc)), Some(session)) =
                                    (acc_lookup, self.session())
                                {
                                    if self.try_consume_launch_slot() {
                                        self.bridge.send(BackendCommand::LaunchGame {
                                            user_id: uid,
                                            encrypted_cookie: enc,
                                            session: session.clone(),
                                            use_credential_manager: self
                                                .config
                                                .use_credential_manager,
                                            place_id,
                                            job_id,
                                            data: if self
                                                .main_panel_state
                                                .data_input
                                                .trim()
                                                .is_empty()
                                            {
                                                None
                                            } else {
                                                Some(
                                                    self.main_panel_state
                                                        .data_input
                                                        .trim()
                                                        .to_string(),
                                                )
                                            },
                                            link_code: None,
                                            access_code: None,
                                            player_path: self
                                                .config
                                                .custom_player_paths
                                                .get(&uid)
                                                .cloned()
                                                .or_else(|| self.config.roblox_player_path.clone()),
                                            multi_instance: self.config.multi_instance_enabled,
                                            kill_background: self.config.kill_background_roblox,
                                            privacy: self.config.privacy_cleanup_options(),
                                        });
                                    }
                                }
                            } else {
                                self.toasts.push(Toast::error(
                                    "No preset or Place ID set. Enter one first.",
                                ));
                            }
                        }
                        sidebar::SidebarAction::AssignGroup { user_ids, group } => {
                            for uid in &user_ids {
                                if let Some(acc) = self.store.find_by_id_mut(*uid) {
                                    acc.group = group.clone();
                                }
                            }
                            self.auto_save();
                        }
                        sidebar::SidebarAction::CreateGroup {
                            name,
                            color,
                            assign_user_ids,
                        } => {
                            self.config.groups.insert(
                                name.clone(),
                                ram_core::models::GroupMeta {
                                    color,
                                    description: String::new(),
                                    sort_order: u32::MAX,
                                },
                            );
                            for uid in &assign_user_ids {
                                if let Some(acc) = self.store.find_by_id_mut(*uid) {
                                    acc.group = name.clone();
                                }
                            }
                            let _ = self.config.save(&self.config_path);
                            self.auto_save();
                        }
                        sidebar::SidebarAction::DeleteGroup(name) => {
                            self.config.groups.remove(&name);
                            for acc in &mut self.store.accounts {
                                if acc.group == name {
                                    acc.group = String::new();
                                }
                            }
                            self.sidebar_state.collapsed_groups.remove(&name);
                            let _ = self.config.save(&self.config_path);
                            self.auto_save();
                        }
                        sidebar::SidebarAction::EditGroup {
                            old_name,
                            new_name,
                            color,
                        } => {
                            let old_meta = self.config.groups.remove(&old_name);
                            let desc = old_meta
                                .as_ref()
                                .map(|m| m.description.clone())
                                .unwrap_or_default();
                            let old_sort = old_meta.map(|m| m.sort_order).unwrap_or(u32::MAX);
                            self.config.groups.insert(
                                new_name.clone(),
                                ram_core::models::GroupMeta {
                                    color,
                                    description: desc,
                                    sort_order: old_sort,
                                },
                            );
                            if old_name != new_name {
                                for acc in &mut self.store.accounts {
                                    if acc.group == old_name {
                                        acc.group = new_name.clone();
                                    }
                                }
                                if self.sidebar_state.collapsed_groups.remove(&old_name) {
                                    self.sidebar_state.collapsed_groups.insert(new_name.clone());
                                }
                            }
                            let _ = self.config.save(&self.config_path);
                            self.auto_save();
                        }
                        sidebar::SidebarAction::ReorderAccount {
                            user_id,
                            target_user_id,
                            insert_after,
                        } => {
                            // Move `user_id` before or after `target_user_id` within the
                            // same group (or both ungrouped). Reassign sort_order values.
                            let group = self
                                .store
                                .find_by_id(user_id)
                                .map(|a| a.group.clone())
                                .unwrap_or_default();
                            // Collect accounts in this group, sorted by current sort_order then name.
                            let mut peers: Vec<(u32, String, u64)> = self
                                .store
                                .accounts
                                .iter()
                                .filter(|a| a.group == group)
                                .map(|a| (a.sort_order, a.label().to_lowercase(), a.user_id))
                                .collect();
                            peers.sort();
                            let mut ids: Vec<u64> =
                                peers.into_iter().map(|(_, _, id)| id).collect();
                            // Remove the dragged account.
                            if let Some(drag_pos) = ids.iter().position(|id| *id == user_id) {
                                ids.remove(drag_pos);
                            }
                            // Find target and insert before or after it.
                            let target_pos = ids
                                .iter()
                                .position(|id| *id == target_user_id)
                                .unwrap_or(ids.len());
                            let insert_pos = if insert_after {
                                target_pos + 1
                            } else {
                                target_pos
                            };
                            ids.insert(insert_pos.min(ids.len()), user_id);
                            // Reassign sequential sort_order values.
                            for (i, uid) in ids.iter().enumerate() {
                                if let Some(acc) = self.store.find_by_id_mut(*uid) {
                                    acc.sort_order = i as u32;
                                }
                            }
                            self.auto_save();
                        }
                        sidebar::SidebarAction::ReorderGroup {
                            group_name,
                            target_group,
                            insert_after,
                        } => {
                            // Move `group_name` before or after `target_group`.
                            let mut ordered: Vec<(u32, String)> = self
                                .config
                                .groups
                                .iter()
                                .map(|(name, meta)| (meta.sort_order, name.clone()))
                                .collect();
                            ordered.sort();
                            let mut names: Vec<String> =
                                ordered.into_iter().map(|(_, n)| n).collect();
                            if let Some(pos) = names.iter().position(|n| *n == group_name) {
                                names.remove(pos);
                            }
                            let target_pos = names
                                .iter()
                                .position(|n| *n == target_group)
                                .unwrap_or(names.len());
                            let insert_pos = if insert_after {
                                target_pos + 1
                            } else {
                                target_pos
                            };
                            names.insert(insert_pos.min(names.len()), group_name);
                            for (i, name) in names.iter().enumerate() {
                                if let Some(meta) = self.config.groups.get_mut(name) {
                                    meta.sort_order = i as u32;
                                }
                            }
                            let _ = self.config.save(&self.config_path);
                        }
                        sidebar::SidebarAction::ResetCustomOrder => {
                            // Clear all custom sort_order values.
                            for acc in &mut self.store.accounts {
                                acc.sort_order = u32::MAX;
                            }
                            for meta in self.config.groups.values_mut() {
                                meta.sort_order = u32::MAX;
                            }
                            let _ = self.config.save(&self.config_path);
                            self.auto_save();
                        }
                        sidebar::SidebarAction::TogglePinAccount(user_id) => {
                            if let Some(acc) = self.store.find_by_id_mut(user_id) {
                                acc.is_pinned = !acc.is_pinned;
                                let pin_status = if acc.is_pinned { "pinned" } else { "unpinned" };
                                self.toasts
                                    .push(Toast::info(format!("Account {}", pin_status)));
                            }
                            self.auto_save();
                        }
                        sidebar::SidebarAction::SetCustomPath { user_ids, path } => {
                            let has_path = path.is_some();
                            for user_id in user_ids {
                                if let Some(path) = path.clone() {
                                    self.config.custom_player_paths.insert(user_id, path);
                                } else {
                                    self.config.custom_player_paths.remove(&user_id);
                                }
                            }
                            let _ = self.config.save(&self.config_path);
                            self.toasts.push(Toast::info(if has_path {
                                "Custom Roblox path set"
                            } else {
                                "Custom Roblox path cleared"
                            }));
                        }
                    }
                }
                // Persist sort mode if it changed.
                let current_mode = self.sidebar_state.sort_order.to_string();
                if self.config.sort_mode != current_mode {
                    self.config.sort_mode = current_mode;
                    let _ = self.config.save(&self.config_path);
                }
                let current_direction = self.sidebar_state.sort_direction.to_string();
                if self.config.sort_direction != current_direction {
                    self.config.sort_direction = current_direction;
                    let _ = self.config.save(&self.config_path);
                }
            });

        // Main panel — single selection shows detail, multi shows group panel
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.selected_ids.len() > 1 {
                // Group control panel
                let selected_accounts: Vec<&ram_core::models::Account> = self
                    .store
                    .accounts
                    .iter()
                    .filter(|a| self.selected_ids.contains(&a.user_id))
                    .collect();
                let preset_view: Vec<ram_core::models::LaunchPreset> =
                    self.presets.iter().map(|(_, p)| p.clone()).collect();
                let action = group_panel::show(
                    ui,
                    &selected_accounts,
                    &mut self.main_panel_state.place_id_input,
                    &mut self.main_panel_state.job_id_input,
                    &mut self.main_panel_state.data_input,
                    &preset_view,
                    self.roblox_running,
                    self.config.anonymize_names,
                    &self.common_inventory_items,
                    self.common_inventory_loading,
                    self.common_inventory_message.as_deref(),
                );
                if let Some(a) = action {
                    match a {
                        group_panel::GroupPanelAction::BulkLaunch {
                            place_id,
                            job_id,
                            data,
                        } => {
                            let accounts: Vec<(u64, Option<String>)> = self
                                .store
                                .accounts
                                .iter()
                                .filter(|a| {
                                    self.selected_ids.contains(&a.user_id) && a.can_launch()
                                })
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()))
                                .collect();
                            if accounts.is_empty() {
                                self.toasts
                                    .push(Toast::error("No selected accounts can be launched."));
                            } else if let Some(session) = self.session() {
                                self.bridge.send(BackendCommand::BulkLaunchEncrypted {
                                    accounts,
                                    session,
                                    use_credential_manager: self.config.use_credential_manager,
                                    place_id,
                                    job_id,
                                    data,
                                    link_code: None,
                                    access_code: None,
                                    multi_instance: self.config.multi_instance_enabled,
                                    kill_background: self.config.kill_background_roblox,
                                    privacy: self.config.privacy_cleanup_options(),
                                    player_path: self.config.roblox_player_path.clone(),
                                    launch_delay_secs: self.config.launch_delay_secs,
                                });
                            }
                        }
                        group_panel::GroupPanelAction::RevalidateSelected => {
                            if let Some(session) = self.session() {
                                let accounts: Vec<(u64, Option<String>)> = self
                                    .store
                                    .accounts
                                    .iter()
                                    .filter(|a| self.selected_ids.contains(&a.user_id))
                                    .map(|a| (a.user_id, a.encrypted_cookie.clone()))
                                    .collect();
                                if accounts.is_empty() {
                                    self.toasts
                                        .push(Toast::warning("Select an account to revalidate."));
                                } else {
                                    self.bridge.send(BackendCommand::RevalidateAll {
                                        accounts,
                                        session,
                                        use_credential_manager: self.config.use_credential_manager,
                                    });
                                }
                            }
                        }
                        group_panel::GroupPanelAction::OpenBrowsers => {
                            let ids: Vec<u64> = self.selected_ids.iter().copied().collect();
                            for user_id in ids {
                                self.open_browser_as(user_id);
                            }
                        }
                        group_panel::GroupPanelAction::CopyIds => {
                            let ids: Vec<String> = self
                                .selected_ids
                                .iter()
                                .copied()
                                .map(|id| id.to_string())
                                .collect();
                            ctx.output_mut(|o| o.copied_text = ids.join(", "));
                            self.toasts.push(Toast::info("Copied selected account IDs"));
                        }
                        group_panel::GroupPanelAction::FindCommonInventory => {
                            let accounts: Vec<(u64, Option<String>)> = selected_accounts
                                .iter()
                                .map(|account| (account.user_id, account.encrypted_cookie.clone()))
                                .collect();
                            if let Some(session) = self.session() {
                                self.common_inventory_items.clear();
                                self.common_inventory_message = None;
                                self.common_inventory_loading = true;
                                self.bridge.send(BackendCommand::FetchBulkCreations {
                                    accounts,
                                    session,
                                    use_credential_manager: self.config.use_credential_manager,
                                });
                            } else {
                                self.toasts.push(Toast::error(
                                    "Unlock the account store before checking inventory.",
                                ));
                            }
                        }
                        group_panel::GroupPanelAction::ClearSelection => {
                            self.selected_ids.clear();
                        }
                        group_panel::GroupPanelAction::OpenPathEditor => {
                            self.sidebar_state.path_editor = Some(sidebar::PathEditorState {
                                user_ids: self.selected_ids.iter().copied().collect(),
                                input: String::new(),
                            });
                        }
                        group_panel::GroupPanelAction::TileWindows => {
                            self.bridge.send(BackendCommand::ArrangeWindows {
                                options: self.config.tiling_options(),
                                delay_secs: 0,
                            });
                            self.toasts.push(Toast::info("Tiling windows..."));
                        }
                        group_panel::GroupPanelAction::KillAll => {
                            self.bridge.send(BackendCommand::KillAll);
                        }
                    }
                }
            } else if self.selected_ids.len() == 1 {
                let id = *self.selected_ids.iter().next().unwrap();
                let account = self.store.find_by_id(id).cloned();
                if let Some(account) = account {
                    let avatar_bytes = if self.config.anonymize_names {
                        self.anonymized_avatar_bytes.get(&account.user_id)
                    } else {
                        self.avatar_bytes.get(&account.user_id)
                    };
                    let preset_view: Vec<ram_core::models::LaunchPreset> =
                        self.presets.iter().map(|(_, p)| p.clone()).collect();
                    let player_path_label = self
                        .config
                        .custom_player_paths
                        .get(&account.user_id)
                        .map(|path| format!("Custom ({})", path.display()))
                        .unwrap_or_else(|| {
                            let path = self
                                .config
                                .roblox_player_path
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| "Auto-detect".to_string());
                            format!("Default ({path})")
                        });
                    let inventory_items = if self.account_inventory_user_id == Some(account.user_id)
                    {
                        self.account_inventory_items.clone()
                    } else {
                        Vec::new()
                    };
                    let inventory_loading = self.account_inventory_user_id == Some(account.user_id)
                        && self.account_inventory_loading;
                    let inventory_error = if self.account_inventory_user_id == Some(account.user_id)
                    {
                        self.account_inventory_error.as_deref()
                    } else {
                        None
                    };
                    let result = main_panel::show(
                        ui,
                        &account,
                        &mut self.main_panel_state,
                        self.roblox_running,
                        avatar_bytes,
                        &preset_view,
                        self.config.anonymize_names,
                        &player_path_label,
                        &inventory_items,
                        inventory_loading,
                        inventory_error,
                    );
                    self.tutorial.launch_btn_rect = result.launch_btn_rect;
                    if let Some(a) = result.action {
                        match a {
                            main_panel::MainPanelAction::OpenPathEditor => {
                                self.sidebar_state.path_editor = Some(sidebar::PathEditorState {
                                    user_ids: vec![account.user_id],
                                    input: self
                                        .config
                                        .custom_player_paths
                                        .get(&account.user_id)
                                        .map(|path| path.display().to_string())
                                        .unwrap_or_default(),
                                });
                            }
                            main_panel::MainPanelAction::LoadInventory(user_id) => {
                                tracing::debug!(user_id, "individual inventory refresh requested");
                                if let Some(session) = self.session() {
                                    self.account_inventory_user_id = Some(user_id);
                                    self.account_inventory_items.clear();
                                    self.account_inventory_error = None;
                                    self.account_inventory_loading = true;
                                    self.bridge.send(BackendCommand::FetchUserInventory {
                                        user_id,
                                        encrypted_cookie: account.encrypted_cookie.clone(),
                                        session,
                                        use_credential_manager: self.config.use_credential_manager,
                                    });
                                } else {
                                    self.toasts.push(Toast::error(
                                        "Unlock the account store before refreshing inventory.",
                                    ));
                                }
                            }
                            main_panel::MainPanelAction::LaunchGame {
                                place_id,
                                job_id,
                                data,
                            } => {
                                // Session first, so a locked store does not
                                // spend the launch-delay slot on a launch that
                                // cannot happen.
                                let ready =
                                    self.session().filter(|_| self.try_consume_launch_slot());
                                if let Some(session) = ready {
                                    self.bridge.send(BackendCommand::LaunchGame {
                                        user_id: account.user_id,
                                        encrypted_cookie: account.encrypted_cookie.clone(),
                                        session,
                                        use_credential_manager: self.config.use_credential_manager,
                                        place_id,
                                        job_id,
                                        data,
                                        link_code: None,
                                        access_code: None,
                                        player_path: self
                                            .config
                                            .custom_player_paths
                                            .get(&account.user_id)
                                            .cloned()
                                            .or_else(|| self.config.roblox_player_path.clone()),
                                        multi_instance: self.config.multi_instance_enabled,
                                        kill_background: self.config.kill_background_roblox,
                                        privacy: self.config.privacy_cleanup_options(),
                                    });
                                }
                            }
                            main_panel::MainPanelAction::RemoveAccount(uid) => {
                                self.confirm_remove = Some(uid);
                            }
                            main_panel::MainPanelAction::UpdateAlias { user_id, alias } => {
                                if let Some(acc) = self.store.find_by_id_mut(user_id) {
                                    acc.alias = alias;
                                }
                                self.auto_save();
                            }
                            main_panel::MainPanelAction::SavePreset {
                                name,
                                place_id,
                                job_id,
                                data,
                            } => {
                                let preset = ram_core::models::LaunchPreset {
                                    name,
                                    place_id,
                                    job_id,
                                    data,
                                };
                                match ram_core::presets::save(&crate::data_dir(), &preset, None) {
                                    Ok(_) => {
                                        self.toasts.push(Toast::success("Preset saved"));
                                        self.reload_presets();
                                    }
                                    Err(e) => {
                                        self.toasts.push(Toast::error(format!("Save failed: {e}")));
                                    }
                                }
                            }
                            main_panel::MainPanelAction::KillAll => {
                                self.bridge.send(BackendCommand::KillAll);
                            }
                            main_panel::MainPanelAction::OpenBrowserAs(uid) => {
                                self.open_browser_as(uid);
                            }
                        }
                    }
                } else {
                    main_panel::show_empty(ui);
                }
            } else {
                main_panel::show_empty(ui);
            }
        });

        // ---- Keyboard shortcuts ----
        let any_text_focused = ctx.memory(|m| m.focused().is_some());
        ctx.input(|i| {
            // Ctrl+A: select all accounts
            if i.modifiers.ctrl && i.key_pressed(egui::Key::A) && !any_text_focused {
                for acc in &self.store.accounts {
                    self.selected_ids.insert(acc.user_id);
                }
            }
            // Escape: clear selection
            if i.key_pressed(egui::Key::Escape) {
                self.selected_ids.clear();
            }
            // Delete: prompt to remove selected account(s)
            if i.key_pressed(egui::Key::Delete) && !any_text_focused && self.selected_ids.len() == 1
            {
                let uid = *self.selected_ids.iter().next().unwrap();
                self.confirm_remove = Some(uid);
            }
        });
    }

    fn show_private_servers_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let has_selection = !self.selected_ids.is_empty();
            let action = private_servers::show(
                ui,
                &mut self.private_servers_state,
                &self.config.private_servers,
                has_selection,
                &self.game_icon_bytes,
            );
            if let Some(a) = action {
                match a {
                    private_servers::PrivateServerAction::Add(server) => {
                        let idx = self.config.private_servers.len();
                        let place_id = server.place_id;
                        let universe_id = server.universe_id;
                        self.config.private_servers.push(server);
                        let _ = self.config.save(&self.config_path);
                        // Auto-resolve the place name
                        self.bridge.send(BackendCommand::ResolvePlace {
                            place_id,
                            universe_id,
                            index: idx,
                        });
                        self.toasts.push(Toast::success("Private server added"));
                    }
                    private_servers::PrivateServerAction::Remove(idx) => {
                        if idx < self.config.private_servers.len() {
                            self.config.private_servers.remove(idx);
                            let _ = self.config.save(&self.config_path);
                            self.toasts.push(Toast::info("Private server removed"));
                        }
                    }
                    private_servers::PrivateServerAction::Launch {
                        place_id,
                        link_code,
                        access_code,
                    } => {
                        let ac = if access_code.is_empty() {
                            None
                        } else {
                            Some(access_code.clone())
                        };
                        if self.selected_ids.len() == 1 {
                            let uid = *self.selected_ids.iter().next().unwrap();
                            let acc_lookup = self
                                .store
                                .find_by_id(uid)
                                .filter(|account| account.can_launch())
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()));
                            // Session first, so a locked store does not spend
                            // the launch-delay slot on a launch that cannot
                            // happen.
                            let ready = self.session().filter(|_| self.try_consume_launch_slot());
                            if let (Some((user_id, enc)), Some(session)) = (acc_lookup, ready) {
                                self.bridge.send(BackendCommand::LaunchGame {
                                    user_id,
                                    encrypted_cookie: enc,
                                    session,
                                    use_credential_manager: self.config.use_credential_manager,
                                    place_id,
                                    job_id: None,
                                    data: None,
                                    link_code: Some(link_code.clone()),
                                    access_code: ac.clone(),
                                    player_path: self
                                        .config
                                        .custom_player_paths
                                        .get(&user_id)
                                        .cloned()
                                        .or_else(|| self.config.roblox_player_path.clone()),
                                    multi_instance: self.config.multi_instance_enabled,
                                    kill_background: self.config.kill_background_roblox,
                                    privacy: self.config.privacy_cleanup_options(),
                                });
                            }
                        } else if self.selected_ids.len() > 1 {
                            let accounts: Vec<(u64, Option<String>)> = self
                                .store
                                .accounts
                                .iter()
                                .filter(|a| {
                                    self.selected_ids.contains(&a.user_id) && a.can_launch()
                                })
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()))
                                .collect();
                            if accounts.is_empty() {
                                self.toasts
                                    .push(Toast::error("No selected accounts can be launched."));
                            } else if let Some(session) = self.session() {
                                self.bridge.send(BackendCommand::BulkLaunchEncrypted {
                                    accounts,
                                    session,
                                    use_credential_manager: self.config.use_credential_manager,
                                    place_id,
                                    job_id: None,
                                    data: None,
                                    link_code: Some(link_code),
                                    access_code: ac,
                                    multi_instance: self.config.multi_instance_enabled,
                                    kill_background: self.config.kill_background_roblox,
                                    privacy: self.config.privacy_cleanup_options(),
                                    player_path: self.config.roblox_player_path.clone(),
                                    launch_delay_secs: self.config.launch_delay_secs,
                                });
                            }
                        }
                    }
                    private_servers::PrivateServerAction::Resolve(idx) => {
                        if let Some(server) = self.config.private_servers.get(idx) {
                            self.bridge.send(BackendCommand::ResolvePlace {
                                place_id: server.place_id,
                                universe_id: server.universe_id,
                                index: idx,
                            });
                        }
                    }
                    private_servers::PrivateServerAction::ResolveShareLink {
                        share_code,
                        server_name,
                    } => {
                        // Need an authenticated account to resolve share links
                        let acc = self
                            .store
                            .accounts
                            .first()
                            .map(|a| (a.user_id, a.encrypted_cookie.clone()));
                        if let (Some((first_user_id, enc)), Some(session)) = (acc, self.session()) {
                            self.bridge.send(BackendCommand::ResolveShareLink {
                                share_code,
                                server_name,
                                first_user_id,
                                encrypted_cookie: enc,
                                session,
                                use_credential_manager: self.config.use_credential_manager,
                            });
                            self.toasts.push(Toast::info("Resolving share link..."));
                        } else {
                            self.toasts.push(Toast::error(
                                "Add at least one account before using share links",
                            ));
                        }
                    }
                }
            }
        });
    }

    fn show_presets_tab(&mut self, ctx: &egui::Context) {
        let mut pending: Option<presets_panel::PresetsAction> = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            pending = presets_panel::show(ui, &mut self.presets_state, &self.presets);
        });
        // Handle the requested action outside the central-panel closure so we
        // can mutate other parts of self without conflicting borrows.
        let Some(action) = pending else { return };
        match action {
            presets_panel::PresetsAction::Save { path, preset } => {
                let data_dir = crate::data_dir();
                match ram_core::presets::save(&data_dir, &preset, path.as_deref()) {
                    Ok(_) => {
                        self.toasts.push(Toast::success("Preset saved"));
                        self.reload_presets();
                    }
                    Err(e) => {
                        self.toasts.push(Toast::error(format!("Save failed: {e}")));
                    }
                }
            }
            presets_panel::PresetsAction::Delete(path) => {
                match ram_core::presets::delete(&path) {
                    Ok(()) => {
                        self.toasts.push(Toast::info("Preset deleted"));
                        // If the editor was pointing at this file, clear it.
                        if self.presets_state.editing.as_deref() == Some(path.as_path()) {
                            self.presets_state = presets_panel::PresetsState::default();
                        }
                        self.reload_presets();
                    }
                    Err(e) => {
                        self.toasts
                            .push(Toast::error(format!("Delete failed: {e}")));
                    }
                }
            }
            presets_panel::PresetsAction::RevealFolder => {
                if let Err(e) = std::fs::create_dir_all(&self.presets_dir) {
                    self.toasts
                        .push(Toast::error(format!("Could not create folder: {e}")));
                    return;
                }
                let _ = std::process::Command::new("explorer")
                    .arg(&self.presets_dir)
                    .spawn();
            }
        }
    }

    fn show_utility_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.utility_warning_visible {
                let warning = egui::Color32::from_rgb(245, 200, 65);
                egui::Frame::default()
                    .fill(warning.gamma_multiply(0.22))
                    .stroke(egui::Stroke::new(1.0_f32, warning))
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                warning,
                                "Warning: Hyperion may detect multi-instance and window-management behavior.",
                            );
                            if ui.button("Close").clicked() {
                                self.utility_warning_visible = false;
                            }
                        });
                    });
            }
        });
    }

    // ------------------------------------------------------------------
    // Asset manager
    // ------------------------------------------------------------------

    /// Most uploads Roblox will accept at once. The spec calls 4 comfortable
    /// and throttles above ~8, but this app shares one client, one connection
    /// pool and one IP with the presence, avatar and revalidation timers, so
    /// leave headroom.
    const MAX_CONCURRENT_UPLOADS: usize = 3;

    /// Audio uploads run one at a time.
    ///
    /// Audio is the only kind with a per-account quota on top of the ordinary
    /// rate limit, and it is by a wide margin the slowest to ingest. Sending
    /// three at once is what made a bulk audio import collapse into a column of
    /// failures while the same files uploaded one by one without complaint.
    const MAX_CONCURRENT_AUDIO_UPLOADS: usize = 1;

    /// Minimum gap between starting two audio uploads. Serialising them is not
    /// enough on its own: back-to-back sends still trip the limiter, and its
    /// window is measured in seconds, not milliseconds.
    const AUDIO_UPLOAD_SPACING: Duration = Duration::from_secs(3);

    /// Largest poll batch per tick. Bounded so a big backlog becomes a steady
    /// trickle instead of one enormous burst.
    const MAX_POLL_BATCH: usize = 25;

    /// Reconcile the index with reality after a restart.
    ///
    /// A row left in `Uploading` means the app died between sending the command
    /// and hearing back, so no operation was ever confirmed. It goes back to
    /// `Queued`; the dedupe check runs again before anything is re-sent, which
    /// is what stops a crash from producing a duplicate asset. Rows in
    /// `Pending` keep their operation ID and simply resume polling.
    fn recover_asset_index(&mut self) {
        // Collected first so the duplicate lookup can borrow the index
        // immutably before anything is mutated.
        let interrupted: Vec<(String, String, ram_core::assets::Creator)> = self
            .asset_index
            .records
            .iter()
            .filter(|r| matches!(r.state, AssetState::Uploading))
            .map(|r| (r.row_id.clone(), r.file_sha256.clone(), r.creator))
            .collect();

        let changed = !interrupted.is_empty();
        for (row_id, sha256, creator) in interrupted {
            // If the previous run got far enough to hash the file and the same
            // bytes are already live under this creator, the upload evidently
            // succeeded. Re-sending would mint a second permanent asset.
            let existing = self
                .asset_index
                .find_uploaded(&sha256, creator)
                .and_then(|r| r.state.asset_id());
            if let Some(record) = self.asset_index.get_mut(&row_id) {
                record.state = match existing {
                    Some(asset_id) => AssetState::Duplicate { asset_id },
                    None => AssetState::Queued,
                };
            }
        }
        let expired =
            ram_core::assets::expire_stale_operations(&mut self.asset_index, chrono::Utc::now());
        if changed || !expired.is_empty() {
            self.save_asset_index();
        }
    }

    /// Write the index now, unless the file on disk is one we must not touch.
    fn save_asset_index(&mut self) {
        self.asset_index_dirty = false;
        self.last_asset_index_save = Some(std::time::Instant::now());
        if self.asset_index_read_only {
            return;
        }
        if let Err(e) = self.asset_index.save(&self.asset_index_path) {
            tracing::error!("failed to save asset index: {e}");
        }
    }

    /// Fill free upload slots from the queue, oldest first.
    ///
    /// Called on every asset event and on the frame timer, so a row whose
    /// retry or audio spacing has not come due yet is simply skipped and picked
    /// up by a later call.
    fn dispatch_next_uploads(&mut self) {
        if self.needs_unlock {
            return;
        }
        let in_flight = self
            .asset_index
            .records
            .iter()
            .filter(|r| matches!(r.state, AssetState::Uploading))
            .count();

        for _ in in_flight..Self::MAX_CONCURRENT_UPLOADS {
            let Some(job) = self.take_next_upload() else {
                break;
            };
            if job.kind == AssetKind::Audio {
                self.last_audio_upload = Some(Instant::now());
            }
            self.bridge.send(BackendCommand::UploadAsset(Box::new(job)));
        }
    }

    /// Whether another audio upload may start right now.
    fn audio_slot_free(&self) -> bool {
        let audio_in_flight = self
            .asset_index
            .records
            .iter()
            .filter(|r| matches!(r.state, AssetState::Uploading) && r.kind == AssetKind::Audio)
            .count();
        if audio_in_flight >= Self::MAX_CONCURRENT_AUDIO_UPLOADS {
            return false;
        }
        self.last_audio_upload
            .is_none_or(|last| last.elapsed() >= Self::AUDIO_UPLOAD_SPACING)
    }

    /// Claim the next queued row and build its job, marking it `Uploading` so
    /// the next call cannot claim it again.
    ///
    /// Loops rather than returning on the first duplicate: a queue whose head
    /// is all duplicates would otherwise stall with real work behind it.
    fn take_next_upload(&mut self) -> Option<UploadJob> {
        let now = chrono::Utc::now();
        let audio_ok = self.audio_slot_free();
        loop {
            // The loop always makes progress: the only path that continues has
            // just moved its row out of `Queued`, so it cannot be picked again.
            let row_id = self
                .asset_index
                .records
                .iter()
                .filter(|r| matches!(r.state, AssetState::Queued))
                // A row waiting out its retry backoff is not ready yet.
                .filter(|r| r.retry_at.is_none_or(|at| now >= at))
                .find(|r| audio_ok || r.kind != AssetKind::Audio)
                .map(|r| r.row_id.clone())?;

            let record = self.asset_index.get(&row_id)?;

            // A retry of a row that was already hashed and already landed must
            // not upload again. Assets are permanent and audio burns a
            // per-account quota, so the check is worth the linear scan.
            if let Some(asset_id) = self
                .asset_index
                .find_uploaded(&record.file_sha256, record.creator)
                .and_then(|r| r.state.asset_id())
            {
                if let Some(record) = self.asset_index.get_mut(&row_id) {
                    record.state = AssetState::Duplicate { asset_id };
                }
                self.asset_index_dirty = true;
                continue;
            }

            return self.build_upload_job(&row_id);
        }
    }

    fn build_upload_job(&mut self, row_id: &str) -> Option<UploadJob> {
        let session = self.session()?;
        let row_id = row_id.to_string();
        let record = self.asset_index.get(&row_id)?;
        let uploader = record.uploaded_by;
        let account = self.store.find_by_id(uploader)?;
        let encrypted_cookie = account.encrypted_cookie.clone();

        let job = UploadJob {
            row_id: row_id.clone(),
            user_id: uploader,
            encrypted_cookie,
            session: session.clone(),
            use_credential_manager: self.config.use_credential_manager,
            creator: record.creator,
            kind: record.kind,
            display_name: record.display_name.clone(),
            description: record.description.clone(),
            file_path: record.file_path.clone(),
        };

        if let Some(record) = self.asset_index.get_mut(&row_id) {
            record.state = AssetState::Uploading;
            record.attempts = record.attempts.saturating_add(1);
            record.retry_at = None;
        }
        self.asset_index_dirty = true;
        Some(job)
    }

    /// Ask about everything currently in moderation, grouped by account so one
    /// cookie decrypt covers a whole batch.
    fn dispatch_asset_poll(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        let now = chrono::Utc::now();
        let expired = ram_core::assets::expire_stale_operations(&mut self.asset_index, now);
        if !expired.is_empty() {
            self.save_asset_index();
        }

        let batch =
            ram_core::assets::next_poll_batch(&self.asset_index.records, now, Self::MAX_POLL_BATCH);
        if batch.is_empty() {
            return;
        }

        // Group by uploader. Polling per row would decrypt the same cookie
        // dozens of times and fan out into as many concurrent tasks.
        let mut by_account: HashMap<u64, Vec<(String, String)>> = HashMap::new();
        for (row_id, operation) in batch {
            let Some(record) = self.asset_index.get(&row_id) else {
                continue;
            };
            by_account
                .entry(record.uploaded_by)
                .or_default()
                .push((row_id, operation));
        }

        for (user_id, operations) in by_account {
            let Some(account) = self.store.find_by_id(user_id) else {
                continue;
            };
            self.bridge.send(BackendCommand::PollAssetOperations {
                user_id,
                encrypted_cookie: account.encrypted_cookie.clone(),
                session: session.clone(),
                use_credential_manager: self.config.use_credential_manager,
                operations,
            });
        }
    }

    /// Ask moderation about everything that has an asset id but no verdict.
    ///
    /// Grouped by uploader for the same reason operation polling is: one cookie
    /// decrypt per account rather than one per row.
    fn dispatch_moderation_poll(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        let now = chrono::Utc::now();
        let batch = ram_core::assets::next_review_batch(
            &self.asset_index.records,
            now,
            Self::MAX_POLL_BATCH,
        );
        if batch.is_empty() {
            return;
        }

        let mut by_account: HashMap<u64, Vec<(String, u64)>> = HashMap::new();
        for (row_id, asset_id) in batch {
            let Some(record) = self.asset_index.get(&row_id) else {
                continue;
            };
            by_account
                .entry(record.uploaded_by)
                .or_default()
                .push((row_id, asset_id));
        }

        for (user_id, assets) in by_account {
            let Some(account) = self.store.find_by_id(user_id) else {
                continue;
            };
            self.bridge.send(BackendCommand::PollAssetModeration {
                user_id,
                encrypted_cookie: account.encrypted_cookie.clone(),
                session: session.clone(),
                use_credential_manager: self.config.use_credential_manager,
                assets,
            });
        }
    }

    /// How long until the next poll, from the age of the oldest pending upload.
    fn asset_poll_interval(&self) -> Duration {
        let now = chrono::Utc::now();
        let oldest = self
            .asset_index
            .pending()
            .filter_map(|r| match &r.state {
                AssetState::Pending { since, .. } => Some(*since),
                _ => None,
            })
            .min();
        let age = oldest
            .map(|since| now.signed_duration_since(since))
            .and_then(|d| d.to_std().ok())
            .unwrap_or_default();
        ram_core::assets::poll_interval_for_age(age)
    }

    /// How long until the next moderation poll, from the age of the oldest
    /// asset still in review.
    fn moderation_poll_interval(&self) -> Duration {
        let now = chrono::Utc::now();
        let oldest = self
            .asset_index
            .in_review()
            .filter_map(|r| match &r.state {
                AssetState::InReview { since, .. } => Some(*since),
                _ => None,
            })
            .min();
        let age = oldest
            .map(|since| now.signed_duration_since(since))
            .and_then(|d| d.to_std().ok())
            .unwrap_or_default();
        ram_core::assets::review_poll_interval_for_age(age)
    }

    /// Largest thumbnail batch per request. Roblox accepts long ID lists, but
    /// a bounded batch keeps one screenful of a grid to a single call.
    const MAX_THUMBNAIL_BATCH: usize = 50;

    /// How long to wait before asking again for a thumbnail Roblox has not
    /// rendered. Long enough not to hammer, short enough that an asset fills in
    /// while the user is still looking at the same screen.
    const THUMBNAIL_RETRY: Duration = Duration::from_secs(20);

    /// Fetch thumbnails for assets that have none cached yet.
    ///
    /// Requests are remembered, not just results: without that, an asset whose
    /// thumbnail Roblox has not rendered (it answers `Pending`) would be
    /// re-requested on every single frame.
    fn request_asset_thumbnails(&mut self, wanted: &[u64]) {
        let now = std::time::Instant::now();
        let missing: Vec<u64> = wanted
            .iter()
            .copied()
            .filter(|id| !self.asset_thumbnails.contains_key(id))
            .filter(|id| !self.asset_thumbnails_inflight.contains(id))
            .filter(|id| {
                self.asset_thumbnails_retry_at
                    .get(id)
                    .is_none_or(|retry_at| now >= *retry_at)
            })
            .take(Self::MAX_THUMBNAIL_BATCH)
            .collect();
        if missing.is_empty() {
            return;
        }
        self.asset_thumbnails_inflight.extend(missing.iter());
        self.bridge
            .send(BackendCommand::FetchAssetThumbnails { asset_ids: missing });
    }

    /// Request one page of a creator's inventory.
    fn fetch_creations(
        &mut self,
        node: asset_manager::TreeNode,
        kind: ram_core::assets::AssetKind,
        cursor: Option<String>,
    ) {
        let asset_manager::TreeNode::Inventory(creator) = node else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let user_id = match creator {
            ram_core::assets::Creator::User(user_id) => user_id,
            ram_core::assets::Creator::Group(_) => {
                let Some(user_id) = self.asset_manager_state.acting_user_id else {
                    return;
                };
                user_id
            }
        };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        self.bridge.send(BackendCommand::FetchCreations {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            session: session.clone(),
            use_credential_manager: self.config.use_credential_manager,
            creator,
            kind,
            cursor,
        });
        tracing::debug!(
            user_id,
            ?creator,
            ?kind,
            "individual inventory request queued"
        );
    }

    /// Refresh the universe picker for the acting account, and optionally
    /// resolve a pasted place ID at the same time (one cookie decrypt covers
    /// both).
    fn fetch_universe_targets(&mut self, place_id: Option<u64>) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(user_id) = self.asset_manager_state.acting_user_id else {
            return;
        };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        self.bridge.send(BackendCommand::FetchUniverseTargets {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            session: session.clone(),
            use_credential_manager: self.config.use_credential_manager,
            place_id,
        });
    }

    /// Grant a universe `Use` on the given selection keys.
    ///
    /// The selection spans two key spaces, because the library and the
    /// inventory share one set: a library key is a local `row_id`, an inventory
    /// key is the asset ID of something already on Roblox. Resolving only the
    /// first meant a selection made in the inventory came out empty and
    /// reported itself as an unfinished upload.
    ///
    /// Local rows with no asset ID yet are genuinely skipped: there is nothing
    /// on Roblox to grant against.
    fn grant_universe_access(&mut self, universe_id: u64, keys: Vec<String>) {
        let Some(session) = self.session() else {
            return;
        };
        let mut asset_ids = Vec::new();
        let mut row_assets = Vec::new();
        let mut uploader = None;
        let mut unfinished = 0usize;

        for key in keys {
            match self.asset_index.get(&key) {
                Some(record) => match record.state.asset_id() {
                    Some(asset_id) => {
                        // A local row knows which account uploaded it, which
                        // for a group upload is not the acting account.
                        uploader.get_or_insert(record.uploaded_by);
                        asset_ids.push(asset_id);
                        row_assets.push((key, asset_id));
                    }
                    None => unfinished += 1,
                },
                // Not a local row, so it came from the inventory and the key is
                // the asset ID itself. `row_id` is a UUID, so the two key
                // spaces cannot collide.
                None => match key.parse::<u64>() {
                    Ok(asset_id) => asset_ids.push(asset_id),
                    Err(_) => continue,
                },
            }
        }

        if asset_ids.is_empty() {
            // Two different failures, and telling them apart is the whole point
            // of the message.
            self.toasts.push(Toast::info(if unfinished > 0 {
                "Nothing to grant. Those assets have not finished uploading."
            } else {
                "Nothing to grant. Select some assets first."
            }));
            return;
        }

        // Inventory assets have no uploader on record, so the grant is signed
        // by whichever account is browsing. That is the account whose rights
        // put the asset on screen in the first place.
        let uploader = uploader.or(self.asset_manager_state.acting_user_id);
        let Some(user_id) = uploader else {
            self.toasts
                .push(Toast::error("No account selected to sign the grant."));
            return;
        };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        self.bridge.send(BackendCommand::GrantAssetPermissions {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            session: session.clone(),
            use_credential_manager: self.config.use_credential_manager,
            universe_id,
            asset_ids,
            row_assets,
        });
    }

    /// The "Grant access to" modal, opened from the library selection footer.
    fn show_grant_dialog(&mut self, ctx: &egui::Context) {
        if !self.asset_manager_state.grant_open {
            return;
        }
        let mut open = true;
        let mut granted: Option<u64> = None;
        let mut resolve: Option<u64> = None;
        // Separate from `open`, which the window title bar's close button owns
        // for the duration of `show`.
        let mut cancelled = false;

        egui::Window::new("Grant universe access")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let count = self.asset_manager_state.selected.len();
                ui.label(format!("Let one experience use {count} selected asset(s)."));
                ui.add_space(8.0);

                ui.label("Experience:");
                asset_manager::universe_picker(
                    ui,
                    "grant_universe_pick",
                    &self.universe_targets,
                    &mut self.asset_manager_state.grant_universe,
                );
                if self.universe_targets.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Roblox did not return a list of your experiences. Paste an ID below.",
                        )
                        .small()
                        .color(ui.visuals().weak_text_color()),
                    );
                }

                ui.add_space(8.0);
                ui.label("Or paste a place or universe ID:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.asset_manager_state.grant_manual)
                            .desired_width(200.0)
                            .hint_text("ID or roblox.com/games/... link"),
                    );
                    let parsed =
                        ram_core::assets::parse_id_input(&self.asset_manager_state.grant_manual);
                    if ui
                        .add_enabled(parsed.is_some(), egui::Button::new("Use as universe"))
                        .clicked()
                    {
                        self.asset_manager_state.grant_universe = parsed;
                    }
                    if ui
                        .add_enabled(parsed.is_some(), egui::Button::new("Resolve as place"))
                        .on_hover_text("Look up which universe this place belongs to")
                        .clicked()
                    {
                        resolve = parsed;
                    }
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let target = self.asset_manager_state.grant_universe;
                    let grant =
                        egui::Button::new(egui::RichText::new("Grant").color(ui.theme().on_accent))
                            .fill(ui.visuals().selection.bg_fill);
                    if ui.add_enabled(target.is_some(), grant).clicked() {
                        granted = target;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        if let Some(place_id) = resolve {
            self.fetch_universe_targets(Some(place_id));
        }
        if let Some(universe_id) = granted {
            let rows: Vec<String> = self.asset_manager_state.selected.iter().cloned().collect();
            self.grant_universe_access(universe_id, rows);
            self.asset_manager_state.grant_open = false;
        } else if cancelled || !open {
            self.asset_manager_state.grant_open = false;
        }
    }

    /// Apply the result of an upload operation.
    ///
    /// An approval here means Roblox finished *ingesting* the file, not that
    /// the asset may be used, so the row lands on `InReview` and waits for a
    /// moderation verdict. The one exception is an asset type that never needs
    /// review, and that is decided by the verdict itself rather than guessed at
    /// from the kind.
    fn apply_operation_outcome(&mut self, row_id: &str, outcome: OperationOutcome) {
        let now = chrono::Utc::now();
        let Some(record) = self.asset_index.get_mut(row_id) else {
            return;
        };
        let name = record.display_name.clone();
        match outcome {
            // Nothing changed; the row stays Pending and the timer keeps asking.
            OperationOutcome::StillPending => return,
            OperationOutcome::Approved {
                asset_id,
                revision_id,
            } => {
                record.state = AssetState::InReview {
                    asset_id,
                    revision_id,
                    since: now,
                };
                record.updated_at = Some(now);
                record.retry_at = None;
                // No toast here. Assets that skip review reach Approved in the
                // same breath and would toast twice, and for the ones that do
                // wait, "uploaded" is the claim that was misleading in the
                // first place. The row shows "In review" with a spinner; the
                // toast belongs to the verdict.
            }
            OperationOutcome::Rejected { reason } => {
                record.state = AssetState::Rejected {
                    reason: reason.clone(),
                };
                record.updated_at = Some(now);
                self.toasts
                    .push(Toast::error(format!("{name} was rejected: {reason}")));
            }
            OperationOutcome::Failed { message, retryable } => {
                // Deliberately not routed through `fail_upload`: that re-sends
                // the file, and by the time an operation exists Roblox may
                // already hold the asset. `DEADLINE_EXCEEDED` in particular
                // means "no answer", not "nothing happened", and a silent
                // re-send there mints a duplicate and burns another slice of
                // the audio quota. The user gets a Retry button instead.
                record.state = AssetState::Failed {
                    message: message.clone(),
                    retryable,
                };
                record.updated_at = Some(now);
                self.toasts
                    .push(Toast::error(format!("{name} failed: {message}")));
            }
        }
        self.save_asset_index();
        self.dispatch_next_uploads();
    }

    /// Apply a moderation verdict to a row that is waiting on one.
    fn apply_moderation_status(&mut self, row_id: &str, status: ModerationStatus) {
        let now = chrono::Utc::now();
        let Some(record) = self.asset_index.get_mut(row_id) else {
            return;
        };
        // Only a row actually in review can take a verdict. Anything else is a
        // late answer for a row the user has since retried or removed.
        let AssetState::InReview {
            asset_id,
            revision_id,
            ..
        } = record.state
        else {
            return;
        };
        let name = record.display_name.clone();

        match status {
            // Still waiting. The timer keeps asking.
            ModerationStatus::InReview => return,
            ModerationStatus::Approved => {
                record.state = AssetState::Approved {
                    asset_id,
                    revision_id,
                };
                record.updated_at = Some(now);
                let auto_grant = record.auto_grant_universe;
                self.toasts
                    .push(Toast::success(format!("{name} approved as {asset_id}")));
                // Requirement: an asset that clears moderation is granted to
                // the batch's universe with no further clicks. Deliberately
                // here and not on upload: granting a universe access to an
                // asset that moderation later blocks is a grant that silently
                // does nothing.
                if let Some(universe_id) = auto_grant {
                    self.grant_universe_access(universe_id, vec![row_id.to_string()]);
                }
            }
            ModerationStatus::Rejected => {
                let reason = format!("Moderation blocked asset {asset_id}");
                record.state = AssetState::Rejected {
                    reason: reason.clone(),
                };
                record.updated_at = Some(now);
                self.toasts
                    .push(Toast::error(format!("{name} was rejected: {reason}")));
            }
        }
        self.save_asset_index();
    }

    /// Record an upload failure, and put the row back in the queue if it is
    /// worth another attempt.
    ///
    /// Only for failures raised before an operation existed, where nothing was
    /// created on Roblox and re-sending is therefore free of consequence. The
    /// automatic re-send is the difference between a bulk batch that rides out
    /// a rate limit and one that leaves a screen of dead rows. Attempts are
    /// counted on the record, so a row cannot loop: once the budget is spent it
    /// stays `Failed` and waits for the user.
    fn fail_upload(&mut self, row_id: &str, message: String, retryable: bool) {
        let now = chrono::Utc::now();
        let Some(record) = self.asset_index.get_mut(row_id) else {
            return;
        };
        let name = record.display_name.clone();
        let attempts = record.attempts;

        if retryable && attempts < ram_core::assets::MAX_UPLOAD_ATTEMPTS {
            let wait = ram_core::assets::upload_retry_backoff(attempts.saturating_sub(1));
            record.state = AssetState::Queued;
            record.retry_at = Some(
                now + chrono::Duration::from_std(wait)
                    .unwrap_or_else(|_| chrono::TimeDelta::seconds(30)),
            );
            record.updated_at = Some(now);
            tracing::info!(
                "upload of {row_id} failed ({message}); attempt {attempts} of {}, retrying in {wait:?}",
                ram_core::assets::MAX_UPLOAD_ATTEMPTS
            );
        } else {
            record.state = AssetState::Failed {
                message: message.clone(),
                retryable,
            };
            record.retry_at = None;
            record.updated_at = Some(now);
            self.toasts
                .push(Toast::error(format!("{name} failed: {message}")));
        }

        self.save_asset_index();
        self.dispatch_next_uploads();
    }

    fn show_asset_manager_tab(&mut self, ctx: &egui::Context) {
        // OS drag and drop. Read once per frame here so the panel stays
        // ctx-free, matching how every other panel is structured.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });

        let mut result = asset_manager::AssetManagerResult::default();
        let mut tree_action = None;

        // The tree only makes sense next to the library. Hiding it in the
        // queue gives the queue's wide File Path column the room it needs.
        if self.asset_manager_state.view == asset_manager::View::Library {
            egui::SidePanel::left("asset_tree")
                .default_width(200.0)
                .width_range(150.0..=340.0)
                .resizable(true)
                .show(ctx, |ui| {
                    let mut cx = asset_manager::AssetsCtx {
                        state: &mut self.asset_manager_state,
                        index: &mut self.asset_index,
                        accounts: &self.store.accounts,
                        anonymize: self.config.anonymize_names,
                        universes: &self.universe_targets,
                        groups: &self.publish_groups,
                        remote: &self.remote_inventory,
                        thumbnails: &self.asset_thumbnails,
                        unlocked: self.store_session.is_some()
                            || self.config.use_credential_manager,
                        read_only: self.asset_index_read_only,
                    };
                    tree_action = asset_manager::show_tree(ui, &mut cx);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut cx = asset_manager::AssetsCtx {
                state: &mut self.asset_manager_state,
                index: &mut self.asset_index,
                accounts: &self.store.accounts,
                anonymize: self.config.anonymize_names,
                universes: &self.universe_targets,
                groups: &self.publish_groups,
                remote: &self.remote_inventory,
                thumbnails: &self.asset_thumbnails,
                unlocked: self.store_session.is_some() || self.config.use_credential_manager,
                read_only: self.asset_index_read_only,
            };
            result = asset_manager::show(ui, &mut cx);
        });
        result.action = result.action.or(tree_action);

        // Fetch thumbnails for what was actually drawn. Batched and
        // request-once, so scrolling a large grid does not re-ask every frame.
        self.request_asset_thumbnails(&result.want_thumbnails);

        // Populate the universe picker the first time the tab is opened for an
        // account, so "Grant access to" is not empty when the user reaches it.
        if !self.needs_unlock
            && self.asset_manager_state.acting_user_id.is_some()
            && self.universe_targets_user != self.asset_manager_state.acting_user_id
        {
            self.universe_targets_user = self.asset_manager_state.acting_user_id;
            self.universe_targets.clear();
            self.publish_groups.clear();
            self.fetch_universe_targets(None);
            if let Some(user_id) = self.asset_manager_state.acting_user_id {
                let enc = self
                    .store
                    .find_by_id(user_id)
                    .map(|a| a.encrypted_cookie.clone());
                if let (Some(encrypted_cookie), Some(session)) = (enc, self.session()) {
                    self.bridge.send(BackendCommand::FetchPublishGroups {
                        user_id,
                        encrypted_cookie,
                        session,
                        use_credential_manager: self.config.use_credential_manager,
                    });
                }
            }
        }

        if result.index_changed {
            self.asset_index_dirty = true;
        }
        if !dropped.is_empty() {
            self.asset_manager_state.view = asset_manager::View::ImportQueue;
            self.stage_files(dropped);
        }

        // Handled after the central panel closes so `self` can be mutated
        // freely, per the borrow note on `show_presets_tab`.
        let Some(action) = result.action else { return };
        match action {
            asset_manager::AssetManagerAction::PickFiles => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter(
                        "Roblox assets",
                        &[
                            "png", "jpg", "jpeg", "bmp", "tga", "mp3", "ogg", "wav", "flac", "fbx",
                            "gltf", "glb", "rbxm", "rbxmx", "mp4", "mov",
                        ],
                    )
                    .add_filter("All files", &["*"])
                    .pick_files()
                {
                    self.stage_files(paths);
                }
            }
            asset_manager::AssetManagerAction::RemoveRow(row_id) => {
                self.asset_index.remove(&row_id);
                self.asset_manager_state.checked.remove(&row_id);
                self.save_asset_index();
            }
            asset_manager::AssetManagerAction::ClearFinished => {
                self.asset_index.records.retain(|r| !r.state.is_terminal());
                self.save_asset_index();
            }
            asset_manager::AssetManagerAction::RetryRow(row_id) => {
                if let Some(record) = self.asset_index.get_mut(&row_id) {
                    record.state = AssetState::Queued;
                    // A hand-pressed retry is a fresh decision, so it gets a
                    // fresh budget and goes out now rather than serving out the
                    // backoff of the automatic attempts that preceded it.
                    record.attempts = 0;
                    record.retry_at = None;
                }
                self.save_asset_index();
                self.dispatch_next_uploads();
            }
            asset_manager::AssetManagerAction::RequestUpload(rows) => {
                self.asset_manager_state.confirm_upload = Some(rows.len());
                self.pending_upload_rows = rows;
            }
            asset_manager::AssetManagerAction::LoadInventory { node, filter } => {
                // "All types" is a fan-out, not a filter: the listing endpoint
                // requires an assetType, so one request per kind is the only
                // way to honor the label.
                let kinds: Vec<ram_core::assets::AssetKind> = match filter {
                    Some(kind) => vec![kind],
                    None => ram_core::assets::AssetKind::selectable().to_vec(),
                };
                self.remote_inventory = asset_manager::RemoteInventory {
                    node: Some(node),
                    filter,
                    requested: true,
                    inflight: kinds.len(),
                    ..Default::default()
                };
                for kind in kinds {
                    self.fetch_creations(node, kind, None);
                }
            }
            asset_manager::AssetManagerAction::LoadMoreInventory => {
                let Some(node) = self.remote_inventory.node else {
                    return;
                };
                // Advance every kind that still has a page left.
                let pending: Vec<(ram_core::assets::AssetKind, String)> =
                    self.remote_inventory.cursors.drain().collect();
                self.remote_inventory.inflight += pending.len();
                for (kind, cursor) in pending {
                    self.fetch_creations(node, kind, Some(cursor));
                }
            }
            asset_manager::AssetManagerAction::RevealFile(path) => {
                // `/select,` highlights the file rather than just opening its
                // folder. Explorer wants a native separator here.
                let _ = std::process::Command::new("explorer")
                    .arg(format!("/select,{}", path.display()))
                    .spawn();
            }
            asset_manager::AssetManagerAction::OpenGrantDialog => {
                self.asset_manager_state.grant_open = true;
                // Refresh the picker each time it opens: the acting account may
                // have changed since the last fetch.
                if self.universe_targets_user != self.asset_manager_state.acting_user_id {
                    self.fetch_universe_targets(None);
                }
            }
        }
    }

    /// Hash and queue a batch of files. Anything unsupported still gets a row,
    /// marked invalid with the reason, rather than being silently dropped: a
    /// file that vanishes from a drop of twenty reads as a bug.
    fn stage_files(&mut self, paths: Vec<PathBuf>) {
        let Some(user_id) = self.asset_manager_state.acting_user_id else {
            self.toasts
                .push(Toast::error("Select an account to upload from first"));
            return;
        };
        let creator = self
            .asset_manager_state
            .batch_creator
            .unwrap_or(ram_core::assets::Creator::User(user_id));
        let now = chrono::Utc::now();
        let mut added = 0usize;
        let mut duplicates = 0usize;

        for path in paths {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let row_id = uuid::Uuid::new_v4().to_string();

            let (kind, invalid) = match ram_core::assets::validate_file(&path, size) {
                Ok((kind, _)) => (kind, None),
                Err(reason) => (ram_core::assets::AssetKind::Other, Some(reason)),
            };

            let mut record = ram_core::assets::AssetRecord::staged(
                row_id.clone(),
                ram_core::assets::StagedFile {
                    path,
                    // Hashing happens on the backend thread just before upload,
                    // so a queue of large files does not freeze the UI here.
                    sha256: String::new(),
                    bytes: size,
                    kind,
                },
                creator,
                user_id,
                now,
            );

            if let Some(reason) = invalid {
                record.state = AssetState::Invalid { reason };
            } else if let Some(existing) = self
                .asset_index
                .records
                .iter()
                .find(|r| r.file_path == record.file_path && r.creator == creator)
                .and_then(|r| r.state.asset_id())
            {
                // Same file, same creator, already uploaded. Flag it rather
                // than silently re-uploading: assets are permanent and audio
                // burns a per-account quota.
                record.state = AssetState::Duplicate { asset_id: existing };
                duplicates += 1;
            } else {
                self.asset_manager_state.checked.insert(row_id);
                added += 1;
            }
            self.asset_index.records.push(record);
        }

        self.save_asset_index();
        if duplicates > 0 {
            self.toasts.push(Toast::info(format!(
                "{added} file(s) queued, {duplicates} already uploaded"
            )));
        } else if added > 0 {
            self.toasts
                .push(Toast::info(format!("{added} file(s) queued")));
        }
    }

    /// Confirmation before any batch. Uploads are permanent, public, and
    /// moderated under a real account, so this is not a formality.
    fn show_upload_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(count) = self.asset_manager_state.confirm_upload else {
            return;
        };
        let total_bytes: u64 = self
            .pending_upload_rows
            .iter()
            .filter_map(|id| self.asset_index.get(id))
            .map(|r| r.file_bytes)
            .sum();
        let creator = self
            .pending_upload_rows
            .first()
            .and_then(|id| self.asset_index.get(id))
            .map(|r| r.creator);
        let creator_text = match creator {
            Some(ram_core::assets::Creator::User(id)) => self
                .store
                .find_by_id(id)
                .map(|a| a.label().to_string())
                .unwrap_or_else(|| format!("user {id}")),
            Some(ram_core::assets::Creator::Group(id)) => format!("group {id}"),
            None => "the selected account".to_string(),
        };

        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new("Confirm upload")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Upload {count} file(s), {:.1} MB in total, as {creator_text}.",
                    total_bytes as f64 / (1024.0 * 1024.0)
                ));
                ui.add_space(4.0);
                ui.colored_label(
                    ui.theme().warning,
                    "This cannot be undone. Every asset is permanent, public, and moderated \
                     under that account.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let upload = egui::Button::new(
                        egui::RichText::new("Upload").color(ui.theme().on_accent),
                    )
                    .fill(ui.visuals().selection.bg_fill);
                    if ui.add(upload).clicked() {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        if confirmed {
            let rows = std::mem::take(&mut self.pending_upload_rows);
            let auto_grant = self.asset_manager_state.auto_grant_universe;
            for row_id in rows {
                if let Some(record) = self.asset_index.get_mut(&row_id) {
                    record.state = AssetState::Queued;
                    // Stamped per row at confirm time, not read from UI state
                    // later, so changing the selector mid-batch cannot retarget
                    // uploads that are already in flight.
                    record.auto_grant_universe = auto_grant;
                }
            }
            self.asset_manager_state.confirm_upload = None;
            self.save_asset_index();
            self.dispatch_next_uploads();
        } else if cancelled || !open {
            self.asset_manager_state.confirm_upload = None;
            self.pending_upload_rows.clear();
        }
    }

    fn show_settings_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let has_password = self
                .store_session
                .as_ref()
                .is_some_and(|s| s.needs_password());
            let before = self.config.clone();
            let action = settings::show(
                ui,
                &mut self.config,
                has_password,
                &mut self.settings_state,
                self.roblox_running,
            );
            let changed = self.config != before;
            match action {
                Some(settings::SettingsAction::SaveConfig) => {
                    if let Err(e) = self.config.save(&self.config_path) {
                        self.toasts
                            .push(Toast::error(format!("Save failed: {e}")));
                    } else {
                        self.toasts.push(Toast::success("Settings saved"));
                    }
                }
                Some(settings::SettingsAction::SetStartupWithWindows(enabled)) => {
                    match crate::startup::set_enabled(enabled) {
                        Ok(()) => self.toasts.push(Toast::success(if enabled {
                            "RM will start with Windows"
                        } else {
                            "RM will not start with Windows"
                        })),
                        Err(e) => {
                            self.config.startup_with_windows = !enabled;
                            self.toasts.push(Toast::error(format!(
                                "Could not update Windows startup: {e}"
                            )));
                        }
                    }
                }
                Some(settings::SettingsAction::RotateMacAddress) => {
                    match ram_core::process::rotate_mac_address(
                        self.config.mac_preserve_oui,
                        &self.config.mac_alternate_oui,
                    ) {
                        Ok(()) => self.toasts.push(Toast::success("MAC address rotated")),
                        Err(e) => self.toasts.push(Toast::error(format!("MAC rotation failed: {e}"))),
                    }
                }
                Some(settings::SettingsAction::EnableMultiInstance) => {
                    if self.roblox_running {
                        // Kill tray processes first, then check again
                        ram_core::process::kill_tray_roblox();
                        // Brief wait for the OS to reap terminated processes
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        // Re-check after killing tray processes
                        let still_running = ram_core::process::is_roblox_running();
                        if still_running {
                            self.toasts.push(Toast::error(
                                "Close all Roblox instances (including tray) before enabling multi-instance.",
                            ));
                            // Don't enable — the checkbox was toggled but we
                            // leave config unchanged, so next frame it resets.
                        } else {
                            // Tray killed, nothing else running — safe to acquire
                            match ram_core::process::enable_multi_instance() {
                                Ok(()) => {
                                    self.config.multi_instance_enabled = true;
                                    self.toasts.push(Toast::success("Multi-instance enabled"));
                                }
                                Err(e) => {
                                    self.toasts.push(Toast::error(format!("Failed: {e}")));
                                }
                            }
                        }
                    } else {
                        match ram_core::process::enable_multi_instance() {
                            Ok(()) => {
                                self.config.multi_instance_enabled = true;
                                self.toasts.push(Toast::success("Multi-instance enabled"));
                            }
                            Err(e) => {
                                self.toasts.push(Toast::error(format!("Failed: {e}")));
                            }
                        }
                    }
                }
                Some(settings::SettingsAction::DisableMultiInstance) => {
                    self.config.multi_instance_enabled = false;
                    self.toasts.push(Toast::info("Multi-instance disabled (takes effect after restart)"));
                }
                Some(settings::SettingsAction::ChangePassword { ref new_password }) => {
                    // Rewraps the data key under the new password. The old
                    // version walked every account re-encrypting cookies one by
                    // one and swallowed failures, which left any cookie that
                    // failed to decrypt stranded on the previous password with
                    // no way back. Nothing per-account happens here at all now.
                    self.rekey_store(Some(new_password.clone()));
                }
                Some(settings::SettingsAction::ClearPassword) => {
                    // Rewraps under the device key. The old version merely
                    // cleared the in-memory password, which left the file on
                    // disk encrypted under a password the app no longer had —
                    // and silently stopped saving, because `auto_save` was
                    // gated on that string being non-empty.
                    self.rekey_store(None);
                }
                Some(settings::SettingsAction::TileWindowsNow) => {
                    self.bridge.send(BackendCommand::ArrangeWindows {
                        options: self.config.tiling_options(),
                        delay_secs: 0,
                    });
                    self.toasts.push(Toast::info("Tiling windows..."));
                }
                Some(settings::SettingsAction::OpenDataFolder) => {
                    let data_dir = crate::data_dir();
                    let result = std::process::Command::new("explorer")
                        .arg(&data_dir)
                        .spawn();
                    if let Err(e) = result {
                        self.toasts
                            .push(Toast::error(format!("Could not open RM data folder: {e}")));
                    }
                }
                Some(settings::SettingsAction::CleanOrphanedData) => {
                    let known_user_ids = self
                        .store
                        .accounts
                        .iter()
                        .map(|account| account.user_id)
                        .collect();
                    match crate::browser_login::clean_orphaned_browse_as_data(
                        &crate::data_dir(),
                        &known_user_ids,
                    ) {
                        Ok(0) => self.toasts.push(Toast::info("No orphaned data found")),
                        Ok(count) => self.toasts.push(Toast::success(format!(
                            "Removed {count} orphaned browse-as profile(s)"
                        ))),
                        Err(e) => self.toasts.push(Toast::error(format!(
                            "Could not clean orphaned data: {e}"
                        ))),
                    }
                }
                None => {}
            }
            if action.is_some() || changed {
                if let Err(e) = self.config.save(&self.config_path) {
                    self.toasts
                        .push(Toast::error(format!("Save failed: {e}")));
                }
            }
        });
    }

    fn show_add_dialog(&mut self, ctx: &egui::Context) {
        // Reset the per-step tutorial highlight every frame so stale rects
        // from a previous dialog step don't continue to glow after the user
        // has moved on (e.g., advanced from Choose → Browser, or closed the
        // dialog entirely). The Choose-step renderer below re-populates it.
        self.tutorial.browser_login_btn_rect = egui::Rect::NOTHING;

        if !self.add_dialog.open {
            return;
        }

        // While the embedded login window is open we need the UI to keep
        // ticking so the mpsc receiver below gets polled even without user
        // input. Request a repaint a few times a second.
        if self.add_dialog.browser_login_pending {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        // Poll the embedded-login receiver for a completed outcome.
        if let Some(rx) = &self.add_dialog.browser_login_rx {
            match rx.try_recv() {
                Ok(crate::browser_login::LoginOutcome::Success(cookie)) => {
                    self.add_dialog.cookie_input = cookie;
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                    self.add_dialog.last_error = None;
                    // Nothing left for the user to confirm: encryption sets
                    // itself up. Send the cookie straight to the backend rather
                    // than making them click "Add" redundantly.
                    let cookie = self.add_dialog.cookie_input.trim().to_string();
                    if let Some(session) = self.ensure_session() {
                        self.add_dialog.loading = true;
                        self.bridge.send(BackendCommand::AddAccount {
                            cookie,
                            session,
                            use_credential_manager: self.config.use_credential_manager,
                        });
                    }
                }
                Ok(crate::browser_login::LoginOutcome::Cancelled) => {
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                }
                Ok(crate::browser_login::LoginOutcome::Failed(e)) => {
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                    self.add_dialog.last_error = Some(format!("Browser login failed: {e}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                }
            }
        }

        // -----------------------------------------------------------------
        // Moderation-warning short-circuit. When validation came back with an
        // active moderation we render a confirm pane instead of the usual
        // add flow. Buttons signal back via these flags so we can mutate
        // self after the borrow on add_dialog ends.
        // -----------------------------------------------------------------
        let mut open = self.add_dialog.open;
        let mut mod_open_browser = false;
        let mut mod_add_anyway = false;
        let mut mod_cancel = false;
        let mut mod_revalidate = false;

        if self.add_dialog.pending_moderated.is_some() {
            let pending = self.add_dialog.pending_moderated.as_deref().unwrap();
            egui::Window::new("Account moderated")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(420.0)
                .show(ctx, |ui| {
                    let acc = &pending.account;
                    let info = acc.moderation.as_ref().expect("moderation present");
                    let banned = info.is_banned;
                    let title_color = if banned {
                        ui.theme().danger_text
                    } else {
                        ui.theme().warning_text
                    };
                    ui.colored_label(
                        title_color,
                        egui::RichText::new(if banned {
                            "\u{26a0} This account is terminated."
                        } else {
                            "\u{26a0} This account is currently moderated."
                        })
                        .strong()
                        .size(15.0),
                    );
                    ui.add_space(4.0);
                    if !self.config.anonymize_names {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} (@{})",
                                acc.display_name, acc.username
                            ))
                            .color(ui.visuals().weak_text_color()),
                        );
                    }
                    if let Some(reason) = &info.reason {
                        ui.add_space(6.0);
                        ui.label(reason);
                    }
                    match &info.expires_at {
                        Some(exp) => {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Expires: {}",
                                    exp.format("%Y-%m-%d %H:%M UTC")
                                ))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                            );
                        }
                        None if banned => {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Permanent termination.")
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        _ => {}
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button("\u{1f310} Open browser as")
                            .on_hover_text(
                                "Sign in via webview to view the full moderation message or appeal",
                            )
                            .clicked()
                        {
                            mod_open_browser = true;
                        }
                        if ui
                            .button("Re-validate")
                            .on_hover_text(
                                "Re-check moderation status. Use after resolving a warning or appeal in the browser.",
                            )
                            .clicked()
                        {
                            mod_revalidate = true;
                        }
                        if ui.button("Add anyway").clicked() {
                            mod_add_anyway = true;
                        }
                        if ui.button("Cancel").clicked() {
                            mod_cancel = true;
                        }
                    });
                });
            self.add_dialog.open = open;

            if mod_open_browser {
                let user_id = self
                    .add_dialog
                    .pending_moderated
                    .as_deref()
                    .map(|p| p.account.user_id);
                let enc = self
                    .add_dialog
                    .pending_moderated
                    .as_deref()
                    .and_then(|p| p.encrypted_cookie.clone());
                if let Some(uid) = user_id {
                    let label = if self.config.anonymize_names {
                        format!("#{uid}")
                    } else {
                        self.add_dialog
                            .pending_moderated
                            .as_deref()
                            .map(|p| p.account.username.clone())
                            .unwrap_or_default()
                    };
                    let profile_dir = crate::data_dir()
                        .join("webview_browse_as")
                        .join(uid.to_string());
                    if let Some(session) = self.session() {
                        self.bridge.send(BackendCommand::BrowseAsAccount {
                            user_id: uid,
                            encrypted_cookie: enc,
                            session,
                            use_credential_manager: self.config.use_credential_manager,
                            profile_dir,
                            label,
                            destination_url: None,
                        });
                    }
                }
            }
            if mod_revalidate {
                // User likely just resolved a warning in the browser. Decrypt
                // the cookie we kept on the pending entry and re-run the
                // AddAccount cycle from scratch — same flow as if they'd
                // pasted the cookie fresh, so a clean account now skips the
                // moderation confirm.
                if let Some(pending) = self.add_dialog.pending_moderated.take() {
                    let session = self.session();
                    let raw_cookie = if self.config.use_credential_manager {
                        ram_core::crypto::credential_load(pending.account.user_id).ok()
                    } else {
                        pending
                            .encrypted_cookie
                            .as_ref()
                            .zip(session.as_ref())
                            .and_then(|(enc, s)| ram_core::crypto::decrypt_cookie(enc, s).ok())
                    };
                    match raw_cookie.zip(session) {
                        Some((cookie, session)) => {
                            self.add_dialog.loading = true;
                            self.add_dialog.last_error = None;
                            self.bridge.send(BackendCommand::AddAccount {
                                cookie,
                                session,
                                use_credential_manager: self.config.use_credential_manager,
                            });
                        }
                        None => {
                            // Couldn't recover the cookie — put the pending
                            // entry back so the dialog stays usable, and tell
                            // the user.
                            self.add_dialog.pending_moderated = Some(pending);
                            self.toasts.push(Toast::error(
                                "Couldn't re-decrypt the cookie. Cancel and re-add manually.",
                            ));
                        }
                    }
                }
            }
            if mod_add_anyway {
                if let Some(pending) = self.add_dialog.pending_moderated.take() {
                    let name = if self.config.anonymize_names {
                        "Account".to_string()
                    } else {
                        pending.account.username.clone()
                    };
                    self.store.remove_by_id(pending.account.user_id);
                    self.store.accounts.push(pending.account);
                    self.toasts.push(Toast::success(format!("Added {name}")));
                    self.add_dialog.open = false;
                    self.add_dialog.cookie_input.clear();
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                    self.tutorial
                        .advance_from(tutorial::TutorialStep::EnterCookie);
                    self.auto_save();
                }
            }
            if mod_cancel || !self.add_dialog.open {
                // Clean up: if we stored a credential during validation, drop
                // it so we don't leak an orphan secret in the OS keyring.
                if self.config.use_credential_manager {
                    if let Some(pending) = self.add_dialog.pending_moderated.as_deref() {
                        let _ = ram_core::crypto::credential_delete(pending.account.user_id);
                    }
                }
                self.add_dialog.pending_moderated = None;
                self.add_dialog.open = false;
                self.add_dialog.cookie_input.clear();
                self.add_dialog.browser_login_pending = false;
                self.add_dialog.browser_login_rx = None;
            }
            return;
        }

        let dialog_title = if self
            .add_dialog
            .last_error
            .as_deref()
            .is_some_and(|message| message.starts_with("Moderated\n"))
        {
            "Moderated"
        } else {
            "Add Account"
        };

        egui::Window::new(dialog_title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(360.0)
            .show(ctx, |ui| {
                match self.add_dialog.step {
                    AddAccountStep::Choose => {
                        ui.label("How would you like to add this account?");
                        ui.add_space(10.0);

                        let full_w = ui.available_width();
                        let browser_btn_resp = ui.add_sized(
                            [full_w, 48.0],
                            egui::Button::new(
                                egui::RichText::new("🌐  Log in with browser")
                                    .size(15.0),
                            ),
                        );
                        self.tutorial.browser_login_btn_rect = browser_btn_resp.rect;
                        if browser_btn_resp.clicked() {
                            let (tx, rx) = std::sync::mpsc::channel();
                            let profile_dir = crate::data_dir().join("webview_profile");
                            // Wipe the profile between attempts so stale sessions don't leak.
                            let _ = std::fs::remove_dir_all(&profile_dir);
                            crate::browser_login::spawn(profile_dir, tx);
                            self.add_dialog.browser_login_rx = Some(rx);
                            self.add_dialog.browser_login_pending = true;
                            self.add_dialog.last_error = None;
                            self.add_dialog.step = AddAccountStep::Browser;
                        }
                        ui.add_space(6.0);
                        if ui
                            .add_sized(
                                [full_w, 48.0],
                                egui::Button::new(
                                    egui::RichText::new("📋  Paste cookie manually")
                                        .size(15.0),
                                ),
                            )
                            .clicked()
                        {
                            self.add_dialog.step = AddAccountStep::Manual;
                            self.add_dialog.last_error = None;
                        }
                        ui.add_space(6.0);
                        if ui
                            .add_sized(
                                [full_w, 48.0],
                                egui::Button::new(
                                    egui::RichText::new("📥  Bulk import")
                                        .size(15.0),
                                ),
                            )
                            .on_hover_text(
                                "Paste many cookies at once, one per line or comma-separated",
                            )
                            .clicked()
                        {
                            self.add_dialog.step = AddAccountStep::Bulk;
                            self.add_dialog.last_error = None;
                            self.add_dialog.bulk_input.clear();
                        }
                    }

                    AddAccountStep::Browser => {
                        if ui
                            .add_enabled(
                                !self.add_dialog.loading,
                                egui::Button::new("Back").small(),
                            )
                            .clicked()
                        {
                            self.add_dialog.step = AddAccountStep::Choose;
                            self.add_dialog.cookie_input.clear();
                            self.add_dialog.browser_login_rx = None;
                            self.add_dialog.browser_login_pending = false;
                            self.add_dialog.last_error = None;
                        }
                        ui.add_space(8.0);

                        if self.add_dialog.browser_login_pending {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Sign in to Roblox in the opened window.");
                            });
                        } else if !self.add_dialog.cookie_input.is_empty() {
                            ui.label(
                                egui::RichText::new("Cookie captured.")
                                    .color(ui.theme().success_text),
                            );
                        } else {
                            ui.label("Sign-in canceled.");
                            ui.add_space(6.0);
                            if ui.button("\u{1f310} Try again").clicked() {
                                let (tx, rx) = std::sync::mpsc::channel();
                                let profile_dir = crate::data_dir().join("webview_profile");
                                let _ = std::fs::remove_dir_all(&profile_dir);
                                crate::browser_login::spawn(profile_dir, tx);
                                self.add_dialog.browser_login_rx = Some(rx);
                                self.add_dialog.browser_login_pending = true;
                                self.add_dialog.last_error = None;
                            }
                        }
                        ui.add_space(8.0);
                    }

                    AddAccountStep::Manual => {
                        if ui
                            .add_enabled(
                                !self.add_dialog.loading,
                                egui::Button::new("Back").small(),
                            )
                            .clicked()
                        {
                            self.add_dialog.step = AddAccountStep::Choose;
                            self.add_dialog.cookie_input.clear();
                            self.add_dialog.last_error = None;
                        }
                        ui.add_space(8.0);

                        // Multiline because long cookies (~2000 chars) make a
                        // singleline TextEdit oscillate width frame-to-frame.
                        // password(true) still masks the value as dots.
                        let cookie_edit =
                            egui::TextEdit::multiline(&mut self.add_dialog.cookie_input)
                                .password(true)
                                .desired_width(f32::INFINITY)
                                .hint_text("Paste your .ROBLOSECURITY cookie");
                        ui.add_enabled(!self.add_dialog.loading, cookie_edit);
                        ui.add_space(8.0);
                    }

                    AddAccountStep::Bulk => {
                        let busy = self.add_dialog.bulk_running
                            && (self.add_dialog.bulk_succeeded
                                + self.add_dialog.bulk_failed)
                                < self.add_dialog.bulk_total;

                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new("Back").small(),
                            )
                            .clicked()
                        {
                            self.add_dialog.step = AddAccountStep::Choose;
                            self.add_dialog.bulk_input.clear();
                            self.add_dialog.bulk_running = false;
                            self.add_dialog.bulk_queue.clear();
                            self.add_dialog.bulk_total = 0;
                            self.add_dialog.bulk_succeeded = 0;
                            self.add_dialog.bulk_failed = 0;
                            self.add_dialog.last_error = None;
                        }
                        ui.add_space(8.0);

                        if self.add_dialog.bulk_running {
                            let done = self.add_dialog.bulk_succeeded
                                + self.add_dialog.bulk_failed;
                            let total = self.add_dialog.bulk_total;
                            if done < total {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(format!(
                                        "Importing {done}/{total}...",
                                    ));
                                });
                            } else {
                                ui.label(format!(
                                    "Done: {} added, {} failed.",
                                    self.add_dialog.bulk_succeeded,
                                    self.add_dialog.bulk_failed,
                                ));
                                ui.add_space(8.0);
                                if ui.button("Close").clicked() {
                                    self.add_dialog.open = false;
                                    self.add_dialog.bulk_running = false;
                                    self.add_dialog.bulk_input.clear();
                                    self.add_dialog.bulk_queue.clear();
                                    self.add_dialog.bulk_total = 0;
                                    self.add_dialog.bulk_succeeded = 0;
                                    self.add_dialog.bulk_failed = 0;
                                    self.add_dialog.step = AddAccountStep::Choose;
                                }
                            }
                        } else {
                            ui.label(
                                "Paste one cookie per line, or comma-separated:",
                            );
                            ui.add_space(4.0);
                            if ui
                                .button("\u{1f4c2}  Browse file...")
                                .on_hover_text(
                                    "Load cookies from a .txt or .csv file",
                                )
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Text/CSV", &["txt", "csv", "tsv"])
                                    .add_filter("All files", &["*"])
                                    .pick_file()
                                {
                                    match std::fs::read_to_string(&path) {
                                        Ok(contents) => {
                                            // Append rather than replace so the user can
                                            // combine multiple sources without losing prior paste.
                                            if !self.add_dialog.bulk_input.is_empty()
                                                && !self
                                                    .add_dialog
                                                    .bulk_input
                                                    .ends_with('\n')
                                            {
                                                self.add_dialog.bulk_input.push('\n');
                                            }
                                            self.add_dialog.bulk_input.push_str(&contents);
                                        }
                                        Err(e) => {
                                            self.toasts.push(Toast::error(format!(
                                                "Failed to read {}: {e}",
                                                path.display(),
                                            )));
                                        }
                                    }
                                }
                            }
                            ui.add_space(4.0);
                            ui.add(
                                egui::TextEdit::multiline(
                                    &mut self.add_dialog.bulk_input,
                                )
                                .password(true)
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .hint_text(
                                    "Paste .ROBLOSECURITY cookies",
                                ),
                            );
                            let count = parse_bulk_cookies(
                                &self.add_dialog.bulk_input,
                            )
                            .len();
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{count} cookie(s) detected",
                                ))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                            );
                            ui.add_space(8.0);

                            // No password prompt: `dispatch_next_bulk` sets up
                            // device-locked encryption on its own.
                            if ui
                                .add_enabled(
                                    count > 0,
                                    egui::Button::new(format!(
                                        "Import {count} account(s)",
                                    )),
                                )
                                .clicked()
                            {
                                let mut cookies = parse_bulk_cookies(
                                    &self.add_dialog.bulk_input,
                                );
                                // Reverse so pop() yields paste order.
                                cookies.reverse();
                                self.add_dialog.bulk_total = cookies.len();
                                self.add_dialog.bulk_succeeded = 0;
                                self.add_dialog.bulk_failed = 0;
                                self.add_dialog.bulk_queue = cookies;
                                self.add_dialog.bulk_running = true;
                                self.add_dialog.last_error = None;
                                self.dispatch_next_bulk();
                            }
                        }
                    }
                }

                // Shared footer — error and submit. Skipped on Choose (nothing
                // to submit) and Bulk (handles its own submit/progress UI).
                //
                // There is no master-password field here any more: a new store
                // is device-locked, which needs nothing from the user. Setting a
                // password is now a deliberate choice made in Settings.
                if matches!(
                    self.add_dialog.step,
                    AddAccountStep::Choose | AddAccountStep::Bulk
                ) {
                    return;
                }

                if let Some(err) = &self.add_dialog.last_error {
                    ui.add_space(4.0);
                    ui.set_max_width(360.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("⚠ {err}"))
                                .color(ui.theme().danger),
                        )
                        .wrap(),
                    );
                    ui.add_space(4.0);
                }

                if self.add_dialog.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Validating cookie...");
                    });
                } else {
                    let valid = !self.add_dialog.cookie_input.trim().is_empty();
                    let button_label = if self.add_dialog.last_error.is_some() {
                        "Retry"
                    } else {
                        "Add"
                    };
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(valid, egui::Button::new(button_label))
                            .clicked()
                        {
                            let cookie = self.add_dialog.cookie_input.trim().to_string();
                            // Sets up device-locked encryption on the first add.
                            if let Some(session) = self.ensure_session() {
                                self.add_dialog.loading = true;
                                self.add_dialog.last_error = None;
                                self.add_dialog.rejected_cookie = None;
                                self.bridge.send(BackendCommand::AddAccount {
                                    cookie,
                                    session,
                                    use_credential_manager: self.config.use_credential_manager,
                                });
                            }
                        }
                        // When the backend rejected the cookie, give the user
                        // a way to investigate (e.g. see the moderation page)
                        // without leaving the app.
                        if self.add_dialog.rejected_cookie.is_some()
                            && ui
                                .button("\u{1f310} Open browser as")
                                .on_hover_text(
                                    "Open a webview signed in with this cookie to see why it was rejected",
                                )
                                .clicked()
                        {
                            if let Some(cookie) =
                                self.add_dialog.rejected_cookie.clone()
                            {
                                // Temp investigation profile, wiped each call so
                                // we never carry state across separate cookies.
                                let profile_dir =
                                    crate::data_dir().join("webview_investigate");
                                let _ = std::fs::remove_dir_all(&profile_dir);
                                if let Err(e) =
                                    crate::browser_login::spawn_browse_as(
                                        profile_dir,
                                        cookie,
                                        "investigation".to_string(),
                                    )
                                {
                                    self.toasts.push(Toast::error(format!(
                                        "Browser launch failed: {e}"
                                    )));
                                } else {
                                    self.toasts.push(Toast::info(
                                        "Opening browser to investigate the cookie...",
                                    ));
                                }
                            }
                        }
                        if self.add_dialog.rejected_cookie.is_some()
                            && ui
                                .button("Add anyway")
                                .on_hover_text(
                                    "Save the account even though validation failed (terminated alts, pending warnings, etc.)",
                                )
                                .clicked()
                        {
                            self.add_dialog.force_add_form_open =
                                !self.add_dialog.force_add_form_open;
                            if self.add_dialog.force_add_form_open {
                                self.add_dialog.force_add_username.clear();
                            }
                        }
                    });

                    // Inline "add anyway" form — username lookup is required
                    // because validate_cookie didn't run, so we have no
                    // user_id / display_name from Roblox yet.
                    if self.add_dialog.force_add_form_open
                        && self.add_dialog.rejected_cookie.is_some()
                    {
                        ui.add_space(6.0);
                        egui::Frame::default()
                            .inner_margin(egui::Margin::same(8.0))
                            .rounding(egui::Rounding::same(4.0))
                            .fill(ui.visuals().faint_bg_color)
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            ))
                            .show(ui, |ui: &mut egui::Ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new("Add anyway").strong(),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Enter the account's Roblox username so we can identify it. \
                                         The cookie will be stored as-is and marked expired \
                                         until you resolve the moderation in a browser.",
                                    )
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                                );
                                ui.add_space(6.0);
                                let txt = ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.add_dialog.force_add_username,
                                    )
                                    .hint_text("Username")
                                    .desired_width(f32::INFINITY),
                                );
                                let enter = txt.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    let name_ok = !self
                                        .add_dialog
                                        .force_add_username
                                        .trim()
                                        .is_empty();
                                    let go = ui
                                        .add_enabled(name_ok, egui::Button::new("Add"))
                                        .clicked();
                                    if (go || (enter && name_ok))
                                        && self
                                            .add_dialog
                                            .rejected_cookie
                                            .is_some()
                                    {
                                        let cookie = self
                                            .add_dialog
                                            .rejected_cookie
                                            .clone()
                                            .unwrap();
                                        let username = self
                                            .add_dialog
                                            .force_add_username
                                            .trim()
                                            .to_string();
                                        if let Some(session) = self.ensure_session() {
                                            self.add_dialog.loading = true;
                                            self.add_dialog.last_error = None;
                                            self.bridge.send(
                                                BackendCommand::AddAccountForced {
                                                    cookie,
                                                    username,
                                                    session,
                                                    use_credential_manager: self
                                                        .config
                                                        .use_credential_manager,
                                                },
                                            );
                                        }
                                    }
                                    if ui.button("Cancel").clicked() {
                                        self.add_dialog.force_add_form_open = false;
                                    }
                                });
                            });
                    }
                }
            });
        self.add_dialog.open = open;
    }

    fn show_confirm_remove_dialog(&mut self, ctx: &egui::Context) {
        let Some(uid) = self.confirm_remove else {
            return;
        };
        let label = if self.config.anonymize_names {
            "this account".to_string()
        } else {
            self.store
                .find_by_id(uid)
                .map(|a| a.label().to_string())
                .unwrap_or_else(|| uid.to_string())
        };

        let mut keep_open = true;
        egui::Window::new("Confirm Removal")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Remove account \"{label}\"? This cannot be undone."
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("🗑  Remove").clicked() {
                        self.bridge
                            .send(BackendCommand::RemoveAccount { user_id: uid });
                        keep_open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep_open = false;
                    }
                });
            });
        if !keep_open {
            self.confirm_remove = None;
        }
    }

    fn show_browse_launch_prompt(&mut self, ctx: &egui::Context) {
        let Some(place_id) = self.browse_launch_prompt else {
            return;
        };

        let mut open = true;
        let mut confirm = false;
        let mut close = false;
        egui::Window::new("Roblox launch blocked")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Launching Roblox from the browser does not work here.");
                ui.label("We pre-populated the Place ID field for you.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        confirm = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        if confirm {
            self.main_panel_state.place_id_input = place_id.to_string();
            self.active_tab = Tab::Accounts;
            self.browse_launch_prompt = None;
        } else if close || !open {
            self.browse_launch_prompt = None;
        }
    }

    /// The one-time offer, shown to an existing password user after they
    /// unlock, to switch this PC over to automatic unlocking.
    ///
    /// Deliberately a question rather than a silent migration: dropping a
    /// password someone chose to set is a security-relevant change, and the
    /// answer is remembered either way so it is asked exactly once.
    fn show_passwordless_offer_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_passwordless_offer {
            return;
        }
        let mut open = true;
        let mut accept = false;
        let mut decline = false;

        egui::Window::new("Stop asking for your password?")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(430.0);
                ui.label(
                    "RM can unlock your accounts automatically on this PC, so you never have \
                     to type a master password again.",
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Your accounts stay encrypted either way. The difference is where the \
                         key comes from: Windows Credential Manager instead of your password.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Keep the password if you want the store to stay unreadable even to \
                         someone using your Windows account.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Stop asking on this PC").clicked() {
                        accept = true;
                    }
                    if ui.button("Keep my password").clicked() {
                        decline = true;
                    }
                });
            });

        if accept {
            self.rekey_store(None);
        }
        if accept || decline || !open {
            self.dismiss_passwordless_offer();
        }
    }

    /// Recovery options on the unlock screen.
    ///
    /// The original version of this dialog offered exactly one action, wiping
    /// the store, on the premise that a failed unlock means a forgotten
    /// password. It does not: an AES-GCM authentication failure is equally
    /// consistent with a damaged file, which `crypto` recovers from
    /// automatically using the very `.bak` that wiping deletes. So this dialog
    /// leads with the non-destructive options, names the files, and puts the
    /// wipe behind a typed confirmation.
    fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_recovery {
            return;
        }
        let store_path = self.config.accounts_path.clone();
        let backup = ram_core::storage::backup_path(&store_path);
        let folder = store_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let device_mode = self.device_key_missing;

        let mut open = true;
        let mut wipe = false;
        let mut cancel = false;
        let mut reveal = false;

        egui::Window::new("Recovery Options")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(460.0);

                if device_mode {
                    ui.label(
                        "This store is locked to a PC whose key is no longer available, so \
                         there is no password that will open it here.",
                    );
                    ui.add_space(6.0);
                    ui.label(
                        "If you still have the original PC, copy the store back from there. \
                         Otherwise the accounts will have to be added again.",
                    );
                } else {
                    ui.label("Two different things produce this error:");
                    ui.add_space(6.0);
                    ui.label("• The password is wrong. Try other passwords first.");
                    ui.label(
                        "• The file is damaged. RM already tried the backup automatically, \
                         so this is the less likely of the two.",
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "There is no way to recover the accounts without the password. \
                             The encryption has no back door, by design.",
                        )
                        .small()
                        .weak(),
                    );
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    egui::RichText::new("Before wiping anything, copy these files somewhere safe:")
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("{}\n{}", store_path.display(), backup.display()))
                        .small()
                        .monospace(),
                );
                ui.add_space(6.0);
                if !folder.is_empty() && ui.button("📂  Open containing folder").clicked() {
                    reveal = true;
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    egui::RichText::new("Start over with an empty account store")
                        .strong()
                        .color(ui.theme().danger),
                );
                ui.add_space(4.0);
                ui.label(
                    "This permanently deletes the store and its backup, and any cookies RM \
                     saved to Windows Credential Manager. It cannot be undone. Your settings, \
                     presets and private servers are kept.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Type");
                    ui.label(egui::RichText::new("DELETE").strong().monospace());
                    ui.label("to confirm:");
                });
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.recovery_confirm_input)
                        .hint_text("DELETE")
                        .desired_width(140.0),
                );

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let confirmed = self.recovery_confirm_input.trim() == "DELETE";
                    if ui
                        .add_enabled(
                            confirmed,
                            egui::Button::new("🗑  Delete everything and start over"),
                        )
                        .clicked()
                    {
                        wipe = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if reveal {
            self.reveal_in_file_manager(&store_path);
        }
        self.show_recovery = open && !cancel && !wipe;
        if cancel || wipe || !open {
            self.recovery_confirm_input.clear();
        }
        if wipe {
            self.wipe_account_store();
        }
    }

    /// Open the store's folder in the OS file manager so the user can copy the
    /// files out before wiping.
    fn reveal_in_file_manager(&mut self, path: &std::path::Path) {
        let Some(folder) = path.parent() else {
            return;
        };
        #[cfg(windows)]
        let result = std::process::Command::new("explorer").arg(folder).spawn();
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(folder).spawn();
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open").arg(folder).spawn();

        if let Err(e) = result {
            tracing::warn!("Could not open {}: {e}", folder.display());
            self.toasts
                .push(Toast::error(format!("Could not open {}", folder.display())));
        }
    }

    /// Delete the account store, its backup and every credential RM owns, then
    /// carry on in-process with an empty store.
    ///
    /// Deliberately does not relaunch. Spawning a replacement and calling
    /// `exit(0)` raced the process being replaced: the child runs
    /// `AppState::new`, which re-acquires the Roblox singleton mutex, while the
    /// parent still holds it, so multi-instance silently switched itself off.
    /// `exit(0)` also skipped the config save. Resetting state in place has
    /// neither problem and lands the user on the same first-run screen.
    fn wipe_account_store(&mut self) {
        let store_path = self.config.accounts_path.clone();

        // Credential Manager entries first, while the roster that names them is
        // still loaded. Deleting the store first would orphan them permanently:
        // nothing else records which user IDs RM created.
        let mut orphaned = 0usize;
        for account in &self.store.accounts {
            if let Err(e) = ram_core::crypto::credential_delete(account.user_id) {
                tracing::warn!(
                    "Could not delete the credential for user {}: {e}",
                    account.user_id
                );
                orphaned += 1;
            }
        }
        if let Err(e) = ram_core::crypto::delete_device_key() {
            tracing::warn!("Could not delete the device key: {e}");
        }

        // Both copies, and any temp file a crashed write left behind.
        let mut failed = Vec::new();
        for path in [
            store_path.clone(),
            ram_core::storage::backup_path(&store_path),
        ] {
            match std::fs::remove_file(&path) {
                Ok(()) => tracing::info!("Deleted {}", path.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!("Could not delete {}: {e}", path.display());
                    failed.push(path);
                }
            }
        }

        if !failed.is_empty() {
            // Report exactly what survived. The previous version claimed total
            // failure after having already deleted the backup, which left the
            // user believing their data was intact when the recovery copy was
            // already gone.
            let names: Vec<String> = failed
                .iter()
                .map(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.display().to_string())
                })
                .collect();
            self.toasts.push(Toast::error(format!(
                "Could not delete {}. Delete it by hand, then restart RM.",
                names.join(" and ")
            )));
            return;
        }

        // Reset in place. `store_session` is dropped here, which zeroes the
        // data key it held.
        self.store = AccountStore::default();
        self.store_session = None;
        self.needs_unlock = false;
        self.unlocking = false;
        self.device_key_missing = false;
        self.pending_legacy_upgrade = false;
        self.show_passwordless_offer = false;
        self.unlock_password_input.clear();
        self.unlock_password_used.clear();
        self.selected_ids.clear();
        self.avatar_bytes.clear();
        self.anonymized_avatar_bytes.clear();
        self.active_tab = Tab::Accounts;

        // A store created from here on is passwordless, so the one-time offer
        // has nothing left to ask about.
        self.config.offered_passwordless = true;
        if let Err(e) = self.config.save(&self.config_path) {
            tracing::warn!("Could not save config after wiping the store: {e}");
        }

        if orphaned > 0 {
            self.toasts.push(Toast::error(format!(
                "Account store deleted, but {orphaned} saved credential(s) could not be removed"
            )));
        } else {
            self.toasts.push(Toast::success(
                "Account store deleted. Add an account to start over.",
            ));
        }
    }

    fn show_changelog_window(&mut self, ctx: &egui::Context) {
        if !self.show_changelog {
            return;
        }
        let mut open = true;
        egui::Window::new(format!("What's New in v{}", env!("CARGO_PKG_VERSION")))
            .open(&mut open)
            .resizable(true)
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        let changelog = include_str!("../../CHANGELOG.md");
                        // Show only the section for the current version
                        let current = format!("## v{}", env!("CARGO_PKG_VERSION"));
                        let section = if let Some(start) = changelog.find(&current) {
                            let rest = &changelog[start..];
                            let end = rest[current.len()..]
                                .find("\n## v")
                                .map(|i| i + current.len())
                                .unwrap_or(rest.len());
                            &rest[..end]
                        } else {
                            changelog
                        };
                        // Render markdown-lite
                        for line in section.lines() {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                ui.add_space(2.0);
                            } else if let Some(h) = trimmed.strip_prefix("### ") {
                                ui.add_space(4.0);
                                ui.strong(h);
                            } else if let Some(h) = trimmed.strip_prefix("## ") {
                                ui.heading(h);
                            } else if let Some(item) = trimmed.strip_prefix("- ") {
                                Self::render_md_line(ui, &format!("  • {item}"));
                            } else {
                                Self::render_md_line(ui, trimmed);
                            }
                        }
                    });
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    self.show_changelog = false;
                }
            });
        if !open {
            self.show_changelog = false;
        }
    }

    /// Render a single line with **bold** spans converted to egui RichText.
    fn render_md_line(ui: &mut egui::Ui, line: &str) {
        let mut job = egui::text::LayoutJob::default();
        let style = ui.style();
        let normal_color = style.visuals.text_color();
        let normal_font = egui::FontId::proportional(14.0);
        let bold_font = egui::FontId {
            size: 14.0,
            family: egui::FontFamily::Proportional,
        };

        let mut remaining = line;
        while let Some(start) = remaining.find("**") {
            // Text before the bold marker
            let before = &remaining[..start];
            if !before.is_empty() {
                job.append(
                    before,
                    0.0,
                    egui::text::TextFormat {
                        font_id: normal_font.clone(),
                        color: normal_color,
                        ..Default::default()
                    },
                );
            }
            remaining = &remaining[start + 2..];
            // Find the closing **
            if let Some(end) = remaining.find("**") {
                let bold_text = &remaining[..end];
                job.append(
                    bold_text,
                    0.0,
                    egui::text::TextFormat {
                        font_id: bold_font.clone(),
                        color: normal_color,
                        italics: false,
                        ..Default::default()
                    },
                );
                remaining = &remaining[end + 2..];
            } else {
                // No closing ** — just emit the rest as normal
                job.append(
                    &format!("**{remaining}"),
                    0.0,
                    egui::text::TextFormat {
                        font_id: normal_font.clone(),
                        color: normal_color,
                        ..Default::default()
                    },
                );
                remaining = "";
            }
        }
        // Remaining plain text
        if !remaining.is_empty() {
            job.append(
                remaining,
                0.0,
                egui::text::TextFormat {
                    font_id: normal_font,
                    color: normal_color,
                    ..Default::default()
                },
            );
        }
        ui.label(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ram_core::instances::{Attribution, TrackedInstance};
    use ram_core::models::Account;

    /// A client RM read the launchtime off, which is the normal case.
    fn instance(pid: u32, user_id: u64) -> TrackedInstance {
        TrackedInstance {
            pid,
            start_time: 1_000,
            user_id,
            place_id: 606,
            launchtime: 1_700_000_000_000 + pid as i64,
            launched_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            attribution: Attribution::Exact,
        }
    }

    /// A client whose command line could not be read, paired by appearance
    /// order.
    fn guessed(pid: u32, user_id: u64) -> TrackedInstance {
        TrackedInstance {
            attribution: Attribution::Inferred,
            ..instance(pid, user_id)
        }
    }

    fn account(user_id: u64, username: &str) -> Account {
        Account::new(user_id, username.to_string(), username.to_string())
    }

    #[test]
    fn nothing_tracked_says_so_without_claiming_there_is_nothing_running() {
        let summary = attribution_summary(&[], &[account(7, "alice")], false, 3);
        assert_eq!(
            summary,
            "None of these were launched by RM, or they have not been \
             matched to an account yet."
        );
        // The early return means the count is not mentioned at all, which is
        // the point: with no mappings there is nothing to say about which of
        // the three is whose.
        assert!(!summary.contains('3'), "{summary}");
    }

    #[test]
    fn each_tracked_pid_is_listed_against_its_account_label() {
        let accounts = vec![account(7, "alice"), account(8, "bob")];
        let tracked = vec![instance(1234, 7), instance(5678, 8)];

        let summary = attribution_summary(&tracked, &accounts, false, 2);

        assert_eq!(
            summary,
            "Launched by RM:\n  PID 1234 - alice\n  PID 5678 - bob"
        );
    }

    /// The heading no longer hedges, so the hedge has to live on the individual
    /// line that earned it. A confirmed line and a guessed line must not read
    /// the same.
    #[test]
    fn only_the_guessed_lines_are_marked_as_guesses() {
        let accounts = vec![account(7, "alice"), account(8, "bob")];
        let tracked = vec![instance(1234, 7), guessed(5678, 8)];

        let summary = attribution_summary(&tracked, &accounts, false, 2);

        assert_eq!(
            summary,
            "Launched by RM:\n  PID 1234 - alice\n  PID 5678 - bob  (best guess)"
        );
    }

    // -----------------------------------------------------------------------
    // Window titles
    // -----------------------------------------------------------------------

    #[test]
    fn each_attributed_client_is_titled_with_its_account_label() {
        let mut bob = account(8, "bob");
        bob.alias = "farmer".to_string();
        let accounts = vec![account(7, "alice"), bob];
        let tracked = vec![instance(1234, 7), instance(5678, 8)];

        assert_eq!(
            instance_window_titles(&tracked, &accounts, false),
            vec![
                (1234, "alice".to_string()),
                // The alias, matching what the sidebar shows.
                (5678, "farmer".to_string()),
            ]
        );
    }

    /// The one that matters. A window title is readable by every process on the
    /// machine and appears in screenshots and screen shares, so a user who
    /// turned anonymize on must not have their username written into one.
    #[test]
    fn anonymize_keeps_names_out_of_window_titles() {
        let mut bob = account(8, "bob");
        bob.alias = "farmer".to_string();
        let accounts = vec![account(7, "alice"), bob];
        let tracked = vec![instance(1234, 7), instance(5678, 8)];

        let titles = instance_window_titles(&tracked, &accounts, true);

        for (_, title) in &titles {
            assert!(!title.contains("alice"), "{title}");
            assert!(!title.contains("bob"), "{title}");
            assert!(!title.contains("farmer"), "alias leaked: {title}");
        }
        assert_eq!(
            titles,
            vec![
                (1234, format!("Account #{}", sidebar::anon_tag(7))),
                (5678, format!("Account #{}", sidebar::anon_tag(8))),
            ]
        );
    }

    /// An inferred mapping still gets a title. Being wrong writes the wrong
    /// name on a title bar, which is cosmetic, unlike being wrong about a kill.
    #[test]
    fn an_inferred_client_is_titled_too() {
        let accounts = vec![account(7, "alice")];
        assert_eq!(
            instance_window_titles(&[guessed(1234, 7)], &accounts, false),
            vec![(1234, "alice".to_string())]
        );
    }

    #[test]
    fn a_client_whose_account_is_gone_is_titled_with_its_raw_id() {
        assert_eq!(
            instance_window_titles(&[instance(1234, 99)], &[], false),
            vec![(1234, "user 99".to_string())]
        );
    }

    /// The label, not the username: an aliased account is shown the way the
    /// sidebar shows it.
    #[test]
    fn an_alias_wins_over_the_username() {
        let mut a = account(7, "alice");
        a.alias = "main".to_string();
        let summary = attribution_summary(&[instance(1234, 7)], &[a], false, 1);
        assert!(summary.contains("PID 1234 - main"), "{summary}");
        assert!(!summary.contains("alice"), "{summary}");
    }

    /// The stale-attribution case: the registry still maps a PID to an account
    /// the store no longer has. It must degrade to the raw ID rather than drop
    /// the line or panic.
    #[test]
    fn an_instance_whose_account_is_gone_falls_back_to_the_raw_id() {
        let summary = attribution_summary(&[instance(1234, 99)], &[], false, 1);
        assert!(summary.contains("PID 1234 - user 99"), "{summary}");
    }

    #[test]
    fn anonymize_replaces_every_name_with_its_stable_tag() {
        let accounts = vec![account(7, "alice"), account(8, "bob")];
        let tracked = vec![instance(1234, 7), instance(5678, 8)];

        let summary = attribution_summary(&tracked, &accounts, true, 2);

        assert!(!summary.contains("alice"), "{summary}");
        assert!(!summary.contains("bob"), "{summary}");
        assert!(
            summary.contains(&format!("PID 1234 - Account #{}", sidebar::anon_tag(7))),
            "{summary}"
        );
        assert!(
            summary.contains(&format!("PID 5678 - Account #{}", sidebar::anon_tag(8))),
            "{summary}"
        );
    }

    /// Anonymizing hides the name, not the identity of the account the store
    /// no longer knows: there is no name to hide, so the raw ID still shows.
    /// Worth pinning because it is the one way a user ID reaches the tooltip
    /// with anonymize on.
    #[test]
    fn anonymize_does_not_cover_an_account_missing_from_the_store() {
        let summary = attribution_summary(&[instance(1234, 99)], &[], true, 1);
        assert!(summary.contains("PID 1234 - user 99"), "{summary}");
    }

    #[test]
    fn clients_rm_did_not_launch_are_counted_on_a_trailing_line() {
        let accounts = vec![account(7, "alice")];
        let summary = attribution_summary(&[instance(1234, 7)], &accounts, false, 4);
        assert!(
            summary.ends_with("3 other instance(s) not matched to an account."),
            "{summary}"
        );
    }

    #[test]
    fn the_trailing_line_is_omitted_when_rm_launched_everything() {
        let accounts = vec![account(7, "alice")];
        let summary = attribution_summary(&[instance(1234, 7)], &accounts, false, 1);
        assert!(!summary.contains("other instance"), "{summary}");
    }

    /// The subtraction is saturating for a reason: the running count comes from
    /// a different sample of the process table than the map, so it can lag
    /// behind and be smaller. That must not underflow.
    #[test]
    fn a_running_count_behind_the_map_does_not_underflow() {
        let accounts = vec![account(7, "alice"), account(8, "bob")];
        let tracked = vec![instance(1234, 7), instance(5678, 8)];
        let summary = attribution_summary(&tracked, &accounts, false, 0);
        assert!(!summary.contains("other instance"), "{summary}");
    }
}
