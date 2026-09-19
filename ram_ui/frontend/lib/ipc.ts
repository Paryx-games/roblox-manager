import { invoke } from "@tauri-apps/api/core";

export type PresenceStatus = "online" | "warning" | "danger" | "neutral";

export interface AccountSummary {
  userId: number;
  label: string;
  username: string;
  displayName: string;
  alias: string;
  group: string;
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
}

export interface UserSearchResult {
  userId: number;
  username: string;
  displayName: string;
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

export interface LaunchPresetSummary {
  index: number;
  name: string;
  placeId: number;
  jobId: string | null;
  data: string | null;
}

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
  id: number;
  body: string;
  created: string | null;
  poster: GroupPoster | null;
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
  announcements: GroupAnnouncement[];
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

export interface SettingsConfig {
  useCredentialManager: boolean;
  startupWithWindows: boolean;
  refreshOnStartup: boolean;
  autoLaunchOnStartup: boolean;
  autoLaunchAccountId: number | null;
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
  developerOptions: boolean;
  utilityEnabled: boolean;
  logLevel: LogLevel;
}

export type SettingsUpdate = Omit<
  SettingsConfig,
  "startupWithWindows" | "robloxFastFlags"
>;

export interface SettingsInfoCard {
  kind: "info" | "warning" | "caution" | string;
  text: string;
}

export interface SettingsSnapshot {
  config: SettingsConfig;
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
): Promise<InventoryItem[]> {
  return invoke<InventoryItem[]>("fetch_account_inventory", { userId });
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

export async function addAccount(cookie: string): Promise<AccountSummary> {
  return invoke<AccountSummary>("add_account", { cookie });
}

export async function addAccountAnyway(
  cookie: string,
  username: string,
): Promise<AccountSummary> {
  return invoke<AccountSummary>("add_account_anyway", { cookie, username });
}

export async function loginAndAddAccount(): Promise<AccountSummary | null> {
  return invoke<AccountSummary | null>("login_and_add_account");
}

export async function browseAsAccount(
  userId: number,
  inventory = false,
): Promise<void> {
  return invoke<void>("browse_as_account", { userId, inventory });
}

export async function saveLaunchPreset(
  name: string,
  placeId: number,
  jobId: string,
  data: string,
): Promise<void> {
  return invoke<void>("save_launch_preset", {
    name,
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
): Promise<LaunchPresetSummary> {
  return invoke<LaunchPresetSummary>("update_launch_preset", {
    index,
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
