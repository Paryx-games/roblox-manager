//! Bridge between the synchronous `egui` update loop and the `tokio` async runtime.
//!
//! All heavyweight operations (network, file I/O, process spawning) are dispatched
//! as [`BackendCommand`] messages to a background `tokio` runtime. Results come
//! back as [`BackendEvent`] through an mpsc channel polled each frame.

use eframe::egui;
use ram_core::assets::{AssetKind, Creator, ModerationStatus, OperationOutcome};
use ram_core::auth::RobloxClient;
use ram_core::instances::{Attribution, InstanceRegistry, TrackedInstance};
use ram_core::models::{Account, AccountStore, Presence};
use ram_core::{api, assets, assets_api, crypto, process, CoreError};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Everything the instance sweep owns.
///
/// The map and the command-line cache are one lock rather than two because
/// they are only ever touched together: a sweep scans, then reconciles, and a
/// launch snapshots, then queues. Splitting them would add a second lock whose
/// only purpose is to be taken in the same breath as the first.
#[derive(Debug, Default)]
struct InstanceState {
    registry: InstanceRegistry,
    /// Keeps the sweep from re-reading the command line of a process it has
    /// already identified. Lives here, next to the map, so it is pruned on the
    /// same cadence.
    tokens: process::LaunchTokenCache,
}

/// The PID-to-account map, shared between the command loop and the spawned
/// tasks that launch games.
///
/// A `std::sync::Mutex` rather than tokio's: every critical section is a
/// process-table walk and a few vector operations with no `.await` inside, so
/// an async mutex would buy nothing and cost a scheduling point.
type Registry = Arc<Mutex<InstanceState>>;

// ---------------------------------------------------------------------------
// Commands (UI → Backend)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum BackendCommand {
    /// Validate a cookie and add the account.
    AddAccount {
        cookie: String,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// Add an account WITHOUT requiring `validate_cookie` to succeed. Looks
    /// up the canonical user identity by username (works for terminated
    /// accounts) and stores the cookie regardless of its current auth state.
    /// Used by the "add anyway" path when a real cookie was rejected and the
    /// user still wants the account tracked.
    AddAccountForced {
        cookie: String,
        username: String,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// Remove an account by user ID.
    RemoveAccount { user_id: u64 },
    /// Launch the game for an account. The cookie is named by `user_id` and
    /// decrypted on the backend thread; it never crosses the channel in the
    /// clear.
    ///
    /// There used to be a second `LaunchGameEncrypted` beside this, and this
    /// one took a plaintext cookie from the UI. Nothing had constructed the
    /// plaintext variant since cookie decryption moved to the backend, so the
    /// two were not really a pair: one was dead. Same for the `RefreshAvatars`
    /// and `RefreshPresence` variants that sat above, both superseded by
    /// `RefreshAll` / `RefreshPresenceOnly`.
    LaunchGame {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        place_id: u64,
        job_id: Option<String>,
        link_code: Option<String>,
        access_code: Option<String>,
        multi_instance: bool,
        kill_background: bool,
        privacy_mode: bool,
    },
    /// Save the account store to disk.
    SaveStore {
        store: AccountStore,
        path: PathBuf,
        session: crypto::StoreSession,
    },
    /// Open a device-mode store with no user interaction. Sent automatically at
    /// startup when [`crypto::peek_mode`] reports the store needs no password.
    UnlockWithDevice { path: PathBuf },
    /// Open a password-mode (or pre-envelope) store with a typed password.
    UnlockWithPassword { path: PathBuf, password: String },
    /// Rewrite an already-unlocked store under new key material: switching
    /// between device and password mode, changing the master password, or
    /// upgrading a pre-envelope file. Runs on the backend because Argon2id
    /// takes long enough to drop frames on the UI thread.
    RekeyStore {
        store: AccountStore,
        path: PathBuf,
        session: crypto::StoreSession,
        /// `Some` switches to (or stays on) password mode; `None` switches to
        /// device mode.
        new_password: Option<String>,
        /// Set when `session` came from a pre-envelope file, so the data key is
        /// rotated and every cookie re-encrypted rather than merely rewrapped.
        upgrade_legacy: bool,
    },
    /// Kill all Roblox instances.
    KillAll,
    /// Refresh avatars + presence for all accounts, decrypting a cookie on this side.
    RefreshAll {
        user_ids: Vec<u64>,
        /// The first account's encrypted cookie (or None if credential manager).
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// Lightweight presence-only refresh for a subset of visible accounts.
    RefreshPresenceOnly {
        user_ids: Vec<u64>,
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// Launch multiple accounts into the same game sequentially.
    BulkLaunchEncrypted {
        /// (user_id, encrypted_cookie) pairs for each account.
        accounts: Vec<(u64, Option<String>)>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        place_id: u64,
        job_id: Option<String>,
        link_code: Option<String>,
        access_code: Option<String>,
        multi_instance: bool,
        kill_background: bool,
        privacy_mode: bool,
        /// Seconds to wait between launches (Roblox rate-limit avoidance).
        /// 0 = no extra delay beyond the existing tray-kill window.
        launch_delay_secs: u32,
    },
    /// Re-validate all accounts' cookies automatically.
    RevalidateAll {
        /// (user_id, encrypted_cookie) pairs for each account.
        accounts: Vec<(u64, Option<String>)>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// Reconcile the PID-to-account map against the processes actually
    /// running: attribute clients that have appeared since the last sweep,
    /// and drop mappings whose process is gone. Sent on a timer.
    SweepInstances,
    /// Arrange all Roblox windows in a tiled grid.
    ArrangeWindows,
    /// Check GitLab for a newer release.
    CheckForUpdates { current_version: String },
    /// Resolve a place ID to its name (for private server auto-check).
    ResolvePlace {
        place_id: u64,
        universe_id: Option<u64>,
        /// Index into the private_servers list so the UI can update the right entry.
        index: usize,
    },
    /// Resolve a share link code into (place_id, link_code) via the Roblox API.
    ResolveShareLink {
        share_code: String,
        server_name: String,
        /// The encrypted cookie + auth info needed for the authenticated API call.
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// Decrypt the cookie and open a webview pre-logged-in as this account.
    BrowseAsAccount {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        profile_dir: PathBuf,
        /// Label for the webview window title (username or anon tag).
        label: String,
    },
    /// Upload one staged file. Boxed because the payload dwarfs every other
    /// variant, and `clippy::large_enum_variant` is a hard error in CI.
    UploadAsset(Box<UploadJob>),
    /// Poll a batch of in-flight operations for a single account. One cookie
    /// decrypt covers the whole batch, and results stream back per item.
    PollAssetOperations {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        /// `(row_id, operation)` pairs. The caller caps the length.
        operations: Vec<(String, String)>,
    },
    /// Ask whether a batch of already-uploaded assets has cleared moderation.
    /// Separate from `PollAssetOperations` because it runs on its own, slower
    /// cadence and against a different endpoint.
    PollAssetModeration {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        /// `(row_id, asset_id)` pairs. The caller caps the length.
        assets: Vec<(String, u64)>,
    },
    /// Grant one universe `Use` access to a batch of assets.
    GrantAssetPermissions {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        universe_id: u64,
        /// Everything to grant, local uploads and inventory picks alike.
        asset_ids: Vec<u64>,
        /// `(row_id, asset_id)` for the subset that came from local rows, so
        /// the library can stamp what happened. Not parallel to `asset_ids`:
        /// an asset picked from the inventory has no local row and appears
        /// only in the list above.
        row_assets: Vec<(String, u64)>,
    },
    /// Populate the universe picker, and turn a pasted place ID into a
    /// universe ID. `place_id` is `None` when only the list is wanted.
    FetchUniverseTargets {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        place_id: Option<u64>,
    },
    /// Groups the account could publish under. Populates the creator picker
    /// and the inventory tree.
    FetchPublishGroups {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
    },
    /// One page of a creator's inventory for the browse pane.
    FetchCreations {
        user_id: u64,
        encrypted_cookie: Option<String>,
        session: crypto::StoreSession,
        use_credential_manager: bool,
        creator: Creator,
        kind: AssetKind,
        cursor: Option<String>,
    },
    /// Thumbnail images for the icon views. Unauthenticated, so no cookie.
    FetchAssetThumbnails { asset_ids: Vec<u64> },
}

/// Everything one upload needs. The cookie arrives encrypted and is decrypted
/// on the backend thread, never in the UI, matching `BrowseAsAccount`.
pub struct UploadJob {
    pub row_id: String,
    pub user_id: u64,
    pub encrypted_cookie: Option<String>,
    pub session: crypto::StoreSession,
    pub use_credential_manager: bool,
    pub creator: Creator,
    pub kind: AssetKind,
    pub display_name: String,
    pub description: String,
    pub file_path: PathBuf,
}

impl BackendCommand {
    /// Commands that write the account store to disk. These are handled inline
    /// (serially, in receive order) by `backend_loop` rather than spawned, so
    /// two saves can never interleave into a torn file or land out of order.
    fn is_serial_persistence(&self) -> bool {
        matches!(
            self,
            BackendCommand::SaveStore { .. } | BackendCommand::RekeyStore { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// Events (Backend → UI)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum BackendEvent {
    /// An account was validated and is ready to be added.
    AccountValidated {
        account: Box<Account>,
        encrypted_cookie: Option<String>,
    },
    /// Sibling of [`AccountValidated`] for the "add anyway" path. Skips the
    /// moderation-confirm dialog because the user already opted in.
    AccountForceAdded {
        account: Box<Account>,
        encrypted_cookie: Option<String>,
    },
    /// Account removed.
    AccountRemoved { user_id: u64 },
    /// Avatar URLs fetched.
    AvatarsUpdated(Vec<(u64, String)>),
    /// Raw avatar image bytes downloaded.
    AvatarImagesReady(Vec<(u64, Vec<u8>)>),
    /// Presences fetched.
    PresencesUpdated(Vec<(u64, Presence)>),
    /// Game launched successfully.
    GameLaunched,
    /// Store saved.
    StoreSaved,
    /// Store opened from disk, with the session that holds its data key.
    StoreUnlocked {
        store: Box<AccountStore>,
        session: Box<crypto::StoreSession>,
        /// The file predates the envelope format and should be upgraded before
        /// anything tries to save it.
        legacy: bool,
    },
    /// The store could not be opened because this device's key is gone. No
    /// password will help, so the UI offers recovery rather than a retry.
    DeviceKeyMissing,
    /// A re-key finished: the store and session below supersede the current
    /// ones and are already on disk.
    StoreRekeyed {
        store: Box<AccountStore>,
        session: Box<crypto::StoreSession>,
    },
    /// All Roblox instances killed (count).
    Killed(usize),
    /// Progress update during a bulk launch (launched_so_far, total).
    BulkLaunchProgress { launched: usize, total: usize },
    /// Bulk launch completed.
    BulkLaunchComplete { launched: usize, failed: usize },
    /// Account cookie revalidation result.
    AccountRevalidated {
        user_id: u64,
        valid: bool,
        username: String,
        display_name: String,
        /// Latest moderation snapshot, or `None` if no enforcement is active.
        moderation: Option<ram_core::models::ModerationInfo>,
    },
    /// An error occurred during a background operation.
    Error(String),
    /// The result of a sweep. Sent whenever the map or the running count
    /// changed, so the UI can render per-account instance state without
    /// enumerating processes on the paint thread.
    InstancesUpdated {
        instances: Vec<TrackedInstance>,
        /// Every running client, including ones RM did not launch.
        running_count: usize,
    },
    /// Windows were arranged.
    WindowsArranged,
    /// A newer version is available on GitLab.
    UpdateAvailable { version: String, url: String },
    /// Place name resolved for a private server entry.
    PlaceResolved {
        index: usize,
        place_name: String,
        place_id: u64,
        icon_bytes: Option<Vec<u8>>,
    },
    /// Share link resolved — contains the actual place_id, link_code, access_code, and server name.
    ShareLinkResolved {
        server_name: String,
        place_id: u64,
        universe_id: Option<u64>,
        link_code: String,
        access_code: String,
    },
    /// Share link resolution failed.
    ShareLinkFailed(String),
    /// "Open browser as" child window was successfully spawned.
    BrowseAsLaunched,
    /// `validate_cookie` rejected the cookie during AddAccount. Carries the
    /// raw cookie back so the UI can offer "open browser as" to investigate
    /// (e.g. when the account is terminated and the cookie is revoked).
    AddAccountAuthFailed {
        cookie: String,
        /// Best-effort moderation reason scraped despite the validation
        /// failure (some revocations leave the moderation endpoints reachable).
        moderation_message: Option<String>,
    },
    /// The file was read and hashed and the POST is going out. Carries the hash
    /// so the UI can record it without re-reading the file.
    AssetUploadStarted {
        row_id: String,
        file_sha256: String,
        file_bytes: u64,
    },
    /// Roblox accepted the bytes and handed back something to poll. Persisting
    /// this is what lets a restart pick the upload back up.
    AssetOperationCreated {
        row_id: String,
        operation: String,
        started_at: chrono::DateTime<chrono::Utc>,
    },
    /// One operation was polled. `StillPending` is a normal, frequent result.
    AssetOperationResolved {
        row_id: String,
        outcome: OperationOutcome,
    },
    /// The upload failed before any operation existed, so there is nothing to
    /// poll and nothing was created on Roblox.
    AssetUploadFailed {
        row_id: String,
        message: String,
        retryable: bool,
    },
    /// Moderation answered for one asset. Only sent when there is an answer:
    /// an asset Roblox declined to report on stays in review rather than being
    /// guessed at in either direction.
    AssetModerationResolved {
        row_id: String,
        status: ModerationStatus,
    },
    /// A `PollAssetOperations` batch finished. Carries nothing: every result
    /// already streamed out as its own event.
    AssetPollBatchDone,
    /// A universe was granted `Use` on these assets. `refused` counts the ones
    /// Roblox named in the response's per-asset error list, which a 200 does
    /// not rule out.
    AssetPermissionsGranted {
        universe_id: u64,
        row_ids: Vec<String>,
        granted: usize,
        refused: usize,
    },
    /// The grant failed. Kept separate from `Error` so the wording can say
    /// which universe, and so a wrong request shape cannot be mistaken for an
    /// upload problem.
    AssetPermissionsFailed { universe_id: u64, message: String },
    /// The universe picker's contents, and optionally the universe a pasted
    /// place ID resolved to.
    UniverseTargetsFetched {
        user_id: u64,
        universes: Vec<assets_api::UniverseTarget>,
        resolved_place: Option<(u64, u64)>,
    },
    /// Groups the account belongs to.
    PublishGroupsFetched {
        user_id: u64,
        groups: Vec<assets_api::GroupTarget>,
    },
    /// One page of an inventory. `error` is set when the provisional endpoint
    /// did not cooperate, so the pane can say so without failing the tab.
    CreationsFetched {
        creator: Creator,
        kind: AssetKind,
        /// True when this is a continuation, so the UI appends instead of
        /// replacing.
        appended: bool,
        page: assets_api::CreationPage,
        error: Option<String>,
    },
    /// Thumbnail results. `requested` is echoed back so the caller can tell
    /// which assets Roblox declined to render and schedule a retry, rather
    /// than leaving them as a permanent placeholder.
    AssetThumbnailsReady {
        requested: Vec<u64>,
        /// `(asset_id, png bytes)`. Only assets Roblox has finished rendering,
        /// so this can be shorter than `requested`.
        images: Vec<(u64, Vec<u8>)>,
    },
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

pub struct BackendBridge {
    pub cmd_tx: mpsc::UnboundedSender<BackendCommand>,
    pub evt_rx: mpsc::UnboundedReceiver<BackendEvent>,
    repaint_ctx: Option<egui::Context>,
}

impl BackendBridge {
    /// Spawn the `tokio` runtime on a dedicated thread and return the bridge.
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<BackendCommand>();
        let (evt_tx, evt_rx) = mpsc::unbounded_channel::<BackendEvent>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(backend_loop(cmd_rx, evt_tx));
        });

        Self { cmd_tx, evt_rx, repaint_ctx: None }
    }

    /// Give the bridge an egui context so it can request repaints when events arrive.
    pub fn set_repaint_ctx(&mut self, ctx: egui::Context) {
        if self.repaint_ctx.is_none() {
            self.repaint_ctx = Some(ctx);
        }
    }

    /// Send a command to the backend (non-blocking).
    pub fn send(&self, cmd: BackendCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Drain all pending events. Call once per frame.
    pub fn poll(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        while let Ok(evt) = self.evt_rx.try_recv() {
            events.push(evt);
        }
        if !events.is_empty() {
            if let Some(ctx) = &self.repaint_ctx {
                ctx.request_repaint();
            }
        }
        events
    }
}

// ---------------------------------------------------------------------------
// Async event loop
// ---------------------------------------------------------------------------

async fn backend_loop(
    mut rx: mpsc::UnboundedReceiver<BackendCommand>,
    tx: mpsc::UnboundedSender<BackendEvent>,
) {
    let client = RobloxClient::default();
    // Owned here, not by the UI: a launch has to snapshot the PID set on the
    // same side of the channel that issues the launch, or a slow frame between
    // the two would let an unrelated client slip into the window.
    let registry: Registry = Arc::new(Mutex::new(InstanceState::default()));

    while let Some(cmd) = rx.recv().await {
        let client = client.clone();
        let tx = tx.clone();
        let registry = Arc::clone(&registry);

        // Account-store writes MUST run serially and in the order they were
        // enqueued. Spawning them (as we do for everything else) let two saves
        // overlap on the same file — interleaving into a torn AES-GCM blob that
        // no longer decrypts — or finish out of order, so an older store
        // clobbered a newer one. Both were the v1.4.4 lockout/corruption bug.
        // Handle them inline so the next command isn't even dequeued until the
        // write has fully landed. The write itself is fast (encrypt + atomic
        // rename), so blocking the loop here is fine.
        if cmd.is_serial_persistence() {
            match handle_command(cmd, &client, &tx, &registry).await {
                Ok(evt) => {
                    let _ = tx.send(evt);
                }
                Err(e) => {
                    error!("Backend error: {e}");
                    let _ = tx.send(BackendEvent::Error(e.to_string()));
                }
            }
            continue;
        }

        // Every other command runs as its own spawned task for concurrency.
        tokio::spawn(async move {
            match handle_command(cmd, &client, &tx, &registry).await {
                Ok(evt) => {
                    let _ = tx.send(evt);
                }
                Err(e) => {
                    error!("Backend error: {e}");
                    let _ = tx.send(BackendEvent::Error(e.to_string()));
                }
            }
        });
    }
}

async fn handle_command(
    cmd: BackendCommand,
    client: &RobloxClient,
    tx: &mpsc::UnboundedSender<BackendEvent>,
    registry: &Registry,
) -> Result<BackendEvent, CoreError> {
    match cmd {
        BackendCommand::AddAccount {
            cookie,
            session,
            use_credential_manager,
        } => {
            let (user_id, username, display_name) = match client
                .validate_cookie(&cookie)
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    // Cookie rejected at the auth layer (401/403 → typically
                    // terminated or otherwise revoked). Try the moderation
                    // endpoint anyway — it sometimes still works and gives
                    // us a real reason to show — then bounce back to the UI
                    // so the user can open the cookie in a browser instead.
                    info!("AddAccount: validate failed ({e}); probing moderation endpoint");
                    let mod_msg = api::fetch_moderation_message(client, &cookie)
                        .await
                        .map(|(r, _)| r);
                    return Ok(BackendEvent::AddAccountAuthFailed {
                        cookie,
                        moderation_message: mod_msg,
                    });
                }
            };
            let mut account = Account::new(user_id, username, display_name);

            let encrypted = if use_credential_manager {
                crypto::credential_store(user_id, &cookie)?;
                None
            } else {
                Some(crypto::encrypt_cookie(&cookie, &session)?)
            };
            account.encrypted_cookie = encrypted.clone();
            account.last_validated = Some(chrono::Utc::now());

            // Detect any active moderation on this account so the UI can
            // either warn the user (add flow) or flag it visually (revalidation).
            match api::fetch_moderation_status(client, user_id, &cookie).await {
                Ok(info) => account.moderation = info,
                Err(e) => info!("Moderation check failed for {user_id} (non-fatal): {e}"),
            }

            // Fetch avatar URL and image bytes immediately after validation
            if let Ok(avatars) = api::fetch_avatars(client, &[user_id]).await {
                if let Some((_, url)) = avatars.first() {
                    account.avatar_url = url.clone();
                }
                let images = api::download_avatar_images(client, &avatars).await;
                if !images.is_empty() {
                    let _ = tx.send(BackendEvent::AvatarImagesReady(images));
                }
            }

            info!("Validated account {} ({})", account.username, user_id);
            Ok(BackendEvent::AccountValidated {
                account: Box::new(account),
                encrypted_cookie: encrypted,
            })
        }
        BackendCommand::AddAccountForced {
            cookie,
            username,
            session,
            use_credential_manager,
        } => {
            // Cookie didn't validate but the user wants to add the account
            // anyway. Resolve the canonical identity by username so the entry
            // we store points at a real Roblox account.
            let (user_id, canonical_username, display_name) =
                api::lookup_username(client, &username)
                    .await?
                    .ok_or_else(|| CoreError::AccountNotFound(username.clone()))?;

            let mut account =
                Account::new(user_id, canonical_username, display_name);

            let encrypted = if use_credential_manager {
                crypto::credential_store(user_id, &cookie)?;
                None
            } else {
                Some(crypto::encrypt_cookie(&cookie, &session)?)
            };
            account.encrypted_cookie = encrypted.clone();
            // Cookie failed validation upstream — record it as expired so the
            // sidebar/main panel reflect reality. The next revalidation will
            // unmark it if the user resolves things in the browser.
            account.cookie_expired = true;

            // Best-effort moderation: public ban flag + cookie-only message
            // probe. Either may fail (cookie revoked, network, etc.) and we
            // still want to add the account, so wrap in ok().
            let is_banned = api::fetch_public_ban_status(client, user_id)
                .await
                .unwrap_or(false);
            let msg = api::fetch_moderation_message(client, &cookie).await;
            if is_banned || msg.is_some() {
                let (reason, expires_at) = match msg {
                    Some((r, e)) => (Some(r), e),
                    None => (None, None),
                };
                account.moderation = Some(ram_core::models::ModerationInfo {
                    is_banned,
                    reason,
                    expires_at,
                    last_checked: Some(chrono::Utc::now()),
                });
            }

            // Best-effort avatar fetch. thumbnails.roblox.com is public, and
            // we no longer send a cookie with it, so a dead cookie on this
            // account is irrelevant here.
            if let Ok(avatars) = api::fetch_avatars(client, &[user_id]).await {
                if let Some((_, url)) = avatars.first() {
                    account.avatar_url = url.clone();
                }
                let images = api::download_avatar_images(client, &avatars).await;
                if !images.is_empty() {
                    let _ = tx.send(BackendEvent::AvatarImagesReady(images));
                }
            }

            info!(
                "Force-added account {} ({}) with cookie_expired=true",
                account.username, user_id
            );
            Ok(BackendEvent::AccountForceAdded {
                account: Box::new(account),
                encrypted_cookie: encrypted,
            })
        }
        BackendCommand::RemoveAccount { user_id } => {
            // Best-effort delete from credential manager
            let _ = crypto::credential_delete(user_id);
            Ok(BackendEvent::AccountRemoved { user_id })
        }
        BackendCommand::LaunchGame {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            place_id,
            job_id,
            link_code,
            access_code,
            multi_instance,
            kill_background,
            privacy_mode,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie stored for this account".into())
                })?;
                crypto::decrypt_cookie(&enc, &session)?
            };
            if multi_instance {
                process::enable_multi_instance()?;
            }
            if kill_background || multi_instance {
                process::kill_tray_roblox();
            }
            if privacy_mode {
                process::clear_roblox_cookies();
            }
            let ticket = client.generate_auth_ticket(&cookie).await?;
            let launchtime = note_launch(registry, user_id, place_id);
            process::launch_game(
                &ticket,
                place_id,
                job_id.as_deref(),
                link_code.as_deref(),
                access_code.as_deref(),
                launchtime,
            )?;
            Ok(BackendEvent::GameLaunched)
        }
        BackendCommand::SaveStore {
            store,
            path,
            session,
        } => {
            crypto::save_store(&path, &store, &session)?;
            Ok(BackendEvent::StoreSaved)
        }
        BackendCommand::UnlockWithDevice { path } => match crypto::unlock_with_device(&path) {
            Ok((store, session)) => Ok(BackendEvent::StoreUnlocked {
                store: Box::new(store),
                session: Box::new(session),
                legacy: false,
            }),
            // Surfaced as its own event: a missing device key is not something
            // the user can retype their way out of.
            Err(CoreError::DeviceKeyMissing) => Ok(BackendEvent::DeviceKeyMissing),
            Err(e) => Err(e),
        },
        BackendCommand::UnlockWithPassword { path, password } => {
            let (store, session) = crypto::unlock_with_password(&path, &password)?;
            let legacy = session.is_legacy();
            Ok(BackendEvent::StoreUnlocked {
                store: Box::new(store),
                session: Box::new(session),
                legacy,
            })
        }
        BackendCommand::RekeyStore {
            store,
            path,
            session,
            new_password,
            upgrade_legacy,
        } => {
            let (store, session) = if upgrade_legacy {
                // Rotates onto a fresh data key and re-encrypts every cookie.
                // All-or-nothing: a failure here leaves the old file in place.
                crypto::upgrade_v1(&store, &session, new_password.as_deref())?
            } else {
                (store, crypto::rewrap(&session, new_password.as_deref())?)
            };
            // Not `save_store`: that would leave a `.bak` readable under the
            // key we just retired, which backup recovery would happily open.
            crypto::save_rekeyed(&path, &store, &session)?;
            Ok(BackendEvent::StoreRekeyed {
                store: Box::new(store),
                session: Box::new(session),
            })
        }
        BackendCommand::KillAll => {
            let count = process::kill_all_roblox()?;
            Ok(BackendEvent::Killed(count))
        }
        BackendCommand::RefreshAll {
            user_ids,
            first_user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for refresh".into())
                })?;
                crypto::decrypt_cookie(&enc, &session)?
            };
            // Avatars and presence are independent calls over the same account
            // list. They used to share a `?`, so one failed avatar batch
            // aborted the whole command and left every account with a
            // placeholder image *and* a stale grey presence dot. Keep the
            // avatar leg self-contained so it can fail alone.
            match api::fetch_avatars(client, &user_ids).await {
                Ok(avatars) => {
                    let _ = tx.send(BackendEvent::AvatarsUpdated(avatars.clone()));
                    // Download actual image bytes (skips failures)
                    let images = api::download_avatar_images(client, &avatars).await;
                    if !images.is_empty() {
                        let _ = tx.send(BackendEvent::AvatarImagesReady(images));
                    }
                }
                Err(e) => info!("Avatar refresh failed (non-fatal): {e}"),
            }
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::RefreshPresenceOnly {
            user_ids,
            first_user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for refresh".into())
                })?;
                crypto::decrypt_cookie(&enc, &session)?
            };
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::BulkLaunchEncrypted {
            accounts,
            session,
            use_credential_manager,
            place_id,
            job_id,
            link_code,
            access_code,
            multi_instance,
            kill_background,
            privacy_mode,
            launch_delay_secs,
        } => {
            if multi_instance {
                process::enable_multi_instance()?;
            }
            if kill_background || multi_instance {
                process::kill_tray_roblox();
            }
            if privacy_mode {
                process::clear_roblox_cookies();
            }

            // If no Job ID was provided and no link_code (private server), resolve
            // a public server so all accounts land in the same server.
            let resolved_job_id = if job_id.is_some() || link_code.is_some() {
                job_id
            } else {
                // Decrypt the first account's cookie to make the API call
                let first = accounts.first().ok_or_else(|| {
                    CoreError::Process("no accounts to launch".into())
                })?;
                let first_cookie = if use_credential_manager {
                    crypto::credential_load(first.0)?
                } else {
                    match &first.1 {
                        Some(enc) => crypto::decrypt_cookie(enc, &session)?,
                        None => {
                            return Err(CoreError::Crypto(
                                "no encrypted cookie for first account".into(),
                            ))
                        }
                    }
                };
                match api::fetch_servers(client, &first_cookie, place_id, None).await {
                    Ok((servers, _)) => {
                        if let Some(server) = servers.into_iter().next() {
                            info!("Bulk launch: resolved server {} ({}/{} players)",
                                  server.id, server.playing, server.max_players);
                            Some(server.id)
                        } else {
                            info!("Bulk launch: no public servers found, launching without Job ID");
                            None
                        }
                    }
                    Err(e) => {
                        info!("Bulk launch: server fetch failed ({e}), launching without Job ID");
                        None
                    }
                }
            };

            let total = accounts.len();
            let mut launched = 0usize;
            let mut failed = 0usize;
            for (i, (user_id, encrypted_cookie)) in accounts.iter().enumerate() {
                let cookie_result = if use_credential_manager {
                    crypto::credential_load(*user_id)
                } else {
                    match encrypted_cookie {
                        Some(enc) => crypto::decrypt_cookie(enc, &session),
                        None => Err(CoreError::Crypto(
                            "no encrypted cookie stored".into(),
                        )),
                    }
                };
                match cookie_result {
                    Ok(cookie) => {
                        match client.generate_auth_ticket(&cookie).await {
                            Ok(ticket) => {
                                // Snapshot before each launch in the batch, not
                                // once for the batch: by the time account three
                                // goes out, accounts one and two have clients
                                // that must not look new.
                                let launchtime = note_launch(registry, *user_id, place_id);
                                if let Err(e) = process::launch_game(
                                    &ticket,
                                    place_id,
                                    resolved_job_id.as_deref(),
                                    link_code.as_deref(),
                                    access_code.as_deref(),
                                    launchtime,
                                ) {
                                    error!("Bulk launch failed for user {user_id}: {e}");
                                    failed += 1;
                                } else {
                                    launched += 1;
                                }
                            }
                            Err(e) => {
                                error!("Auth ticket failed for user {user_id}: {e}");
                                failed += 1;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Cookie decrypt failed for user {user_id}: {e}");
                        failed += 1;
                    }
                }
                let _ = tx.send(BackendEvent::BulkLaunchProgress {
                    launched: i + 1,
                    total,
                });
                // Inter-launch pacing: the user-configured throttle plus the
                // existing 3s tray-kill window (which is also a de-facto
                // launch delay). Pick the larger of the two so the user's
                // setting always wins when they want a longer cooldown.
                if i + 1 < total {
                    let user_delay = launch_delay_secs as u64;
                    let needs_tray_kill = kill_background || multi_instance;
                    let tray_window: u64 = if needs_tray_kill { 3 } else { 0 };
                    let wait = user_delay.max(tray_window);
                    if wait > 0 {
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    }
                    if needs_tray_kill {
                        process::kill_tray_roblox();
                    }
                }
            }
            Ok(BackendEvent::BulkLaunchComplete { launched, failed })
        }
        BackendCommand::RevalidateAll {
            accounts,
            session,
            use_credential_manager,
        } => {
            for (user_id, encrypted_cookie) in &accounts {
                let cookie_result = if use_credential_manager {
                    crypto::credential_load(*user_id)
                } else {
                    match encrypted_cookie {
                        Some(enc) => crypto::decrypt_cookie(enc, &session),
                        None => continue,
                    }
                };
                let cookie = match cookie_result {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                match client.validate_cookie(&cookie).await {
                    Ok((_, username, display_name)) => {
                        // Cookie still works — also refresh moderation state.
                        let moderation =
                            api::fetch_moderation_status(client, *user_id, &cookie)
                                .await
                                .ok()
                                .flatten();
                        let _ = tx.send(BackendEvent::AccountRevalidated {
                            user_id: *user_id,
                            valid: true,
                            username,
                            display_name,
                            moderation,
                        });
                    }
                    Err(_) => {
                        info!("Cookie expired for user {user_id}");
                        // Cookie's dead — try the moderation endpoint anyway
                        // (it sometimes still works for accounts that were
                        // *just* terminated and gives us the real reason
                        // before further auth revocation kicks in), and fall
                        // back to the public is-banned flag.
                        let is_banned =
                            api::fetch_public_ban_status(client, *user_id)
                                .await
                                .unwrap_or(false);
                        let msg =
                            api::fetch_moderation_message(client, &cookie).await;
                        let (reason, expires_at) = match msg {
                            Some((r, e)) => (Some(r), e),
                            None => (None, None),
                        };
                        let moderation = if is_banned || reason.is_some() {
                            Some(ram_core::models::ModerationInfo {
                                is_banned,
                                reason,
                                expires_at,
                                last_checked: Some(chrono::Utc::now()),
                            })
                        } else {
                            None
                        };
                        let _ = tx.send(BackendEvent::AccountRevalidated {
                            user_id: *user_id,
                            valid: false,
                            username: String::new(),
                            display_name: String::new(),
                            moderation,
                        });
                    }
                }
            }
            Ok(BackendEvent::StoreSaved)
        }
        BackendCommand::SweepInstances => Ok(sweep_instances(registry)),
        BackendCommand::ArrangeWindows => {
            // Small delay to let Roblox windows finish appearing
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            process::arrange_roblox_windows();
            Ok(BackendEvent::WindowsArranged)
        }
        BackendCommand::CheckForUpdates { current_version } => {
            match api::check_for_updates(&current_version).await {
                Ok(Some((version, url))) => {
                    Ok(BackendEvent::UpdateAvailable { version, url })
                }
                Ok(None) => Ok(BackendEvent::StoreSaved), // no-op event
                Err(e) => {
                    info!("Update check failed (non-fatal): {e}");
                    Ok(BackendEvent::StoreSaved) // silently ignore
                }
            }
        }
        BackendCommand::ResolvePlace { place_id, universe_id, index } => {
            // Both the game name and icon endpoints work without auth when we
            // have a universe_id. If we don't, we can't resolve without auth.
            if let Some(uid) = universe_id {
                let name = api::resolve_universe_name(client, uid).await
                    .unwrap_or_default();
                let icon_bytes = match api::fetch_game_icons(client, "", &[uid]).await {
                    Ok(icons) => {
                        if let Some((_, url)) = icons.into_iter().next() {
                            client.get_bytes(&url, "").await.ok()
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        info!("Game icon fetch failed for universe {uid}: {e}");
                        None
                    }
                };
                Ok(BackendEvent::PlaceResolved { index, place_name: name, place_id, icon_bytes })
            } else {
                // No universe_id — cannot resolve without auth. Return empty.
                Ok(BackendEvent::PlaceResolved { index, place_name: String::new(), place_id, icon_bytes: None })
            }
        }
        BackendCommand::BrowseAsAccount {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            profile_dir,
            label,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie stored for this account".into())
                })?;
                crypto::decrypt_cookie(&enc, &session)?
            };
            crate::browser_login::spawn_browse_as(profile_dir, cookie, label)
                .map_err(CoreError::Process)?;
            Ok(BackendEvent::BrowseAsLaunched)
        }
        BackendCommand::ResolveShareLink {
            share_code,
            server_name,
            first_user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for share link resolution".into())
                })?;
                crypto::decrypt_cookie(&enc, &session)?
            };
            match api::resolve_share_link(client, &cookie, &share_code).await {
                Ok((place_id, universe_id, link_code, access_code)) => {
                    Ok(BackendEvent::ShareLinkResolved {
                        server_name,
                        place_id,
                        universe_id,
                        link_code,
                        access_code,
                    })
                }
                Err(e) => {
                    info!("ResolveShareLink failed: {e}");
                    Ok(BackendEvent::ShareLinkFailed(e.to_string()))
                }
            }
        }
        BackendCommand::UploadAsset(job) => Ok(upload_asset(*job, client, tx).await),
        BackendCommand::PollAssetOperations {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            operations,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;
            poll_asset_operations(client, &cookie, &operations, tx).await;
            Ok(BackendEvent::AssetPollBatchDone)
        }
        BackendCommand::PollAssetModeration {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            assets,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;
            poll_asset_moderation(client, &cookie, &assets, tx).await;
            Ok(BackendEvent::AssetPollBatchDone)
        }
        BackendCommand::GrantAssetPermissions {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            universe_id,
            asset_ids,
            row_assets,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;
            match assets_api::grant_use_permission(client, &cookie, universe_id, &asset_ids).await {
                Ok(outcome) => {
                    for (asset_id, message) in &outcome.failures {
                        match asset_id {
                            Some(id) => info!("grant refused for asset {id}: {message}"),
                            None => info!("grant refused: {message}"),
                        }
                    }
                    // Only the rows Roblox actually confirmed get stamped, so
                    // the local mirror never claims access the experience does
                    // not have.
                    let granted_rows = row_assets
                        .into_iter()
                        .filter(|(_, asset_id)| outcome.granted.contains(asset_id))
                        .map(|(row_id, _)| row_id)
                        .collect();
                    Ok(BackendEvent::AssetPermissionsGranted {
                        universe_id,
                        row_ids: granted_rows,
                        granted: outcome.granted.len(),
                        refused: outcome.failures.len(),
                    })
                }
                // Reported against the grant, not as a generic error: a wrong
                // request shape here must not read as an upload failure.
                Err(e) => Ok(BackendEvent::AssetPermissionsFailed {
                    universe_id,
                    message: e.to_string(),
                }),
            }
        }
        BackendCommand::FetchUniverseTargets {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            place_id,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;
            // The listing is provisional, so a failure must not take the place
            // resolution down with it. Both legs degrade independently.
            let universes = match assets_api::list_manageable_universes(client, &cookie).await {
                Ok(list) => list,
                Err(e) => {
                    info!("universe listing unavailable: {e}");
                    Vec::new()
                }
            };
            let mut resolved_place = None;
            if let Some(place_id) = place_id {
                match assets_api::resolve_place_universe(client, &cookie, place_id).await {
                    Ok(universe_id) => resolved_place = Some((place_id, universe_id)),
                    Err(e) => info!("could not resolve place {place_id}: {e}"),
                }
            }
            Ok(BackendEvent::UniverseTargetsFetched {
                user_id,
                universes,
                resolved_place,
            })
        }
        BackendCommand::FetchPublishGroups {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;
            let groups = match assets_api::list_publishable_groups(client, &cookie).await {
                Ok(groups) => groups,
                Err(e) => {
                    info!("group listing unavailable: {e}");
                    Vec::new()
                }
            };
            Ok(BackendEvent::PublishGroupsFetched { user_id, groups })
        }
        BackendCommand::FetchCreations {
            user_id,
            encrypted_cookie,
            session,
            use_credential_manager,
            creator,
            kind,
            cursor,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;
            let appended = cursor.is_some();
            match assets_api::list_creations(client, &cookie, creator, kind, cursor.as_deref())
                .await
            {
                Ok(page) => Ok(BackendEvent::CreationsFetched {
                    creator,
                    kind,
                    appended,
                    page,
                    error: None,
                }),
                // Reported on the node, not as a toast. This endpoint is
                // undocumented, and the library still works from the local
                // index without it. Logged as well: routing a failure only
                // into a hover tooltip left a real 404 invisible in rm.log,
                // which made the first version of this take far longer to
                // diagnose than it should have.
                Err(e) => {
                    error!("inventory listing failed for {creator:?} {kind:?}: {e}");
                    Ok(BackendEvent::CreationsFetched {
                        creator,
                        kind,
                        appended,
                        page: assets_api::CreationPage::default(),
                        error: Some(e.to_string()),
                    })
                }
            }
        }
        BackendCommand::FetchAssetThumbnails { asset_ids } => {
            let images = match assets_api::fetch_asset_thumbnails(client, &asset_ids).await {
                Ok(images) => images,
                Err(e) => {
                    // Logged, not silent: a thumbnail that never appears used
                    // to leave no trace anywhere.
                    info!("thumbnail batch failed: {e}");
                    Vec::new()
                }
            };
            if images.len() < asset_ids.len() {
                info!(
                    "{} of {} thumbnails not rendered yet; will retry",
                    asset_ids.len() - images.len(),
                    asset_ids.len()
                );
            }
            Ok(BackendEvent::AssetThumbnailsReady {
                requested: asset_ids,
                images,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Asset uploads
// ---------------------------------------------------------------------------

/// How long the uploading task keeps polling before handing the operation over
/// to the UI's long-horizon timer. Decals and short audio usually resolve
/// inside this window, so the common case reaches an asset ID in a few seconds
/// without waiting for the next tick.
const UPLOAD_POLL_BURST: &[u64] = &[1, 2, 4];

/// The same burst for audio, which is slower to ingest than anything else this
/// app uploads. Giving up after 7 seconds left a queue full of rows that had
/// perfectly good operations behind them and simply had not been asked twice.
const AUDIO_POLL_BURST: &[u64] = &[2, 3, 5, 8, 12];

/// Gap between polls inside one batch. Keeps a large backlog to a few requests
/// per second per account instead of a burst.
const POLL_SPACING_MS: u64 = 250;

/// Mint this launch's attribution token, snapshot the running clients, and
/// queue the mapping. Returns the token to stamp into the launch URI.
///
/// Must be called immediately before the launch itself, and the token must be
/// minted here rather than inside `launch_game`, so that the mapping is on
/// record before the client it describes can possibly exist. A sweep landing in
/// the gap would otherwise see an unrecognised client.
///
/// The snapshot matters only to the appearance-order fallback: it is what
/// distinguishes the client this launch produces from the ones already up.
/// Enumerating processes takes a few milliseconds, which is why it happens
/// after the auth ticket (a network round trip) rather than before it.
fn note_launch(registry: &Registry, user_id: u64, place_id: u64) -> i64 {
    let launchtime = process::next_launchtime();
    match registry.lock() {
        Ok(mut state) => {
            let live = state.tokens.scan();
            state
                .registry
                .note_launch(user_id, place_id, launchtime, &live, chrono::Utc::now());
        }
        // A poisoned registry means a previous holder panicked. Attribution is
        // a convenience; refusing to launch over it would not be. The token is
        // still stamped into the URI, so a later sweep on a healthy registry
        // simply reports the client as unattributed rather than mis-assigning
        // it.
        Err(e) => warn!("instance registry unavailable, launch will not be attributed: {e}"),
    }
    launchtime
}

/// Reconcile the registry against reality and report the result.
fn sweep_instances(registry: &Registry) -> BackendEvent {
    let Ok(mut state) = registry.lock() else {
        // No cache to scan through, but the badge still needs a number.
        return BackendEvent::InstancesUpdated {
            instances: Vec::new(),
            running_count: process::roblox_pids().len(),
        };
    };
    let live = state.tokens.scan();
    let running_count = live.len();
    let outcome = state.registry.sweep(&live, chrono::Utc::now());

    for instance in &outcome.attributed {
        // Say which kind. "Matched" is a fact about the process; "guessed" is
        // an inference, and a log that does not distinguish them is a log that
        // cannot be used to work out why an attribution was wrong.
        let how = match instance.attribution {
            Attribution::Exact => "matched",
            Attribution::Inferred => "guessed (command line unreadable)",
        };
        info!(
            "Roblox PID {} {how} to user {} (place {})",
            instance.pid, instance.user_id, instance.place_id
        );
    }
    for instance in &outcome.upgraded {
        info!(
            "Roblox PID {} confirmed as user {} by its command line",
            instance.pid, instance.user_id
        );
    }
    for instance in &outcome.exited {
        info!("Roblox PID {} (user {}) exited", instance.pid, instance.user_id);
    }
    for launch in &outcome.abandoned {
        // Worth a line: it means either the launch failed silently or the
        // client took longer than the window, and both are things a user
        // reporting "it did not launch" will want in the log.
        info!(
            "No Roblox client appeared for user {} within the attribution window",
            launch.user_id
        );
    }

    BackendEvent::InstancesUpdated {
        instances: state.registry.snapshot(),
        running_count,
    }
}

/// The cookie-decrypt idiom used by every account-scoped command.
fn decrypt_for(
    user_id: u64,
    encrypted_cookie: Option<String>,
    session: &crypto::StoreSession,
    use_credential_manager: bool,
) -> Result<String, CoreError> {
    if use_credential_manager {
        return crypto::credential_load(user_id);
    }
    let enc = encrypted_cookie
        .ok_or_else(|| CoreError::Crypto("no encrypted cookie stored for this account".into()))?;
    crypto::decrypt_cookie(&enc, session)
}

/// Run one upload to completion, or as far as it gets. Errors are reported
/// against the row rather than propagated, so one bad file cannot surface as an
/// anonymous toast with no way to tell which row it came from.
async fn upload_asset(
    job: UploadJob,
    client: &RobloxClient,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> BackendEvent {
    let row_id = job.row_id.clone();
    match upload_asset_inner(job, client, tx).await {
        Ok(event) => event,
        Err(e) => {
            let retryable = match &e {
                CoreError::RobloxApi { status, .. } => assets_api::is_retryable_status(*status),
                CoreError::RateLimited | CoreError::Http(_) => true,
                // Both of these look terminal and usually are not. Roblox
                // emits bare 403s on `apis.roblox.com` under load with a
                // perfectly good cookie, and a bulk batch rotating the CSRF
                // token out from under its own concurrent requests exhausts
                // the token retries the same way. Calling either one fatal is
                // what turned a transient blip into a queue of dead rows with
                // no way back. If the cookie really is revoked the retries
                // exhaust and the row still fails, just later and honestly.
                CoreError::CookieRejected | CoreError::AuthFailed(_) => true,
                // A local read failure or a decrypt failure will not fix itself.
                _ => false,
            };
            error!("upload failed for row {row_id}: {e}");
            BackendEvent::AssetUploadFailed {
                row_id,
                message: e.to_string(),
                retryable,
            }
        }
    }
}

async fn upload_asset_inner(
    job: UploadJob,
    client: &RobloxClient,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> Result<BackendEvent, CoreError> {
    let UploadJob {
        row_id,
        user_id,
        encrypted_cookie,
        session,
        use_credential_manager,
        creator,
        kind,
        display_name,
        description,
        file_path,
    } = job;

    let cookie = decrypt_for(user_id, encrypted_cookie, &session, use_credential_manager)?;

    // Reading and hashing 20 MB is tens of milliseconds of blocking work. Doing
    // it on a runtime worker would stall the presence and avatar tasks that
    // share this runtime.
    let path_for_read = file_path.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path_for_read))
        .await
        .map_err(|e| CoreError::Process(format!("file read task failed: {e}")))??;

    let file_bytes = bytes.len() as u64;
    let (_, mime) = assets::classify_path(&file_path).ok_or_else(|| CoreError::RobloxApi {
        status: 400,
        message: "This file type cannot be uploaded to Roblox".to_string(),
    })?;

    let hash_bytes = bytes.clone();
    let file_sha256 = tokio::task::spawn_blocking(move || assets::sha256_hex(&hash_bytes))
        .await
        .map_err(|e| CoreError::Process(format!("hash task failed: {e}")))?;

    let _ = tx.send(BackendEvent::AssetUploadStarted {
        row_id: row_id.clone(),
        file_sha256,
        file_bytes,
    });

    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload")
        .to_string();

    let request = assets_api::UploadRequest {
        kind,
        display_name,
        description,
        creator,
        file_name,
        mime,
        bytes,
    };
    let created = assets_api::create_asset(client, &cookie, &request).await?;

    let Some(operation) = created.operation else {
        // No operation to poll. Either Roblox resolved it inline, or it told us
        // nothing useful, and `parse_operation_response` already decided which.
        return Ok(finish_upload(client, &cookie, row_id, created.outcome, tx).await);
    };

    let _ = tx.send(BackendEvent::AssetOperationCreated {
        row_id: row_id.clone(),
        operation: operation.clone(),
        started_at: chrono::Utc::now(),
    });

    if !matches!(created.outcome, OperationOutcome::StillPending) {
        return Ok(finish_upload(client, &cookie, row_id, created.outcome, tx).await);
    }

    let burst = match kind {
        AssetKind::Audio => AUDIO_POLL_BURST,
        _ => UPLOAD_POLL_BURST,
    };
    for wait in burst {
        tokio::time::sleep(Duration::from_secs(*wait)).await;
        match assets_api::poll_operation(client, &cookie, &operation).await {
            Ok(OperationOutcome::StillPending) => continue,
            Ok(outcome) => return Ok(finish_upload(client, &cookie, row_id, outcome, tx).await),
            // A failed poll is not a failed upload. Leave it Pending and let
            // the UI's timer keep asking.
            Err(e) => {
                info!("poll burst for {row_id} failed, deferring to the timer: {e}");
                break;
            }
        }
    }

    Ok(BackendEvent::AssetOperationResolved {
        row_id,
        outcome: OperationOutcome::StillPending,
    })
}

/// Report an operation's outcome, and for an approval ask moderation about it
/// once before handing the row to the timer.
///
/// The immediate check is what keeps the common case fast: Decals and Models
/// come back `DoesNotRequire` and land on Approved in the same breath as the
/// upload, so only assets that genuinely are in review ever show as such.
async fn finish_upload(
    client: &RobloxClient,
    cookie: &str,
    row_id: String,
    outcome: OperationOutcome,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> BackendEvent {
    let OperationOutcome::Approved { asset_id, .. } = outcome else {
        return BackendEvent::AssetOperationResolved { row_id, outcome };
    };

    // Order matters: the row has to reach `InReview` (which is what
    // `AssetOperationResolved` with an approval now means) before any verdict
    // lands on it, or the verdict has nothing to apply to.
    let _ = tx.send(BackendEvent::AssetOperationResolved {
        row_id: row_id.clone(),
        outcome,
    });

    match assets_api::fetch_moderation_statuses(client, cookie, &[asset_id]).await {
        Ok(statuses) => match statuses.into_iter().next() {
            Some((_, status)) => BackendEvent::AssetModerationResolved { row_id, status },
            None => BackendEvent::AssetPollBatchDone,
        },
        // Not knowing is not a verdict. The row stays in review and the timer
        // asks again.
        Err(e) => {
            info!("first moderation check for {row_id} failed, deferring to the timer: {e}");
            BackendEvent::AssetPollBatchDone
        }
    }
}

/// Poll a batch serially, streaming one event per operation. Serial and paced
/// so a large backlog stays a steady trickle rather than a burst.
async fn poll_asset_operations(
    client: &RobloxClient,
    cookie: &str,
    operations: &[(String, String)],
    tx: &mpsc::UnboundedSender<BackendEvent>,
) {
    for (index, (row_id, operation)) in operations.iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(POLL_SPACING_MS)).await;
        }
        match assets_api::poll_operation(client, cookie, operation).await {
            Ok(OperationOutcome::StillPending) => {}
            Ok(outcome) => {
                let _ = tx.send(BackendEvent::AssetOperationResolved {
                    row_id: row_id.clone(),
                    outcome,
                });
            }
            // Transient: say nothing and let the next tick try again. Reporting
            // it would spam the toast stack every poll interval while Roblox is
            // having a bad day.
            Err(e) => info!("poll of {operation} failed: {e}"),
        }
    }
}

/// Ask moderation about a batch and stream back only the rows that got a
/// verdict.
///
/// One request covers the whole batch, so unlike operation polling there is
/// nothing to pace. Rows Roblox says nothing about are left alone.
async fn poll_asset_moderation(
    client: &RobloxClient,
    cookie: &str,
    assets: &[(String, u64)],
    tx: &mpsc::UnboundedSender<BackendEvent>,
) {
    let asset_ids: Vec<u64> = assets.iter().map(|(_, id)| *id).collect();
    let statuses = match assets_api::fetch_moderation_statuses(client, cookie, &asset_ids).await {
        Ok(statuses) => statuses,
        Err(e) => {
            info!("moderation poll of {} asset(s) failed: {e}", asset_ids.len());
            return;
        }
    };

    for (row_id, asset_id) in assets {
        let Some((_, status)) = statuses.iter().find(|(id, _)| id == asset_id) else {
            continue;
        };
        let _ = tx.send(BackendEvent::AssetModerationResolved {
            row_id: row_id.clone(),
            status: status.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Serial-persistence classification
    // -----------------------------------------------------------------------

    /// What [`BackendCommand::is_serial_persistence`] is required to answer, as
    /// a second opinion written out variant by variant.
    ///
    /// This match deliberately has no `_` arm. `BackendCommand` carries
    /// `#[allow(dead_code)]`, so a variant nobody constructs yet still compiles
    /// and the classifier in the production code would silently treat it as
    /// non-serial. Here it cannot: adding a variant breaks this file until
    /// somebody writes down which side of the line it falls on.
    ///
    /// The line: does handling this command write the account store to disk? If
    /// yes it must be `true`, because `backend_loop` only handles `true`
    /// commands inline. A `false` command is `tokio::spawn`ed, and two spawned
    /// writers on the same file are the v1.4.4 corruption bug.
    fn writes_the_store(cmd: &BackendCommand) -> bool {
        use BackendCommand as C;
        match cmd {
            // Encrypts and rewrites the store file.
            C::SaveStore { .. } => true,
            // Rewraps the key material and rewrites the store file.
            C::RekeyStore { .. } => true,

            // Everything below either only reads the store, touches a different
            // file, or does no file I/O at all.
            C::AddAccount { .. }
            | C::AddAccountForced { .. }
            | C::RemoveAccount { .. }
            | C::LaunchGame { .. }
            | C::UnlockWithDevice { .. }
            | C::UnlockWithPassword { .. }
            | C::KillAll
            | C::RefreshAll { .. }
            | C::RefreshPresenceOnly { .. }
            | C::BulkLaunchEncrypted { .. }
            | C::RevalidateAll { .. }
            | C::SweepInstances
            | C::ArrangeWindows
            | C::CheckForUpdates { .. }
            | C::ResolvePlace { .. }
            | C::ResolveShareLink { .. }
            | C::BrowseAsAccount { .. }
            | C::UploadAsset(_)
            | C::PollAssetOperations { .. }
            | C::PollAssetModeration { .. }
            | C::GrantAssetPermissions { .. }
            | C::FetchUniverseTargets { .. }
            | C::FetchPublishGroups { .. }
            | C::FetchCreations { .. }
            | C::FetchAssetThumbnails { .. } => false,
        }
    }

    /// A throwaway session. `create_password_session` derives its key with
    /// Argon2id and never touches the OS credential store, so it works headless
    /// and needs no cleanup. Built once and cloned: the KDF is the slow part.
    fn session() -> crypto::StoreSession {
        crypto::create_password_session("test-only").expect("password session")
    }

    fn path() -> PathBuf {
        PathBuf::from("accounts.rmenc")
    }

    /// One command of every shape that matters to the classification, plus the
    /// two that must be serial.
    fn sample_commands() -> Vec<BackendCommand> {
        let s = session();
        vec![
            BackendCommand::SaveStore {
                store: AccountStore::default(),
                path: path(),
                session: s.clone(),
            },
            BackendCommand::RekeyStore {
                store: AccountStore::default(),
                path: path(),
                session: s.clone(),
                new_password: None,
                upgrade_legacy: false,
            },
            // The other three corners of `RekeyStore`: still a store write, so
            // still serial, whichever direction the re-key goes.
            BackendCommand::RekeyStore {
                store: AccountStore::default(),
                path: path(),
                session: s.clone(),
                new_password: Some("hunter2".to_string()),
                upgrade_legacy: true,
            },
            BackendCommand::SweepInstances,
            BackendCommand::KillAll,
            BackendCommand::ArrangeWindows,
            BackendCommand::RemoveAccount { user_id: 7 },
            BackendCommand::UnlockWithDevice { path: path() },
            BackendCommand::UnlockWithPassword {
                path: path(),
                password: "hunter2".to_string(),
            },
            BackendCommand::AddAccount {
                cookie: "COOKIE".to_string(),
                session: s.clone(),
                use_credential_manager: false,
            },
            BackendCommand::LaunchGame {
                user_id: 7,
                encrypted_cookie: None,
                session: s.clone(),
                use_credential_manager: false,
                place_id: 606,
                job_id: None,
                link_code: None,
                access_code: None,
                multi_instance: false,
                kill_background: false,
                privacy_mode: false,
            },
            BackendCommand::CheckForUpdates {
                current_version: "1.8.1".to_string(),
            },
            BackendCommand::FetchAssetThumbnails {
                asset_ids: Vec::new(),
            },
        ]
    }

    #[test]
    fn only_store_writes_are_classified_as_serial() {
        for cmd in sample_commands() {
            assert_eq!(
                cmd.is_serial_persistence(),
                writes_the_store(&cmd),
                "a command was classified against what it does to the store file"
            );
        }
    }

    /// Stated flat, so the failure message names the command rather than an
    /// index into a loop.
    #[test]
    fn save_and_rekey_are_serial() {
        let s = session();
        assert!(BackendCommand::SaveStore {
            store: AccountStore::default(),
            path: path(),
            session: s.clone(),
        }
        .is_serial_persistence());
        assert!(BackendCommand::RekeyStore {
            store: AccountStore::default(),
            path: path(),
            session: s,
            new_password: None,
            upgrade_legacy: false,
        }
        .is_serial_persistence());
    }

    /// The sweep runs on a timer and would otherwise stall the whole command
    /// loop behind a process-table walk every few seconds. It must stay
    /// spawnable.
    #[test]
    fn the_instance_sweep_is_not_serial() {
        assert!(!BackendCommand::SweepInstances.is_serial_persistence());
    }

    // -----------------------------------------------------------------------
    // Registry glue
    // -----------------------------------------------------------------------

    fn fresh_registry() -> Registry {
        Arc::new(Mutex::new(InstanceState::default()))
    }

    /// Poison the registry the way production would: a holder of the lock
    /// panics. The panic is expected and its backtrace on stderr is noise, not
    /// a failure.
    fn poisoned_registry() -> Registry {
        let registry = fresh_registry();
        let handle = Arc::clone(&registry);
        let _ = std::thread::spawn(move || {
            let _guard = handle.lock().expect("first lock is clean");
            panic!("poisoning the registry on purpose");
        })
        .join();
        assert!(registry.lock().is_err(), "the registry should be poisoned");
        registry
    }

    /// Reads the real process table, which is fine: the assertion is about the
    /// pending queue, and that grows whether or not Roblox is running.
    #[test]
    fn note_launch_queues_one_pending_attribution_per_call() {
        let registry = fresh_registry();

        note_launch(&registry, 7, 606);
        assert_eq!(registry.lock().unwrap().registry.pending_count(), 1);

        note_launch(&registry, 8, 606);
        assert_eq!(registry.lock().unwrap().registry.pending_count(), 2);
    }

    /// Two launches must never share a token, or the sweep would attribute one
    /// account's client to the other with full confidence. Back-to-back calls
    /// are the case that matters: a bulk launch issues them inside the same
    /// millisecond.
    #[test]
    fn every_launch_gets_a_token_of_its_own() {
        let registry = fresh_registry();
        let tokens: Vec<i64> = (0..50).map(|_| note_launch(&registry, 7, 606)).collect();

        let mut sorted = tokens.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), tokens.len(), "duplicate launchtime: {tokens:?}");
        // And they only ever go up, so an older launch cannot collide with a
        // newer one after a clock correction either.
        assert!(tokens.windows(2).all(|w| w[0] < w[1]), "{tokens:?}");
    }

    /// A poisoned registry must not take the launch down with it: attribution
    /// is a tooltip, and refusing to launch over a lost tooltip would be worse
    /// than the lost tooltip.
    #[test]
    fn note_launch_gives_up_quietly_on_a_poisoned_registry() {
        let registry = poisoned_registry();

        // The token is still minted, so the launch URI is still stamped and the
        // client simply reads as unattributed later.
        assert!(note_launch(&registry, 7, 606) > 0);

        let inner = registry.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            inner.registry.pending_count(),
            0,
            "nothing should have been queued through a poisoned lock"
        );
    }

    #[test]
    fn sweeping_a_registry_that_launched_nothing_attributes_nothing() {
        let registry = fresh_registry();

        let BackendEvent::InstancesUpdated {
            instances,
            running_count: _,
        } = sweep_instances(&registry)
        else {
            panic!("the sweep must report InstancesUpdated");
        };

        // Whatever this machine happens to be running, none of it was launched
        // through this registry, so none of it can be attributed. The count
        // itself is a live sample of the process table and is pinned in
        // `sweeping_a_poisoned_registry_still_reports_a_count` instead.
        assert!(instances.is_empty(), "{instances:?}");
    }

    /// The poisoned path still has to report the running count, because the
    /// UI's instance badge is driven by it. Only the attributions are lost.
    #[test]
    fn sweeping_a_poisoned_registry_still_reports_a_count() {
        let registry = poisoned_registry();

        // The count is read from the live process table, so it is pinned to the
        // window this test brackets around the call rather than to a fixed
        // number. A machine with Roblox open must still see it reported.
        let before = process::roblox_pids().len();
        let BackendEvent::InstancesUpdated {
            instances,
            running_count,
        } = sweep_instances(&registry)
        else {
            panic!("the sweep must report InstancesUpdated");
        };
        let after = process::roblox_pids().len();

        assert!(instances.is_empty(), "{instances:?}");
        assert!(
            running_count >= before.min(after) && running_count <= before.max(after),
            "{running_count} outside the sampled range {before}..={after}"
        );
    }
}
