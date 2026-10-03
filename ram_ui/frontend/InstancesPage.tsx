import { useEffect, useRef, useState } from "react";
import { AccountPicker } from "./components/AccountPicker";
import { InstanceIdentity, InstanceRow, instanceKey } from "./components/InstanceRow";
import { RangeField, SettingRow, ToggleField } from "./components/SettingRow";
import Select from "./components/Select";
import { Icon } from "./components/Icon";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import { ConfirmModal } from "./ConfirmModal";
import { Toast, type ToastItem } from "./Toast";
import { arrangeAccountWindows, focusInstance, getSettings, joinUserGame, killAllAccounts, killInstance, operationError, type AccountSummary, type InstanceSummary, type InstanceWorkspace, type LaunchProgress } from "./lib/ipc";

const phaseLabels = { waiting: "Waiting for launch slot", authenticating: "Authenticating", launching: "Starting Roblox", requested: "Launch requested", failed: "Launch failed" };

type ClientPreview = { fps: string; graphics: number; fullscreen: boolean; muted: boolean };
const defaultPreview: ClientPreview = { fps: "60", graphics: 5, fullscreen: false, muted: false };
const fpsOptions = ["30", "60", "120", "144", "240"].map((value) => ({ value, label: `${value} FPS` }));

export function InstancesPage({ workspace, accounts, selectedIds, onSelectedIdsChange, launches, isLoading, error, onRefresh }: {
  workspace: InstanceWorkspace;
  accounts: AccountSummary[];
  selectedIds: Set<number>;
  onSelectedIdsChange: (ids: Set<number>) => void;
  launches: LaunchProgress[];
  isLoading: boolean;
  error: string | null;
  onRefresh: () => Promise<void>;
}) {
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [closeTarget, setCloseTarget] = useState<InstanceSummary | "all" | null>(null);
  const [toast, setToast] = useState<ToastItem | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [previews, setPreviews] = useState<Record<string, ClientPreview>>({});
  const actionPending = useRef(false);
  const launchAccounts = accounts.filter((account) => account.canLaunch && !account.cookieExpired);
  const selectedInstance = workspace.instances.find((instance) => instanceKey(instance) === selectedKey);
  const preview = selectedKey ? previews[selectedKey] ?? defaultPreview : defaultPreview;
  const controlsDisabled = isLoading || error !== null;

  useEffect(() => {
    if (isLoading || error) return;
    const liveKeys = new Set(workspace.instances.map(instanceKey));
    setSelectedKey((current) => current && liveKeys.has(current) ? current : null);
    setPreviews((current) => {
      const entries = Object.entries(current).filter(([key]) => liveKeys.has(key));
      return entries.length === Object.keys(current).length ? current : Object.fromEntries(entries);
    });
  }, [workspace.instances, isLoading, error]);

  function updatePreview(update: Partial<ClientPreview>) {
    if (!selectedInstance || controlsDisabled) return;
    const key = instanceKey(selectedInstance);
    setPreviews((current) => ({ ...current, [key]: { ...defaultPreview, ...current[key], ...update } }));
  }

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
    <main className="instances-page assets-page" data-walkthrough="instances">
      <div className="assets-main">
        <div className="assets-toolbar instances-toolbar">
          <div className="instances-heading"><h1 className="header-title">Instances</h1><p className="instances-description">Manage and control your running Roblox clients.</p></div>
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
          <span className="instances-count" role="status">{isLoading ? "Checking clients..." : error ? "Client status unavailable" : `${workspace.runningCount} client${workspace.runningCount === 1 ? "" : "s"} running`}</span>
        </div>
        {launches.length > 0 && <div className="assets-notice" role="status">{launches.map((launch) => <p key={launch.requestId}>{accounts.find((account) => account.userId === launch.userId)?.label ?? "Account"}: {phaseLabels[launch.phase]}</p>)}</div>}
        <div className="instances-workspace">
          <section className="instances-list-panel settings-section" aria-labelledby="running-clients-heading" aria-busy={isLoading}>
            <h2 id="running-clients-heading">Running clients{!isLoading && !error && ` (${workspace.instances.length})`}</h2>
            {isLoading ? <LoadingSkeleton layout="results" label="Loading running instances" /> : error ? <div className="instances-empty" role="alert"><p className="selectable-text">{error}</p><button className="account-button" type="button" onClick={() => void onRefresh()}><Icon name="refresh" />Retry</button></div> : workspace.instances.length === 0 ? <div className="instances-empty"><Icon name="app-window" /><strong>No Roblox clients running</strong><p>Launch an account from Accounts, Presets or Private Servers to track it here.</p></div> : <ul className="instances-client-list">{workspace.instances.map((instance) => {
                const canJoin = instance.userId !== null && launchAccounts.some((account) => selectedIds.has(account.userId) && account.userId !== instance.userId);
                return <InstanceRow key={instanceKey(instance)} instance={instance} account={accounts.find((account) => account.userId === instance.userId)} selected={selectedKey === instanceKey(instance)} onSelect={() => setSelectedKey(instanceKey(instance))} actions={<>
                    <button className="account-button" type="button" disabled={pendingAction !== null} onClick={() => void runAction(`focus-${instance.pid}`, () => focusInstance(instance), "Instance focused")}><Icon name="focus" />{pendingAction === `focus-${instance.pid}` ? "Focusing..." : "Focus"}</button>
                    <button className="account-button" type="button" disabled={!canJoin || pendingAction !== null} data-tip={instance.userId === null ? "Identify this client's account before joining its server" : !canJoin ? "Select another valid account above to join this client's server" : "Launch the selected accounts into this client's server"} onClick={() => void joinServer(instance)}><Icon name="launch" />{pendingAction === `join-${instance.pid}` ? "Joining..." : "Join server"}</button>
                    <button className="account-button" type="button" disabled={instance.attribution !== "exact" || pendingAction !== null} data-tip={instance.attribution === "exact" ? "Close this verified Roblox client" : "An exact account match is required to kill an individual client"} onClick={() => setCloseTarget(instance)}><Icon name="kill" />Kill</button>
                  </>} />;
              })}</ul>}
          </section>
          <section className="instances-controls-panel settings-section" aria-labelledby="client-controls-heading">
            <h2 id="client-controls-heading">Client controls</h2>
            <p className="instances-description">Adjust settings for the selected client.</p>
            {selectedInstance ? <>
              <InstanceIdentity instance={selectedInstance} account={accounts.find((account) => account.userId === selectedInstance.userId)} />
              <div className="instances-preview-notice" id="client-preview-notice" role="note"><Icon name="info-mark" tone="current-color" /><p><strong>Preview only — controls do not work yet.</strong> Changes do not affect Roblox and are not saved. Values are temporary for each client while this page is open.</p></div>
              <div role="group" aria-label="Client settings preview" aria-describedby="client-preview-notice">
                <SettingRow label="FPS limit" description="Choose a maximum frame rate.">{() => <Select value={preview.fps} options={fpsOptions} onChange={(fps) => updatePreview({ fps })} ariaLabel="FPS limit (preview only)" disabled={controlsDisabled} />}</SettingRow>
                <SettingRow label="Graphics quality" description="Choose a quality level from 1 to 10.">{(labelId, descriptionId) => <RangeField min={1} max={10} value={preview.graphics} onChange={(graphics) => updatePreview({ graphics })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
                <SettingRow label="Fullscreen" description="Show the client in fullscreen.">{(labelId, descriptionId) => <ToggleField checked={preview.fullscreen} onChange={(fullscreen) => updatePreview({ fullscreen })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
                <SettingRow label="Muted audio" description="Mute this client's audio output.">{(labelId, descriptionId) => <ToggleField checked={preview.muted} onChange={(muted) => updatePreview({ muted })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
              </div>
            </> : <div className="instances-empty"><Icon name="settings" /><strong>Select a client</strong><p>Choose a running client to view its controls.</p></div>}
          </section>
        </div>
      </div>
    </main>
    {closeTarget && <ConfirmModal title={closeTarget === "all" ? "Kill all Roblox clients?" : "Kill this instance?"} message={closeTarget === "all" ? "This closes every Roblox client, including clients not launched by RM." : `Close ${closeTarget.label}'s Roblox client (PID ${closeTarget.pid})? RM will verify the process again before closing it.`} confirmLabel="Kill" confirmIcon="kill" onConfirm={confirmClose} onCancel={() => setCloseTarget(null)} />}
    {toast && <Toast item={toast} onDismiss={() => setToast(null)} />}
  </>;
}
