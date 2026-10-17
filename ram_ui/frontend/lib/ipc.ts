import { invoke } from "@tauri-apps/api/core";

export type HistoryKind = "observed" | "joined" | "left" | "changedGame" | "changedServer" | "online" | "offline" | "studio" | "leftStudio" | "unknown" | "moderated";
export interface HistoryEvent { id: number; observedAt: string; userId: number; kind: HistoryKind; location: string; placeId: number | null; jobId: string | null; }
export interface HistorySnapshot { events: HistoryEvent[]; persistent: boolean; error: string | null; }
export function getSessionHistory(): Promise<HistorySnapshot> { return invoke("get_session_history"); }
export function setHistoryPersistence(persistent: boolean): Promise<HistorySnapshot> { return invoke("set_history_persistence", { persistent }); }
export function clearSessionHistory(): Promise<HistorySnapshot> { return invoke("clear_session_history"); }

export type JoinMode = "visibleServer" | "temporaryFollow";
export interface JoinResult { userId: number; requested: boolean; message: string; cleanupPending: boolean; }
export interface JoinProgress { operationId: string; userId: number; message: string; }
export interface PendingFollow { userId: number; targetUserId: number; recordedAt: string; }
export function joinUserAccounts(operationId: string, userIds: number[], target: string, mode: JoinMode): Promise<JoinResult[]> { return invoke("join_user_accounts", { operationId, userIds, target, mode }); }
export function cancelUserJoin(operationId: string): Promise<void> { return invoke("cancel_user_join", { operationId }); }
export function getJoinCleanups(): Promise<PendingFollow[]> { return invoke("get_join_cleanups"); }
export function retryJoinCleanup(userId: number, targetUserId: number): Promise<PendingFollow[]> { return invoke("retry_join_cleanup", { userId, targetUserId }); }
export function resetJoinCleanupJournal(): Promise<void> { return invoke("reset_join_cleanup_journal", { confirmation: "REVIEWED FOLLOWS" }); }

export interface RobloxSettingResult {
  userId: number;
  success: boolean;
  message: string;
}

export function changeDisplayNames(userIds: number[], name: string): Promise<RobloxSettingResult[]> {
  return invoke("change_display_names", { userIds, name });
}

export function checkRobloxDisplayName(userIds: number[], name: string): Promise<void> {
  return invoke("check_roblox_display_name", { userIds, name });
}

export type RobloxPrivacyField = "whoCanJoinMeInExperiences" | "whoCanSeeMyOnlineStatus";
export interface RobloxPrivacySetting { field: RobloxPrivacyField; currentValue: string; options: string[]; }
export interface AccountPrivacy { userId: number; settings: RobloxPrivacySetting[]; error: string | null; }
export function getRobloxPrivacy(userIds: number[]): Promise<AccountPrivacy[]> {
  return invoke("get_roblox_privacy", { userIds });
}
export function changeRobloxPrivacy(userIds: number[], field: RobloxPrivacyField, value: string): Promise<RobloxSettingResult[]> {
  return invoke("change_roblox_privacy", { userIds, field, value });
}

export type DiscordEvent = "moderationDetected" | "batchLaunchFinished" | "sessionExpired" | "launchFailed" | "clientExited" | "friendRequestReceived" | "friendRequestSent" | "friendAdded" | "friendRemoved" | "friendRequestDeclined" | "userBlocked";
export interface DiscordDestination { id: string; name: string; enabled: boolean; events: DiscordEvent[]; }
export interface DiscordNotificationSettings {
  enabled: boolean;
  moderationDetected: boolean;
  batchLaunchFinished: boolean;
  sessionExpired: boolean;
  launchFailed: boolean;
  clientExited: boolean;
  friendRequestReceived: boolean;
  friendRequestSent: boolean;
  friendAdded: boolean;
  friendRemoved: boolean;
  friendRequestDeclined: boolean;
  userBlocked: boolean;
  destinations: DiscordDestination[];
}
export function getDiscordNotifications(): Promise<DiscordNotificationSettings> { return invoke("get_discord_notifications"); }
export function resetDiscordNotifications(): Promise<DiscordNotificationSettings> { return invoke("reset_discord_notifications"); }
export function saveDiscordNotifications(settings: DiscordNotificationSettings): Promise<void> { return invoke("save_discord_notifications", { settings }); }
export function saveDiscordDestination(id: string | null, name: string, url: string): Promise<DiscordNotificationSettings> { return invoke("save_discord_destination", { id, name, url }); }
export function removeDiscordDestination(id: string): Promise<DiscordNotificationSettings> { return invoke("remove_discord_destination", { id }); }
export function testDiscordDestination(id: string): Promise<void> { return invoke("test_discord_destination", { id }); }
export function launchAccounts(userIds: number[], placeId: number, jobId: string, data: string): Promise<void> {
  return invoke("launch_accounts", { userIds, placeId, jobId: jobId.trim() || null, data: data.trim() || null });
}

declare global {
  interface Window { __RM_BENCHMARK__?: boolean; }
}

export function benchmarkReady(): Promise<void> {
  return invoke<void>("benchmark_ready");
}

export type PresenceStatus = "online" | "warning" | "danger" | "neutral";

export interface AccountSummary {
  userId: number;
  credentialRevision: number;
  label: string;
  username: string;
  displayName: string;
  alias: string;
  group: string;
  tagIds: string[];
  avatarUrl: string;
  isPinned: boolean;
  sortOrder: number;
  cookieExpired: boolean;
  moderationActive: boolean;
  moderationBanned: boolean;
  moderationReason: string | null;
  moderationExpiresAt: string | null;
  playerPath: string | null;
  createdAt: string | null;
  presence: PresenceStatus;
  presenceText: string;
  presenceLocation: string;
  canLaunch: boolean;
  lastActivity: string | null;
  lastUsed: string | null;
}

export interface StoreStatus {
  exists: boolean;
  unlocked: boolean;
  needsPassword: boolean;
  legacy: boolean;
  accountCount: number;
}

export interface InventoryItem {
  assetId: number;
  name: string;
  assetType: string;
  iconUrl: string | null;
  priceRobux: number | null;
}

export interface UserSearchResult {
  userId: number;
  username: string;
  displayName: string;
  avatarUrl: string | null;
}

export interface PresenceUpdate {
  userId: number;
  presence: PresenceStatus;
  presenceText: string;
  location: string;
}

export interface AccountGroupSummary {
  name: string;
  color: [number, number, number];
  sortOrder: number;
}

export interface PrivateServerSummary {
  index: number;
  name: string;
  placeId: number;
  placeName: string;
  iconUrl: string;
  url: string;
  ownerUsername?: string;
  ownerProfileUrl?: string;
}

export interface PrivateServerCheck {
  status: "valid" | "invalid" | "unknown";
  ownership: "owned" | "unknown" | "not-owned";
  ownerId: number | null;
  ownerUsername: string | null;
  message: string;
  ownerLookup: "notChecked" | "found" | "notListed" | "rateLimited" | "unavailable" | "incomplete";
}

export async function checkPrivateServer(index: number): Promise<PrivateServerCheck> {
  return invoke<PrivateServerCheck>("check_private_server", { index });
}

export interface LaunchPresetSummary {
  index: number;
  name: string;
  placeId: number;
  iconUrl: string;
  jobId: string | null;
  data: string | null;
  tagIds: string[];
}

export interface AccountTag { id: string; name: string; color: [number, number, number]; }
export interface TagSnapshot { tags: AccountTag[]; assignments: { userId: number; tagIds: string[] }[]; }
export function listAccountTags(): Promise<TagSnapshot> { return invoke("list_account_tags"); }
export function saveAccountTag(id: string | null, name: string, color?: [number, number, number]): Promise<TagSnapshot> { return invoke("save_account_tag", { id, name, color }); }
export function deleteAccountTag(id: string): Promise<TagSnapshot> { return invoke("delete_account_tag", { id }); }
export function assignAccountTags(userIds: number[], tagIds: string[], remove: boolean): Promise<TagSnapshot> { return invoke("assign_account_tags", { userIds, tagIds, remove }); }

export interface GroupSearchResult {
  id: number;
  name: string;
  description: string;
  memberCount: number;
  hasVerifiedBadge: boolean;
}

export interface GroupPoster {
  username: string;
  displayName: string;
}

export interface GroupShout {
  body: string;
  created: string | null;
  poster: GroupPoster | null;
}

export interface GroupAnnouncement {
  title: string;
  body: string;
  created: string | null;
  imageUrl: string | null;
  reactions: { label: string; count: number }[];
}

export interface GroupForumCategory {
  id: string;
  name: string;
  posts: {
    id: string;
    title: string;
    body: string;
    created: string | null;
    author: string | null;
    commentCount: number;
  }[];
}

export interface GroupOwner {
  id: number;
  username: string;
  displayName: string;
}

export interface GroupInfo {
  id: number;
  name: string;
  description: string;
  memberCount: number;
  publicEntryAllowed: boolean;
  hasVerifiedBadge: boolean;
  hasSocialModules: boolean;
  communityTier: number | null;
  created: string | null;
  shout: GroupShout | null;
  owner: GroupOwner | null;
}

export interface GroupMembership {
  userId: number;
  joined: boolean;
  roleName: string | null;
  roleRank: number;
}

export interface GroupWorkspace {
  group: GroupInfo;
  iconDataUrl: string | null;
  announcement: GroupAnnouncement | null;
  forums: GroupForumCategory[];
  forumStatus: string | null;
  memberships: GroupMembership[];
}

export interface GroupMembershipResult {
  userId: number;
  join: boolean;
  ok: boolean;
  challenge: boolean;
  message: string | null;
}

export type LogLevel = "Error" | "Warn" | "Info" | "Debug" | "Trace";

export type MonitorTarget =
  | { type: "Primary" }
  | { type: "All" }
  | { type: "Index"; value: number };

export type TilingLayoutMode =
  | { type: "Auto" }
  | { type: "FixedColumns"; value: number }
  | { type: "FixedRows"; value: number }
  | { type: "CustomGrid"; value: { cols: number; rows: number } }
  | { type: "SideBySide" }
  | { type: "Stacked" };

export interface MonitorGeometry {
  index: number;
  name: string;
  is_primary: boolean;
  total_x: number;
  total_y: number;
  total_w: number;
  total_h: number;
  work_x: number;
  work_y: number;
  work_w: number;
  work_h: number;
}

export interface PageVisibility {
  accounts: boolean;
  instances: boolean;
  groups: boolean;
  friends: boolean;
  privateServers: boolean;
  presets: boolean;
  inventories: boolean;
  assetManager: boolean;
}

export const defaultPageVisibility: PageVisibility = {
  accounts: true, instances: true, groups: true, friends: true, privateServers: true,
  presets: true, inventories: false, assetManager: false,
};

export const workspacePages = [
  { key: "accounts", label: "Accounts" },
  { key: "instances", label: "Instances" },
  { key: "groups", label: "Groups" },
  { key: "friends", label: "Friends" },
  { key: "privateServers", label: "Private Servers" },
  { key: "presets", label: "Presets" },
  { key: "inventories", label: "Inventories" },
  { key: "assetManager", label: "Asset Manager" },
] as const;

export interface SettingsConfig {
  sessionHistoryPersist: boolean;
  useCredentialManager: boolean;
  startupWithWindows: boolean;
  refreshOnStartup: boolean;
  autoLaunchOnStartup: boolean;
  autoLaunchAccountId: number | null;
  autoLaunchPlaceId: number | null;
  multiInstanceEnabled: boolean;
  killBackgroundRoblox: boolean;
  confirmKillAll: boolean;
  launchDelaySecs: number;
  customGameArgs: string;
  robloxPlayerPath: string | null;
  robloxFastFlags: Record<string, string>;
  privacyMode: boolean;
  privacyCleanCookies: boolean;
  privacyCleanLocalStorage: boolean;
  privacyCleanFullProfile: boolean;
  privacyCleanOnExit: boolean;
  privacyClearClipboard: boolean;
  macRotationEnabled: boolean;
  macPreserveOui: boolean;
  macAlternateOui: string;
  autoArrangeWindows: boolean;
  tilingTargetMonitor: MonitorTarget;
  tilingLayoutMode: TilingLayoutMode;
  tilingCustomCols: number;
  tilingCustomRows: number;
  tilingPadding: number;
  renameRobloxWindows: boolean;
  anonymizeNames: boolean;
  hideFromScreenCapture: boolean;
  developerOptions: boolean;
  pageVisibility: PageVisibility;
  utilityEnabled: boolean;
  logLevel: LogLevel;
  logTargets: string[];
}

export type SettingsUpdate = Omit<
  SettingsConfig,
  "startupWithWindows" | "sessionHistoryPersist"
>;

export interface SettingsInfoCard {
  kind: "info" | "warning" | "caution" | string;
  text: string;
}

export interface SettingsSnapshot {
  config: SettingsConfig;
  appVersion: string;
  systemArchitecture: string;
  monitors: MonitorGeometry[];
  hasPassword: boolean;
  hasDiscordWebhook: boolean;
  robloxRunning: boolean;
  infoCards: Record<string, SettingsInfoCard>;
}

export interface TilingOptions {
  target_monitor: MonitorTarget;
  layout_mode: TilingLayoutMode;
  custom_cols: number;
  custom_rows: number;
  padding: number;
}

export async function listPrivateServers(): Promise<PrivateServerSummary[]> {
  return invoke<PrivateServerSummary[]>("list_private_servers");
}

export async function addPrivateServer(
  name: string,
  url: string,
): Promise<PrivateServerSummary> {
  return invoke<PrivateServerSummary>("add_private_server", { name, url });
}

export async function removePrivateServer(index: number): Promise<void> {
  return invoke<void>("remove_private_server", { index });
}

export async function renamePrivateServer(
  index: number,
  name: string,
): Promise<void> {
  return invoke<void>("rename_private_server", { index, name });
}

export async function updatePrivateServer(
  index: number,
  name: string,
  url: string,
): Promise<PrivateServerSummary> {
  return invoke<PrivateServerSummary>("update_private_server", {
    index,
    name,
    url,
  });
}

export async function launchPrivateServer(
  index: number,
  userIds: number[],
): Promise<void> {
  return invoke<void>("launch_private_server", { index, userIds });
}

export async function listAccounts(): Promise<AccountSummary[]> {
  return invoke<AccountSummary[]>("list_accounts");
}

export interface AssetRow {
  rowId: string;
  displayName: string;
  kind: string;
  fileBytes: number;
  uploadedBy: number;
  creator: { kind: "user" | "group"; id: number };
  state: string;
  assetId: number | null;
  message: string | null;
  canRetry: boolean;
  canRemove: boolean;
  grantedUniverses: number[];
}

export interface AssetWorkspace {
  rows: AssetRow[];
  isUploading: boolean;
  isReadOnly: boolean;
  notice: string | null;
}

export interface AssetUniverse {
  id: number;
  name: string;
}

export async function listAssetWorkspace(): Promise<AssetWorkspace> {
  return invoke<AssetWorkspace>("list_asset_workspace");
}

export type AssetCreator = { kind: "user" | "group"; id: number };
export async function addAssetFiles(userId: number, options: { paths?: string[]; universeId?: number; creator?: AssetCreator } = {}): Promise<AssetWorkspace> {
  return invoke<AssetWorkspace>("add_asset_files", { userId, paths: options.paths ?? [], universeId: options.universeId ?? null, creator: options.creator ?? { kind: "user", id: userId } });
}

export async function uploadAssets(userId: number, rowIds: string[]): Promise<void> {
  return invoke<void>("upload_assets", { userId, rowIds });
}

export interface CreationRow { assetId: number; name: string; kind: string; updatedAt: string | null; thumbnailUrl: string | null }
export interface CreationPage { rows: CreationRow[]; nextCursor: string | null }
export async function listAssetCreators(userId: number): Promise<AssetUniverse[]> { return invoke("list_asset_creators", { userId }); }
export async function updateAssetRow(userId: number, edit: { rowId: string; name: string; kind: string; creator: AssetCreator }): Promise<AssetWorkspace> { return invoke("update_asset_row", { userId, edit }); }
export async function listAssetCreations(userId: number, options: { creator: AssetCreator; kind: string; cursor?: string }): Promise<CreationPage> { return invoke("list_asset_creations", { userId, ...options, cursor: options.cursor ?? null }); }
export async function grantAssetAccess(userId: number, universeId: number, assetIds: number[]): Promise<{ granted: number[]; failures: Array<number | null>; notice: string | null }> { return invoke("grant_asset_access", { userId, universeId, assetIds }); }
export async function revealAssetFile(rowId: string): Promise<void> { return invoke("reveal_asset_file", { rowId }); }

export async function changeAssetQueue(userId: number, action: "clearFinished" | "retry" | "remove", options: { rowId?: string } = {}): Promise<AssetWorkspace> {
  return invoke<AssetWorkspace>("change_asset_queue", { userId, action, rowId: options.rowId ?? null });
}

export async function listAssetUniverses(userId: number): Promise<AssetUniverse[]> {
  return invoke<AssetUniverse[]>("list_asset_universes", { userId });
}

export async function searchGroups(
  keyword: string,
): Promise<GroupSearchResult[]> {
  return invoke<GroupSearchResult[]>("search_groups", { keyword });
}

export async function loadGroup(
  groupId: number,
  userIds: number[],
): Promise<GroupWorkspace> {
  return invoke<GroupWorkspace>("load_group", { groupId, userIds });
}

export async function changeGroupMembership(
  groupId: number,
  join: boolean,
  userIds: number[],
): Promise<GroupMembershipResult[]> {
  return invoke<GroupMembershipResult[]>("change_group_membership", {
    groupId,
    join,
    userIds,
  });
}

export async function openGroupChallenge(
  groupId: number,
  userId: number,
): Promise<void> {
  return invoke<void>("open_group_challenge", { groupId, userId });
}

export async function listAccountGroupColors(): Promise<
  Record<string, [number, number, number]>
> {
  return invoke<Record<string, [number, number, number]>>(
    "list_account_group_colors",
  );
}

export async function listAccountGroups(): Promise<AccountGroupSummary[]> {
  return invoke<AccountGroupSummary[]>("list_account_groups");
}

export async function reorderAccountGroups(
  names: string[],
): Promise<AccountGroupSummary[]> {
  return invoke<AccountGroupSummary[]>("reorder_account_groups", { names });
}

export async function getStoreStatus(): Promise<StoreStatus> {
  return invoke<StoreStatus>("store_status");
}

export async function unlockDevice(): Promise<StoreStatus> {
  return invoke<StoreStatus>("unlock_device");
}

export async function createDeviceStore(): Promise<StoreStatus> {
  return invoke<StoreStatus>("create_device_store");
}

export async function unlockPassword(password: string): Promise<StoreStatus> {
  return invoke<StoreStatus>("unlock_password", { password });
}

export async function updateAccountAlias(
  userId: number,
  alias: string,
): Promise<AccountSummary> {
  return invoke<AccountSummary>("update_account_alias", { userId, alias });
}

export async function toggleAccountPin(
  userId: number,
): Promise<AccountSummary> {
  return invoke<AccountSummary>("toggle_account_pin", { userId });
}

export async function updateAccountGroup(
  userId: number,
  group: string,
): Promise<AccountSummary> {
  return invoke<AccountSummary>("update_account_group", { userId, group });
}

export async function reorderAccounts(
  userIds: number[],
): Promise<AccountSummary[]> {
  return invoke<AccountSummary[]>("reorder_accounts", { userIds });
}

export async function updatePlayerPath(
  userId: number,
  path: string | null,
): Promise<AccountSummary> {
  return invoke<AccountSummary>("update_player_path", { userId, path });
}

export async function createAccountGroup(name: string): Promise<void> {
  return invoke<void>("create_account_group", { name });
}

export async function deleteAccountGroup(name: string): Promise<void> {
  return invoke<void>("delete_account_group", { name });
}

export async function updateAccountGroupMeta(
  oldName: string,
  newName: string,
  color: [number, number, number],
): Promise<AccountGroupSummary[]> {
  return invoke<AccountGroupSummary[]>("update_account_group_meta", {
    oldName,
    newName,
    color,
  });
}

export async function openAccountUrl(
  userId: number,
  inventory: boolean,
): Promise<void> {
  return invoke<void>("open_account_url", { userId, inventory });
}

export async function openAccountGuide(): Promise<void> {
  return invoke<void>("open_account_guide");
}

export async function removeAccount(userId: number): Promise<void> {
  return invoke<void>("remove_account", { userId });
}

export async function launchAccount(
  userId: number,
  placeId: number,
  jobId: string,
  data: string,
): Promise<void> {
  return invoke<void>("launch_account", {
    userId,
    placeId,
    jobId: jobId.trim() || null,
    data: data.trim() || null,
  });
}

export async function fetchAccountInventory(
  userId: number,
  forceRefresh = false,
): Promise<InventoryItem[]> {
  return (await fetchInventorySnapshot(userId, forceRefresh)).items;
}

export interface InventorySnapshot {
  items: InventoryItem[];
  cached: boolean;
}

export async function fetchInventorySnapshot(userId: number, forceRefresh = false): Promise<InventorySnapshot> {
  return invoke<InventorySnapshot>("fetch_account_inventory", { userId, forceRefresh });
}

export type InventoryBrowserTarget = {
  userId: number;
  assetId: number;
};

export async function openInventoryAssets(targets: InventoryBrowserTarget[]): Promise<void> {
  return invoke<void>("open_inventory_assets", { targets });
}

export async function searchConnectionUsers(
  keyword: string,
): Promise<UserSearchResult[]> {
  return invoke<UserSearchResult[]>("search_connection_users", { keyword });
}

export async function refreshAccountPresence(
  userIds: number[],
): Promise<PresenceUpdate[]> {
  return invoke<PresenceUpdate[]>("refresh_account_presence", { userIds });
}

export async function revalidateAccounts(
  userIds: number[],
): Promise<AccountSummary[]> {
  return invoke<AccountSummary[]>("revalidate_accounts", { userIds });
}

export async function killAllAccounts(): Promise<number> {
  return invoke<number>("kill_all_accounts");
}

export async function arrangeAccountWindows(): Promise<void> {
  return invoke<void>("arrange_account_windows");
}

export async function runConnectionAction(
  userId: number,
  targetUserId: number,
  action: string,
): Promise<void> {
  return invoke<void>("connection_action", { userId, targetUserId, action });
}

export async function joinUserGame(
  userId: number,
  targetUserId: number,
): Promise<void> {
  return invoke<void>("join_user_game", { userId, targetUserId });
}

export async function addAccount(cookie: string): Promise<AccountAddition> {
  return invoke<AccountAddition>("add_account", { cookie });
}

export async function addAccountAnyway(
  cookie: string,
  username: string,
): Promise<AccountAddition> {
  return invoke<AccountAddition>("add_account_anyway", { cookie, username });
}

export async function loginAndAddAccount(): Promise<AccountAddition | null> {
  return invoke<AccountAddition | null>("login_and_add_account");
}

export async function browseAsAccount(
  userId: number,
  inventory = false,
  profileUserId: number | null = null,
): Promise<void> {
  return invoke<void>("browse_as_account", { userId, inventory, profileUserId });
}

export type AccountExportFormat = "json" | "csv" | "txt";
export type AccountExportEncryption = "none" | "aes256Gcm" | "aes128Gcm";
export type AccountExportField = "username" | "accountName" | "displayName" | "userId" | "alias" | "group" | "createdAt" | "accountAge" | "lastUsed" | "cookie";
export interface AccountExportOptions { accountIds: number[]; fields: AccountExportField[]; format: AccountExportFormat; encryption: AccountExportEncryption; password: string; }
export function exportAccounts(options: AccountExportOptions): Promise<{count: number; encrypted: boolean} | null> {
  return invoke("export_accounts", { options });
}

export async function openAccountHome(userId: number): Promise<void> {
  return invoke<void>("open_account_home", { userId });
}

export async function saveLaunchPreset(
  name: string,
  placeId: number,
  jobId: string,
  data: string,
  tagIds: string[] = [],
): Promise<void> {
  return invoke<void>("save_launch_preset", {
    name,
    tagIds,
    placeId,
    jobId: jobId.trim() || null,
    data: data.trim() || null,
  });
}

export async function listLaunchPresets(): Promise<LaunchPresetSummary[]> {
  return invoke<LaunchPresetSummary[]>("list_launch_presets");
}

export async function updateLaunchPreset(
  index: number,
  name: string,
  placeId: number,
  jobId: string,
  data: string,
  tagIds: string[] = [],
): Promise<LaunchPresetSummary> {
  return invoke<LaunchPresetSummary>("update_launch_preset", {
    index,
    tagIds,
    name,
    placeId,
    jobId: jobId.trim() || null,
    data: data.trim() || null,
  });
}

export async function removeLaunchPreset(index: number): Promise<void> {
  return invoke<void>("remove_launch_preset", { index });
}

export async function launchLaunchPreset(
  index: number,
  userIds: number[],
): Promise<void> {
  return invoke<void>("launch_launch_preset", { index, userIds });
}

export async function getSettings(): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("get_settings");
}

export async function browseRobloxDirectory(): Promise<string | null> {
  return invoke<string | null>("browse_roblox_directory");
}

export async function saveSettings(
  settings: SettingsUpdate,
): Promise<SettingsConfig> {
  return invoke<SettingsConfig>("save_settings", { settings });
}

export async function setStartupWithWindows(enabled: boolean): Promise<void> {
  return invoke<void>("set_startup_with_windows", {
    change: { enabled },
  });
}

export async function enableMultiInstance(): Promise<void> {
  return invoke<void>("enable_multi_instance");
}

export async function arrangeSettingsWindows(
  options: TilingOptions,
): Promise<void> {
  return invoke<void>("arrange_settings_windows", { options });
}

export async function rotateMacAddress(
  preserveOui: boolean,
  alternateOui: string,
): Promise<void> {
  return invoke<void>("rotate_mac_address", {
    rotation: { preserveOui, alternateOui },
  });
}

export async function openDataFolder(): Promise<void> {
  return invoke<void>("open_data_folder");
}

export async function openPresetsFolder(): Promise<void> {
  return invoke<void>("open_presets_folder");
}

export async function cleanOrphanedData(): Promise<number> {
  return invoke<number>("clean_orphaned_data");
}

export async function clearApplicationCaches(): Promise<number> {
  return invoke<number>("clear_application_caches");
}

export async function restartApp(): Promise<void> {
  return invoke<void>("restart_app");
}

export async function saveDiscordWebhook(url: string): Promise<void> {
  return invoke<void>("save_discord_webhook", { url });
}

export async function removeDiscordWebhook(): Promise<void> {
  return invoke<void>("remove_discord_webhook");
}

export async function testDiscordWebhook(url: string): Promise<void> {
  return invoke<void>("test_discord_webhook", { url });
}

export async function changePassword(newPassword: string): Promise<void> {
  return invoke<void>("change_password", { newPassword });
}

export async function clearPassword(): Promise<void> {
  return invoke<void>("clear_password");
}

export interface AccountAddition {
  account: AccountSummary;
  confirmationId: string | null;
  isReplacement: boolean;
}

export async function confirmAccountAddition(confirmationId: string): Promise<AccountSummary> {
  return invoke<AccountSummary>("confirm_account_addition", { confirmationId });
}

export async function cancelAccountAddition(confirmationId: string): Promise<void> {
  return invoke<void>("cancel_account_addition", { confirmationId });
}

export interface InstanceSummary {
  pid: number;
  startTime: number;
  userId: number | null;
  label: string;
  placeId: number | null;
  jobId: string | null;
  attribution: "exact" | "inferred" | "unmatched";
  launchedAt: string | null;
}

export interface InstanceWorkspace {
  instances: InstanceSummary[];
  runningCount: number;
  startingCount: number;
}

export interface ClientLaunchSettings {
  enabled: boolean;
  fps: number;
  graphics: number;
  fullscreen: boolean;
  muted: boolean;
}

export function getClientLaunchSettings(): Promise<ClientLaunchSettings> {
  return invoke<ClientLaunchSettings>("get_client_launch_settings");
}

export function saveClientLaunchSettings(settings: ClientLaunchSettings): Promise<ClientLaunchSettings> {
  return invoke<ClientLaunchSettings>("save_client_launch_settings", { settings });
}

export interface LaunchProgress {
  requestId: string;
  userId: number;
  phase: "waiting" | "authenticating" | "launching" | "requested" | "failed";
}

export async function listInstances(): Promise<InstanceWorkspace> {
  return invoke<InstanceWorkspace>("list_instances");
}

export async function focusInstance(instance: InstanceSummary): Promise<void> {
  return invoke<void>("focus_instance", { pid: instance.pid, startTime: instance.startTime });
}

export async function killInstance(instance: InstanceSummary): Promise<void> {
  return invoke<void>("kill_instance", { pid: instance.pid, startTime: instance.startTime });
}

export function operationError(error: unknown, fallback: string): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : fallback;
}

export interface StartupStatus {
  needsTutorial: boolean;
  changelog: string | null;
  passwordlessOffer: boolean;
  legacyMigrationAvailable: boolean;
  migrationNotice: string | null;
}
export async function startupStatus(): Promise<StartupStatus> { return invoke("startup_status"); }
export async function acknowledgeStartup(section: "version" | "passwordless"): Promise<void> { return invoke("acknowledge_startup", { section }); }
export async function migrateLegacyData(): Promise<void> { return invoke("migrate_legacy_data"); }
export async function resetAccountStore(confirmation: string): Promise<StoreStatus> { return invoke("reset_account_store", { confirmation }); }
export async function checkReleaseUpdate(): Promise<[string, string] | null> { return invoke("check_release_update"); }
export async function openReleasePage(url: string): Promise<void> { return invoke("open_release_page", { url }); }

export async function loggingTargets(): Promise<string[]> { return invoke<string[]>("logging_targets"); }

export interface PrivateServerCheckSnapshot { failed: boolean; checks: Record<number, PrivateServerCheck>; checking: boolean; }
export async function cachedPrivateServerChecks(): Promise<PrivateServerCheckSnapshot> { return invoke<PrivateServerCheckSnapshot>("cached_private_server_checks"); }

export interface FriendUser { userId: number; username: string; displayName: string; avatarUrl: string; }
export type FriendStatus = "playing" | "studio" | "online" | "offline" | "unknown";
export interface Friend extends FriendUser { status: FriendStatus; activity: string; placeId: number | null; jobId: string | null; gameName: string | null; placeName: string | null; gameIcon: string | null; }
export interface FriendsSnapshot { friends: Friend[]; refreshedAt: string | null; warnings: string[]; }
export interface FriendRequestsPage { users: FriendUser[]; nextCursor: string | null; warnings: string[]; }
export type FriendDetailTab = "details" | "mutual" | "groups";
export interface FriendDetails { profile: {description: string; created: string | null} | null; mutual: FriendUser[]; groups: {id: number; name: string; role: string}[]; }
export type FriendAction = "accept" | "decline" | "remove" | "block";
export function getFriends(userId: number, refresh = false): Promise<FriendsSnapshot> { return invoke("get_friends", {userId, refresh}); }
export function getFriendRequests(userId: number, cursor: string | null = null, refresh = false): Promise<FriendRequestsPage> { return invoke("get_friend_requests", {userId, cursor, refresh}); }
export function getFriendDetails(userId: number, targetId: number, section: FriendDetailTab): Promise<FriendDetails> { return invoke("get_friend_details", {userId, targetId, section}); }
export function friendAction(userId: number, targetId: number, action: FriendAction): Promise<void> { return invoke("friend_action", {userId, targetId, action}); }
export function joinFriendGame(userId: number, targetId: number, expectedJobId: string | null, reuseConfirmed: boolean, requireNewInstance = false): Promise<void> { return invoke("join_friend_game", {userId, targetId, expectedJobId, reuseConfirmed, requireNewInstance}); }
