//! Bridge between the synchronous `egui` update loop and the `tokio` async runtime.
//!
//! All heavyweight operations (network, file I/O, process spawning) are dispatched
//! as [`BackendCommand`] messages to a background `tokio` runtime. Results come
//! back as [`BackendEvent`] through an mpsc channel polled each frame.

use eframe::egui;
use ram_core::assets::{AssetKind, Creator, OperationOutcome};
use ram_core::auth::RobloxClient;
use ram_core::models::{Account, AccountStore, Presence};
use ram_core::{api, assets, assets_api, crypto, process, CoreError};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};

// ---------------------------------------------------------------------------
// Commands (UI → Backend)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum BackendCommand {
    /// Validate a cookie and add the account.
    AddAccount {
        cookie: String,
        password: String,
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
        password: String,
        use_credential_manager: bool,
    },
    /// Remove an account by user ID.
    RemoveAccount { user_id: u64 },
    /// Refresh avatar URLs for all accounts.
    RefreshAvatars { user_ids: Vec<u64>, cookie: String },
    /// Refresh presence for all accounts.
    RefreshPresence { user_ids: Vec<u64>, cookie: String },
    /// Launch the game for an account.
    LaunchGame {
        cookie: String,
        place_id: u64,
        job_id: Option<String>,
        link_code: Option<String>,
        access_code: Option<String>,
        multi_instance: bool,
        kill_background: bool,
        privacy_mode: bool,
    },
    /// Launch the game, decrypting the cookie on the backend side.
    LaunchGameEncrypted {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
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
        password: String,
    },
    /// Load the account store from disk.
    LoadStore { path: PathBuf, password: String },
    /// Kill all Roblox instances.
    KillAll,
    /// Refresh avatars + presence for all accounts, decrypting a cookie on this side.
    RefreshAll {
        user_ids: Vec<u64>,
        /// The first account's encrypted cookie (or None if credential manager).
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
    },
    /// Lightweight presence-only refresh for a subset of visible accounts.
    RefreshPresenceOnly {
        user_ids: Vec<u64>,
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
    },
    /// Launch multiple accounts into the same game sequentially.
    BulkLaunchEncrypted {
        /// (user_id, encrypted_cookie) pairs for each account.
        accounts: Vec<(u64, Option<String>)>,
        password: String,
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
        password: String,
        use_credential_manager: bool,
    },
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
        password: String,
        use_credential_manager: bool,
    },
    /// Decrypt the cookie and open a webview pre-logged-in as this account.
    BrowseAsAccount {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
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
        password: String,
        use_credential_manager: bool,
        /// `(row_id, operation)` pairs. The caller caps the length.
        operations: Vec<(String, String)>,
    },
    /// Grant one universe `Use` access to a batch of assets.
    GrantAssetPermissions {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
        universe_id: u64,
        asset_ids: Vec<u64>,
        /// Row IDs to stamp on success, so the library can show what happened.
        row_ids: Vec<String>,
    },
    /// Populate the universe picker, and turn a pasted place ID into a
    /// universe ID. `place_id` is `None` when only the list is wanted.
    FetchUniverseTargets {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
        place_id: Option<u64>,
    },
    /// Groups the account could publish under. Populates the creator picker
    /// and the inventory tree.
    FetchPublishGroups {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
    },
    /// One page of a creator's inventory for the browse pane.
    FetchCreations {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
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
    pub password: String,
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
        matches!(self, BackendCommand::SaveStore { .. })
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
    /// Store loaded from disk.
    StoreLoaded(AccountStore),
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
    /// A `PollAssetOperations` batch finished. Carries nothing: every result
    /// already streamed out as its own event.
    AssetPollBatchDone,
    /// A universe was granted `Use` on these assets.
    AssetPermissionsGranted {
        universe_id: u64,
        row_ids: Vec<String>,
        granted: usize,
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

    while let Some(cmd) = rx.recv().await {
        let client = client.clone();
        let tx = tx.clone();

        // Account-store writes MUST run serially and in the order they were
        // enqueued. Spawning them (as we do for everything else) let two saves
        // overlap on the same file — interleaving into a torn AES-GCM blob that
        // no longer decrypts — or finish out of order, so an older store
        // clobbered a newer one. Both were the v1.4.4 lockout/corruption bug.
        // Handle them inline so the next command isn't even dequeued until the
        // write has fully landed. The write itself is fast (encrypt + atomic
        // rename), so blocking the loop here is fine.
        if cmd.is_serial_persistence() {
            match handle_command(cmd, &client, &tx).await {
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
            match handle_command(cmd, &client, &tx).await {
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
) -> Result<BackendEvent, CoreError> {
    match cmd {
        BackendCommand::AddAccount {
            cookie,
            password,
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
                Some(crypto::encrypt_cookie(&cookie, &password)?)
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
            password,
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
                Some(crypto::encrypt_cookie(&cookie, &password)?)
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
        BackendCommand::RefreshAvatars { user_ids, cookie: _ } => {
            // Avatars are cosmetic: log a failure rather than surfacing an
            // error toast for something the user can't act on.
            let avatars = api::fetch_avatars(client, &user_ids)
                .await
                .unwrap_or_else(|e| {
                    info!("Avatar refresh failed (non-fatal): {e}");
                    Vec::new()
                });
            Ok(BackendEvent::AvatarsUpdated(avatars))
        }
        BackendCommand::RefreshPresence { user_ids, cookie } => {
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::LaunchGameEncrypted {
            user_id,
            encrypted_cookie,
            password,
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
                crypto::decrypt_cookie(&enc, &password)?
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
            process::launch_game(&ticket, place_id, job_id.as_deref(), link_code.as_deref(), access_code.as_deref())?;
            Ok(BackendEvent::GameLaunched)
        }
        BackendCommand::LaunchGame {
            cookie,
            place_id,
            job_id,
            link_code,
            access_code,
            multi_instance,
            kill_background,
            privacy_mode,
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
            let ticket = client.generate_auth_ticket(&cookie).await?;
            process::launch_game(&ticket, place_id, job_id.as_deref(), link_code.as_deref(), access_code.as_deref())?;
            Ok(BackendEvent::GameLaunched)
        }
        BackendCommand::SaveStore {
            store,
            path,
            password,
        } => {
            crypto::save_encrypted(&path, &store, &password)?;
            Ok(BackendEvent::StoreSaved)
        }
        BackendCommand::LoadStore { path, password } => {
            let store = crypto::load_encrypted(&path, &password)?;
            Ok(BackendEvent::StoreLoaded(store))
        }
        BackendCommand::KillAll => {
            let count = process::kill_all_roblox()?;
            Ok(BackendEvent::Killed(count))
        }
        BackendCommand::RefreshAll {
            user_ids,
            first_user_id,
            encrypted_cookie,
            password,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for refresh".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
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
            password,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for refresh".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
            };
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::BulkLaunchEncrypted {
            accounts,
            password,
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
                        Some(enc) => crypto::decrypt_cookie(enc, &password)?,
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
                        Some(enc) => crypto::decrypt_cookie(enc, &password),
                        None => Err(CoreError::Crypto(
                            "no encrypted cookie stored".into(),
                        )),
                    }
                };
                match cookie_result {
                    Ok(cookie) => {
                        match client.generate_auth_ticket(&cookie).await {
                            Ok(ticket) => {
                                if let Err(e) = process::launch_game(
                                    &ticket,
                                    place_id,
                                    resolved_job_id.as_deref(),
                                    link_code.as_deref(),
                                    access_code.as_deref(),
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
            password,
            use_credential_manager,
        } => {
            for (user_id, encrypted_cookie) in &accounts {
                let cookie_result = if use_credential_manager {
                    crypto::credential_load(*user_id)
                } else {
                    match encrypted_cookie {
                        Some(enc) => crypto::decrypt_cookie(enc, &password),
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
            password,
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
                crypto::decrypt_cookie(&enc, &password)?
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
            password,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for share link resolution".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
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
            password,
            use_credential_manager,
            operations,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &password, use_credential_manager)?;
            poll_asset_operations(client, &cookie, &operations, tx).await;
            Ok(BackendEvent::AssetPollBatchDone)
        }
        BackendCommand::GrantAssetPermissions {
            user_id,
            encrypted_cookie,
            password,
            use_credential_manager,
            universe_id,
            asset_ids,
            row_ids,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &password, use_credential_manager)?;
            match assets_api::grant_use_permission(client, &cookie, universe_id, &asset_ids).await {
                Ok(granted) => Ok(BackendEvent::AssetPermissionsGranted {
                    universe_id,
                    row_ids,
                    granted: granted.len(),
                }),
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
            password,
            use_credential_manager,
            place_id,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &password, use_credential_manager)?;
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
            password,
            use_credential_manager,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &password, use_credential_manager)?;
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
            password,
            use_credential_manager,
            creator,
            kind,
            cursor,
        } => {
            let cookie = decrypt_for(user_id, encrypted_cookie, &password, use_credential_manager)?;
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

/// Gap between polls inside one batch. Keeps a large backlog to a few requests
/// per second per account instead of a burst.
const POLL_SPACING_MS: u64 = 250;

/// The cookie-decrypt idiom used by every account-scoped command.
fn decrypt_for(
    user_id: u64,
    encrypted_cookie: Option<String>,
    password: &str,
    use_credential_manager: bool,
) -> Result<String, CoreError> {
    if use_credential_manager {
        return crypto::credential_load(user_id);
    }
    let enc = encrypted_cookie
        .ok_or_else(|| CoreError::Crypto("no encrypted cookie stored for this account".into()))?;
    crypto::decrypt_cookie(&enc, password)
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
                // A dead cookie or a local read failure will not fix itself.
                CoreError::RateLimited | CoreError::Http(_) => true,
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
        password,
        use_credential_manager,
        creator,
        kind,
        display_name,
        description,
        file_path,
    } = job;

    let cookie = decrypt_for(user_id, encrypted_cookie, &password, use_credential_manager)?;

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
        return Ok(BackendEvent::AssetOperationResolved {
            row_id,
            outcome: created.outcome,
        });
    };

    let _ = tx.send(BackendEvent::AssetOperationCreated {
        row_id: row_id.clone(),
        operation: operation.clone(),
        started_at: chrono::Utc::now(),
    });

    if !matches!(created.outcome, OperationOutcome::StillPending) {
        return Ok(BackendEvent::AssetOperationResolved {
            row_id,
            outcome: created.outcome,
        });
    }

    for wait in UPLOAD_POLL_BURST {
        tokio::time::sleep(Duration::from_secs(*wait)).await;
        match assets_api::poll_operation(client, &cookie, &operation).await {
            Ok(OperationOutcome::StillPending) => continue,
            Ok(outcome) => return Ok(BackendEvent::AssetOperationResolved { row_id, outcome }),
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
