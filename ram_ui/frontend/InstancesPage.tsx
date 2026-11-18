import { useEffect, useRef, useState } from "react";
import { AccountPicker } from "./components/AccountPicker";
import { InstanceIdentity, InstanceRow, instanceKey } from "./components/InstanceRow";
import { RangeField, RangeNumberField, SettingRow, ToggleField } from "./components/SettingRow";
import Select from "./components/Select";
import { Icon } from "./components/Icon";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import { ConfirmModal } from "./ConfirmModal";
import { Toast, type ToastItem } from "./Toast";
import { arrangeAccountWindows, focusInstance, getSettings, getClientLaunchSettings, getResourceMonitorSettings, saveClientLaunchSettings, saveResourceMonitorSettings, joinUserGame, killAllAccounts, killInstance, operationError, type AccountSummary, type ClientLaunchSettings, type InstanceSummary, type InstanceWorkspace, type LaunchProgress, type ResourceMonitorSettings, type ResourceMonitorSettingsUpdate } from "./lib/ipc";
import { hasResourceMonitorSettingsChanged } from "./lib/resourceMonitorSettings";

const phaseLabels: Record<LaunchProgress["phase"], string> = {
  queued: "Queued",
  starting: "Starting Roblox",
  retrying: "Retrying Roblox request",
  started: "Roblox started",
  failed: "Launch failed",
  blocked: "Launch blocked",
};

const defaultSettings: ClientLaunchSettings = { enabled: false, rmOnly: true, fps: 60, graphics: 5, fullscreen: false, muted: false };
const defaultResourceSettings: ResourceMonitorSettingsUpdate = { cpuLimitPercent: null, memoryLimitMb: null, cpuAction: "warn", memoryAction: "warn" };
const fpsOptions = ["30", "60", "120", "144", "240"].map((value) => ({ value, label: `${value} FPS` }));
const cpuActionOptions = [{ value: "warn", label: "Warn" }, { value: "throttle", label: "Throttle" }, { value: "kill", label: "Kill" }];
const memoryActionOptions = [{ value: "warn", label: "Warn" }, { value: "kill", label: "Kill" }];

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
  const [draft, setDraft] = useState<ClientLaunchSettings>(defaultSettings);
  const [savedSettings, setSavedSettings] = useState<ClientLaunchSettings>(defaultSettings);
  const [settingsLoading, setSettingsLoading] = useState(true);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [settingsRevision, setSettingsRevision] = useState(0);
  const [resourceDraft, setResourceDraft] = useState<ResourceMonitorSettingsUpdate>(defaultResourceSettings);
  const [resourceSaved, setResourceSaved] = useState<ResourceMonitorSettingsUpdate>(defaultResourceSettings);
  const [resourceMaximum, setResourceMaximum] = useState<number | null>(null);
  const [resourceLoading, setResourceLoading] = useState(true);
  const [resourceError, setResourceError] = useState<string | null>(null);
  const [resourceRevision, setResourceRevision] = useState(0);
  const actionPending = useRef(false);
  const launchAccounts = accounts.filter((account) => account.canLaunch && !account.cookieExpired);
  const selectedInstance = workspace.instances.find((instance) => instanceKey(instance) === selectedKey);
  const controlsDisabled = settingsLoading || settingsError !== null || pendingAction !== null;
  const hasSettingsChanges = JSON.stringify(draft) !== JSON.stringify(savedSettings);
  const hasResourceChanges = hasResourceMonitorSettingsChanged(resourceDraft, resourceSaved);

  useEffect(() => {
    let cancelled = false;
    setSettingsLoading(true);
    setSettingsError(null);
    void getClientLaunchSettings().then((settings) => {
      if (cancelled) return;
      setDraft(settings);
      setSavedSettings(settings);
    }).catch((failure) => {
      if (!cancelled) setSettingsError(operationError(failure, "Client launch settings could not be loaded."));
    }).finally(() => { if (!cancelled) setSettingsLoading(false); });
    return () => { cancelled = true; };
  }, [settingsRevision]);

  useEffect(() => {
    let cancelled = false;
    setResourceLoading(true);
    setResourceError(null);
    void getResourceMonitorSettings().then((settings: ResourceMonitorSettings) => {
      if (cancelled) return;
      const { systemMemoryLimitMb, ...editable } = settings;
      setResourceMaximum(systemMemoryLimitMb);
      setResourceDraft(editable);
      setResourceSaved(editable);
    }).catch((failure) => {
      if (!cancelled) setResourceError(operationError(failure, "Resource monitoring settings could not be loaded."));
    }).finally(() => { if (!cancelled) setResourceLoading(false); });
    return () => { cancelled = true; };
  }, [resourceRevision]);

  useEffect(() => {
    if (isLoading || error) return;
    const liveKeys = new Set(workspace.instances.map(instanceKey));
    setSelectedKey((current) => current && liveKeys.has(current) ? current : null);
  }, [workspace.instances, isLoading, error]);

  function updateSettings(update: Partial<ClientLaunchSettings>) {
    if (controlsDisabled) return;
    setDraft((current) => ({ ...current, ...update }));
  }

  function updateResourceSettings(update: Partial<ResourceMonitorSettingsUpdate>) {
    if (pendingAction !== null) return;
    setResourceDraft((current) => ({ ...current, ...update }));
  }

  async function saveControls() {
    await runAction("save-controls", async () => {
      const saved = await saveClientLaunchSettings(draft);
      setDraft(saved);
      setSavedSettings(saved);
    }, draft.enabled ? "Settings saved for future Roblox launches" : "Launch overrides disabled", draft.enabled ? `${draft.fps} FPS, graphics quality ${draft.graphics}, fullscreen ${draft.fullscreen ? "on" : "off"} and audio mute ${draft.muted ? "on" : "off"} were saved. ${draft.rmOnly ? "Only RM launches will use these overrides." : "RM launches will update Roblox's shared settings, which also affect launches outside RM."} Existing clients are unchanged.` : "RM will stop reapplying overrides before launch. Roblox keeps its last saved settings; existing clients are unchanged.");
  }

  async function resetControls() {
    await runAction("reset-controls", async () => {
      await saveClientLaunchSettings(defaultSettings);
      setSettingsRevision((current) => current + 1);
    }, "Launch overrides reset", "Saved shared launch overrides were reset and disabled. Roblox retains its last saved settings; running clients were not changed.");
  }

  async function saveResourceControls() {
    await runAction("save-resource-controls", async () => {
      const saved = await saveResourceMonitorSettings(resourceDraft);
      const { systemMemoryLimitMb, ...editable } = saved;
      setResourceMaximum(systemMemoryLimitMb);
      setResourceDraft(editable);
      setResourceSaved(editable);
    }, "Resource monitoring saved", "Thresholds apply to exactly attributed Roblox clients while RM is running. Saved resource limits no longer cap new launches.");
  }

  async function runAction(action: string, operation: () => Promise<unknown>, success?: string, detail?: string) {
    if (actionPending.current) return;
    actionPending.current = true;
    setPendingAction(action);
    try {
      await operation();
      if (success) setToast({ id: Date.now(), title: success, message: detail ?? "Refresh Instances to see the latest details.", kind: "success", duration: "standard" });
    } catch (error) {
      setToast({ id: Date.now(), title: "Instance action failed", message: `Couldn’t ${action.startsWith("focus-") ? "focus this window" : action.startsWith("join-") ? "join this server" : action === "close" ? "close Roblox" : action === "save-controls" ? "save launch settings" : action === "save-resource-controls" ? "save resource monitoring settings" : action === "reset-controls" ? "reset launch settings" : action === "check-confirmation" ? "check the close-confirmation setting" : action === "arrange" ? "arrange Roblox windows" : action}. ${operationError(error, "Refresh Instances and try again.")}`, kind: "error", duration: "long" });
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
        setToast({ id: Date.now(), title: "Roblox clients closed", message: "Closed all detected Roblox clients, including those opened outside RM.", kind: "success", duration: "standard" });
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
    }, target === "all" ? "Roblox clients closed" : "Instance closed", target === "all" ? "Closed all detected Roblox clients, including those opened outside RM." : `Closed Roblox for ${target.label}.`);
  }

  async function joinServer(target: InstanceSummary) {
    if (target.userId === null) return;
    const targetUserId = target.userId;
    const userIds = launchAccounts.filter((account) => selectedIds.has(account.userId) && account.userId !== targetUserId).map((account) => account.userId);
    await runAction(`join-${target.pid}`, async () => {
      for (const userId of userIds) await joinUserGame(userId, targetUserId);
    }, "Joining server", `Joining with ${userIds.length} accounts. Check Instances for progress.`);
  }

  return <>
    <main className="instances-page assets-page" data-walkthrough="instances">
      <div className="assets-main">
        <div className="assets-toolbar instances-toolbar">
          <div className="instances-heading"><h1 className="header-title">Instances</h1><p className="instances-description">Manage and control your running Roblox clients.</p></div>
          <div className="instances-toolbar-actions">
          <button className="account-button" type="button" disabled={isLoading || pendingAction !== null} onClick={() => void onRefresh()}><Icon name="refresh" />{isLoading ? "Refreshing..." : "Refresh"}</button>
          <button className="account-button" type="button" disabled={!workspace.runningCount || pendingAction !== null || isLoading} onClick={() => void runAction("arrange", arrangeAccountWindows, "Windows arranged", "Roblox windows arranged using your saved layout.")}><Icon name="grid" />{pendingAction === "arrange" ? "Arranging..." : "Arrange windows"}</button>
          <button className="account-button danger" type="button" disabled={!workspace.runningCount || pendingAction !== null || isLoading} onClick={() => void requestKillAll()}><Icon name="kill" tone="current-color" />{pendingAction === "check-confirmation" || pendingAction === "close" ? "Working..." : "Close all Roblox"}</button>
          </div>
        </div>
        <div className="assets-toolbar instances-join-section">
          <div className="instances-join-fields">
          <span className="instances-picker-label">Join server as</span>
          <AccountPicker accounts={launchAccounts} mode="multiple" showAllAccounts open={isPickerOpen} onOpenChange={setIsPickerOpen} selectedIds={selectedIds} onSelectedIdsChange={onSelectedIdsChange} disabled={pendingAction !== null || !launchAccounts.length} unselectedLabel="Select accounts to join a server" />
          <span className="instances-description">{launchAccounts.length ? "Choose accounts, then use Join server on a matched client." : "Add a valid account in Accounts to join a client's server."}</span>
          </div>
          <span className="instances-count" role="status">{isLoading ? "Checking clients..." : error ? "Client status unavailable" : `${workspace.runningCount} client${workspace.runningCount === 1 ? "" : "s"} running`}</span>
        </div>
        {launches.length > 0 && <div className="assets-notice" role="status">{launches.map((launch) => <p key={launch.requestId}>{accounts.find((account) => account.userId === launch.userId)?.label ?? "Account"}: {phaseLabels[launch.phase]}</p>)}</div>}
        <div className="instances-workspace">
          <section className="instances-list-panel settings-section" aria-labelledby="running-clients-heading" aria-busy={isLoading}>
            <h2 id="running-clients-heading">Running clients{!isLoading && !error && ` (${workspace.instances.length})`}</h2>
            {isLoading ? <LoadingSkeleton layout="results" label="Loading running instances" /> : error ? <div className="instances-empty" role="alert"><p className="selectable-text">{error}</p><button className="account-button" type="button" onClick={() => void onRefresh()}><Icon name="refresh" />Retry</button></div> : workspace.instances.length === 0 ? <div className="account-empty-inline"><Icon name="app-window" /><strong>No Roblox clients running</strong><span>Launch an account from Accounts, Presets or Private Servers to track it here.</span></div> : <ul className="instances-client-list">{workspace.instances.map((instance) => {
                const canJoin = instance.userId !== null && launchAccounts.some((account) => selectedIds.has(account.userId) && account.userId !== instance.userId);
                return <InstanceRow key={instanceKey(instance)} instance={instance} account={accounts.find((account) => account.userId === instance.userId)} selected={selectedKey === instanceKey(instance)} onSelect={() => setSelectedKey(instanceKey(instance))} actions={<>
                    <button className="account-button" type="button" disabled={pendingAction !== null} onClick={() => void runAction(`focus-${instance.pid}`, () => focusInstance(instance), "Window focused", `Switched to ${instance.label}’s Roblox window.`)}><Icon name="focus" />{pendingAction === `focus-${instance.pid}` ? "Focusing..." : "Focus"}</button>
                    <button className="account-button" type="button" disabled={!canJoin || pendingAction !== null} data-tip={instance.userId === null ? "Identify this client's account before joining its server" : !canJoin ? "Select another valid account above to join this client's server" : "Launch the selected accounts into this client's server"} onClick={() => void joinServer(instance)}><Icon name="launch" />{pendingAction === `join-${instance.pid}` ? "Joining..." : "Join server"}</button>
                    <button className="account-button danger" type="button" disabled={instance.attribution !== "exact" || pendingAction !== null} data-tip={instance.attribution === "exact" ? "Close this Roblox client" : "RM must confirm this client’s account before closing it"} onClick={() => setCloseTarget(instance)}><Icon name="kill" tone="current-color" />Close</button>
                  </>} />;
              })}</ul>}
          </section>
          <section className="instances-controls-panel settings-section" aria-labelledby="client-controls-heading">
            <h2 id="client-controls-heading">Client controls</h2>
            <p className="instances-description">Shared settings for new Roblox clients.</p>
            {selectedInstance && <InstanceIdentity instance={selectedInstance} account={accounts.find((account) => account.userId === selectedInstance.userId)} />}
            {settingsLoading ? <LoadingSkeleton layout="status" label="Loading client launch settings" /> : settingsError ? <div className="instances-empty" role="alert"><p className="selectable-text">{settingsError}</p><button className="account-button" type="button" disabled={pendingAction !== null} onClick={() => setSettingsRevision((current) => current + 1)}>Retry</button><button className="account-button" type="button" disabled={pendingAction !== null} onClick={() => void resetControls()}>{pendingAction === "reset-controls" ? "Resetting..." : "Reset launch settings"}</button></div> : <>
              <div className="instances-controls-notice" id="client-controls-notice" role="note"><Icon name="info-mark" tone="current-color" /><p><strong>Applies to new launches.</strong> Running clients won't change. {draft.rmOnly ? "Only launches through RM use these overrides." : "RM updates Roblox's shared settings, which also affect launches outside RM."}</p></div>
              <div role="group" aria-label="Shared client launch settings" aria-describedby="client-controls-notice">
                <SettingRow label="Apply on launch" description="Use these overrides for new clients.">{(labelId, descriptionId) => <ToggleField checked={draft.enabled} onChange={(enabled) => updateSettings({ enabled })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
                {draft.enabled && <SettingRow label="Only RM launches" description="Leave website and Roblox app launches on Roblox's saved settings.">{(labelId, descriptionId) => <ToggleField checked={draft.rmOnly} onChange={(rmOnly) => updateSettings({ rmOnly })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>}
                <SettingRow label="FPS limit" description="Choose a maximum frame rate.">{() => <Select value={String(draft.fps)} options={fpsOptions} onChange={(fps) => updateSettings({ fps: Number(fps) })} ariaLabel="FPS limit for future launches" disabled={controlsDisabled} />}</SettingRow>
                <SettingRow label="Graphics quality" description="Choose a quality level from 1 to 10.">{(labelId, descriptionId) => <RangeField min={1} max={10} value={draft.graphics} onChange={(graphics) => updateSettings({ graphics })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
                <SettingRow label="Fullscreen" description="Start new clients in fullscreen.">{(labelId, descriptionId) => <ToggleField checked={draft.fullscreen} onChange={(fullscreen) => updateSettings({ fullscreen })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
                <SettingRow label="Muted audio" description="Start new clients with audio muted.">{(labelId, descriptionId) => <ToggleField checked={draft.muted} onChange={(muted) => updateSettings({ muted })} labelId={labelId} descriptionId={descriptionId} disabled={controlsDisabled} />}</SettingRow>
              </div>
              {hasSettingsChanges && <p className="instances-description" role="status">Unsaved launch settings</p>}
              <button className="account-button" type="button" disabled={controlsDisabled || !hasSettingsChanges} onClick={() => void saveControls()}>{pendingAction === "save-controls" ? "Saving..." : "Save launch settings"}</button>
            </>}
            <div className="instances-resource-controls">
              <h3>Resource monitoring</h3>
              <p className="instances-description">Checks exact RM-attributed clients every few seconds. Each action runs once per client and resource.</p>
              {resourceLoading ? <LoadingSkeleton layout="status" label="Loading resource monitoring settings" /> : resourceError ? <div className="instances-empty" role="alert"><p className="selectable-text">{resourceError}</p><button className="account-button" type="button" disabled={pendingAction !== null} onClick={() => setResourceRevision((current) => current + 1)}>Retry</button></div> : <>
                <div className="instances-controls-notice" id="resource-monitor-notice" role="note"><Icon name="info-mark" tone="current-color" /><p><strong>Warnings notify once per client.</strong> Warn and Throttle send Windows and routed Discord notifications. Kill closes only the verified client and does not send a notification. Saved limits from Settings are now thresholds.</p></div>
                <div role="group" aria-label="Roblox resource monitoring settings" aria-describedby="resource-monitor-notice">
                  <SettingRow label="CPU usage threshold" description="Percent of total logical-processor capacity.">{(labelId, descriptionId) => <div className="resource-threshold-control">
                    <label className="settings-toggle"><input type="checkbox" aria-label="Monitor CPU usage" checked={resourceDraft.cpuLimitPercent !== null} disabled={pendingAction !== null} onChange={(event) => updateResourceSettings({ cpuLimitPercent: event.target.checked ? 80 : null })} /><span>{resourceDraft.cpuLimitPercent === null ? "Off" : "On"}</span></label>
                    {resourceDraft.cpuLimitPercent !== null && <RangeNumberField min={1} max={99} value={resourceDraft.cpuLimitPercent} unit="%" onChange={(cpuLimitPercent) => updateResourceSettings({ cpuLimitPercent })} labelId={labelId} descriptionId={descriptionId} disabled={pendingAction !== null} />}
                  </div>}</SettingRow>
                  <SettingRow label="CPU action" description="Throttle halves the configured threshold as the CPU cap. Kill closes the client silently.">{() => <Select value={resourceDraft.cpuAction} options={cpuActionOptions} onChange={(cpuAction) => {
                    if (cpuAction === "warn" || cpuAction === "throttle" || cpuAction === "kill") updateResourceSettings({ cpuAction });
                  }} ariaLabel="Action when CPU threshold is reached" disabled={resourceDraft.cpuLimitPercent === null || pendingAction !== null} />}</SettingRow>
                  <SettingRow label="RAM usage threshold" description="Per-client resident memory (working set), in MB.">{(labelId, descriptionId) => <div className="resource-threshold-control">
                    <label className="settings-toggle"><input type="checkbox" aria-label="Monitor RAM usage" checked={resourceDraft.memoryLimitMb !== null} disabled={pendingAction !== null || (resourceMaximum === null && resourceDraft.memoryLimitMb === null)} onChange={(event) => updateResourceSettings({ memoryLimitMb: event.target.checked ? Math.min(8192, Math.max(1024, resourceMaximum ?? 8192)) : null })} /><span>{resourceDraft.memoryLimitMb === null ? "Off" : "On"}</span></label>
                    {resourceDraft.memoryLimitMb !== null && <RangeNumberField min={1024} max={Math.max(1024, resourceMaximum ?? 1024)} value={resourceDraft.memoryLimitMb} unit="MB" onChange={(memoryLimitMb) => updateResourceSettings({ memoryLimitMb })} labelId={labelId} descriptionId={descriptionId} disabled={pendingAction !== null || resourceMaximum === null} />}
                  </div>}</SettingRow>
                  <SettingRow label="RAM action" description="Windows cannot smoothly throttle RAM. Kill closes the client silently.">{() => <Select value={resourceDraft.memoryAction} options={memoryActionOptions} onChange={(memoryAction) => {
                    if (memoryAction === "warn" || memoryAction === "kill") updateResourceSettings({ memoryAction });
                  }} ariaLabel="Action when RAM threshold is reached" disabled={resourceDraft.memoryLimitMb === null || pendingAction !== null} />}</SettingRow>
                </div>
                {resourceMaximum === null && <p className="instances-description" role="status">Installed memory could not be determined. RAM thresholds are unavailable.</p>}
                {hasResourceChanges && <p className="instances-description" role="status">Unsaved resource monitoring settings</p>}
                <button className="account-button" type="button" disabled={pendingAction !== null || !hasResourceChanges} onClick={() => void saveResourceControls()}>{pendingAction === "save-resource-controls" ? "Saving..." : "Save resource settings"}</button>
              </>}
            </div>
          </section>
        </div>
      </div>
    </main>
    {closeTarget && <ConfirmModal title={closeTarget === "all" ? "Close all Roblox clients?" : "Close this Roblox client?"} message={closeTarget === "all" ? "This closes every Roblox client, including those opened outside RM." : `Close Roblox for ${closeTarget.label}?`} confirmLabel={closeTarget === "all" ? "Close all Roblox" : "Close Roblox"} confirmIcon="kill" onConfirm={confirmClose} onCancel={() => setCloseTarget(null)} />}
    {toast && <Toast item={toast} onDismiss={() => setToast(null)} />}
  </>;
}
