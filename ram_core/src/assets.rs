//! Asset Manager model: what an asset is, what state an upload is in, and the
//! on-disk index that survives restarts.
//!
//! Everything here is pure. No network, and the only I/O is [`AssetIndex::load`]
//! and [`AssetIndex::save`]. The HTTP calls that consume these types live in
//! `assets_api`. Keeping the split means the fiddly parts (extension tables,
//! operation-response parsing, moderation classification) are unit-testable
//! without a cookie or a socket.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::error::CoreError;
use crate::storage;

/// Schema version written into new index files.
///
/// 2 added the `inReview` state and the per-row retry counters. Reading a
/// version 1 file is unchanged (every new field defaults), but a version 1
/// build cannot read a file containing `inReview` rows, so the bump is what
/// makes that downgrade fail loudly against the backup instead of quietly.
pub const CURRENT_SCHEMA: u32 = 2;

/// How long Roblox keeps an operation id resolvable. Past this an upload has no
/// discoverable verdict, though the asset itself may still have been published.
pub const OPERATION_TTL_HOURS: i64 = 24;

/// How long to keep asking for a moderation verdict before giving up on the row.
///
/// Deliberately much longer than [`OPERATION_TTL_HOURS`]: an operation is a
/// short-lived server-side job, but audio review routinely runs into hours and
/// occasionally days. Expiring a row at 24h would report "timed out" on assets
/// that were about to be approved.
pub const REVIEW_TTL_HOURS: i64 = 96;

/// How many times a retryable upload failure is re-sent on its own before the
/// row is left for the user. Four covers a rate-limit window or a Roblox blip
/// without turning a genuinely bad file into an endless loop.
pub const MAX_UPLOAD_ATTEMPTS: u32 = 4;

/// Roblox caps a `displayName` well below this, but the exact limit is not
/// documented. 50 matches what Studio's import dialog accepts.
pub const MAX_DISPLAY_NAME_CHARS: usize = 50;

// ---------------------------------------------------------------------------
// Asset kinds
// ---------------------------------------------------------------------------

/// The `assetType` values this app can upload, plus a catch-all so an index
/// written by a future version still loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AssetKind {
    Decal,
    Audio,
    Model,
    Animation,
    Video,
    /// An `assetType` this build does not know. Never produced by
    /// [`classify_path`]; only by reading an index from a newer build.
    #[default]
    Other,
}

impl AssetKind {
    /// The spelling the Assets API expects in `assetType`.
    pub fn as_api_str(self) -> &'static str {
        match self {
            AssetKind::Decal => "Decal",
            AssetKind::Audio => "Audio",
            AssetKind::Model => "Model",
            AssetKind::Animation => "Animation",
            AssetKind::Video => "Video",
            AssetKind::Other => "Other",
        }
    }

    pub fn from_api_str(s: &str) -> Option<AssetKind> {
        match s {
            "Decal" => Some(AssetKind::Decal),
            "Audio" => Some(AssetKind::Audio),
            "Model" => Some(AssetKind::Model),
            "Animation" => Some(AssetKind::Animation),
            "Video" => Some(AssetKind::Video),
            "Other" => Some(AssetKind::Other),
            _ => None,
        }
    }

    /// Only Models can have their content replaced in place. Audio, Decal and
    /// Video are immutable: a "new version" is a new upload and a new id.
    pub fn is_updatable(self) -> bool {
        matches!(self, AssetKind::Model)
    }

    /// Every kind this app offers as a manual override in the import queue.
    pub fn selectable() -> &'static [AssetKind] {
        &[
            AssetKind::Decal,
            AssetKind::Audio,
            AssetKind::Model,
            AssetKind::Animation,
            AssetKind::Video,
        ]
    }
}

// Hand-written rather than derived so an unrecognised `assetType` from a newer
// build degrades to `Other` instead of failing the whole index load.
impl Serialize for AssetKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_api_str())
    }
}

impl<'de> Deserialize<'de> for AssetKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(AssetKind::from_api_str(&raw).unwrap_or(AssetKind::Other))
    }
}

/// Extension to (kind, MIME, advisory size cap).
///
/// A `None` cap means Roblox documents a limit in some other unit (audio is
/// capped by duration and a per-account quota, video by duration and
/// resolution), so there is nothing useful to pre-check locally.
///
/// `.rbxm` / `.rbxmx` map to `Model`; they are also the Animation extensions,
/// and the import queue lets the user override the kind per row. Model is the
/// safer default because an Animation must have been authored in Studio.
///
/// `.fbx` is the only Model input Roblox actually documents. The rest are
/// best-effort: a wrong entry surfaces as a 400 naming the extension.
const EXT_TABLE: &[(&str, AssetKind, &str, Option<u64>)] = &[
    ("png", AssetKind::Decal, "image/png", Some(20 * MB)),
    ("jpg", AssetKind::Decal, "image/jpeg", Some(20 * MB)),
    ("jpeg", AssetKind::Decal, "image/jpeg", Some(20 * MB)),
    ("bmp", AssetKind::Decal, "image/bmp", Some(20 * MB)),
    ("tga", AssetKind::Decal, "image/tga", Some(20 * MB)),
    ("mp3", AssetKind::Audio, "audio/mpeg", None),
    ("ogg", AssetKind::Audio, "audio/ogg", None),
    ("wav", AssetKind::Audio, "audio/wav", None),
    ("flac", AssetKind::Audio, "audio/flac", None),
    ("fbx", AssetKind::Model, "model/fbx", Some(20 * MB)),
    ("gltf", AssetKind::Model, "model/gltf+json", Some(20 * MB)),
    ("glb", AssetKind::Model, "model/gltf-binary", Some(20 * MB)),
    ("rbxm", AssetKind::Model, "model/x-rbxm", Some(20 * MB)),
    ("rbxmx", AssetKind::Model, "model/x-rbxmx", Some(20 * MB)),
    ("mp4", AssetKind::Video, "video/mp4", None),
    ("mov", AssetKind::Video, "video/mov", None),
];

const MB: u64 = 1024 * 1024;

/// Infer the asset kind and MIME type from a path's extension.
/// `None` for anything this app cannot upload, including `.mesh`: meshes are a
/// by-product of a Model import and have no upload endpoint at all.
pub fn classify_path(path: &Path) -> Option<(AssetKind, &'static str)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    EXT_TABLE
        .iter()
        .find(|(e, ..)| *e == ext)
        .map(|(_, kind, mime, _)| (*kind, *mime))
}

/// The advisory byte cap for a path's extension, if Roblox documents one.
pub fn max_bytes_for(path: &Path) -> Option<u64> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    EXT_TABLE
        .iter()
        .find(|(e, ..)| *e == ext)
        .and_then(|(.., cap)| *cap)
}

/// Pre-flight a file before it is queued. `Err` carries a message meant to be
/// shown verbatim on the row.
pub fn validate_file(path: &Path, size: u64) -> Result<(AssetKind, &'static str), String> {
    let Some((kind, mime)) = classify_path(path) else {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("(none)");
        return Err(format!("{ext} files cannot be uploaded to Roblox"));
    };
    if size == 0 {
        return Err("File is empty".to_string());
    }
    if let Some(cap) = max_bytes_for(path) {
        if size > cap {
            return Err(format!(
                "File is {:.1} MB. The limit is {} MB.",
                size as f64 / MB as f64,
                cap / MB
            ));
        }
    }
    Ok((kind, mime))
}

// ---------------------------------------------------------------------------
// Creator
// ---------------------------------------------------------------------------

/// Who owns the uploaded asset. Exactly one of `userId` / `groupId` goes into
/// `creationContext.creator`, never both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum Creator {
    User(u64),
    Group(u64),
}

impl Creator {
    pub fn id(self) -> u64 {
        match self {
            Creator::User(id) | Creator::Group(id) => id,
        }
    }

    pub fn is_group(self) -> bool {
        matches!(self, Creator::Group(_))
    }
}

// ---------------------------------------------------------------------------
// Upload state
// ---------------------------------------------------------------------------

/// Where a queue row is in its life. Persisted, so a restart resumes exactly
/// where it left off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AssetState {
    /// Staged locally and ready to upload.
    Queued,
    /// Rejected before any network call (bad extension, too large, unreadable).
    Invalid { reason: String },
    /// The same bytes were already uploaded under the same creator.
    Duplicate { asset_id: u64 },
    /// A command is in flight but no operation id has come back yet.
    Uploading,
    /// Roblox accepted the bytes and the upload operation is still running.
    ///
    /// This is *not* moderation. The operation finishing only means the file was
    /// ingested and an asset id minted; whether the asset may be used is a
    /// separate verdict, tracked by [`AssetState::InReview`].
    Pending {
        operation: String,
        since: DateTime<Utc>,
    },
    /// The upload operation finished and an asset id exists, but Roblox has not
    /// published a moderation verdict yet.
    ///
    /// Split out from `Approved` because the operations endpoint reports
    /// ingestion, not moderation: audio in particular comes back `done: true`
    /// with an asset id and then sits in review for minutes to days. Treating
    /// that as approved handed out ids for assets that later turned out to be
    /// blocked. Verdicts come from [`ModerationStatus`].
    InReview {
        asset_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision_id: Option<u64>,
        since: DateTime<Utc>,
    },
    Approved {
        asset_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision_id: Option<u64>,
    },
    /// Moderation said no. Terminal: re-uploading identical bytes gets the same
    /// verdict, so the row offers no retry.
    Rejected { reason: String },
    Failed { message: String, retryable: bool },
    /// Past [`OPERATION_TTL_HOURS`] with no verdict. Not a failure: the asset
    /// may well have been published, we just cannot ask about it any more.
    Expired { operation: String },
    Cancelled,
}

impl AssetState {
    /// Still moving. The upload pump and the poll timers all key off this.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AssetState::Uploading | AssetState::Pending { .. } | AssetState::InReview { .. }
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AssetState::Approved { .. }
                | AssetState::Rejected { .. }
                | AssetState::Expired { .. }
                | AssetState::Cancelled
        )
    }

    /// The asset id, once one exists.
    ///
    /// `InReview` counts: the id is real and permanent from the moment the
    /// upload operation completes. Withholding it here would let the dedupe
    /// check miss an in-flight upload and mint a second copy, which for audio
    /// also burns a second slice of the per-account quota.
    pub fn asset_id(&self) -> Option<u64> {
        match self {
            AssetState::Approved { asset_id, .. }
            | AssetState::InReview { asset_id, .. }
            | AssetState::Duplicate { asset_id } => Some(*asset_id),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Moderation
// ---------------------------------------------------------------------------

/// Where an asset stands with moderation, independent of its upload operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationStatus {
    /// Review has not finished. Keep asking.
    InReview,
    /// Cleared, or never needed clearing.
    Approved,
    /// Blocked. Terminal.
    Rejected,
}

/// Read one asset's verdict out of a `develop.roblox.com/v1/assets` entry.
///
/// The rules mirror the ones proven out in newnew13bot_v2, which has polled
/// this endpoint through thousands of uploads:
///
/// - `reviewStatus` is `"Finished"` once a human or the automated pass has
///   ruled, and `"DoesNotRequire"` for asset types that skip review entirely.
///   Anything else (`"Pending"`, `"InReview"`, ...) means keep waiting.
/// - `isModerated` is the verdict itself, and is only meaningful once review
///   has finished. `DoesNotRequire` overrides it: an asset that never needed
///   review is never blocked by it.
///
/// Returns `None` when the entry carries neither field, which is treated by
/// callers as "no answer this time" rather than as a verdict.
pub fn parse_moderation_entry(entry: &serde_json::Value) -> Option<ModerationStatus> {
    let review_status = entry.get("reviewStatus").and_then(|v| v.as_str());
    let is_moderated = entry.get("isModerated").and_then(|v| v.as_bool());
    if review_status.is_none() && is_moderated.is_none() {
        return None;
    }

    // No review required: approved regardless of what `isModerated` says.
    if review_status == Some("DoesNotRequire") {
        return Some(ModerationStatus::Approved);
    }
    // A missing `reviewStatus` alongside a present `isModerated` is treated as
    // unfinished, so the row waits rather than being called approved early.
    if review_status != Some("Finished") {
        return Some(ModerationStatus::InReview);
    }
    Some(match is_moderated {
        Some(true) => ModerationStatus::Rejected,
        _ => ModerationStatus::Approved,
    })
}

/// Pair every `assetId` in a moderation response with its verdict.
///
/// Keyed on the id in the payload, never on position: this endpoint drops ids
/// it cannot resolve, and pairing by index would then attach one asset's
/// verdict to another. The same mistake caused v1.4.6's wrong-avatar bug.
pub fn parse_moderation_response(body: &serde_json::Value) -> Vec<(u64, ModerationStatus)> {
    let Some(entries) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let asset_id = entry
                .get("id")
                .or_else(|| entry.get("assetId"))
                .and_then(json_u64)?;
            Some((asset_id, parse_moderation_entry(entry)?))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Records and index
// ---------------------------------------------------------------------------

/// One row of the import queue, and once approved, one entry in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRecord {
    /// Stable key correlating a UI row with backend events, and the egui `Id`
    /// salt for the row. A `String`, not a `Uuid`: the workspace pins `uuid`
    /// with only the `v4` feature, so `Uuid` has no serde impls.
    pub row_id: String,
    #[serde(default)]
    pub file_path: PathBuf,
    /// Lowercase hex SHA-256 of the file. The dedupe key, with `creator`.
    #[serde(default)]
    pub file_sha256: String,
    #[serde(default)]
    pub file_bytes: u64,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: AssetKind,
    pub creator: Creator,
    /// Account whose cookie signs the upload. Differs from `creator` for group
    /// uploads, which is why both are stored.
    pub uploaded_by: u64,
    pub state: AssetState,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    /// Universes granted `Use` on this asset. A local mirror for display only;
    /// Roblox stays authoritative and this is never treated as proof.
    #[serde(default)]
    pub granted_universes: Vec<u64>,
    /// Set from the import queue's "Grant access to" selector. Applied once the
    /// asset clears moderation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_grant_universe: Option<u64>,
    /// How many times this row has been sent to Roblox. Drives the automatic
    /// re-send of retryable failures and its backoff, and is persisted so a
    /// restart cannot reset a row into another full round of attempts.
    #[serde(default)]
    pub attempts: u32,
    /// Earliest time this row may be sent again. Set when a retryable failure
    /// is scheduled for another attempt; `None` means "eligible now".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<DateTime<Utc>>,
    /// Fields written by a newer build, preserved verbatim across a round trip
    /// so a downgrade does not silently delete them.
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A file that has been read and hashed but not yet queued. Bundled rather
/// than passed as loose arguments so [`AssetRecord::staged`] stays under
/// clippy's argument limit, which is a hard error in CI.
pub struct StagedFile {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub kind: AssetKind,
}

impl AssetRecord {
    /// A record for a freshly staged file. `row_id` is caller-supplied so the
    /// UI can generate it without pulling `uuid` into `ram_ui`'s hot path.
    pub fn staged(
        row_id: String,
        file: StagedFile,
        creator: Creator,
        uploaded_by: u64,
        now: DateTime<Utc>,
    ) -> Self {
        let StagedFile {
            path: file_path,
            sha256: file_sha256,
            bytes: file_bytes,
            kind,
        } = file;
        let display_name = sanitize_display_name_from_path(&file_path);
        Self {
            row_id,
            file_path,
            file_sha256,
            file_bytes,
            display_name,
            description: String::new(),
            kind,
            creator,
            uploaded_by,
            state: AssetState::Queued,
            created_at: now,
            updated_at: None,
            granted_universes: Vec::new(),
            auto_grant_universe: None,
            attempts: 0,
            retry_at: None,
            extra: serde_json::Map::new(),
        }
    }
}

/// The persisted asset index, `%APPDATA%\RM\assets.json`.
///
/// Deliberately unencrypted: it holds no secrets (asset ids are public, file
/// paths are local), it has to survive a forgotten master password, and users
/// can hand-edit it the way they already can with `presets/*.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    #[serde(default = "default_schema")]
    pub version: u32,
    #[serde(default)]
    pub records: Vec<AssetRecord>,
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_schema() -> u32 {
    CURRENT_SCHEMA
}

impl Default for AssetIndex {
    fn default() -> Self {
        Self {
            version: CURRENT_SCHEMA,
            records: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

/// What [`AssetIndex::load`] found, so the caller can react without guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexLoad {
    /// No file yet, or a good file.
    Ok,
    /// The primary failed to parse and `.bak` was used instead.
    RecoveredFromBackup,
    /// Neither parsed. The index is empty and the damaged file was left alone.
    Corrupt,
    /// Written by a newer build. Loaded, but must not be saved over: `extra`
    /// cannot round-trip a renamed or restructured field.
    NewerSchema,
}

impl IndexLoad {
    /// Saving over the file would risk destroying data we cannot represent.
    pub fn is_read_only(self) -> bool {
        matches!(self, IndexLoad::Corrupt | IndexLoad::NewerSchema)
    }
}

/// Where the index lives, given the app data directory.
pub fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("assets.json")
}

impl AssetIndex {
    /// Read the index, falling back to the backup that [`storage::atomic_write`]
    /// maintains. Never fails: a broken index must not make the tab unopenable,
    /// and a damaged file is never clobbered until the user does something that
    /// implies consent.
    pub fn load(path: &Path) -> (AssetIndex, IndexLoad) {
        let primary = std::fs::read_to_string(path).ok();
        if let Some(text) = primary.as_deref() {
            match serde_json::from_str::<AssetIndex>(text) {
                Ok(index) => {
                    let status = if index.version > CURRENT_SCHEMA {
                        tracing::warn!(
                            "asset index schema {} is newer than {CURRENT_SCHEMA}; read-only",
                            index.version
                        );
                        IndexLoad::NewerSchema
                    } else {
                        IndexLoad::Ok
                    };
                    return (index, status);
                }
                Err(e) => tracing::error!("asset index at {} failed to parse: {e}", path.display()),
            }
        } else if !path.exists() {
            return (AssetIndex::default(), IndexLoad::Ok);
        }

        let backup = storage::backup_path(path);
        if let Ok(text) = std::fs::read_to_string(&backup) {
            if let Ok(index) = serde_json::from_str::<AssetIndex>(&text) {
                tracing::warn!("recovered asset index from {}", backup.display());
                return (index, IndexLoad::RecoveredFromBackup);
            }
        }

        tracing::error!("asset index unreadable; starting empty and leaving the file in place");
        (AssetIndex::default(), IndexLoad::Corrupt)
    }

    /// Persist through the same temp + fsync + rename path as every other piece
    /// of state in this app.
    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let json = serde_json::to_string_pretty(self)?;
        storage::atomic_write(path, json.as_bytes())
    }

    /// A previously approved upload of the same bytes under the same creator.
    ///
    /// Keyed on creator as well as hash: the same PNG uploaded to your account
    /// and to a group are genuinely two different assets and both should exist.
    pub fn find_uploaded(&self, sha256: &str, creator: Creator) -> Option<&AssetRecord> {
        // An empty hash means "not hashed yet", not "hash of nothing". Without
        // this guard every un-hashed row would match every other un-hashed row.
        if sha256.is_empty() {
            return None;
        }
        // `InReview` counts as uploaded. The bytes are already on Roblox and
        // the id is already minted, so re-sending them would create a second
        // asset, not retry the first.
        self.records.iter().find(|r| {
            r.file_sha256 == sha256
                && r.creator == creator
                && matches!(
                    r.state,
                    AssetState::Approved { .. } | AssetState::InReview { .. }
                )
        })
    }

    pub fn get(&self, row_id: &str) -> Option<&AssetRecord> {
        self.records.iter().find(|r| r.row_id == row_id)
    }

    // Linear scans. Fine to a few thousand records (a scan is sub-millisecond);
    // past that, add a HashMap<String, usize> side index rather than discovering
    // the cost as a stutter.
    pub fn get_mut(&mut self, row_id: &str) -> Option<&mut AssetRecord> {
        self.records.iter_mut().find(|r| r.row_id == row_id)
    }

    pub fn remove(&mut self, row_id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.row_id != row_id);
        self.records.len() < before
    }

    /// Rows whose upload operation has not finished.
    pub fn pending(&self) -> impl Iterator<Item = &AssetRecord> {
        self.records
            .iter()
            .filter(|r| matches!(r.state, AssetState::Pending { .. }))
    }

    /// Rows that have an asset id but no moderation verdict.
    pub fn in_review(&self) -> impl Iterator<Item = &AssetRecord> {
        self.records
            .iter()
            .filter(|r| matches!(r.state, AssetState::InReview { .. }))
    }

    pub fn has_active(&self) -> bool {
        self.records.iter().any(|r| r.state.is_active())
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        // Writing to a String cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Turn a file path into a default `displayName`: stem only, whitespace
/// collapsed, clamped to [`MAX_DISPLAY_NAME_CHARS`].
pub fn sanitize_display_name_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    sanitize_display_name(stem)
}

/// Clean a user-entered or path-derived display name.
pub fn sanitize_display_name(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "Untitled".to_string();
    }
    // Truncate on a character boundary. Byte slicing here panics on any
    // multi-byte name, which is not hypothetical: Roblox usernames and file
    // names are routinely non-ASCII.
    match collapsed.char_indices().nth(MAX_DISPLAY_NAME_CHARS) {
        Some((byte_idx, _)) => collapsed[..byte_idx].to_string(),
        None => collapsed,
    }
}

/// Body for `POST /assets`. Exactly one of `userId` / `groupId`, as strings:
/// Roblox rejects the numeric form.
pub fn build_create_request_json(
    kind: AssetKind,
    display_name: &str,
    description: &str,
    creator: Creator,
) -> serde_json::Value {
    let creator_json = match creator {
        Creator::User(id) => serde_json::json!({ "userId": id.to_string() }),
        Creator::Group(id) => serde_json::json!({ "groupId": id.to_string() }),
    };
    serde_json::json!({
        "assetType": kind.as_api_str(),
        "displayName": display_name,
        "description": description,
        "creationContext": { "creator": creator_json },
    })
}

/// Most permission grants Roblox accepts in one request. Batches are chunked
/// to this size.
pub const MAX_PERMISSION_REQUESTS: usize = 50;

/// Body for `PATCH /asset-permissions-api/v1/assets/permissions`.
///
/// **One subject, many assets.** The subject lives at the top level and
/// `requests` carries nothing but asset entries. This is the shape the
/// endpoint's own description implies ("grant *a subject* permission to
/// *multiple assets*"), and the earlier per-request `subject` object was the
/// mistake behind `{"code":"InvalidRequest","message":"Invalid SubjectType is
/// invalid."}`: with no `subjectType` at the top level the server parsed the
/// field as its `Invalid` default and echoed that name straight back.
///
/// `assetId` is a number here while `subjectId` is a string. That asymmetry
/// looks like a typo and is not; it is what the working request uses.
///
/// A pure function with an exact-match test, so correcting it stays a
/// one-place change: fix the literal here and the literal in
/// `permissions_body_matches_the_working_shape`, and every caller follows.
pub fn build_permissions_body(universe_id: u64, asset_ids: &[u64]) -> serde_json::Value {
    let requests: Vec<serde_json::Value> = asset_ids
        .iter()
        .map(|asset_id| {
            serde_json::json!({
                "assetId": asset_id,
                // Models carry meshes and textures as separate assets, and a
                // grant that stops at the top-level asset leaves the
                // experience unable to render it.
                "grantToDependencies": true,
                "parentVersionNumber": 0,
            })
        })
        .collect();
    serde_json::json!({
        "subjectType": "Universe",
        // A string, matching how the Assets API wants creator IDs.
        "subjectId": universe_id.to_string(),
        "action": "Use",
        "requests": requests,
        "enableDeepAccessCheck": true,
    })
}

/// What the grant endpoint reported, per asset.
///
/// A 200 does not mean every asset in the batch was granted: the response
/// carries `successAssetIds` alongside a per-asset `errors` list. Treating the
/// status code as the answer reported a clean grant for assets Roblox had just
/// refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantOutcome {
    pub granted: Vec<u64>,
    /// `(asset_id, message)` for the ones that did not take. The id is absent
    /// when Roblox reported an error without naming an asset.
    pub failures: Vec<(Option<u64>, String)>,
}

/// Read a grant response.
///
/// Lenient on purpose: an unrecognised shape yields no successes and no
/// failures, which the caller reports as "nothing confirmed" rather than
/// inventing a result in either direction.
pub fn parse_grant_response(body: &serde_json::Value) -> GrantOutcome {
    let granted = body
        .get("successAssetIds")
        .and_then(|v| v.as_array())
        .map(|ids| ids.iter().filter_map(json_u64).collect())
        .unwrap_or_default();

    let failures = body
        .get("errors")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    // Roblox is inconsistent here: sometimes an object with an
                    // id and a message, sometimes a bare string.
                    let asset_id = entry.get("assetId").and_then(json_u64);
                    let message = entry
                        .get("message")
                        .or_else(|| entry.get("error"))
                        .and_then(|m| m.as_str())
                        .or_else(|| entry.as_str())
                        .unwrap_or("Roblox refused this asset without saying why")
                        .to_string();
                    (asset_id, message)
                })
                .collect()
        })
        .unwrap_or_default();

    GrantOutcome { granted, failures }
}

/// What an operation poll told us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationOutcome {
    StillPending,
    Approved {
        asset_id: u64,
        revision_id: Option<u64>,
    },
    /// Moderation said no. Terminal.
    Rejected { reason: String },
    /// A transport-level or request-level problem, not a verdict.
    Failed { message: String, retryable: bool },
}

/// How to treat a `done: true` response that carried an `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// A moderation verdict. Terminal, and re-uploading the same bytes will
    /// reproduce it.
    Moderation,
    /// Anything else. Not a verdict, so it is never reported as "Rejected".
    Failed { retryable: bool },
}

/// Classify an operation error.
///
/// The default for an unrecognised code is a non-retryable **failure**, not a
/// moderation rejection. Calling a malformed request "Rejected by moderation"
/// sends the user chasing a problem they cannot fix, which is exactly the class
/// of mistake the `CookieRejected` split in `auth.rs` exists to avoid. The
/// reverse mistake merely mislabels a verdict.
pub fn classify_operation_error(code: &str, message: &str) -> ErrorClass {
    let code_upper = code.to_ascii_uppercase();
    let haystack = format!("{code_upper} {}", message.to_ascii_uppercase());
    if haystack.contains("MODERAT") || haystack.contains("INAPPROPRIATE") {
        return ErrorClass::Moderation;
    }
    match code_upper.as_str() {
        // Roblox-side or transient: worth another go.
        "UNAVAILABLE" | "INTERNAL" | "DEADLINE_EXCEEDED" | "ABORTED" => {
            ErrorClass::Failed { retryable: true }
        }
        // Our request, our credentials, or a hard quota. Retrying repeats it.
        _ => ErrorClass::Failed { retryable: false },
    }
}

/// Interpret an operations-endpoint response body.
///
/// `{done:false}` (or no `done` at all) means keep waiting. `done:true` with a
/// `response.assetId` is an approval. `done:true` with an `error` goes through
/// [`classify_operation_error`].
pub fn parse_operation_response(body: &serde_json::Value) -> OperationOutcome {
    let done = body.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
    if !done {
        return OperationOutcome::StillPending;
    }

    if let Some(error) = body.get("error") {
        let code = error
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Roblox reported an error with no message")
            .to_string();
        return match classify_operation_error(&code, &message) {
            ErrorClass::Moderation => OperationOutcome::Rejected { reason: message },
            ErrorClass::Failed { retryable } => OperationOutcome::Failed { message, retryable },
        };
    }

    let response = body.get("response");
    let asset_id = response.and_then(|r| r.get("assetId")).and_then(json_u64);
    match asset_id {
        Some(asset_id) => OperationOutcome::Approved {
            asset_id,
            revision_id: response.and_then(|r| r.get("revisionId")).and_then(json_u64),
        },
        None => OperationOutcome::Failed {
            message: "Roblox reported the upload as done but returned no asset ID".to_string(),
            retryable: false,
        },
    }
}

/// Roblox is inconsistent about quoting IDs, so accept both forms.
fn json_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Accept a bare ID or a Roblox URL and pull the number out. Used by the
/// permission dialog's manual place-or-universe field.
pub fn parse_id_input(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if let Ok(id) = trimmed.parse::<u64>() {
        return (id > 0).then_some(id);
    }
    // roblox.com/games/<id>/Name
    if let Some(idx) = trimmed.find("/games/") {
        let digits: String = trimmed[idx + "/games/".len()..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(id) = digits.parse::<u64>() {
            return (id > 0).then_some(id);
        }
    }
    // ?placeId=<id> / &universeId=<id>
    for key in ["placeid=", "universeid="] {
        let lower = trimmed.to_ascii_lowercase();
        if let Some(idx) = lower.find(key) {
            let digits: String = trimmed[idx + key.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(id) = digits.parse::<u64>() {
                return (id > 0).then_some(id);
            }
        }
    }
    None
}

/// Poll cadence, from the age of the oldest thing being waited on. Images clear
/// in seconds, audio can take minutes, so a fixed interval either hammers
/// Roblox or feels dead.
pub fn poll_interval_for_age(age: Duration) -> Duration {
    if age < Duration::from_secs(120) {
        Duration::from_secs(15)
    } else if age < Duration::from_secs(900) {
        Duration::from_secs(60)
    } else {
        Duration::from_secs(300)
    }
}

/// Row IDs to poll this tick: oldest first, capped, expired entries skipped.
pub fn next_poll_batch(
    records: &[AssetRecord],
    now: DateTime<Utc>,
    max: usize,
) -> Vec<(String, String)> {
    let mut candidates: Vec<(&DateTime<Utc>, &AssetRecord)> = records
        .iter()
        .filter_map(|r| match &r.state {
            AssetState::Pending { since, .. } if !is_expired(*since, now) => Some((since, r)),
            _ => None,
        })
        .collect();
    candidates.sort_by_key(|(since, _)| **since);
    candidates
        .into_iter()
        .take(max)
        .filter_map(|(_, r)| match &r.state {
            AssetState::Pending { operation, .. } => {
                Some((r.row_id.clone(), operation.clone()))
            }
            _ => None,
        })
        .collect()
}

fn is_expired(since: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(since).num_hours() >= OPERATION_TTL_HOURS
}

fn review_is_expired(since: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(since).num_hours() >= REVIEW_TTL_HOURS
}

/// How often to ask for a moderation verdict, from the age of the oldest asset
/// still in review.
///
/// Slower than [`poll_interval_for_age`] at every step. An operation is a job
/// that finishes in seconds; a review is a queue an asset can sit in for hours,
/// and polling it at upload cadence is pure noise on an endpoint that is already
/// quick to rate limit.
pub fn review_poll_interval_for_age(age: Duration) -> Duration {
    if age < Duration::from_secs(300) {
        Duration::from_secs(20)
    } else if age < Duration::from_secs(3600) {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(600)
    }
}

/// `(row_id, asset_id)` for the rows whose verdict to ask about this tick.
/// Oldest first, capped, rows past [`REVIEW_TTL_HOURS`] skipped.
pub fn next_review_batch(
    records: &[AssetRecord],
    now: DateTime<Utc>,
    max: usize,
) -> Vec<(String, u64)> {
    let mut candidates: Vec<(&DateTime<Utc>, &AssetRecord, u64)> = records
        .iter()
        .filter_map(|r| match &r.state {
            AssetState::InReview {
                since, asset_id, ..
            } if !review_is_expired(*since, now) => Some((since, r, *asset_id)),
            _ => None,
        })
        .collect();
    candidates.sort_by_key(|(since, _, _)| **since);
    candidates
        .into_iter()
        .take(max)
        .map(|(_, r, asset_id)| (r.row_id.clone(), asset_id))
        .collect()
}

/// How long to wait before re-sending an upload that failed retryably.
///
/// Exponential from 5s and capped at 2 minutes. The floor matters more than the
/// ceiling here: Roblox's audio limiter is per-minute, and the transport's own
/// 429 backoff tops out in seconds, so retrying immediately just spends the next
/// attempt on the same rejection.
pub fn upload_retry_backoff(attempt: u32) -> Duration {
    const BASE_SECS: u64 = 5;
    const CAP_SECS: u64 = 120;
    let shift = attempt.min(8);
    Duration::from_secs((BASE_SECS << shift).min(CAP_SECS))
}

/// Move every operation past its TTL to [`AssetState::Expired`]. Returns the
/// row IDs that changed so the caller knows whether to persist.
///
/// Also covers rows stuck in review past [`REVIEW_TTL_HOURS`]. Both land on
/// `Expired` for the same reason: the asset exists and may well be live, we
/// just have no way left to find out.
pub fn expire_stale_operations(index: &mut AssetIndex, now: DateTime<Utc>) -> Vec<String> {
    let mut expired = Vec::new();
    for record in &mut index.records {
        let next = match &record.state {
            AssetState::Pending { operation, since } if is_expired(*since, now) => {
                AssetState::Expired {
                    operation: operation.clone(),
                }
            }
            AssetState::InReview {
                asset_id, since, ..
            } if review_is_expired(*since, now) => AssetState::Expired {
                operation: asset_id.to_string(),
            },
            _ => continue,
        };
        expired.push(record.row_id.clone());
        record.state = next;
        record.updated_at = Some(now);
    }
    expired
}

/// A record read back from a newer schema can carry an `AssetKind::Other`,
/// which has no upload path. Surfaces as a clear message rather than a 400.
pub fn reject_unuploadable(kind: AssetKind) -> Result<(), String> {
    if kind == AssetKind::Other {
        return Err("This asset type is not supported by this version of RM".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 2, 0, 0, 0).unwrap() + chrono::Duration::hours(hour)
    }

    fn record(row_id: &str, state: AssetState) -> AssetRecord {
        AssetRecord {
            row_id: row_id.to_string(),
            file_path: PathBuf::from("a.png"),
            file_sha256: "abc".to_string(),
            file_bytes: 1,
            display_name: "a".to_string(),
            description: String::new(),
            kind: AssetKind::Decal,
            creator: Creator::User(1),
            uploaded_by: 1,
            state,
            created_at: at(0),
            updated_at: None,
            granted_universes: Vec::new(),
            auto_grant_universe: None,
            attempts: 0,
            retry_at: None,
            extra: serde_json::Map::new(),
        }
    }

    // ---- classify_path / validate_file ----

    #[test]
    fn classifies_every_documented_extension() {
        let cases = [
            ("a.png", AssetKind::Decal, "image/png"),
            ("a.jpg", AssetKind::Decal, "image/jpeg"),
            ("a.jpeg", AssetKind::Decal, "image/jpeg"),
            ("a.bmp", AssetKind::Decal, "image/bmp"),
            ("a.tga", AssetKind::Decal, "image/tga"),
            ("a.mp3", AssetKind::Audio, "audio/mpeg"),
            ("a.ogg", AssetKind::Audio, "audio/ogg"),
            ("a.wav", AssetKind::Audio, "audio/wav"),
            ("a.flac", AssetKind::Audio, "audio/flac"),
            ("a.fbx", AssetKind::Model, "model/fbx"),
            ("a.gltf", AssetKind::Model, "model/gltf+json"),
            ("a.glb", AssetKind::Model, "model/gltf-binary"),
            ("a.rbxm", AssetKind::Model, "model/x-rbxm"),
            ("a.rbxmx", AssetKind::Model, "model/x-rbxmx"),
            ("a.mp4", AssetKind::Video, "video/mp4"),
            ("a.mov", AssetKind::Video, "video/mov"),
        ];
        for (name, kind, mime) in cases {
            assert_eq!(
                classify_path(Path::new(name)),
                Some((kind, mime)),
                "for {name}"
            );
        }
        assert_eq!(cases.len(), EXT_TABLE.len(), "EXT_TABLE has untested rows");
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        assert_eq!(
            classify_path(Path::new("A.PNG")),
            Some((AssetKind::Decal, "image/png"))
        );
        assert_eq!(
            classify_path(Path::new("A.Png")),
            Some((AssetKind::Decal, "image/png"))
        );
    }

    #[test]
    fn rejects_unknown_and_missing_extensions() {
        assert_eq!(classify_path(Path::new("a.txt")), None);
        assert_eq!(classify_path(Path::new("a.exe")), None);
        assert_eq!(classify_path(Path::new("noext")), None);
    }

    #[test]
    fn rejects_mesh_which_has_no_upload_endpoint() {
        assert_eq!(classify_path(Path::new("a.mesh")), None);
    }

    #[test]
    fn double_extension_uses_the_last_one() {
        assert_eq!(
            classify_path(Path::new("archive.tar.png")),
            Some((AssetKind::Decal, "image/png"))
        );
        assert_eq!(classify_path(Path::new("a.png.gz")), None);
    }

    #[test]
    fn validate_rejects_oversized_and_empty_files() {
        assert!(validate_file(Path::new("a.png"), 21 * MB)
            .unwrap_err()
            .contains("20 MB"));
        assert_eq!(validate_file(Path::new("a.png"), 0).unwrap_err(), "File is empty");
    }

    #[test]
    fn validate_accepts_exactly_the_limit() {
        assert!(validate_file(Path::new("a.png"), 20 * MB).is_ok());
    }

    #[test]
    fn validate_skips_the_cap_where_roblox_documents_none() {
        // Audio is capped by duration and account quota, not bytes.
        assert!(validate_file(Path::new("a.mp3"), 500 * MB).is_ok());
    }

    #[test]
    fn validate_names_the_offending_extension() {
        let err = validate_file(Path::new("a.exe"), 10).unwrap_err();
        assert!(err.contains("exe"), "got: {err}");
    }

    // ---- display names ----

    #[test]
    fn display_name_strips_path_and_extension() {
        assert_eq!(
            sanitize_display_name_from_path(Path::new(r"C:\x\Oak Bark.png")),
            "Oak Bark"
        );
    }

    #[test]
    fn display_name_collapses_whitespace() {
        assert_eq!(sanitize_display_name("  a \t\n b  "), "a b");
    }

    #[test]
    fn display_name_truncates_on_a_char_boundary() {
        let long = "日".repeat(200);
        let out = sanitize_display_name(&long);
        assert_eq!(out.chars().count(), MAX_DISPLAY_NAME_CHARS);
    }

    #[test]
    fn display_name_never_empty() {
        assert_eq!(sanitize_display_name("   "), "Untitled");
        assert_eq!(sanitize_display_name_from_path(Path::new("")), "Untitled");
    }

    // ---- request body ----

    #[test]
    fn create_request_uses_string_user_id() {
        let body = build_create_request_json(
            AssetKind::Decal,
            "Oak Bark",
            "",
            Creator::User(1916532448),
        );
        assert_eq!(
            body,
            serde_json::json!({
                "assetType": "Decal",
                "displayName": "Oak Bark",
                "description": "",
                "creationContext": { "creator": { "userId": "1916532448" } }
            })
        );
    }

    #[test]
    fn create_request_never_sends_both_creator_forms() {
        let body = build_create_request_json(AssetKind::Audio, "n", "d", Creator::Group(8497064));
        let creator = &body["creationContext"]["creator"];
        assert_eq!(creator["groupId"], "8497064");
        assert!(creator.get("userId").is_none());
    }

    // ---- operation parsing ----

    #[test]
    fn not_done_is_still_pending() {
        let body = serde_json::json!({ "path": "operations/9f", "done": false });
        assert_eq!(parse_operation_response(&body), OperationOutcome::StillPending);
    }

    #[test]
    fn missing_done_is_still_pending() {
        let body = serde_json::json!({ "path": "operations/9f" });
        assert_eq!(parse_operation_response(&body), OperationOutcome::StillPending);
    }

    #[test]
    fn done_with_asset_id_is_approved() {
        let body = serde_json::json!({
            "done": true,
            "response": { "assetId": "1234567890", "revisionId": "2" }
        });
        assert_eq!(
            parse_operation_response(&body),
            OperationOutcome::Approved {
                asset_id: 1234567890,
                revision_id: Some(2)
            }
        );
    }

    #[test]
    fn asset_id_is_accepted_as_a_number_too() {
        let body = serde_json::json!({ "done": true, "response": { "assetId": 42 } });
        assert_eq!(
            parse_operation_response(&body),
            OperationOutcome::Approved {
                asset_id: 42,
                revision_id: None
            }
        );
    }

    #[test]
    fn moderation_error_is_a_rejection() {
        let body = serde_json::json!({
            "done": true,
            "error": { "code": "MODERATION_FAILED", "message": "Asset was moderated" }
        });
        assert_eq!(
            parse_operation_response(&body),
            OperationOutcome::Rejected {
                reason: "Asset was moderated".to_string()
            }
        );
    }

    #[test]
    fn invalid_argument_is_not_a_moderation_rejection() {
        let body = serde_json::json!({
            "done": true,
            "error": { "code": "INVALID_ARGUMENT", "message": "bad assetType" }
        });
        assert_eq!(
            parse_operation_response(&body),
            OperationOutcome::Failed {
                message: "bad assetType".to_string(),
                retryable: false
            }
        );
    }

    #[test]
    fn done_with_neither_response_nor_error_is_a_failure() {
        let body = serde_json::json!({ "done": true });
        assert!(matches!(
            parse_operation_response(&body),
            OperationOutcome::Failed { retryable: false, .. }
        ));
    }

    #[test]
    fn unknown_error_codes_default_to_non_retryable_failure() {
        assert_eq!(
            classify_operation_error("", ""),
            ErrorClass::Failed { retryable: false }
        );
        assert_eq!(
            classify_operation_error("WEIRD_NEW_CODE", "who knows"),
            ErrorClass::Failed { retryable: false }
        );
    }

    #[test]
    fn roblox_side_codes_are_retryable() {
        assert_eq!(
            classify_operation_error("UNAVAILABLE", "try later"),
            ErrorClass::Failed { retryable: true }
        );
        assert_eq!(
            classify_operation_error("INTERNAL", ""),
            ErrorClass::Failed { retryable: true }
        );
    }

    #[test]
    fn moderation_is_detected_from_the_message_too() {
        assert_eq!(
            classify_operation_error("UNKNOWN", "Asset failed moderation review"),
            ErrorClass::Moderation
        );
    }

    // ---- permissions ----

    #[test]
    fn permissions_body_matches_the_working_shape() {
        assert_eq!(
            build_permissions_body(456, &[123]),
            serde_json::json!({
                "subjectType": "Universe",
                "subjectId": "456",
                "action": "Use",
                "requests": [{
                    "assetId": 123,
                    "grantToDependencies": true,
                    "parentVersionNumber": 0
                }],
                "enableDeepAccessCheck": true
            })
        );
    }

    #[test]
    fn the_subject_is_top_level_not_per_request() {
        // A `subject` object inside each request is what produced
        // `{"code":"InvalidRequest","message":"Invalid SubjectType is
        // invalid."}`: the server found no `subjectType` where it looks, parsed
        // its `Invalid` default, and echoed that name back.
        let body = build_permissions_body(456, &[123]);
        assert_eq!(body["subjectType"], "Universe");
        assert!(body["requests"][0].get("subject").is_none(), "got: {body}");
        assert!(
            body["requests"][0].get("subjectType").is_none(),
            "got: {body}"
        );
    }

    #[test]
    fn the_subject_id_is_a_string_and_the_asset_id_is_not() {
        // Asymmetric, and deliberately so: it is what the working request uses.
        let body = build_permissions_body(456, &[123]);
        assert!(body["subjectId"].is_string(), "got: {body}");
        assert!(body["requests"][0]["assetId"].is_number(), "got: {body}");
    }

    #[test]
    fn permissions_body_carries_one_request_per_asset() {
        let body = build_permissions_body(1, &[10, 11, 12]);
        let requests = body["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[2]["assetId"], 12);
        // One subject covers the whole batch.
        assert_eq!(body["subjectId"], "1");
    }

    #[test]
    fn permissions_body_with_no_assets_is_an_empty_list() {
        let body = build_permissions_body(1, &[]);
        assert!(body["requests"].as_array().unwrap().is_empty());
    }

    // ---- grant response ----

    #[test]
    fn a_grant_response_reports_successes_and_refusals_separately() {
        // A 200 is not a blanket yes.
        let body = serde_json::json!({
            "successAssetIds": [123, "456"],
            "errors": [{ "assetId": 789, "message": "Requester cannot manage permissions" }]
        });
        let outcome = parse_grant_response(&body);
        assert_eq!(outcome.granted, vec![123, 456]);
        assert_eq!(
            outcome.failures,
            vec![(
                Some(789),
                "Requester cannot manage permissions".to_string()
            )]
        );
    }

    #[test]
    fn a_grant_response_accepts_bare_string_errors() {
        let body = serde_json::json!({ "successAssetIds": [], "errors": ["nope"] });
        let outcome = parse_grant_response(&body);
        assert!(outcome.granted.is_empty());
        assert_eq!(outcome.failures, vec![(None, "nope".to_string())]);
    }

    #[test]
    fn an_unrecognised_grant_response_claims_nothing() {
        // The caller treats this as "nothing confirmed" and falls back, rather
        // than inventing a result in either direction.
        assert_eq!(parse_grant_response(&serde_json::json!({})), GrantOutcome::default());
        assert_eq!(
            parse_grant_response(&serde_json::json!({ "successAssetIds": "nope" })),
            GrantOutcome::default()
        );
    }

    // ---- id parsing ----

    #[test]
    fn parses_bare_and_urlish_ids() {
        assert_eq!(parse_id_input(" 12345 "), Some(12345));
        assert_eq!(
            parse_id_input("https://www.roblox.com/games/88684053528456/Study"),
            Some(88684053528456)
        );
        assert_eq!(parse_id_input("https://x/?placeId=777&a=1"), Some(777));
        assert_eq!(parse_id_input("universeId=10545428548"), Some(10545428548));
    }

    #[test]
    fn rejects_garbage_and_zero() {
        assert_eq!(parse_id_input("nope"), None);
        assert_eq!(parse_id_input(""), None);
        assert_eq!(parse_id_input("0"), None);
    }

    // ---- polling ----

    #[test]
    fn poll_interval_backs_off_in_tiers() {
        assert_eq!(
            poll_interval_for_age(Duration::from_secs(30)),
            Duration::from_secs(15)
        );
        assert_eq!(
            poll_interval_for_age(Duration::from_secs(120)),
            Duration::from_secs(60)
        );
        assert_eq!(
            poll_interval_for_age(Duration::from_secs(3600)),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn poll_batch_is_oldest_first_and_capped() {
        let mut records = Vec::new();
        for (i, hour) in [5i64, 1, 3].into_iter().enumerate() {
            records.push(record(
                &format!("r{i}"),
                AssetState::Pending {
                    operation: format!("op{i}"),
                    since: at(hour),
                },
            ));
        }
        let batch = next_poll_batch(&records, at(6), 2);
        assert_eq!(
            batch,
            vec![
                ("r1".to_string(), "op1".to_string()),
                ("r2".to_string(), "op2".to_string())
            ]
        );
    }

    #[test]
    fn poll_batch_skips_expired_and_inactive_rows() {
        let records = vec![
            record(
                "old",
                AssetState::Pending {
                    operation: "op".to_string(),
                    since: at(0),
                },
            ),
            record("queued", AssetState::Queued),
            record(
                "fresh",
                AssetState::Pending {
                    operation: "op2".to_string(),
                    since: at(30),
                },
            ),
        ];
        let batch = next_poll_batch(&records, at(31), 10);
        assert_eq!(batch, vec![("fresh".to_string(), "op2".to_string())]);
    }

    #[test]
    fn expiry_is_exactly_at_the_ttl() {
        let mut index = AssetIndex::default();
        index.records.push(record(
            "a",
            AssetState::Pending {
                operation: "op".to_string(),
                since: at(0),
            },
        ));

        // 23h59m survives.
        let almost = at(0) + chrono::Duration::minutes(23 * 60 + 59);
        assert!(expire_stale_operations(&mut index, almost).is_empty());

        let past = at(0) + chrono::Duration::minutes(24 * 60 + 1);
        assert_eq!(expire_stale_operations(&mut index, past), vec!["a"]);
        assert!(matches!(
            index.records[0].state,
            AssetState::Expired { .. }
        ));
    }

    // ---- moderation ----

    // These encode the rule the Asset Manager got wrong: an operation reporting
    // `done` is ingestion, not a verdict. Every case below came from the shape
    // newnew13bot_v2 polls in production.

    #[test]
    fn an_unfinished_review_is_not_an_approval() {
        for status in ["Pending", "InReview", "AwaitingReview", ""] {
            let entry = serde_json::json!({ "reviewStatus": status, "isModerated": false });
            assert_eq!(
                parse_moderation_entry(&entry),
                Some(ModerationStatus::InReview),
                "reviewStatus {status}"
            );
        }
    }

    #[test]
    fn a_finished_unmoderated_review_is_an_approval() {
        let entry = serde_json::json!({ "reviewStatus": "Finished", "isModerated": false });
        assert_eq!(
            parse_moderation_entry(&entry),
            Some(ModerationStatus::Approved)
        );
    }

    #[test]
    fn a_finished_moderated_review_is_a_rejection() {
        let entry = serde_json::json!({ "reviewStatus": "Finished", "isModerated": true });
        assert_eq!(
            parse_moderation_entry(&entry),
            Some(ModerationStatus::Rejected)
        );
    }

    #[test]
    fn does_not_require_wins_over_is_moderated() {
        // Asset types that skip review report `isModerated: true` in this
        // combination on occasion. Reading it as a block would fail every
        // upload of a type that was never going to be reviewed.
        let entry = serde_json::json!({ "reviewStatus": "DoesNotRequire", "isModerated": true });
        assert_eq!(
            parse_moderation_entry(&entry),
            Some(ModerationStatus::Approved)
        );
    }

    #[test]
    fn a_missing_review_status_is_treated_as_unfinished() {
        // Waiting on an asset that turns out to be fine costs a poll. Calling
        // it approved when it is not is what this whole change exists to stop,
        // so the ambiguous case waits.
        let entry = serde_json::json!({ "isModerated": false });
        assert_eq!(
            parse_moderation_entry(&entry),
            Some(ModerationStatus::InReview)
        );
    }

    #[test]
    fn an_entry_with_neither_field_yields_no_verdict() {
        assert_eq!(parse_moderation_entry(&serde_json::json!({})), None);
        assert_eq!(
            parse_moderation_entry(&serde_json::json!({ "name": "x" })),
            None
        );
    }

    #[test]
    fn moderation_response_pairs_by_id_not_position() {
        let body = serde_json::json!({
            "data": [
                { "id": 30, "reviewStatus": "Finished", "isModerated": true },
                { "id": 10, "reviewStatus": "DoesNotRequire", "isModerated": false }
            ]
        });
        assert_eq!(
            parse_moderation_response(&body),
            vec![
                (30, ModerationStatus::Rejected),
                (10, ModerationStatus::Approved),
            ]
        );
    }

    #[test]
    fn moderation_response_degrades_rather_than_failing() {
        assert!(parse_moderation_response(&serde_json::json!({})).is_empty());
        assert!(parse_moderation_response(&serde_json::json!({ "data": "nope" })).is_empty());
        // An entry with no id, and one with no verdict fields, are both skipped
        // while the usable entry survives.
        let mixed = serde_json::json!({
            "data": [
                { "reviewStatus": "Finished" },
                { "id": 5 },
                { "id": 7, "reviewStatus": "Finished", "isModerated": false }
            ]
        });
        assert_eq!(
            parse_moderation_response(&mixed),
            vec![(7, ModerationStatus::Approved)]
        );
    }

    // ---- review scheduling ----

    #[test]
    fn review_polling_is_slower_than_operation_polling_at_every_tier() {
        for age in [30u64, 600, 7200] {
            let age = Duration::from_secs(age);
            assert!(
                review_poll_interval_for_age(age) > poll_interval_for_age(age),
                "age {age:?}"
            );
        }
    }

    #[test]
    fn review_batch_is_oldest_first_and_capped() {
        let mut records = Vec::new();
        for (i, hour) in [5i64, 1, 3].into_iter().enumerate() {
            records.push(record(
                &format!("r{i}"),
                AssetState::InReview {
                    asset_id: 100 + i as u64,
                    revision_id: None,
                    since: at(hour),
                },
            ));
        }
        let batch = next_review_batch(&records, at(6), 2);
        assert_eq!(
            batch,
            vec![("r1".to_string(), 101), ("r2".to_string(), 102)]
        );
    }

    #[test]
    fn a_review_outlives_an_operation_by_days() {
        let mut index = AssetIndex::default();
        index.records.push(record(
            "a",
            AssetState::InReview {
                asset_id: 7,
                revision_id: None,
                since: at(0),
            },
        ));

        // Well past OPERATION_TTL_HOURS, and still waiting: audio review runs
        // long, and expiring it at the operation's TTL reported "timed out" on
        // assets that were about to be approved.
        let day_and_a_half = at(0) + chrono::Duration::hours(36);
        assert!(expire_stale_operations(&mut index, day_and_a_half).is_empty());
        assert!(!next_review_batch(&index.records, day_and_a_half, 10).is_empty());

        let past = at(0) + chrono::Duration::hours(REVIEW_TTL_HOURS + 1);
        assert_eq!(expire_stale_operations(&mut index, past), vec!["a"]);
        assert!(next_review_batch(&index.records, past, 10).is_empty());
    }

    #[test]
    fn an_asset_in_review_counts_as_uploaded() {
        // The bytes are on Roblox and the id is minted. Re-sending them would
        // mint a second asset and, for audio, burn a second slice of quota.
        let mut index = AssetIndex::default();
        let mut in_review = record(
            "a",
            AssetState::InReview {
                asset_id: 42,
                revision_id: None,
                since: at(0),
            },
        );
        in_review.file_sha256 = "hash".to_string();
        index.records.push(in_review);

        let found = index.find_uploaded("hash", Creator::User(1));
        assert_eq!(found.and_then(|r| r.state.asset_id()), Some(42));
    }

    #[test]
    fn in_review_is_active_but_not_terminal() {
        let state = AssetState::InReview {
            asset_id: 1,
            revision_id: None,
            since: at(0),
        };
        // Active keeps the poll timers running; terminal would let
        // "Clear finished" delete a row that has not finished.
        assert!(state.is_active());
        assert!(!state.is_terminal());
    }

    // ---- upload retry backoff ----

    #[test]
    fn retry_backoff_grows_and_then_stops() {
        assert_eq!(upload_retry_backoff(0), Duration::from_secs(5));
        assert_eq!(upload_retry_backoff(1), Duration::from_secs(10));
        assert_eq!(upload_retry_backoff(2), Duration::from_secs(20));
        // Capped, and no overflow at any attempt a caller could reach.
        assert_eq!(upload_retry_backoff(20), Duration::from_secs(120));
        assert_eq!(upload_retry_backoff(u32::MAX), Duration::from_secs(120));
    }

    #[test]
    fn the_first_retry_waits_long_enough_to_outlast_a_limiter_window() {
        // Roblox's audio limiter is measured in seconds. Retrying instantly
        // just spends the next attempt on the same rejection.
        assert!(upload_retry_backoff(0) >= Duration::from_secs(5));
    }

    // ---- index ----

    #[test]
    fn same_hash_different_creator_is_not_a_duplicate() {
        let mut index = AssetIndex::default();
        let mut rec = record("a", AssetState::Approved { asset_id: 7, revision_id: None });
        rec.creator = Creator::User(1);
        index.records.push(rec);

        assert!(index.find_uploaded("abc", Creator::User(1)).is_some());
        assert!(index.find_uploaded("abc", Creator::Group(1)).is_none());
        assert!(index.find_uploaded("abc", Creator::User(2)).is_none());
    }

    #[test]
    fn an_unhashed_row_is_never_a_duplicate() {
        let mut index = AssetIndex::default();
        let mut rec = record("a", AssetState::Approved { asset_id: 7, revision_id: None });
        rec.file_sha256 = String::new();
        index.records.push(rec);
        assert!(index.find_uploaded("", Creator::User(1)).is_none());
    }

    #[test]
    fn only_approved_rows_count_as_duplicates() {
        let mut index = AssetIndex::default();
        index
            .records
            .push(record("a", AssetState::Rejected { reason: "no".into() }));
        assert!(index.find_uploaded("abc", Creator::User(1)).is_none());
    }

    #[test]
    fn unknown_asset_kind_degrades_to_other() {
        let kind: AssetKind = serde_json::from_str("\"Hologram\"").unwrap();
        assert_eq!(kind, AssetKind::Other);
        assert!(reject_unuploadable(kind).is_err());
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let raw = r#"{
            "version": 1,
            "records": [],
            "futureTopLevelField": {"a": 1}
        }"#;
        let index: AssetIndex = serde_json::from_str(raw).unwrap();
        let out = serde_json::to_string(&index).unwrap();
        assert!(out.contains("futureTopLevelField"), "got: {out}");
    }

    #[test]
    fn a_minimal_record_parses_with_defaults() {
        let raw = r#"{
            "records": [{
                "rowId": "r1",
                "creator": {"kind": "user", "id": 5},
                "uploadedBy": 5,
                "state": {"state": "queued"},
                "createdAt": "2026-08-02T00:00:00Z"
            }]
        }"#;
        let index: AssetIndex = serde_json::from_str(raw).unwrap();
        assert_eq!(index.version, CURRENT_SCHEMA);
        let rec = &index.records[0];
        assert_eq!(rec.kind, AssetKind::Other);
        assert_eq!(rec.file_bytes, 0);
        assert!(rec.granted_universes.is_empty());
    }

    #[test]
    fn sha256_matches_the_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    // ---- load / save ----

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ram_assets_{}_{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("assets.json")
    }

    #[test]
    fn missing_file_loads_empty_and_writable() {
        let path = scratch("missing");
        let _ = std::fs::remove_file(&path);
        let (index, status) = AssetIndex::load(&path);
        assert!(index.records.is_empty());
        assert_eq!(status, IndexLoad::Ok);
        assert!(!status.is_read_only());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn round_trips_through_disk() {
        let path = scratch("roundtrip");
        let mut index = AssetIndex::default();
        index.records.push(record("a", AssetState::Queued));
        index.save(&path).unwrap();

        let (loaded, status) = AssetIndex::load(&path);
        assert_eq!(status, IndexLoad::Ok);
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.records[0].row_id, "a");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_primary_recovers_from_backup_without_clobbering() {
        let path = scratch("recover");
        let mut index = AssetIndex::default();
        index.records.push(record("good", AssetState::Queued));
        index.save(&path).unwrap();
        // A second save moves the first copy to .bak.
        index.records.push(record("newer", AssetState::Queued));
        index.save(&path).unwrap();

        std::fs::write(&path, b"{ not json").unwrap();
        let (loaded, status) = AssetIndex::load(&path);
        assert_eq!(status, IndexLoad::RecoveredFromBackup);
        assert_eq!(loaded.records.len(), 1);
        // The damaged file is left exactly as it was.
        assert_eq!(std::fs::read(&path).unwrap(), b"{ not json");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unrecoverable_index_is_empty_and_read_only() {
        let path = scratch("corrupt");
        std::fs::write(&path, b"{ not json").unwrap();
        let _ = std::fs::remove_file(storage::backup_path(&path));
        let (loaded, status) = AssetIndex::load(&path);
        assert!(loaded.records.is_empty());
        assert_eq!(status, IndexLoad::Corrupt);
        assert!(status.is_read_only());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_newer_schema_loads_read_only() {
        let path = scratch("newer");
        std::fs::write(
            &path,
            format!(r#"{{"version": {}, "records": []}}"#, CURRENT_SCHEMA + 1),
        )
        .unwrap();
        let (_, status) = AssetIndex::load(&path);
        assert_eq!(status, IndexLoad::NewerSchema);
        assert!(status.is_read_only());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
