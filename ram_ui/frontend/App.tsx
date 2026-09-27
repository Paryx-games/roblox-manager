import { useEffect, useState, type MouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { InstancesPage } from "./InstancesPage";
import { Toast, type ToastItem } from "./Toast";
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
    </div>
  );
}
