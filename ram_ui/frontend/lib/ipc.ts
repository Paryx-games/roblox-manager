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

export async function listAccounts(): Promise<AccountSummary[]> {
  return invoke<AccountSummary[]>("list_accounts");
}

export async function listAccountGroupColors(): Promise<
  Record<string, [number, number, number]>
> {
  return invoke<Record<string, [number, number, number]>>(
    "list_account_group_colors",
  );
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

export async function addAccount(cookie: string): Promise<AccountSummary> {
  return invoke<AccountSummary>("add_account", { cookie });
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
