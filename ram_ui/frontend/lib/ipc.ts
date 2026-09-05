import { invoke } from "@tauri-apps/api/core";

export type PresenceStatus = "online" | "warning" | "danger" | "neutral";

export interface AccountSummary {
  userId: number;
  label: string;
  username: string;
  displayName: string;
  presence: PresenceStatus;
  presenceText: string;
  canLaunch: boolean;
  lastActivity: string | null;
}

export async function listAccounts(): Promise<AccountSummary[]> {
  return invoke<AccountSummary[]>("list_accounts");
}
