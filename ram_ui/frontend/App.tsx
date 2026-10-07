import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { InstancesPage } from "./InstancesPage";
import { Toast, type ToastItem } from "./Toast";
import { Popup } from "./components/Popup";
import { ReleaseNotes } from "./components/ReleaseNotes";
import { Walkthrough, walkthroughSteps } from "./components/Walkthrough";
import { WalkthroughAccounts, type DemoAccountName } from "./components/WalkthroughAccounts";
import { ConfirmModal } from "./ConfirmModal";
import { benchmarkReady, acknowledgeStartup, checkReleaseUpdate, clearPassword, migrateLegacyData, openReleasePage, startupStatus, type StartupStatus } from "./lib/ipc";
import { defaultPageVisibility, workspacePages, listInstances, listAccounts, getSettings, clearApplicationCaches, operationError, type SettingsConfig, type AccountSummary, type InstanceWorkspace, type LaunchProgress } from "./lib/ipc";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AccountsPage } from "./AccountsPage";
import { GroupsPage } from "./GroupsPage";
import { PrivateServersPage } from "./PrivateServersPage";
import { PresetsPage } from "./PresetsPage";
import { SettingsPage, type SettingsNavigationGuard } from "./SettingsPage";
import { InventoriesPage } from "./InventoriesPage";
import { AssetsPage } from "./AssetsPage";
import { isSelectAllShortcut, isTextSelectionTarget } from "./lib/selectAllShortcut";

type NavItem = {
  label: string;
  icon: string;
  page?: PageName;
};

type PageName =
  | "Accounts"
  | "Instances"
  | "Groups"
  | "Private Servers"
  | "Presets"
  | "Inventories"
  | "Asset Manager"
  | "Settings";

const navItems: NavItem[] = [
  { label: "Accounts", icon: "id-card", page: "Accounts" },
  { label: "Instances", icon: "app-window", page: "Instances" },
  { label: "Groups", icon: "users", page: "Groups" },
  { label: "Private Servers", icon: "game", page: "Private Servers" },
  { label: "Presets", icon: "star", page: "Presets" },
  { label: "Inventories", icon: "inventory", page: "Inventories" },
  { label: "Asset Manager", icon: "package", page: "Asset Manager" },
];

function Icon({ name }: { name: string }) {
  return (
    <img
      className="h-full w-full object-contain"
      src={`/icons/${name}.svg`}
      alt=""
      aria-hidden="true"
    />
  );
}

function RailButton({
  label,
  icon,
  page,
  active = false,
  disabled = false,
  onClick,
}: NavItem & { active?: boolean; disabled?: boolean; onClick: () => void }) {
  return (
    <button
      className={`rail-button ${active ? "is-active" : ""}`}
      type="button"
      aria-label={label}
      aria-current={active ? "page" : undefined}
      data-tip={label}
      data-walkthrough-page={page}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="accent-bar" aria-hidden="true" />
      <span className="icon-box">
        <Icon name={icon} />
      </span>
    </button>
  );
}

function WindowButton({
  label,
  icon,
  close = false,
  onClick,
}: {
  label: string;
  icon: string;
  close?: boolean;
  onClick: () => Promise<void>;
}) {
  return (
    <button
      className={`window-button ${close ? "window-button-close" : ""}`}
      type="button"
      aria-label={label}
      data-tip={label}
      onMouseDown={(event) => event.stopPropagation()}
      onClick={onClick}
    >
      <Icon name={icon} />
    </button>
  );
}

export function App() {
  const [activeNav, setActiveNav] = useState<PageName>("Accounts");
  const displayedNav = activeNav;
  const settingsGuardRef = useRef<SettingsNavigationGuard | null>(null);
  const [pendingNavigation, setPendingNavigation] = useState<PageName | null>(null);
  const [isNavigationSaving, setIsNavigationSaving] = useState(false);
  const updateSettingsGuard = useCallback((guard: SettingsNavigationGuard | null) => {
    settingsGuardRef.current = guard;
  }, []);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [workspace, setWorkspace] = useState<InstanceWorkspace>({ instances: [], runningCount: 0 });
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [launches, setLaunches] = useState<Record<string, LaunchProgress>>({});
  const [isInstancesLoading, setIsInstancesLoading] = useState(true);
  const benchmarkDataReady = useRef(false);
  const [instancesError, setInstancesError] = useState<string | null>(null);
  const [runtimeToast, setRuntimeToast] = useState<ToastItem | null>(null);
  const [settings, setSettings] = useState<SettingsConfig | null>(null);
  const [isClearingCache, setIsClearingCache] = useState(false);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [isStartupPending, setIsStartupPending] = useState(false);
  const [tutorialStep, setTutorialStep] = useState(0);
  const [isTourCompletionVisible, setIsTourCompletionVisible] = useState(false);
  const [selectedDemoAccount, setSelectedDemoAccount] = useState<DemoAccountName | null>(null);
  const [isMigrationDismissed, setIsMigrationDismissed] = useState(false);
  const [update, setUpdate] = useState<[string, string] | null>(null);
  const [isReleaseDialogOpen, setIsReleaseDialogOpen] = useState(false);
  const [browserPlaceId, setBrowserPlaceId] = useState<number | null>(null);
  const [prefilledPlaceId, setPrefilledPlaceId] = useState<number>();
  const isTourVisible = !!startup?.needsTutorial && (!startup.legacyMigrationAvailable || isMigrationDismissed);

  useEffect(() => {
    if (isTourVisible) setActiveNav(walkthroughSteps[tutorialStep].page);
  }, [isTourVisible, tutorialStep]);

  async function refreshStartup() {
    try {
      const status = await startupStatus();
      setStartup(status);
      setStartupError(null);
      if (status.migrationNotice) setRuntimeToast({ id: Date.now(), title: "Older favourites need attention", message: status.migrationNotice, kind: "warning", duration: "long" });
      if (!status.needsTutorial && !status.changelog) await acknowledgeStartup("version");
    } catch (error) { setStartupError(operationError(error, "Startup information could not be loaded.")); }
  }

  async function completeStartup(action: () => Promise<void>) {
    if (isStartupPending) return;
    setIsStartupPending(true);
    try { await action(); await refreshStartup(); }
    catch (error) { setStartupError(operationError(error, "The startup action could not be completed. Retry.")); }
    finally { setIsStartupPending(false); }
  }

  async function finishWalkthrough() {
    if (isStartupPending) return;
    setIsStartupPending(true);
    try {
      await acknowledgeStartup("version");
      setStartup((current) => current ? { ...current, needsTutorial: false } : current);
      navigateTo("Accounts");
      setIsTourCompletionVisible(true);
      setSelectedDemoAccount(null);
      setStartupError(null);
    } catch (error) {
      setStartupError(operationError(error, "The tour could not be completed. Retry."));
    } finally { setIsStartupPending(false); }
  }

  useEffect(() => {
    if (!isTourCompletionVisible) return;
    const timeout = window.setTimeout(() => setIsTourCompletionVisible(false), 5000);
    return () => window.clearTimeout(timeout);
  }, [isTourCompletionVisible]);

  useEffect(() => {
    let isActive = true;
    const stops: Array<() => void> = [];
    void refreshStartup();
    void checkReleaseUpdate().then((result) => { if (isActive) setUpdate(result); }).catch(() => {
      if (isActive) setRuntimeToast({ id: Date.now(), title: "Update check unavailable", message: "The startup update check could not be completed. Check releases on GitHub later.", kind: "warning", duration: "standard" });
    });
    void Promise.allSettled([
      listen<number>("browser-play-request", (event) => { if (isActive) setBrowserPlaceId(event.payload); }),
      listen("store-unlocked", () => { if (isActive) void refreshStartup(); }),
    ]).then((results) => results.forEach((result) => { if (result.status === "fulfilled") { if (isActive) stops.push(result.value); else result.value(); } }));
    return () => { isActive = false; stops.forEach((stop) => stop()); };
  }, []);
  const visibility = { ...defaultPageVisibility, ...settings?.pageVisibility };
  const updateVersion = update?.[0].startsWith("v") ? update[0] : update ? `v${update[0]}` : null;
  const visibleNavItems = navItems.filter((item) => {
    const page = workspacePages.find((page) => page.label === item.page);
    return isTourVisible || !page || visibility[page.key];
  });

  async function clearCache() {
    if (isClearingCache) return;
    setIsClearingCache(true);
    try {
      const clearedCount = await clearApplicationCaches();
      setRuntimeToast(clearedCount > 0
        ? { id: Date.now(), title: "Cache cleared", message: `${clearedCount} application cache entries were removed. RM can fetch this information again when the related workspace is opened.`, kind: "success", duration: "standard" }
        : { id: Date.now(), title: "No cache to clear", message: "RM found no application cache entries to remove. Your accounts, presets and settings were not changed.", kind: "info", duration: "standard" });
    } catch (error) {
      setRuntimeToast({ id: Date.now(), title: "Cache could not be cleared", message: operationError(error, "Application cache cleanup could not be completed. Open Settings and retry Clear Cache after checking access to the RM data folder."), kind: "error", duration: "long" });
    } finally { setIsClearingCache(false); }
  }

  async function refreshInstances() {
    setIsInstancesLoading(true);
    try {
      setWorkspace(await listInstances());
      setInstancesError(null);
    } catch (error) {
      setInstancesError(operationError(error, "Running instances could not be loaded. Retry to reconnect."));
    } finally {
      setIsInstancesLoading(false);
    }
  }

  useEffect(() => {
    let isActive = true;
    let hasInstanceEvent = false;
    let hasAccountEvent = false;
    const stops: Array<() => void> = [];
    async function subscribe() {
      const results = await Promise.allSettled([
        listen<SettingsConfig>("settings-updated", (event) => { if (isActive) setSettings(event.payload); }),
        listen<InstanceWorkspace>("instances-updated", (event) => {
          if (!isActive) return;
          hasInstanceEvent = true;
          setWorkspace(event.payload);
          setIsInstancesLoading(false);
          setInstancesError(null);
        }),
        listen<AccountSummary[]>("accounts-updated", (event) => {
          if (!isActive) return;
          hasAccountEvent = true;
          setAccounts(event.payload);
          if (event.payload.length > 0) void refreshStartup();
        }),
        listen<LaunchProgress>("launch-progress", (event) => {
          if (!isActive) return;
          setLaunches((current) => {
            const next = { ...current };
            if (event.payload.phase === "requested" || event.payload.phase === "failed") delete next[event.payload.requestId];
            else next[event.payload.requestId] = event.payload;
            return next;
          });
        }),
        listen<string>("background-notice", (event) => {
          if (isActive) setRuntimeToast({ id: Date.now(), title: "Background action needs attention", message: event.payload, kind: "warning", duration: "long" });
        }),
      ]);
      for (const result of results) {
        if (result.status === "fulfilled") {
          if (isActive) stops.push(result.value);
          else result.value();
        } else if (isActive) {
          setRuntimeToast({ id: Date.now(), title: "Live updates unavailable", message: "RM could not connect to live account, client or launch updates. Use Refresh in Accounts or Instances to reload the latest state. Restart RM to retry the live-update connection.", kind: "error", duration: "long" });
        }
      }
      if (!isActive) return;
      try { const snapshot = await getSettings(); if (isActive) setSettings(snapshot.config); } catch { /* settings remain unavailable until a successful save */ }
      const [instanceResult, accountResult] = await Promise.allSettled([listInstances(), listAccounts()]);
      if (!isActive) return;
      if (instanceResult.status === "fulfilled" && !hasInstanceEvent) setWorkspace(instanceResult.value);
      if (instanceResult.status === "rejected" && !hasInstanceEvent) setInstancesError("Running instances could not be loaded. Retry to reconnect.");
      if (accountResult.status === "fulfilled" && !hasAccountEvent) setAccounts(accountResult.value);
      if (window.__RM_BENCHMARK__) {
        benchmarkDataReady.current = (instanceResult.status === "fulfilled" || hasInstanceEvent)
          && (accountResult.status === "fulfilled" || hasAccountEvent);
      }
      setIsInstancesLoading(false);
    }
    void subscribe();
    return () => { isActive = false; stops.forEach((stop) => stop()); };
  }, []);

  useEffect(() => {
    if (!window.__RM_BENCHMARK__ || !benchmarkDataReady.current || isInstancesLoading || !settings || !startup) return;
    // allow the committed initial shell/data a paint opportunity before notifying the harness.
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => { void benchmarkReady().catch(() => {}); });
    });
    return () => { cancelAnimationFrame(firstFrame); cancelAnimationFrame(secondFrame); };
  }, [isInstancesLoading, settings, startup]);

  useEffect(() => {
    if (!settings || isTourVisible || activeNav === "Settings") return;
    const visibility = { ...defaultPageVisibility, ...settings.pageVisibility };
    const active = workspacePages.find((page) => page.label === activeNav);
    if (active && !visibility[active.key]) {
      setActiveNav(workspacePages.find((page) => visibility[page.key])?.label ?? "Settings");
    }
  }, [settings, activeNav, isTourVisible]);


  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (isSelectAllShortcut(event) && !isTextSelectionTarget(event.target)) event.preventDefault();
    }
    function onContextMenu(event: Event) {
      if (isTextSelectionTarget(event.target)) return;
      event.preventDefault();
    }
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("contextmenu", onContextMenu, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("contextmenu", onContextMenu, true);
    };
  }, []);

  function navigateTo(nextPage: PageName) {
    if (nextPage === activeNav) {
      return;
    }

    const guard = settingsGuardRef.current;
    if (activeNav === "Settings" && guard) {
      if (guard.isBusy) return;
      if (guard.isDirty) {
        setPendingNavigation(nextPage);
        return;
      }
    }
    setActiveNav(nextPage);
  }

  async function saveBeforeNavigating() {
    if (!pendingNavigation || isNavigationSaving) return;
    const guard = settingsGuardRef.current;
    if (!guard || guard.isBusy) return;
    setIsNavigationSaving(true);
    try {
      if (await guard.save()) {
        setActiveNav(pendingNavigation);
        setPendingNavigation(null);
      } else {
        setPendingNavigation(null);
      }
    } finally {
      setIsNavigationSaving(false);
    }
  }

  function renderPage(page: PageName) {
    if (page === "Accounts") {
      if (isTourVisible) return <WalkthroughAccounts selectedName={selectedDemoAccount} onSelect={setSelectedDemoAccount} />;
      return (
        <AccountsPage
          selectedIds={selectedIds}
          setSelectedIds={setSelectedIds}
          prefilledPlaceId={prefilledPlaceId}
        />
      );
    }

    if (page === "Instances") {
      return <InstancesPage workspace={workspace} accounts={accounts} selectedIds={selectedIds} onSelectedIdsChange={setSelectedIds} launches={Object.values(launches)} isLoading={isInstancesLoading} error={instancesError} onRefresh={refreshInstances} />;
    }

    if (page === "Groups") {
      return (
        <GroupsPage
          selectedIds={selectedIds}
          setSelectedIds={setSelectedIds}
          onNavigateAccounts={() => navigateTo("Accounts")}
        />
      );
    }

    if (page === "Private Servers") {
      return (
        <PrivateServersPage
          selectedIds={selectedIds}
          onNavigateAccounts={() => navigateTo("Accounts")}
        />
      );
    }

    if (page === "Presets") {
      return (
        <PresetsPage
          selectedIds={selectedIds}
          onNavigateAccounts={() => navigateTo("Accounts")}
        />
      );
    }

    if (page === "Settings") {
      return <SettingsPage onNavigationGuardChange={updateSettingsGuard} />;
    }

    if (page === "Inventories") {
      return <InventoriesPage initialSelectedIds={selectedIds} />;
    }

    if (page === "Asset Manager") {
      return <AssetsPage initialSelectedIds={selectedIds} />;
    }

    return (
      <>
        <div className="header-row">
          <h1 className="header-title">{page}</h1>
        </div>
        <main className="content">
          <section className="empty-state" aria-labelledby="whoops-title">
            <h2 id="whoops-title">Whoops!</h2>
            <p>
              You need to disable 'Auto-Pick Clients' inside the settings to use
              this feature.
            </p>
          </section>
        </main>
      </>
    );
  }

  function handleTitlebarMouseDown(event: MouseEvent<HTMLElement>) {
    const target = event.target;
    const clickedButton = target instanceof Element && target.closest("button");

    if (event.button === 0 && !clickedButton) {
      void getCurrentWindow().startDragging();
    }
  }

  function handleTitlebarDoubleClick(event: MouseEvent<HTMLElement>) {
    const target = event.target;
    const clickedButton = target instanceof Element && target.closest("button");

    if (!clickedButton) {
      void getCurrentWindow().toggleMaximize();
    }
  }

  return (
    <div className="flex h-screen min-h-0 flex-col overflow-hidden bg-canvas text-primary">
      <header
        className="titlebar"
        onMouseDown={handleTitlebarMouseDown}
        onDoubleClick={handleTitlebarDoubleClick}
      >
        <span className="titlebar-logo" aria-label="Roblox Manager">
          <Icon name="feather" />
        </span>
        <span className="titlebar-runtime-status" role="status">{workspace.runningCount} Roblox running{Object.keys(launches).length ? ` | ${Object.keys(launches).length} launches pending` : ""}</span>
        <span className="titlebar-version">
          {import.meta.env.VITE_RM_VERSION}
        </span>
        {update && <button type="button" className="account-button titlebar-update" onClick={() => setIsReleaseDialogOpen(true)}>Update to {updateVersion}</button>}
        <div className="window-controls" aria-label="Window controls">
          <WindowButton
            label="Minimize"
            icon="minus"
            onClick={() => getCurrentWindow().minimize()}
          />
          <WindowButton
            label="Maximize"
            icon="square"
            onClick={() => getCurrentWindow().toggleMaximize()}
          />
          <WindowButton
            label="Close"
            icon="close"
            close
            onClick={() => getCurrentWindow().close()}
          />
        </div>
      </header>

      <div className={`body-row ${isTourVisible ? "is-walkthrough-active" : ""} ${isTourCompletionVisible ? "is-walkthrough-complete" : ""}`} data-walkthrough-content>
        <nav className="sidebar" aria-label="Primary navigation" data-walkthrough="navigation">
          {visibleNavItems.map((item) => (
            <RailButton
              {...item}
              key={item.label}
              active={activeNav === item.label}
              onClick={() => {
                if (item.page) {
                  navigateTo(item.page);
                }
              }}
            />
          ))}
          <span className="sidebar-spacer" />
          {settings?.utilityEnabled && <RailButton label="Clear Cache" icon="eraser" disabled={isClearingCache} onClick={() => void clearCache()} />}
          <RailButton
            label="Settings"
            icon="settings"
            page="Settings"
            active={activeNav === "Settings"}
            onClick={() => navigateTo("Settings")}
          />
        </nav>

        <div className="main-col" data-walkthrough="workspace">
          <div className="page-transition-viewport">
            <div
              key={displayedNav}
              className="page-transition-layer"
            >
              {renderPage(displayedNav)}
            </div>
            <span className="sr-only" aria-live="polite">{displayedNav} page</span>
          </div>
        </div>
      </div>
      {pendingNavigation && <Popup className="confirm-modal" backdropClassName="confirm-modal-backdrop" labelledBy="unsaved-navigation-title" describedBy="unsaved-navigation-message" busy={isNavigationSaving} onClose={() => setPendingNavigation(null)}>
        <div className="confirm-modal-header"><h2 id="unsaved-navigation-title">Save settings before leaving?</h2></div>
        <p id="unsaved-navigation-message" className="confirm-modal-message">Your settings have unsaved changes. Save them, discard them, or stay to keep editing.</p>
        <div className="confirm-modal-actions">
          <button className="account-button" type="button" disabled={isNavigationSaving} onClick={() => setPendingNavigation(null)}>Stay</button>
          <button className="account-button" type="button" disabled={isNavigationSaving} onClick={() => {
            settingsGuardRef.current?.discard();
            setActiveNav(pendingNavigation);
            setPendingNavigation(null);
          }}>Discard</button>
          <button className="account-button primary" type="button" disabled={isNavigationSaving} onClick={() => void saveBeforeNavigating()}>{isNavigationSaving ? "Saving..." : "Save and leave"}</button>
        </div>
      </Popup>}
      {isReleaseDialogOpen && update && updateVersion && <Popup className="confirm-modal" backdropClassName="confirm-modal-backdrop" labelledBy="release-update-title" describedBy="release-update-message" onClose={() => setIsReleaseDialogOpen(false)} closeOnBackdrop>
        <div className="confirm-modal-header"><h2 id="release-update-title">A new version is available</h2></div>
        <p id="release-update-message" className="confirm-modal-message">RM {updateVersion} is ready. Open the latest GitHub release to download it.</p>
        <div className="confirm-modal-actions">
          <button className="account-button" type="button" onClick={() => setIsReleaseDialogOpen(false)}>Cancel</button>
          <button className="account-button primary" type="button" onClick={() => void openReleasePage(update[1]).then(() => setIsReleaseDialogOpen(false)).catch(() => setStartupError("The release page could not be opened. Try again."))}>Go to latest release</button>
        </div>
      </Popup>}
      {runtimeToast && <Toast item={runtimeToast} onDismiss={() => setRuntimeToast(null)} />}
      {startup?.legacyMigrationAvailable && !isMigrationDismissed ? <ConfirmModal title="Migrate older RM data?" message="Copy your older configuration and encrypted account store into the standard RM data folder. Original files remain in place; existing modern data will never be overwritten." confirmLabel={isStartupPending ? "Migrating..." : "Copy to RM data folder"} confirmDisabled={isStartupPending} confirmIcon="import" onConfirm={() => void completeStartup(migrateLegacyData)} onCancel={() => setIsMigrationDismissed(true)} /> : isTourVisible ? <Walkthrough
        stepIndex={tutorialStep}
        isPageReady={displayedNav === walkthroughSteps[tutorialStep].page}
        isPending={isStartupPending}
        error={startupError}
        onBack={() => setTutorialStep((step) => Math.max(0, step - 1))}
        onNext={() => setTutorialStep((step) => Math.min(walkthroughSteps.length - 1, step + 1))}
        onFinish={() => void finishWalkthrough()}
        onReturnToStep={() => navigateTo(walkthroughSteps[tutorialStep].page)}
      /> : startup?.changelog ? <Popup className="confirm-modal changelog-modal" backdropClassName="confirm-modal-backdrop" labelledBy="changelog-title">
        <h2 id="changelog-title">What's changed in RM</h2><ReleaseNotes markdown={startup.changelog} /><button className="account-button" type="button" disabled={isStartupPending} onClick={() => void completeStartup(() => acknowledgeStartup("version"))}>Continue</button>
      </Popup> : startup?.passwordlessOffer ? <ConfirmModal title="Stop asking for a password on this PC?" message="Device encryption keeps your store encrypted and unlocks it through Windows Credential Manager. Keep a master password if you need to move the store between PCs." confirmLabel={isStartupPending ? "Changing encryption..." : "Use device encryption"} confirmDisabled={isStartupPending} confirmIcon="lock" onConfirm={() => void completeStartup(async () => { await clearPassword(); await acknowledgeStartup("passwordless"); })} onCancel={() => { if (!isStartupPending) void completeStartup(() => acknowledgeStartup("passwordless")); }} /> : browserPlaceId ? <ConfirmModal title="Launch this game through RM?" message={`The account browser blocked an external Roblox launch for Place ID ${browserPlaceId}. Prefill it in Accounts, then choose which account to launch.`} confirmLabel="Prefill Place ID" confirmIcon="game" onConfirm={() => { setPrefilledPlaceId(browserPlaceId); setBrowserPlaceId(null); navigateTo("Accounts"); }} onCancel={() => setBrowserPlaceId(null)} /> : null}
      {startupError && <Toast item={{ id: 1, title: "Startup action needs attention", message: startupError, kind: "error", duration: "long" }} onDismiss={() => setStartupError(null)} />}
      {isTourCompletionVisible && displayedNav === "Accounts" && <div className="walkthrough-completion" role="status">You're ready. Use the highlighted + button to add an account.</div>}
    </div>
  );
}
