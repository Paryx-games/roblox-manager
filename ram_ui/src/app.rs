//! Top-level application state and the `eframe::App` implementation that ties
//! the sidebar, main panel, settings, toast system, and backend bridge together.

use eframe::egui;
use ram_core::models::{AccountStore, AppConfig, PrivateServer};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ram_core::assets::{AssetState, OperationOutcome};

use crate::bridge::{BackendBridge, BackendCommand, BackendEvent, UploadJob};
use crate::components::{
    asset_manager, group_panel, main_panel, presets_panel, private_servers, settings, sidebar,
    tutorial,
};
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

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Accounts,
    PrivateServers,
    Presets,
    /// Only reachable while `config.developer_options` is on.
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
    /// Staging field for password — only committed on submit.
    password_input: String,
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
    master_password: String,

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
    settings_state: settings::SettingsState,
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

    /// Cached flag from sysinfo (refreshed lazily).
    roblox_running: bool,
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
    /// Poll cadence for in-flight asset uploads. Interval is adaptive, so
    /// unlike the timers above it is recomputed each tick from the age of the
    /// oldest pending operation.
    last_asset_poll: Option<std::time::Instant>,
    /// Debounce for asset index writes. A batch of uploads would otherwise fire
    /// one atomic write per state change.
    last_asset_index_save: Option<std::time::Instant>,
    /// Wall-clock timestamp of the last user-initiated game launch. Used to
    /// enforce `config.launch_delay_secs` so the user can't trigger another
    /// single/quick launch inside the cooldown window.
    last_launch: Option<std::time::Instant>,

    /// Password prompt shown on first launch when store file exists.
    needs_unlock: bool,
    unlock_password_input: String,

    /// When set, shows a confirmation dialog before removing the account.
    confirm_remove: Option<u64>,

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
        let needs_unlock = config.accounts_path.is_file();

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
            _ => sidebar::SortOrder::Custom,
        };

        let mut state = Self {
            config,
            config_path,
            store: AccountStore::default(),
            master_password: String::new(),
            bridge,
            toasts: Toasts::default(),
            active_tab: Tab::Accounts,
            selected_ids: HashSet::new(),
            sidebar_state,
            main_panel_state: main_panel::MainPanelState::default(),
            private_servers_state: private_servers::PrivateServerState::default(),
            presets_state: presets_panel::PresetsState::default(),
            asset_manager_state: asset_manager::AssetManagerState::default(),
            settings_state: settings::SettingsState::default(),
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
            asset_thumbnails: HashMap::new(),
            asset_thumbnails_inflight: HashSet::new(),
            asset_thumbnails_retry_at: HashMap::new(),
            visible_user_ids: Vec::new(),
            roblox_running: false,
            frame_count: 0,
            last_tray_kill: None,
            // Seeded to "now" so the first tick lands one full interval in,
            // rather than firing a redundant round at startup (StoreLoaded
            // already kicks off a refresh and revalidation).
            last_presence_poll: Some(std::time::Instant::now()),
            last_avatar_refresh: Some(std::time::Instant::now()),
            last_revalidation: Some(std::time::Instant::now()),
            // Not seeded: if a previous run left operations pending, they
            // should be polled on the first frame, not one interval later.
            last_asset_poll: None,
            last_asset_index_save: None,
            last_launch: None,
            needs_unlock,
            unlock_password_input: String::new(),
            confirm_remove: None,
            update_available: None,
            show_changelog: false,
            tutorial: tutorial::TutorialState::default(),
        };

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
                    let moderated =
                        account.moderation.as_ref().is_some_and(|m| m.is_active());
                    if moderated {
                        self.add_dialog.loading = false;
                        self.add_dialog.last_error = None;
                        self.add_dialog.pending_moderated =
                            Some(Box::new(PendingModeratedAdd {
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
                        self.add_dialog.password_input.clear();
                        self.add_dialog.browser_login_pending = false;
                        self.add_dialog.browser_login_rx = None;
                        self.add_dialog.rejected_cookie = None;
                        self.tutorial.advance_from(tutorial::TutorialStep::EnterCookie);
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
                    self.toasts.push(Toast::success("Game launched"));
                    if self.config.auto_arrange_windows {
                        self.bridge.send(BackendCommand::ArrangeWindows);
                    }
                }
                BackendEvent::BulkLaunchProgress { launched, total } => {
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
                    if self.config.auto_arrange_windows {
                        self.bridge.send(BackendCommand::ArrangeWindows);
                    }
                }
                BackendEvent::StoreSaved => {
                    // silent
                }
                BackendEvent::StoreLoaded(store) => {
                    self.store = store;
                    self.needs_unlock = false;
                    self.toasts
                        .push(Toast::success("Account store unlocked"));
                    self.trigger_refresh();
                    self.trigger_revalidation();
                }
                BackendEvent::Killed(count) => {
                    self.toasts
                        .push(Toast::info(format!("Killed {count} instance(s)")));
                }
                BackendEvent::WindowsArranged => {
                    // silent — arrangement complete
                }
                BackendEvent::AccountRevalidated {
                    user_id,
                    valid,
                    username,
                    display_name,
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
                        let was_active =
                            acc.moderation.as_ref().is_some_and(|m| m.is_active());
                        let now_active =
                            moderation.as_ref().is_some_and(|m| m.is_active());
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
                            !matches!(
                                r.trim(),
                                "Account terminated." | "Account moderated."
                            )
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
                BackendEvent::PlaceResolved { index, place_name, place_id, icon_bytes } => {
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
                    self.toasts.push(Toast::success("Share link resolved, private server added"));
                }
                BackendEvent::ShareLinkFailed(msg) => {
                    self.toasts.push(Toast::error(format!(
                        "Failed to resolve share link: {msg}"
                    )));
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
                    self.add_dialog.password_input.clear();
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                    self.add_dialog.rejected_cookie = None;
                    self.add_dialog.force_add_form_open = false;
                    self.add_dialog.force_add_username.clear();
                    self.tutorial.advance_from(tutorial::TutorialStep::EnterCookie);
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
                            Some(m) => format!(
                                "Cookie was rejected by Roblox.\n\nLikely reason: {m}",
                            ),
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
                BackendEvent::AssetUploadFailed {
                    row_id,
                    message,
                    retryable,
                } => {
                    if let Some(record) = self.asset_index.get_mut(&row_id) {
                        record.state = AssetState::Failed { message, retryable };
                        record.updated_at = Some(chrono::Utc::now());
                    }
                    self.asset_index_dirty = true;
                    self.dispatch_next_uploads();
                }
                BackendEvent::AssetPollBatchDone => {}
                BackendEvent::AssetPermissionsGranted {
                    universe_id,
                    row_ids,
                    granted,
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
                    self.toasts.push(Toast::success(format!(
                        "Granted {granted} asset(s) to universe {universe_id}"
                    )));
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
                            self.remote_inventory.error = Some(message);
                        }
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

    fn auto_save(&self) {
        if !self.master_password.is_empty() {
            self.bridge.send(BackendCommand::SaveStore {
                store: self.store.clone(),
                path: self.config.accounts_path.clone(),
                password: self.master_password.clone(),
            });
        }
    }

    /// Pop the next queued cookie and dispatch an AddAccount for it. When the
    /// queue is empty the batch is done: save once, refresh avatars/presence
    /// for the newly added accounts, and clear the loading flag so the bulk
    /// summary screen renders.
    fn dispatch_next_bulk(&mut self) {
        match self.add_dialog.bulk_queue.pop() {
            Some(cookie) => {
                self.add_dialog.loading = true;
                self.bridge.send(BackendCommand::AddAccount {
                    cookie,
                    password: self.master_password.clone(),
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
        let user_ids: Vec<u64> = self.store.accounts.iter().map(|a| a.user_id).collect();
        if user_ids.is_empty() {
            return;
        }
        if let Some(first) = self.first_account_with_cookie() {
            self.bridge.send(BackendCommand::RefreshAll {
                user_ids,
                first_user_id: first.user_id,
                encrypted_cookie: first.encrypted_cookie.clone(),
                password: self.master_password.clone(),
                use_credential_manager: self.config.use_credential_manager,
            });
        }
    }

    /// Lightweight presence-only refresh for the currently visible accounts.
    fn trigger_presence_refresh(&self) {
        if self.visible_user_ids.is_empty() {
            return;
        }
        if let Some(first) = self.first_account_with_cookie() {
            self.bridge.send(BackendCommand::RefreshPresenceOnly {
                user_ids: self.visible_user_ids.clone(),
                first_user_id: first.user_id,
                encrypted_cookie: first.encrypted_cookie.clone(),
                password: self.master_password.clone(),
                use_credential_manager: self.config.use_credential_manager,
            });
        }
    }

    /// Resolve place names and game icons for private servers that are missing them.
    fn resolve_private_server_icons(&self) {
        for (i, server) in self.config.private_servers.iter().enumerate() {
            if server.place_name.is_empty() || !self.game_icon_bytes.contains_key(&server.place_id) {
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
                self.toasts.push(Toast::info(format!(
                    "Launch cooldown: wait {remaining}s",
                )));
                return false;
            }
        }
        self.last_launch = Some(now);
        true
    }

    fn open_browser_as(&mut self, user_id: u64) {
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
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
            profile_dir,
            label,
        });
    }

    /// Revalidate all account cookies in the background.
    fn trigger_revalidation(&self) {
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
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
        });
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for AppState {
    /// Flush anything the 500 ms index debounce is still holding. Without this,
    /// closing the app within half a second of a state change loses it.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
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

        // Periodically refresh roblox_running flag (every ~120 frames ≈ 2s)
        if self.frame_count.is_multiple_of(120) {
            self.roblox_running = ram_core::process::is_roblox_running();
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

        // Flush index changes that the debounce has been holding.
        if self.asset_index_dirty
            && interval_due(&mut self.last_asset_index_save, Duration::from_millis(500))
        {
            self.save_asset_index();
        }

        // ---- Unlock screen ----
        if self.needs_unlock {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
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
                        let pw = self.unlock_password_input.clone();
                        self.master_password = pw.clone();
                        self.bridge.send(BackendCommand::LoadStore {
                            path: self.config.accounts_path.clone(),
                            password: pw,
                        });
                    }
                });
            });
            self.toasts.show(ctx);
            return;
        }

        // Turning Developer options off must not strand the user on a tab that
        // is no longer in the bar.
        if !self.config.developer_options && self.active_tab == Tab::AssetManager {
            self.active_tab = Tab::Accounts;
        }

        // ---- Top bar ----
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Accounts, "📋 Accounts");
                ui.selectable_value(&mut self.active_tab, Tab::PrivateServers, "🔒 Private Servers");
                ui.selectable_value(&mut self.active_tab, Tab::Presets, "⭐ Presets");
                if self.config.developer_options {
                    ui.selectable_value(
                        &mut self.active_tab,
                        Tab::AssetManager,
                        "\u{1f4e6} Asset Manager",
                    );
                }
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙ Settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((ref version, ref url)) = self.update_available {
                        let text = format!("⬆ Update v{version} available");
                        if ui.link(text).on_hover_text("Click to open the download page").clicked() {
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
                        let count = ram_core::process::roblox_instance_count();
                        ui.colored_label(
                            egui::Color32::from_rgb(30, 144, 255),
                            format!("● {count} Roblox instance{}", if count == 1 { "" } else { "s" }),
                        );
                        ui.separator();
                    }
                    if self.selected_ids.len() > 1 {
                        ui.colored_label(
                            egui::Color32::from_rgb(130, 180, 255),
                            format!("{} selected", self.selected_ids.len()),
                        );
                        ui.separator();
                    }
                    ui.label(format!("{} account(s)", self.store.accounts.len()));
                });
            });
        });


        match self.active_tab {
            Tab::Accounts => self.show_accounts_tab(ctx),
            Tab::PrivateServers => self.show_private_servers_tab(ctx),
            Tab::Presets => self.show_presets_tab(ctx),
            Tab::AssetManager => self.show_asset_manager_tab(ctx),
            Tab::Settings => self.show_settings_tab(ctx),
        }

        // ---- Floating add-account dialog ----
        self.show_add_dialog(ctx);

        // ---- Confirmation dialog for account removal ----
        self.show_confirm_remove_dialog(ctx);

        // ---- Confirmation before an asset upload batch ----
        self.show_upload_confirm_dialog(ctx);

        // ---- Grant universe access to selected assets ----
        self.show_grant_dialog(ctx);

        // ---- Changelog window ----
        self.show_changelog_window(ctx);

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
                );
                self.visible_user_ids = result.visible_user_ids;
                self.tutorial.add_btn_rect = result.add_btn_rect;
                self.tutorial.sidebar_accounts_rect = result.accounts_rect;
                // Tutorial: advance when the sidebar account list area is known
                if !self.selected_ids.is_empty() {
                    self.tutorial.advance_from(tutorial::TutorialStep::SelectAccount);
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
                            self.add_dialog.password_input = self.master_password.clone();
                            self.tutorial.advance_from(tutorial::TutorialStep::AddAccount);
                        }
                        sidebar::SidebarAction::CopyJobId(job_id) => {
                            ui.output_mut(|o| o.copied_text = job_id.clone());
                            self.toasts.push(Toast::info("Copied to clipboard"));
                        }
                        sidebar::SidebarAction::OpenBrowserAs(user_id) => {
                            self.open_browser_as(user_id);
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
                                    let pid = self
                                        .main_panel_state
                                        .place_id_input
                                        .parse::<u64>()
                                        .ok();
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
                                    .map(|a| (a.user_id, a.encrypted_cookie.clone()));
                                if let Some((uid, enc)) = acc_lookup {
                                    if self.try_consume_launch_slot() {
                                        self.bridge.send(BackendCommand::LaunchGameEncrypted {
                                            user_id: uid,
                                            encrypted_cookie: enc,
                                            password: self.master_password.clone(),
                                            use_credential_manager: self.config.use_credential_manager,
                                            place_id,
                                            job_id,
                                            link_code: None,
                                            access_code: None,
                                            multi_instance: self.config.multi_instance_enabled,
                                            kill_background: self.config.kill_background_roblox,
                                            privacy_mode: self.config.privacy_mode,
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
                        sidebar::SidebarAction::CreateGroup { name, color, assign_user_ids } => {
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
                        sidebar::SidebarAction::EditGroup { old_name, new_name, color } => {
                            let old_meta = self.config.groups.remove(&old_name);
                            let desc = old_meta.as_ref().map(|m| m.description.clone()).unwrap_or_default();
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
                                    self.sidebar_state
                                        .collapsed_groups
                                        .insert(new_name.clone());
                                }
                            }
                            let _ = self.config.save(&self.config_path);
                            self.auto_save();
                        }
                        sidebar::SidebarAction::ReorderAccount { user_id, target_user_id, insert_after } => {
                            // Move `user_id` before or after `target_user_id` within the
                            // same group (or both ungrouped). Reassign sort_order values.
                            let group = self.store.find_by_id(user_id)
                                .map(|a| a.group.clone())
                                .unwrap_or_default();
                            // Collect accounts in this group, sorted by current sort_order then name.
                            let mut peers: Vec<(u32, String, u64)> = self.store.accounts.iter()
                                .filter(|a| a.group == group)
                                .map(|a| (a.sort_order, a.label().to_lowercase(), a.user_id))
                                .collect();
                            peers.sort();
                            let mut ids: Vec<u64> = peers.into_iter().map(|(_, _, id)| id).collect();
                            // Remove the dragged account.
                            if let Some(drag_pos) = ids.iter().position(|id| *id == user_id) {
                                ids.remove(drag_pos);
                            }
                            // Find target and insert before or after it.
                            let target_pos = ids.iter().position(|id| *id == target_user_id)
                                .unwrap_or(ids.len());
                            let insert_pos = if insert_after { target_pos + 1 } else { target_pos };
                            ids.insert(insert_pos.min(ids.len()), user_id);
                            // Reassign sequential sort_order values.
                            for (i, uid) in ids.iter().enumerate() {
                                if let Some(acc) = self.store.find_by_id_mut(*uid) {
                                    acc.sort_order = i as u32;
                                }
                            }
                            self.auto_save();
                        }
                        sidebar::SidebarAction::ReorderGroup { group_name, target_group, insert_after } => {
                            // Move `group_name` before or after `target_group`.
                            let mut ordered: Vec<(u32, String)> = self.config.groups.iter()
                                .map(|(name, meta)| (meta.sort_order, name.clone()))
                                .collect();
                            ordered.sort();
                            let mut names: Vec<String> = ordered.into_iter().map(|(_, n)| n).collect();
                            if let Some(pos) = names.iter().position(|n| *n == group_name) {
                                names.remove(pos);
                            }
                            let target_pos = names.iter().position(|n| *n == target_group)
                                .unwrap_or(names.len());
                            let insert_pos = if insert_after { target_pos + 1 } else { target_pos };
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
                    }
                }
                // Persist sort mode if it changed.
                let current_mode = self.sidebar_state.sort_order.to_string();
                if self.config.sort_mode != current_mode {
                    self.config.sort_mode = current_mode;
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
                    &preset_view,
                    self.roblox_running,
                    self.config.anonymize_names,
                );
                if let Some(a) = action {
                    match a {
                        group_panel::GroupPanelAction::BulkLaunch { place_id, job_id } => {
                            let accounts: Vec<(u64, Option<String>)> = self
                                .store
                                .accounts
                                .iter()
                                .filter(|a| self.selected_ids.contains(&a.user_id))
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()))
                                .collect();
                            self.bridge.send(BackendCommand::BulkLaunchEncrypted {
                                accounts,
                                password: self.master_password.clone(),
                                use_credential_manager: self.config.use_credential_manager,
                                place_id,
                                job_id,
                                link_code: None,
                                access_code: None,
                                multi_instance: self.config.multi_instance_enabled,
                                kill_background: self.config.kill_background_roblox,
                                privacy_mode: self.config.privacy_mode,
                                launch_delay_secs: self.config.launch_delay_secs,
                            });
                        }
                        group_panel::GroupPanelAction::ClearSelection => {
                            self.selected_ids.clear();
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
                    let result = main_panel::show(
                        ui,
                        &account,
                        &mut self.main_panel_state,
                        self.roblox_running,
                        avatar_bytes,
                        &preset_view,
                        self.config.anonymize_names,
                    );
                    self.tutorial.launch_btn_rect = result.launch_btn_rect;
                    if let Some(a) = result.action {
                        match a {
                            main_panel::MainPanelAction::LaunchGame { place_id, job_id } => {
                                if self.try_consume_launch_slot() {
                                    self.bridge.send(BackendCommand::LaunchGameEncrypted {
                                        user_id: account.user_id,
                                        encrypted_cookie: account.encrypted_cookie.clone(),
                                        password: self.master_password.clone(),
                                        use_credential_manager: self.config.use_credential_manager,
                                        place_id,
                                        job_id,
                                        link_code: None,
                                        access_code: None,
                                        multi_instance: self.config.multi_instance_enabled,
                                        kill_background: self.config.kill_background_roblox,
                                        privacy_mode: self.config.privacy_mode,
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
                            } => {
                                let preset = ram_core::models::LaunchPreset {
                                    name,
                                    place_id,
                                    job_id,
                                };
                                match ram_core::presets::save(
                                    &crate::data_dir(),
                                    &preset,
                                    None,
                                ) {
                                    Ok(_) => {
                                        self.toasts.push(Toast::success("Preset saved"));
                                        self.reload_presets();
                                    }
                                    Err(e) => {
                                        self.toasts
                                            .push(Toast::error(format!("Save failed: {e}")));
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
            if i.key_pressed(egui::Key::Delete) && !any_text_focused
                && self.selected_ids.len() == 1
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
                    private_servers::PrivateServerAction::Launch { place_id, link_code, access_code } => {
                        let ac = if access_code.is_empty() { None } else { Some(access_code.clone()) };
                        if self.selected_ids.len() == 1 {
                            let uid = *self.selected_ids.iter().next().unwrap();
                            let acc_lookup = self
                                .store
                                .find_by_id(uid)
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()));
                            if let Some((user_id, enc)) = acc_lookup {
                                if self.try_consume_launch_slot() {
                                    self.bridge.send(BackendCommand::LaunchGameEncrypted {
                                        user_id,
                                        encrypted_cookie: enc,
                                        password: self.master_password.clone(),
                                        use_credential_manager: self.config.use_credential_manager,
                                        place_id,
                                        job_id: None,
                                        link_code: Some(link_code.clone()),
                                        access_code: ac.clone(),
                                        multi_instance: self.config.multi_instance_enabled,
                                        kill_background: self.config.kill_background_roblox,
                                        privacy_mode: self.config.privacy_mode,
                                    });
                                }
                            }
                        } else if self.selected_ids.len() > 1 {
                            let accounts: Vec<(u64, Option<String>)> = self
                                .store
                                .accounts
                                .iter()
                                .filter(|a| self.selected_ids.contains(&a.user_id))
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()))
                                .collect();
                            self.bridge.send(BackendCommand::BulkLaunchEncrypted {
                                accounts,
                                password: self.master_password.clone(),
                                use_credential_manager: self.config.use_credential_manager,
                                place_id,
                                job_id: None,
                                link_code: Some(link_code),
                                access_code: ac,
                                multi_instance: self.config.multi_instance_enabled,
                                kill_background: self.config.kill_background_roblox,
                                privacy_mode: self.config.privacy_mode,
                                launch_delay_secs: self.config.launch_delay_secs,
                            });
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
                        if let Some(acc) = self.store.accounts.first() {
                            self.bridge.send(BackendCommand::ResolveShareLink {
                                share_code,
                                server_name,
                                first_user_id: acc.user_id,
                                encrypted_cookie: acc.encrypted_cookie.clone(),
                                password: self.master_password.clone(),
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
                        self.toasts
                            .push(Toast::error(format!("Save failed: {e}")));
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

    // ------------------------------------------------------------------
    // Asset manager
    // ------------------------------------------------------------------

    /// Most uploads Roblox will accept at once. The spec calls 4 comfortable
    /// and throttles above ~8, but this app shares one client, one connection
    /// pool and one IP with the presence, avatar and revalidation timers, so
    /// leave headroom.
    const MAX_CONCURRENT_UPLOADS: usize = 3;

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
            self.bridge.send(BackendCommand::UploadAsset(Box::new(job)));
        }
    }

    /// Claim the next queued row and build its job, marking it `Uploading` so
    /// the next call cannot claim it again.
    ///
    /// Loops rather than returning on the first duplicate: a queue whose head
    /// is all duplicates would otherwise stall with real work behind it.
    fn take_next_upload(&mut self) -> Option<UploadJob> {
        loop {
            let row_id = self
                .asset_index
                .records
                .iter()
                .find(|r| matches!(r.state, AssetState::Queued))
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
        let row_id = row_id.to_string();
        let record = self.asset_index.get(&row_id)?;
        let uploader = record.uploaded_by;
        let account = self.store.find_by_id(uploader)?;
        let encrypted_cookie = account.encrypted_cookie.clone();

        let job = UploadJob {
            row_id: row_id.clone(),
            user_id: uploader,
            encrypted_cookie,
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
            creator: record.creator,
            kind: record.kind,
            display_name: record.display_name.clone(),
            description: record.description.clone(),
            file_path: record.file_path.clone(),
        };

        if let Some(record) = self.asset_index.get_mut(&row_id) {
            record.state = AssetState::Uploading;
        }
        self.asset_index_dirty = true;
        Some(job)
    }

    /// Ask about everything currently in moderation, grouped by account so one
    /// cookie decrypt covers a whole batch.
    fn dispatch_asset_poll(&mut self) {
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
                password: self.master_password.clone(),
                use_credential_manager: self.config.use_credential_manager,
                operations,
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
        let Some(user_id) = self.asset_manager_state.acting_user_id else {
            return;
        };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        self.bridge.send(BackendCommand::FetchCreations {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
            creator,
            kind,
            cursor,
        });
    }

    /// Refresh the universe picker for the acting account, and optionally
    /// resolve a pasted place ID at the same time (one cookie decrypt covers
    /// both).
    fn fetch_universe_targets(&mut self, place_id: Option<u64>) {
        let Some(user_id) = self.asset_manager_state.acting_user_id else {
            return;
        };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        self.bridge.send(BackendCommand::FetchUniverseTargets {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
            place_id,
        });
    }

    /// Grant a universe `Use` on the given rows. Rows with no asset ID yet are
    /// skipped: there is nothing on Roblox to grant against.
    fn grant_universe_access(&mut self, universe_id: u64, row_ids: Vec<String>) {
        let mut asset_ids = Vec::new();
        let mut granted_rows = Vec::new();
        let mut uploader = None;
        for row_id in row_ids {
            let Some(record) = self.asset_index.get(&row_id) else {
                continue;
            };
            let Some(asset_id) = record.state.asset_id() else {
                continue;
            };
            uploader.get_or_insert(record.uploaded_by);
            asset_ids.push(asset_id);
            granted_rows.push(row_id);
        }
        if asset_ids.is_empty() {
            self.toasts
                .push(Toast::info("Nothing to grant. Assets must finish uploading first."));
            return;
        }
        let Some(user_id) = uploader else { return };
        let Some(account) = self.store.find_by_id(user_id) else {
            return;
        };
        self.bridge.send(BackendCommand::GrantAssetPermissions {
            user_id,
            encrypted_cookie: account.encrypted_cookie.clone(),
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
            universe_id,
            asset_ids,
            row_ids: granted_rows,
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
                ui.label(format!(
                    "Let one experience use {count} selected asset(s)."
                ));
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
                    let parsed = ram_core::assets::parse_id_input(
                        &self.asset_manager_state.grant_manual,
                    );
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
                    let grant = egui::Button::new(
                        egui::RichText::new("Grant").color(egui::Color32::WHITE),
                    )
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
            let rows: Vec<String> =
                self.asset_manager_state.selected.iter().cloned().collect();
            self.grant_universe_access(universe_id, rows);
            self.asset_manager_state.grant_open = false;
        } else if cancelled || !open {
            self.asset_manager_state.grant_open = false;
        }
    }

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
                record.state = AssetState::Approved {
                    asset_id,
                    revision_id,
                };
                record.updated_at = Some(now);
                let auto_grant = record.auto_grant_universe;
                self.toasts
                    .push(Toast::success(format!("{name} uploaded as {asset_id}")));
                // Requirement: an asset that clears moderation is granted to
                // the batch's universe with no further clicks.
                if let Some(universe_id) = auto_grant {
                    self.grant_universe_access(universe_id, vec![row_id.to_string()]);
                }
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
                        has_password: !self.master_password.is_empty()
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
                has_password: !self.master_password.is_empty()
                    || self.config.use_credential_manager,
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
                if let Some(account) = self.store.find_by_id(user_id) {
                    self.bridge.send(BackendCommand::FetchPublishGroups {
                        user_id,
                        encrypted_cookie: account.encrypted_cookie.clone(),
                        password: self.master_password.clone(),
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
                            "png", "jpg", "jpeg", "bmp", "tga", "mp3", "ogg", "wav", "flac",
                            "fbx", "gltf", "glb", "rbxm", "rbxmx", "mp4", "mov",
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
                let pending: Vec<(ram_core::assets::AssetKind, String)> = self
                    .remote_inventory
                    .cursors
                    .drain()
                    .collect();
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
                record.state = AssetState::Duplicate {
                    asset_id: existing,
                };
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
                    egui::Color32::from_rgb(220, 160, 40),
                    "This cannot be undone. Every asset is permanent, public, and moderated \
                     under that account.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let upload = egui::Button::new(
                        egui::RichText::new("Upload").color(egui::Color32::WHITE),
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
            let has_password = !self.master_password.is_empty();
            let action = settings::show(
                ui,
                &mut self.config,
                has_password,
                &mut self.settings_state,
                self.roblox_running,
            );
            match action {
                Some(settings::SettingsAction::SaveConfig) => {
                    if let Err(e) = self.config.save(&self.config_path) {
                        self.toasts
                            .push(Toast::error(format!("Save failed: {e}")));
                    } else {
                        self.toasts.push(Toast::success("Settings saved"));
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
                Some(settings::SettingsAction::ChangePassword { new_password }) => {
                    let old_password = self.master_password.clone();
                    // Re-encrypt every account's cookie with the new password
                    for account in &mut self.store.accounts {
                        if let Some(ref enc) = account.encrypted_cookie {
                            if let Ok(plain) = ram_core::crypto::decrypt_cookie(enc, &old_password) {
                                if let Ok(new_enc) = ram_core::crypto::encrypt_cookie(&plain, &new_password) {
                                    account.encrypted_cookie = Some(new_enc);
                                }
                            }
                        }
                    }
                    self.master_password = new_password;
                    self.auto_save();
                    self.toasts.push(Toast::success("Password changed - store re-encrypted"));
                }
                Some(settings::SettingsAction::ClearPassword) => {
                    self.master_password.clear();
                    self.toasts.push(Toast::info("Password cleared"));
                }
                None => {}
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
                    // If the user already has a master password set (or
                    // credential-manager mode is on), there's nothing left
                    // for them to confirm — send the cookie straight to the
                    // backend instead of making them click "Add" redundantly.
                    let needs_password = !self.config.use_credential_manager
                        && self.master_password.is_empty();
                    if !needs_password {
                        let cookie = self.add_dialog.cookie_input.trim().to_string();
                        self.add_dialog.loading = true;
                        self.bridge.send(BackendCommand::AddAccount {
                            cookie,
                            password: self.master_password.clone(),
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
                    self.add_dialog.last_error =
                        Some(format!("Browser login failed: {e}"));
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
                        egui::Color32::from_rgb(255, 110, 110)
                    } else {
                        egui::Color32::from_rgb(240, 180, 80)
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
                    self.bridge.send(BackendCommand::BrowseAsAccount {
                        user_id: uid,
                        encrypted_cookie: enc,
                        password: self.master_password.clone(),
                        use_credential_manager: self.config.use_credential_manager,
                        profile_dir,
                        label,
                    });
                }
            }
            if mod_revalidate {
                // User likely just resolved a warning in the browser. Decrypt
                // the cookie we kept on the pending entry and re-run the
                // AddAccount cycle from scratch — same flow as if they'd
                // pasted the cookie fresh, so a clean account now skips the
                // moderation confirm.
                if let Some(pending) = self.add_dialog.pending_moderated.take() {
                    let raw_cookie = if self.config.use_credential_manager {
                        ram_core::crypto::credential_load(pending.account.user_id).ok()
                    } else {
                        pending.encrypted_cookie.as_ref().and_then(|enc| {
                            ram_core::crypto::decrypt_cookie(enc, &self.master_password).ok()
                        })
                    };
                    match raw_cookie {
                        Some(cookie) => {
                            self.add_dialog.loading = true;
                            self.add_dialog.last_error = None;
                            self.bridge.send(BackendCommand::AddAccount {
                                cookie,
                                password: self.master_password.clone(),
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
                    self.add_dialog.password_input.clear();
                    self.add_dialog.browser_login_pending = false;
                    self.add_dialog.browser_login_rx = None;
                    self.tutorial.advance_from(tutorial::TutorialStep::EnterCookie);
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
                self.add_dialog.password_input.clear();
                self.add_dialog.browser_login_pending = false;
                self.add_dialog.browser_login_rx = None;
            }
            return;
        }

        egui::Window::new("Add Account")
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
                                    .color(egui::Color32::from_rgb(120, 200, 120)),
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

                            let needs_password = !self
                                .config
                                .use_credential_manager
                                && self.master_password.is_empty();
                            if needs_password {
                                ui.label(
                                    "Set a master password for encryption:",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(
                                        &mut self.add_dialog.password_input,
                                    )
                                    .password(true)
                                    .hint_text("Master password"),
                                );
                                ui.add_space(4.0);
                            }

                            let valid = count > 0
                                && (!needs_password
                                    || !self
                                        .add_dialog
                                        .password_input
                                        .is_empty());
                            if ui
                                .add_enabled(
                                    valid,
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
                                if needs_password {
                                    self.master_password = self
                                        .add_dialog
                                        .password_input
                                        .clone();
                                }
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

                // Shared footer — master password (if needed), error, submit.
                // Skipped on Choose (nothing to submit) and Bulk (handles its
                // own submit/progress UI above).
                if matches!(
                    self.add_dialog.step,
                    AddAccountStep::Choose | AddAccountStep::Bulk
                ) {
                    return;
                }

                let needs_password = !self.config.use_credential_manager
                    && self.master_password.is_empty();
                if needs_password {
                    ui.label("Set a master password for encryption:");
                    ui.add_enabled(
                        !self.add_dialog.loading,
                        egui::TextEdit::singleline(&mut self.add_dialog.password_input)
                            .password(true)
                            .hint_text("Master password"),
                    );
                    ui.add_space(4.0);
                }

                if let Some(err) = &self.add_dialog.last_error {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 60, 60),
                            format!("⚠ {err}"),
                        );
                    });
                    ui.add_space(4.0);
                }

                if self.add_dialog.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Validating cookie...");
                    });
                } else {
                    let valid = !self.add_dialog.cookie_input.trim().is_empty()
                        && (!needs_password || !self.add_dialog.password_input.is_empty());
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
                            if needs_password {
                                self.master_password =
                                    self.add_dialog.password_input.clone();
                            }
                            self.add_dialog.loading = true;
                            self.add_dialog.last_error = None;
                            self.add_dialog.rejected_cookie = None;
                            self.bridge.send(BackendCommand::AddAccount {
                                cookie,
                                password: self.master_password.clone(),
                                use_credential_manager: self.config.use_credential_manager,
                            });
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
                                1.0,
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
                                        self.add_dialog.loading = true;
                                        self.add_dialog.last_error = None;
                                        self.bridge.send(
                                            BackendCommand::AddAccountForced {
                                                cookie,
                                                username,
                                                password: self
                                                    .master_password
                                                    .clone(),
                                                use_credential_manager: self
                                                    .config
                                                    .use_credential_manager,
                                            },
                                        );
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
                ui.label(format!("Remove account \"{label}\"? This cannot be undone."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("🗑  Remove")
                        .clicked()
                    {
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
                job.append(before, 0.0, egui::text::TextFormat {
                    font_id: normal_font.clone(),
                    color: normal_color,
                    ..Default::default()
                });
            }
            remaining = &remaining[start + 2..];
            // Find the closing **
            if let Some(end) = remaining.find("**") {
                let bold_text = &remaining[..end];
                job.append(bold_text, 0.0, egui::text::TextFormat {
                    font_id: bold_font.clone(),
                    color: normal_color,
                    italics: false,
                    ..Default::default()
                });
                remaining = &remaining[end + 2..];
            } else {
                // No closing ** — just emit the rest as normal
                job.append(&format!("**{remaining}"), 0.0, egui::text::TextFormat {
                    font_id: normal_font.clone(),
                    color: normal_color,
                    ..Default::default()
                });
                remaining = "";
            }
        }
        // Remaining plain text
        if !remaining.is_empty() {
            job.append(remaining, 0.0, egui::text::TextFormat {
                font_id: normal_font,
                color: normal_color,
                ..Default::default()
            });
        }
        ui.label(job);
    }
}
