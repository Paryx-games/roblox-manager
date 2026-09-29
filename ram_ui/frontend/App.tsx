import { useEffect, useState, type MouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { InstancesPage } from "./InstancesPage";
import { Toast, type ToastItem } from "./Toast";
import { Popup } from "./components/Popup";
import { ReleaseNotes } from "./components/ReleaseNotes";
import { ConfirmModal } from "./ConfirmModal";
import { acknowledgeStartup, checkReleaseUpdate, clearPassword, migrateLegacyData, openReleasePage, startupStatus, type StartupStatus } from "./lib/ipc";
import { listInstances, listAccounts, getSettings, clearApplicationCaches, operationError, type SettingsConfig, type AccountSummary, type InstanceWorkspace, type LaunchProgress } from "./lib/ipc";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AccountsPage } from "./AccountsPage";
import { GroupsPage } from "./GroupsPage";
import { PrivateServersPage } from "./PrivateServersPage";
import { PresetsPage } from "./PresetsPage";
import { SettingsPage } from "./SettingsPage";
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
  const [displayedNav, setDisplayedNav] = useState<PageName>("Accounts");
  const [isPageTransitioning, setIsPageTransitioning] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [workspace, setWorkspace] = useState<InstanceWorkspace>({ instances: [], runningCount: 0 });
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [launches, setLaunches] = useState<Record<string, LaunchProgress>>({});
  const [isInstancesLoading, setIsInstancesLoading] = useState(true);
  const [instancesError, setInstancesError] = useState<string | null>(null);
  const [runtimeToast, setRuntimeToast] = useState<ToastItem | null>(null);
  const [settings, setSettings] = useState<SettingsConfig | null>(null);
  const [isClearingCache, setIsClearingCache] = useState(false);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [isStartupPending, setIsStartupPending] = useState(false);
  const [tutorialStep, setTutorialStep] = useState(0);
  const [isMigrationDismissed, setIsMigrationDismissed] = useState(false);
  const [update, setUpdate] = useState<[string, string] | null>(null);
  const [browserPlaceId, setBrowserPlaceId] = useState<number | null>(null);
  const [prefilledPlaceId, setPrefilledPlaceId] = useState<number>();
  const tutorialSteps = [
    ["Welcome to Roblox Manager", "RM keeps your accounts encrypted and helps you launch and track multiple Roblox clients. This walkthrough explains the main controls."],
    ["Add your accounts", "Open Accounts and choose Add account. Sign in through the browser or explicitly paste a cookie. Re-adding an account replaces its credential while preserving its organisation."],
    ["Choose accounts and a game", "Select accounts in the list, enter a Place ID and choose Launch. Presets and Private Servers let you save destinations for later."],
    ["Track your clients", "Instances shows running clients and launch progress. Exact matches support verified individual kills; inferred matches are guesses. You can focus, arrange or join a client's server."],
    ["Set up your preferences", "Settings controls privacy, encryption, window arrangement and optional developer workspaces. Keep your encrypted account store and backups safe."],
  ];

  async function refreshStartup() {
    try {
      const status = await startupStatus();
      setStartup(status);
      setStartupError(null);
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
  const visibleNavItems = navItems.filter((item) =>
    item.page === "Asset Manager" || item.page === "Inventories" ? settings?.developerOptions : true,
  );

  async function clearCache() {
    if (isClearingCache) return;
    setIsClearingCache(true);
    try {
      await clearApplicationCaches();
      setRuntimeToast({ id: Date.now(), title: "Cache cleared", message: "Cached application data has been cleared.", kind: "success", duration: "standard" });
    } catch (error) {
      setRuntimeToast({ id: Date.now(), title: "Cache could not be cleared", message: operationError(error, "Try again from Settings."), kind: "error", duration: "long" });
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
          setRuntimeToast({ id: Date.now(), title: "Live updates unavailable", message: "Use Refresh in Accounts or Instances to reconnect.", kind: "error", duration: "long" });
        }
      }
      if (!isActive) return;
      try { const snapshot = await getSettings(); if (isActive) setSettings(snapshot.config); } catch { /* settings remain unavailable until a successful save */ }
      const [instanceResult, accountResult] = await Promise.allSettled([listInstances(), listAccounts()]);
      if (!isActive) return;
      if (instanceResult.status === "fulfilled" && !hasInstanceEvent) setWorkspace(instanceResult.value);
      if (instanceResult.status === "rejected" && !hasInstanceEvent) setInstancesError("Running instances could not be loaded. Retry to reconnect.");
      if (accountResult.status === "fulfilled" && !hasAccountEvent) setAccounts(accountResult.value);
      setIsInstancesLoading(false);
    }
    void subscribe();
    return () => { isActive = false; stops.forEach((stop) => stop()); };
  }, []);

  useEffect(() => {
    if (settings && !settings.developerOptions && (activeNav === "Asset Manager" || activeNav === "Inventories")) setActiveNav("Accounts");
  }, [settings, activeNav]);


  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (isSelectAllShortcut(event) && !isTextSelectionTarget(event.target)) event.preventDefault();
    }
    function onTransitionKeyDown(event: KeyboardEvent) {
      if (activeNav === displayedNav || !isSelectAllShortcut(event) || isTextSelectionTarget(event.target)) return;
      event.preventDefault();
      event.stopPropagation();
    }
    window.addEventListener("keydown", onTransitionKeyDown, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onTransitionKeyDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [activeNav, displayedNav]);

  useEffect(() => {
    if (activeNav === displayedNav) return;

    setIsPageTransitioning(true);
    const transitionTimeout = window.setTimeout(() => {
      setDisplayedNav(activeNav);
      setIsPageTransitioning(false);
    }, 240);

    return () => window.clearTimeout(transitionTimeout);
  }, [activeNav, displayedNav]);

  function navigateTo(nextPage: PageName) {
    if (nextPage === activeNav) {
      return;
    }

    setActiveNav(nextPage);
  }

  function renderPage(page: PageName) {
    if (page === "Accounts") {
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
      return <SettingsPage />;
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
        {update && <button type="button" className="account-button titlebar-update" onClick={() => void openReleasePage(update[1]).catch(() => setStartupError("The release page could not be opened. Try again."))}>Update {update[0]}</button>}
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

      <div className="body-row">
        <nav className="sidebar" aria-label="Primary navigation">
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

        <div className="main-col">
          <div className="page-transition-viewport">
            <div
              className={`page-transition-layer ${
                isPageTransitioning ? "is-transitioning" : ""
              }`}
              aria-live="polite"
            >
              {renderPage(displayedNav)}
            </div>
          </div>
        </div>
      </div>
      {runtimeToast && <Toast item={runtimeToast} onDismiss={() => setRuntimeToast(null)} />}
      {startup?.legacyMigrationAvailable && !isMigrationDismissed ? <ConfirmModal title="Migrate older RM data?" message="Copy your older configuration and encrypted account store into the standard RM data folder. Original files remain in place; existing modern data will never be overwritten." confirmLabel={isStartupPending ? "Migrating..." : "Copy to RM data folder"} confirmDisabled={isStartupPending} confirmIcon="import" onConfirm={() => void completeStartup(migrateLegacyData)} onCancel={() => setIsMigrationDismissed(true)} /> : startup?.needsTutorial ? <Popup className="confirm-modal" backdropClassName="confirm-modal-backdrop" labelledBy="tutorial-title">
        <h2 id="tutorial-title">{tutorialSteps[tutorialStep][0]}</h2><p>{tutorialSteps[tutorialStep][1]}</p><p>Step {tutorialStep + 1} of {tutorialSteps.length}</p>
        <div className="confirm-modal-actions"><button className="account-button" type="button" disabled={isStartupPending} onClick={() => void completeStartup(() => acknowledgeStartup("version"))}>Skip walkthrough</button><button className="account-button" type="button" disabled={isStartupPending} onClick={() => tutorialStep === tutorialSteps.length - 1 ? void completeStartup(() => acknowledgeStartup("version")) : setTutorialStep((step) => step + 1)}>{tutorialStep === tutorialSteps.length - 1 ? "Finish" : "Next"}</button></div>
      </Popup> : startup?.changelog ? <Popup className="confirm-modal changelog-modal" backdropClassName="confirm-modal-backdrop" labelledBy="changelog-title">
        <h2 id="changelog-title">What's changed in RM</h2><ReleaseNotes markdown={startup.changelog} /><button className="account-button" type="button" disabled={isStartupPending} onClick={() => void completeStartup(() => acknowledgeStartup("version"))}>Continue</button>
      </Popup> : startup?.passwordlessOffer ? <ConfirmModal title="Stop asking for a password on this PC?" message="Device encryption keeps your store encrypted and unlocks it through Windows Credential Manager. Keep a master password if you need to move the store between PCs." confirmLabel={isStartupPending ? "Changing encryption..." : "Use device encryption"} confirmDisabled={isStartupPending} confirmIcon="lock" onConfirm={() => void completeStartup(async () => { await clearPassword(); await acknowledgeStartup("passwordless"); })} onCancel={() => { if (!isStartupPending) void completeStartup(() => acknowledgeStartup("passwordless")); }} /> : browserPlaceId ? <ConfirmModal title="Launch this game through RM?" message={`The account browser blocked an external Roblox launch for Place ID ${browserPlaceId}. Prefill it in Accounts, then choose which account to launch.`} confirmLabel="Prefill Place ID" confirmIcon="game" onConfirm={() => { setPrefilledPlaceId(browserPlaceId); setBrowserPlaceId(null); setActiveNav("Accounts"); }} onCancel={() => setBrowserPlaceId(null)} /> : null}
      {startupError && <Toast item={{ id: 1, title: "Startup action needs attention", message: startupError, kind: "error", duration: "long" }} onDismiss={() => setStartupError(null)} />}
    </div>
  );
}
