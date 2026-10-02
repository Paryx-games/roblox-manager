export type OperationPage = "Accounts" | "Instances" | "Groups" | "Private Servers" | "Presets" | "Asset Manager" | "Settings";
export type OperationStatus = "pending" | "success" | "failed" | "partial" | "requested";
export type Operation = { id: string; title: string; page: OperationPage; status: OperationStatus; detail: string; updatedAt: number };

const commands: Record<string, [string, OperationPage]> = {
  add_account: ["Validate account addition", "Accounts"],
  add_account_anyway: ["Add account", "Accounts"],
  confirm_account_addition: ["Confirm account addition", "Accounts"],
  login_and_add_account: ["Browser sign-in", "Accounts"],
  revalidate_accounts: ["Validate accounts", "Accounts"],
  remove_account: ["Remove account", "Accounts"],
  update_account_alias: ["Rename account", "Accounts"],
  update_account_group: ["Move account", "Accounts"],
  create_account_group: ["Create account group", "Accounts"],
  delete_account_group: ["Delete account group", "Accounts"],
  update_account_group_meta: ["Edit account group", "Accounts"],
  launch_account: ["Launch account", "Accounts"],
  launch_private_server: ["Launch private server", "Private Servers"],
  launch_launch_preset: ["Launch preset", "Presets"],
  join_user_game: ["Join server", "Instances"],
  kill_all_accounts: ["Close Roblox clients", "Instances"],
  kill_instance: ["Close instance", "Instances"],
  arrange_account_windows: ["Arrange windows", "Instances"],
  save_launch_preset: ["Save preset", "Presets"],
  update_launch_preset: ["Edit preset", "Presets"],
  remove_launch_preset: ["Remove preset", "Presets"],
  add_private_server: ["Add private server", "Private Servers"],
  update_private_server: ["Edit private server", "Private Servers"],
  remove_private_server: ["Remove private server", "Private Servers"],
  change_group_membership: ["Change group membership", "Groups"],
  add_asset_files: ["Stage asset files", "Asset Manager"],
  upload_assets: ["Upload assets", "Asset Manager"],
  change_asset_queue: ["Update upload queue", "Asset Manager"],
  grant_asset_access: ["Grant experience access", "Asset Manager"],
  save_settings: ["Save settings", "Settings"],
  set_startup_with_windows: ["Change Windows startup", "Settings"],
  clear_application_caches: ["Clear application cache", "Settings"],
  clean_orphaned_data: ["Clean unused data", "Settings"],
  change_password: ["Change store password", "Settings"],
  clear_password: ["Use device encryption", "Settings"],
  save_discord_webhook: ["Save Discord integration", "Settings"],
  test_discord_webhook: ["Test Discord notification", "Settings"],
  remove_discord_webhook: ["Remove Discord integration", "Settings"],
};

let entries: Operation[] = [];
let nextId = 0;
const listeners = new Set<() => void>();
export const operationSnapshot = () => entries;
export function subscribeOperations(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; }

function upsert(operation: Operation) {
  const previous = entries.find((entry) => entry.id === operation.id);
  if (previous?.status === operation.status && previous.detail === operation.detail) return;
  entries = [operation, ...entries.filter((entry) => entry.id !== operation.id)].slice(0, 100);
  listeners.forEach((listener) => listener());
}

export function clearCompletedOperations() {
  entries = entries.filter((entry) => entry.status === "pending");
  listeners.forEach((listener) => listener());
}

export function beginOperation(command: string): string | null {
  const description = Object.prototype.hasOwnProperty.call(commands, command) ? commands[command] : undefined;
  if (!description) return null;
  const id = `action-${++nextId}`;
  upsert({ id, title: description[0], page: description[1], status: "pending", detail: "Working…", updatedAt: Date.now() });
  return id;
}

export function finishOperation(id: string | null, command: string, ok: boolean, result?: unknown) {
  if (!id) return;
  const description = Object.prototype.hasOwnProperty.call(commands, command) ? commands[command] : undefined;
  if (!description) return;
  let status: OperationStatus = ok ? "success" : "failed";
  let detail = ok ? "Request completed." : "Action failed. Open the workspace to review and retry.";
  if (ok && /^(launch_account|launch_private_server|launch_launch_preset|join_user_game|upload_assets)$/.test(command)) {
    status = "requested";
    detail = "Request accepted. See live progress and running clients for the result.";
  }
  // retain counts only; never copy request payloads, response text or errors
  if (ok && command === "change_group_membership" && Array.isArray(result)) {
    const failures = result.filter((item: unknown) => !item || typeof item !== "object" || !("ok" in item) || item.ok !== true).length;
    if (failures) { status = failures === result.length ? "failed" : "partial"; detail = `${result.length - failures} succeeded; ${failures} need attention.`; }
  }
  if (ok && command === "grant_asset_access" && result && typeof result === "object" && "failures" in result && Array.isArray(result.failures) && result.failures.length) {
    status = "partial";
    detail = `${result.failures.length} grants need attention. Open Asset Manager to review.`;
  }
  if (ok && command === "revalidate_accounts" && Array.isArray(result)) {
    const expired = result.filter((account: unknown) => account && typeof account === "object" && "cookieExpired" in account && account.cookieExpired === true).length;
    if (expired) { status = "partial"; detail = `${expired} accounts need sign-in. Open Accounts to review.`; }
  }
  upsert({ id, title: description[0], page: description[1], status, detail, updatedAt: Date.now() });
}

const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
export function recordLaunchProgress(requestId: string, phase: string) {
  if (!uuid.test(requestId)) return;
  const phases: Record<string, [OperationStatus, string]> = {
    waiting: ["pending", "Waiting for a launch slot."],
    authenticating: ["pending", "Authenticating the account."],
    launching: ["pending", "Starting Roblox."],
    requested: ["requested", "Roblox launch requested. Check Instances for the running client."],
    failed: ["failed", "Launch failed. Open Accounts to review and retry."],
  };
  const state = Object.prototype.hasOwnProperty.call(phases, phase) ? phases[phase] : undefined;
  if (!state) return;
  upsert({ id: `launch-${requestId}`, title: "Account launch progress", page: "Accounts", status: state[0], detail: state[1], updatedAt: Date.now() });
}

export function recordAssetProgress(rowId: string, state: string) {
  if (!uuid.test(rowId)) return;
  const states: Record<string, [OperationStatus, string]> = {
    uploading: ["pending", "Uploading a queued asset."],
    inReview: ["pending", "Waiting for Roblox moderation."],
    approved: ["success", "Asset approved."],
    rejected: ["failed", "Asset rejected. Open Asset Manager to review."],
    failed: ["failed", "Upload failed. Open Asset Manager to review and retry."],
  };
  const detail = Object.prototype.hasOwnProperty.call(states, state) ? states[state] : undefined;
  if (!detail) return;
  upsert({ id: `asset-${rowId}`, title: "Asset upload progress", page: "Asset Manager", status: detail[0], detail: detail[1], updatedAt: Date.now() });
}
