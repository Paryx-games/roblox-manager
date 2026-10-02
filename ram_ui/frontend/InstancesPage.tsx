import { useRef, useState } from "react";
import { AccountPicker } from "./components/AccountPicker";
import { Icon } from "./components/Icon";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import { ConfirmModal } from "./ConfirmModal";
import { Toast, type ToastItem } from "./Toast";
import { arrangeAccountWindows, focusInstance, getSettings, joinUserGame, killAllAccounts, killInstance, operationError, type AccountSummary, type InstanceSummary, type InstanceWorkspace, type LaunchProgress } from "./lib/ipc";

const phaseLabels = { waiting: "Waiting for launch slot", authenticating: "Authenticating", launching: "Starting Roblox", requested: "Launch requested", failed: "Launch failed" };

export function InstancesPage({ workspace, accounts, selectedIds, onSelectedIdsChange, launches, isLoading, error, onRefresh, onNavigateAccount, onNavigateSettings }: {
  workspace: InstanceWorkspace;
  accounts: AccountSummary[];
  selectedIds: Set<number>;
  onSelectedIdsChange: (ids: Set<number>) => void;
  launches: LaunchProgress[];
  isLoading: boolean;
  error: string | null;
  onRefresh: () => Promise<void>;
  onNavigateAccount: (id: number) => void;
  onNavigateSettings: () => void;
}) {
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [closeTarget, setCloseTarget] = useState<InstanceSummary | "all" | null>(null);
  const [toast, setToast] = useState<ToastItem | null>(null);
  const actionPending = useRef(false);
  const launchAccounts = accounts.filter((account) => account.canLaunch && !account.cookieExpired);

  async function runAction(action: string, operation: () => Promise<unknown>, success?: string) {
    if (actionPending.current) return;
    actionPending.current = true;
    setPendingAction(action);
    try {
      await operation();
      if (success) setToast({ id: Date.now(), title: success, message: "", kind: "success", duration: "standard" });
    } catch (error) {
      setToast({ id: Date.now(), title: "Instance action failed", message: operationError(error, "The action could not be completed. Refresh and try again."), kind: "error", duration: "long" });
    } finally {
      actionPending.current = false;
      setPendingAction(null);
    }
  }

  async function requestKillAll() {
    await runAction("check-confirmation", async () => {
      const settings = await getSettings();
      if (settings.config.confirmKillAll) setCloseTarget("all");
      else {
        await killAllAccounts();
        await onRefresh();
        setToast({ id: Date.now(), title: "Roblox clients closed", message: "", kind: "success", duration: "standard" });
      }
    });
  }

  function confirmClose() {
    const target = closeTarget;
    setCloseTarget(null);
    if (!target) return;
    void runAction("close", async () => {
      if (target === "all") await killAllAccounts();
      else await killInstance(target);
      await onRefresh();
    }, target === "all" ? "Roblox clients closed" : "Instance closed");
  }

  async function joinServer(target: InstanceSummary) {
    if (target.userId === null) return;
    const targetUserId = target.userId;
    const userIds = launchAccounts.filter((account) => selectedIds.has(account.userId) && account.userId !== targetUserId).map((account) => account.userId);
    await runAction(`join-${target.pid}`, async () => {
      for (const userId of userIds) await joinUserGame(userId, targetUserId);
    }, `${userIds.length} server join${userIds.length === 1 ? "" : "s"} requested`);
  }

  return <>
    <div className="header-row"><h1 className="header-title">Instances</h1></div>
    <main className="instances-page assets-page" data-walkthrough="instances">
      <div className="assets-main">
        <div className="assets-toolbar instances-toolbar">
          <button className="account-button" type="button" onClick={onNavigateSettings}><Icon name="settings" />Launch settings</button>
          <span className="instances-count" role="status">{isLoading ? "Checking running clients..." : `${workspace.runningCount} Roblox client${workspace.runningCount === 1 ? "" : "s"} running`}</span>
          <div className="instances-toolbar-actions">
          <button className="account-button" type="button" disabled={isLoading || pendingAction !== null} onClick={() => void onRefresh()}><Icon name="refresh" />{isLoading ? "Refreshing..." : "Refresh"}</button>
          <button className="account-button" type="button" disabled={!workspace.runningCount || pendingAction !== null || isLoading} onClick={() => void runAction("arrange", arrangeAccountWindows, "Windows arranged")}><Icon name="grid" />{pendingAction === "arrange" ? "Arranging..." : "Arrange windows"}</button>
          <button className="account-button" type="button" disabled={!workspace.runningCount || pendingAction !== null || isLoading} onClick={() => void requestKillAll()}><Icon name="kill" />{pendingAction === "check-confirmation" || pendingAction === "close" ? "Working..." : "Kill all Roblox"}</button>
          </div>
        </div>
        <div className="assets-toolbar">
          <span className="instances-picker-label">Join server as</span>
          <AccountPicker accounts={launchAccounts} mode="multiple" open={isPickerOpen} onOpenChange={setIsPickerOpen} selectedIds={selectedIds} onSelectedIdsChange={onSelectedIdsChange} disabled={pendingAction !== null || !launchAccounts.length} unselectedLabel="Select accounts to join a server" />
          <span className="instances-description">{launchAccounts.length ? "Choose accounts, then use Join server on a matched client." : "Add a valid account in Accounts to join a client's server."}</span>
        </div>
        {launches.length > 0 && <div className="assets-notice" role="status">{launches.map((launch) => <p key={launch.requestId}>{accounts.find((account) => account.userId === launch.userId)?.label ?? "Account"}: {phaseLabels[launch.phase]}</p>)}</div>}
        <div className="assets-panel">
          <table className="assets-table">
            <caption className="instances-caption">Running clients</caption>
            <thead><tr><th scope="col">Account</th><th scope="col">PID</th><th scope="col" className="assets-secondary-column">Place ID</th><th scope="col">Match</th><th scope="col" className="assets-secondary-column">Launched</th><th scope="col" className="instances-actions-column">Actions</th></tr></thead>
            <tbody>
              {isLoading ? <tr><td colSpan={6}><LoadingSkeleton layout="results" label="Loading running instances" /></td></tr> : error ? <tr><td colSpan={6}><div role="alert"><p className="selectable-text">{error}</p><button className="account-button" type="button" onClick={() => void onRefresh()}><Icon name="refresh" />Retry</button></div></td></tr> : workspace.instances.length === 0 ? <tr><td colSpan={6}>No Roblox clients running. Launch an account from Accounts, Presets or Private Servers to track it here.</td></tr> : workspace.instances.map((instance) => {
                const canJoin = instance.userId !== null && launchAccounts.some((account) => selectedIds.has(account.userId) && account.userId !== instance.userId);
                const launchedAt = instance.launchedAt ? new Date(instance.launchedAt).toLocaleString() : "Unknown";
                return <tr key={`${instance.pid}:${instance.startTime}`}>
                  <td>{instance.userId !== null ? <button className="assets-copy" type="button" aria-label={`View account ${instance.label}`} onClick={() => onNavigateAccount(instance.userId!)}>{instance.label}</button> : <strong>{instance.label}</strong>}{instance.userId !== null && <small className="selectable-text">{instance.userId}</small>}</td>
                  <td className="selectable-text">{instance.pid}</td>
                  <td className="assets-secondary-column selectable-text">{instance.placeId ?? "Unknown"}</td>
                  <td><span className="instances-attribution" tabIndex={0} data-attribution={instance.attribution} data-tip={instance.attribution === "exact" ? "Verified from this client's launch token. Individual close is available." : instance.attribution === "inferred" ? "Estimated from launch order. Individual close is disabled." : "RM could not identify this client's account. Individual close and server join are disabled."}>{instance.attribution === "exact" ? "Exact" : instance.attribution === "inferred" ? "Inferred" : "Unmatched"}</span></td>
                  <td className="assets-secondary-column">{launchedAt}</td>
                  <td className="instances-actions-column"><div className="assets-row-actions">
                    <button className="account-button" type="button" disabled={pendingAction !== null} onClick={() => void runAction(`focus-${instance.pid}`, () => focusInstance(instance), "Instance focused")}><Icon name="focus" />{pendingAction === `focus-${instance.pid}` ? "Focusing..." : "Focus"}</button>
                    <button className="account-button" type="button" disabled={!canJoin || pendingAction !== null} data-tip={instance.userId === null ? "Identify this client's account before joining its server" : !canJoin ? "Select another valid account above to join this client's server" : "Launch the selected accounts into this client's server"} onClick={() => void joinServer(instance)}><Icon name="launch" />{pendingAction === `join-${instance.pid}` ? "Joining..." : "Join server"}</button>
                    <button className="account-button" type="button" disabled={instance.attribution !== "exact" || pendingAction !== null} data-tip={instance.attribution === "exact" ? "Close this verified Roblox client" : "An exact account match is required to kill an individual client"} onClick={() => setCloseTarget(instance)}><Icon name="kill" />Kill</button>
                  </div></td>
                </tr>;
              })}
            </tbody>
          </table>
        </div>
      </div>
    </main>
    {closeTarget && <ConfirmModal title={closeTarget === "all" ? "Kill all Roblox clients?" : "Kill this instance?"} message={closeTarget === "all" ? "This closes every Roblox client, including clients not launched by RM." : `Close ${closeTarget.label}'s Roblox client (PID ${closeTarget.pid})? RM will verify the process again before closing it.`} confirmLabel="Kill" confirmIcon="kill" onConfirm={confirmClose} onCancel={() => setCloseTarget(null)} />}
    {toast && <Toast item={toast} onDismiss={() => setToast(null)} />}
  </>;
}
