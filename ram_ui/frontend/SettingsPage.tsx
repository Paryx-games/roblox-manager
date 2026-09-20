import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { ConfirmModal } from "./ConfirmModal";
import { Icon } from "./components/Icon";
import { Popup } from "./components/Popup";
import Select from "./components/Select";
import { Toast, type ToastItem, type ToastKind } from "./Toast";
import {
  arrangeSettingsWindows,
  changePassword,
  cleanOrphanedData,
  clearApplicationCaches,
  clearPassword,
  enableMultiInstance,
  getSettings,
  openDataFolder,
  removeDiscordWebhook,
  restartApp,
  rotateMacAddress,
  saveDiscordWebhook,
  saveSettings,
  setStartupWithWindows,
  testDiscordWebhook,
  type LogLevel,
  type MonitorTarget,
  type SettingsConfig,
  type SettingsInfoCard,
  type SettingsSnapshot,
  type SettingsUpdate,
  type TilingLayoutMode,
} from "./lib/ipc";

type SettingsDraft = SettingsUpdate & Pick<SettingsConfig, "startupWithWindows">;

type SettingsSectionId =
  | "account-storage"
  | "launching"
  | "privacy-identity"
  | "app-data"
  | "roblox-installation"
  | "advanced"
  | "integrations"
  | "account-encryption";

type SettingsSectionGroup = {
  id: "workspace" | "application";
  label: string;
  sections: Array<{
    id: SettingsSectionId;
    label: string;
    subsections: Array<{ id: string; label: string }>;
  }>;
};

const SETTINGS_SECTION_GROUPS: SettingsSectionGroup[] = [
  {
    id: "workspace",
    label: "Workspace",
    sections: [
      { id: "account-storage", label: "Account storage", subsections: [] },
      {
        id: "launching",
        label: "Launching",
        subsections: [
          { id: "app-startup", label: "App startup" },
          { id: "launch-safeguards", label: "Launch safeguards" },
          { id: "window-layout", label: "Window layout" },
          { id: "launch-pacing", label: "Launch pacing" },
        ],
      },
      {
        id: "privacy-identity",
        label: "Privacy and identity",
        subsections: [
          { id: "privacy-cleanup", label: "Privacy cleanup" },
          { id: "network-identity", label: "Network identity" },
          { id: "displayed-identity", label: "Displayed identity" },
        ],
      },
    ],
  },
  {
    id: "application",
    label: "Application",
    sections: [
      {
        id: "app-data",
        label: "App and data",
        subsections: [
          { id: "development", label: "Development" },
          { id: "logging", label: "Logging" },
          { id: "data-location", label: "Data location" },
        ],
      },
      { id: "roblox-installation", label: "Roblox installation", subsections: [] },
      {
        id: "advanced",
        label: "Advanced",
        subsections: [
          { id: "launch-arguments", label: "Launch arguments" },
          { id: "fast-flags", label: "Fast flags" },
        ],
      },
      {
        id: "integrations",
        label: "Integrations",
        subsections: [{ id: "discord-notifications", label: "Discord notifications" }],
      },
      { id: "account-encryption", label: "Account encryption", subsections: [] },
    ],
  },
];

const LOG_LEVELS: LogLevel[] = ["Error", "Warn", "Info", "Debug", "Trace"];

function draftFromConfig(config: SettingsConfig): SettingsDraft {
  return { ...config };
}

function settingTitle(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function InfoButton({
  referenceId,
  infoCards,
}: {
  referenceId: string;
  infoCards: Record<string, SettingsInfoCard>;
}) {
  const card = infoCards[referenceId];
  const buttonRef = useRef<HTMLButtonElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const tooltipId = useId();
  const [isTooltipVisible, setIsTooltipVisible] = useState(false);
  const [tooltipPosition, setTooltipPosition] = useState({ top: 0, left: 0 });

  useLayoutEffect(() => {
    if (!isTooltipVisible) return;

    const positionTooltip = () => {
      const button = buttonRef.current;
      const tooltip = tooltipRef.current;
      if (!button || !tooltip) return;

      const rootStyles = window.getComputedStyle(document.documentElement);
      const gap = Number.parseFloat(rootStyles.getPropertyValue("--space-2"));
      const viewportInset = Number.parseFloat(
        rootStyles.getPropertyValue("--space-2"),
      );
      const buttonBounds = button.getBoundingClientRect();
      const tooltipBounds = tooltip.getBoundingClientRect();
      const leftCandidate = buttonBounds.left - tooltipBounds.width - gap;
      const rightCandidate = buttonBounds.right + gap;
      const left =
        leftCandidate >= viewportInset
          ? leftCandidate
          : Math.min(
              rightCandidate,
              window.innerWidth - tooltipBounds.width - viewportInset,
            );
      const centeredTop =
        buttonBounds.top + buttonBounds.height / 2 - tooltipBounds.height / 2;
      const top = Math.min(
        Math.max(centeredTop, viewportInset),
        window.innerHeight - tooltipBounds.height - viewportInset,
      );
      setTooltipPosition({ top, left: Math.max(viewportInset, left) });
    };

    positionTooltip();
    window.addEventListener("resize", positionTooltip);
    window.addEventListener("scroll", positionTooltip, true);
    return () => {
      window.removeEventListener("resize", positionTooltip);
      window.removeEventListener("scroll", positionTooltip, true);
    };
  }, [isTooltipVisible]);

  if (!card) return null;
  return (
    <>
      <button
        ref={buttonRef}
        className={`settings-info settings-info-${card.kind}`}
        type="button"
        aria-label={`${settingTitle(card.kind)}: ${card.text}`}
        aria-describedby={isTooltipVisible ? tooltipId : undefined}
        onBlur={() => setIsTooltipVisible(false)}
        onFocus={() => setIsTooltipVisible(true)}
        onMouseEnter={() => setIsTooltipVisible(true)}
        onMouseLeave={() => setIsTooltipVisible(false)}
      >
        <Icon name="shield-question-mark" tone="current-color" />
      </button>
      {isTooltipVisible &&
        createPortal(
          <div
            ref={tooltipRef}
            id={tooltipId}
            className="settings-tooltip"
            role="tooltip"
            style={
              {
                "--settings-tooltip-left": `${tooltipPosition.left}px`,
                "--settings-tooltip-top": `${tooltipPosition.top}px`,
              } as CSSProperties
            }
          >
            {card.text}
          </div>,
          document.body,
        )}
    </>
  );
}

function SettingRow({
  referenceId,
  infoCards,
  children,
}: {
  referenceId?: string;
  infoCards: Record<string, SettingsInfoCard>;
  children: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-content">{children}</div>
      {referenceId && (
        <InfoButton referenceId={referenceId} infoCards={infoCards} />
      )}
    </div>
  );
}

function Toggle({
  checked,
  label,
  disabled = false,
  onChange,
}: {
  checked: boolean;
  label: string;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className={`settings-toggle ${disabled ? "is-disabled" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>{label}</span>
    </label>
  );
}

function Section({
  id,
  title,
  children,
}: {
  id: SettingsSectionId;
  title: string;
  children: ReactNode;
}) {
  return (
    <section
      id={`settings-section-${id}`}
      className="settings-section"
      data-settings-anchor={id}
    >
      <h2>{title}</h2>
      <div className="settings-section-body">{children}</div>
    </section>
  );
}

function SubsectionHeading({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h3 id={`settings-subsection-${id}`} data-settings-anchor={id}>
      {children}
    </h3>
  );
}

function SettingsSidebar({
  activeAnchor,
  onNavigate,
}: {
  activeAnchor: string;
  onNavigate: (anchorId: string) => void;
}) {
  return (
    <nav className="settings-sidebar" aria-label="Settings sections">
      {SETTINGS_SECTION_GROUPS.flatMap((group) => group.sections).map((section) => (
                <div className="settings-sidebar-item" key={section.id}>
                  <button
                    className={`settings-sidebar-link ${
                      activeAnchor === section.id ? "is-active" : ""
                    }`}
                    type="button"
                    aria-current={activeAnchor === section.id ? "location" : undefined}
                    onClick={() => onNavigate(section.id)}
                  >
                    {section.label}
                  </button>
                  {section.subsections.length > 0 && (
                    <div className="settings-sidebar-subsections">
                      {section.subsections.map((subsection) => (
                        <button
                          className={`settings-sidebar-sublink ${
                            activeAnchor === subsection.id ? "is-active" : ""
                          }`}
                          type="button"
                          aria-current={
                            activeAnchor === subsection.id ? "location" : undefined
                          }
                          onClick={() => onNavigate(subsection.id)}
                          key={subsection.id}
                        >
                          {subsection.label}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
      ))}
    </nav>
  );
}

function WarningText({ children }: { children: ReactNode }) {
  return <p className="settings-warning">{children}</p>;
}

function targetValue(target: MonitorTarget) {
  if (target.type === "All") return "all";
  if (target.type === "Index") return `index:${target.value}`;
  return "primary";
}

function layoutValue(layout: TilingLayoutMode) {
  if (layout.type === "FixedColumns") return "columns";
  if (layout.type === "FixedRows") return "rows";
  if (layout.type === "CustomGrid") return "custom";
  if (layout.type === "SideBySide") return "side-by-side";
  if (layout.type === "Stacked") return "stacked";
  return "auto";
}

function SettingsContext({
  snapshot,
  isDirty,
  isRefreshingStatus,
  onRefreshStatus,
}: {
  snapshot: SettingsSnapshot;
  isDirty: boolean;
  isRefreshingStatus: boolean;
  onRefreshStatus: () => void;
}) {
  const primaryMonitor = snapshot.monitors.find((monitor) => monitor.is_primary);

  return (
    <aside className="settings-context" aria-label="System and app information">
      <section className="settings-context-card">
        <div className="settings-context-heading">
          <h2>System status</h2>
          <button className="settings-context-refresh" type="button" disabled={isRefreshingStatus} aria-label="Refresh system status" onClick={onRefreshStatus}>
            <Icon name="refresh" tone="current-color" />
          </button>
        </div>
        <dl>
          <div><dt>Roblox</dt><dd>{snapshot.robloxRunning ? "Running" : "Not running"}</dd></div>
          <div><dt>Displays</dt><dd>{snapshot.monitors.length} connected</dd></div>
          <div><dt>Settings</dt><dd>{isDirty ? "Unsaved changes" : "Up to date"}</dd></div>
        </dl>
      </section>
      <section className="settings-context-card">
        <h2>System info</h2>
        <dl>
          <div><dt>Platform</dt><dd>Windows</dd></div>
          <div><dt>Architecture</dt><dd>{snapshot.systemArchitecture}</dd></div>
          {primaryMonitor && <div><dt>Primary display</dt><dd>{primaryMonitor.total_w} × {primaryMonitor.total_h}</dd></div>}
        </dl>
      </section>
      <section className="settings-context-card">
        <h2>App info</h2>
        <dl>
          <div><dt>Application</dt><dd>Roblox Manager</dd></div>
          <div><dt>Version</dt><dd>v{snapshot.appVersion}</dd></div>
          <div><dt>Account lock</dt><dd>{snapshot.hasPassword ? "Password" : "Device"}</dd></div>
          <div><dt>Discord webhook</dt><dd>{snapshot.hasDiscordWebhook ? "Connected" : "Not configured"}</dd></div>
        </dl>
      </section>
    </aside>
  );
}

export function SettingsPage() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<SettingsDraft | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [notices, setNotices] = useState<ToastItem[]>([]);
  const nextNoticeId = useRef(0);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isRefreshingStatus, setIsRefreshingStatus] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [pendingLogLevel, setPendingLogLevel] = useState<LogLevel | null>(null);
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [webhookModalOpen, setWebhookModalOpen] = useState(false);
  const [webhookUrl, setWebhookUrl] = useState("");
  const [webhookBusy, setWebhookBusy] = useState<"save" | "test" | null>(null);
  const [newFlagName, setNewFlagName] = useState("");
  const [newFlagValue, setNewFlagValue] = useState("");
  const [isAddingFlag, setIsAddingFlag] = useState(false);
  const [activeAnchor, setActiveAnchor] = useState("account-storage");
  const settingsPageRef = useRef<HTMLElement>(null);

  const notify = useCallback((kind: ToastKind, message: string) => {
    const id = nextNoticeId.current;
    nextNoticeId.current += 1;
    setNotices((current) => [
      ...current,
      { id, kind, title: kind === "error" ? "Settings error" : kind === "info" ? "Settings" : "Done", message, duration: "standard" },
    ]);
  }, []);

  async function loadSettings() {
    setIsLoading(true);
    try {
      const nextSnapshot = await getSettings();
      setSnapshot(nextSnapshot);
      setDraft(draftFromConfig(nextSnapshot.config));
      setLoadError(null);
    } catch (error) {
      setLoadError(String(error));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void loadSettings();
  }, []);

  useEffect(() => {
    if (isLoading) return;
    const animationFrame = window.requestAnimationFrame(() => {
      settingsPageRef.current?.scrollTo({ top: 0, behavior: "auto" });
    });
    return () => window.cancelAnimationFrame(animationFrame);
  }, [isLoading]);

  useEffect(() => {
    const page = settingsPageRef.current;
    if (!page || isLoading) return;

    const updateViewportHeight = () => {
      page.style.setProperty(
        "--settings-scroll-viewport-height",
        `${page.clientHeight}px`,
      );
    };
    const observer = new ResizeObserver(updateViewportHeight);
    updateViewportHeight();
    observer.observe(page);
    return () => {
      observer.disconnect();
      page.style.removeProperty("--settings-scroll-viewport-height");
    };
  }, [isLoading]);

  const passwordsMatch =
    newPassword.length > 0 && newPassword === confirmPassword;
  const passwordMismatch =
    newPassword.length > 0 &&
    confirmPassword.length > 0 &&
    newPassword !== confirmPassword;
  const availableLogLevels = useMemo(
    () =>
      LOG_LEVELS.filter(
        (level) => level !== "Debug" || import.meta.env.DEV,
      ),
    [],
  );
  const savedDraft = useMemo(
    () => (snapshot ? draftFromConfig(snapshot.config) : null),
    [snapshot],
  );
  const isDirty = useMemo(
    () =>
      Boolean(
        draft && savedDraft && JSON.stringify(draft) !== JSON.stringify(savedDraft),
      ),
    [draft, savedDraft],
  );

  function updateDraft(change: Partial<SettingsDraft>) {
    setDraft((current) => (current ? { ...current, ...change } : current));
  }

  function addFastFlag() {
    const name = newFlagName.trim();
    if (!draft || !/^[A-Za-z0-9_]{1,128}$/.test(name) || name in draft.robloxFastFlags ||
      newFlagValue.length > 4096 || /[\0\r\n]/.test(newFlagValue)) {
      notify("error", "Enter a unique flag name using letters, numbers, or underscores and a valid value.");
      return;
    }
    updateDraft({ robloxFastFlags: { ...draft.robloxFastFlags, [name]: newFlagValue } });
    setNewFlagName("");
    setNewFlagValue("");
    setIsAddingFlag(false);
    setNotices([]);
  }

  function removeFastFlag(name: string) {
    if (!draft) return;
    const robloxFastFlags = { ...draft.robloxFastFlags };
    delete robloxFastFlags[name];
    updateDraft({ robloxFastFlags });
  }

  function navigateToAnchor(anchorId: string) {
    setActiveAnchor(anchorId);
    window.requestAnimationFrame(() => {
      const page = settingsPageRef.current;
      const anchor = page?.querySelector<HTMLElement>(
        `[data-settings-anchor="${anchorId}"]`,
      );
      if (!page || !anchor) return;
      const scrollPadding = Number.parseFloat(
        window.getComputedStyle(page).scrollPaddingTop,
      ) || 0;
      const top =
        anchor.getBoundingClientRect().top -
        page.getBoundingClientRect().top +
        page.scrollTop -
        scrollPadding;
      page.scrollTo({
        top,
        behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches
          ? "auto"
          : "smooth",
      });
    });
  }

  function handleCancelChanges() {
    if (!savedDraft) return;
    setDraft(savedDraft);
    setPendingLogLevel(null);
    setIsAddingFlag(false);
    setNewFlagName("");
    setNewFlagValue("");
    setNotices([]);
  }

  function showError(error: unknown) {
    notify("error", String(error));
  }

  const handleSave = useCallback(async () => {
    if (!draft || !snapshot || !isDirty || isSaving) return;
    const shouldRestart = draft.logLevel !== snapshot.config.logLevel;
    const { startupWithWindows, ...settingsUpdate } = draft;
    setIsSaving(true);
    try {
      if (draft.multiInstanceEnabled && !snapshot.config.multiInstanceEnabled) {
        await enableMultiInstance();
      }
      if (startupWithWindows !== snapshot.config.startupWithWindows) {
        await setStartupWithWindows(startupWithWindows);
      }
      const config = await saveSettings(settingsUpdate);
      setSnapshot((current) => (current ? { ...current, config } : current));
      setDraft(draftFromConfig(config));
      if (shouldRestart) {
        notify("info", "Restarting RM to apply the log level");
        await restartApp();
      } else {
        notify("success", "Settings saved");
      }
    } catch (error) {
      try {
        setSnapshot(await getSettings());
      } catch {
        // keep the last known settings so the draft remains available
      }
      notify("error", String(error));
    } finally {
      setIsSaving(false);
    }
  }, [draft, snapshot, isDirty, isSaving, notify]);

  async function refreshSystemStatus() {
    if (isRefreshingStatus) return;
    setIsRefreshingStatus(true);
    try {
      const currentStatus = await getSettings();
      setSnapshot((current) => current ? {
        ...current,
        robloxRunning: currentStatus.robloxRunning,
        monitors: currentStatus.monitors,
      } : current);
    } catch (error) {
      showError(error);
    } finally {
      setIsRefreshingStatus(false);
    }
  }

  useEffect(() => {
    const page = settingsPageRef.current;
    if (!page || isLoading) return;

    const updateActiveAnchor = () => {
      const scrollPadding = Number.parseFloat(
        window.getComputedStyle(page).scrollPaddingTop,
      ) || 0;
      const threshold = page.getBoundingClientRect().top + scrollPadding;
      const anchors = Array.from(
        page.querySelectorAll<HTMLElement>("[data-settings-anchor]"),
      );
      const isAtBottom =
        page.scrollTop + page.clientHeight >= page.scrollHeight - scrollPadding;
      if (isAtBottom) {
        const finalAnchorId = anchors[anchors.length - 1]?.dataset.settingsAnchor;
        if (finalAnchorId) setActiveAnchor(finalAnchorId);
        return;
      }
      const visibleAnchor = anchors.reduce<HTMLElement | null>((current, anchor) => {
        if (anchor.getBoundingClientRect().top > threshold) return current;
        return anchor;
      }, anchors[0] ?? null);
      const anchorId = visibleAnchor?.dataset.settingsAnchor;
      if (anchorId) setActiveAnchor(anchorId);
    };

    updateActiveAnchor();
    page.addEventListener("scroll", updateActiveAnchor, { passive: true });
    return () => page.removeEventListener("scroll", updateActiveAnchor);
  }, [isLoading, snapshot]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "s") return;
      if (!isDirty || isSaving || !draft || !snapshot) return;
      event.preventDefault();
      void handleSave();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleSave, isDirty, isSaving]);

  async function handleAction(action: string, callback: () => Promise<unknown>) {
    setBusyAction(action);
    try {
      const result = await callback();
      const message =
        action === "orphaned-data"
          ? `${String(result)} orphaned profile(s) removed`
          : action === "caches"
            ? "Application caches cleared"
            : action === "tile"
              ? "Roblox windows arranged"
              : action === "mac"
                ? "MAC address rotated"
                : "Action completed";
      notify("success", message);
    } catch (error) {
      showError(error);
    } finally {
      setBusyAction(null);
    }
  }

  async function handlePasswordChange() {
    if (!passwordsMatch) return;
    setBusyAction("password");
    try {
      await changePassword(newPassword);
      setSnapshot(await getSettings());
      setNewPassword("");
      setConfirmPassword("");
      notify("success", "Master password updated");
    } catch (error) {
      showError(error);
    } finally {
      setBusyAction(null);
    }
  }

  async function handleClearPassword() {
    setBusyAction("password");
    try {
      await clearPassword();
      setSnapshot(await getSettings());
      setNewPassword("");
      setConfirmPassword("");
      notify("success", "RM will stop asking for a password");
    } catch (error) {
      showError(error);
    } finally {
      setBusyAction(null);
    }
  }

  async function handleWebhookSave() {
    setWebhookBusy("save");
    try {
      await saveDiscordWebhook(webhookUrl.trim());
      setWebhookModalOpen(false);
      setWebhookUrl("");
      setSnapshot((current) =>
        current ? { ...current, hasDiscordWebhook: true } : current,
      );
      notify("success", "Discord webhook saved");
    } catch (error) {
      showError(error);
    } finally {
      setWebhookBusy(null);
    }
  }

  async function handleWebhookTest() {
    setWebhookBusy("test");
    try {
      await testDiscordWebhook(webhookUrl.trim());
      notify("success", "Discord webhook test sent");
    } catch (error) {
      showError(error);
    } finally {
      setWebhookBusy(null);
    }
  }

  async function handleWebhookRemove() {
    setBusyAction("webhook");
    try {
      await removeDiscordWebhook();
      setSnapshot((current) =>
        current ? { ...current, hasDiscordWebhook: false } : current,
      );
      notify("success", "Discord webhook removed");
    } catch (error) {
      showError(error);
    } finally {
      setBusyAction(null);
    }
  }

  if (isLoading) {
    return (
      <>
        <div className="header-row">
          <h1 className="header-title">Settings</h1>
        </div>
        <main className="content settings-page">
          <div className="settings-loading" aria-live="polite">
            <span className="settings-loading-bar" />
            <span className="settings-loading-bar" />
            <span className="settings-loading-bar" />
          </div>
        </main>
      </>
    );
  }

  if (!snapshot || !draft) {
    return (
      <>
        <div className="header-row">
          <h1 className="header-title">Settings</h1>
        </div>
        <main className="content settings-page">
          <section className="settings-error" role="alert">
            <h2>Settings unavailable</h2>
            <p>{loadError ?? "RM could not load its settings."}</p>
            <button className="account-button primary" type="button" onClick={() => void loadSettings()}>
              <Icon name="refresh" />
              Retry
            </button>
          </section>
        </main>
      </>
    );
  }

  const { config, infoCards } = snapshot;
  const currentLayout = layoutValue(draft.tilingLayoutMode);
  const currentTarget = targetValue(draft.tilingTargetMonitor);
  const passwordLabel = snapshot.hasPassword ? "Change password" : "Set password";

  return (
    <>
      <div className="header-row">
        <h1 className="header-title">Settings</h1>
      </div>
      <main className={`content settings-page ${isDirty ? "has-unsaved-changes" : ""}`}>
        <div className="settings-layout">
          <SettingsSidebar
            activeAnchor={activeAnchor}
            onNavigate={navigateToAnchor}
          />
          <section ref={settingsPageRef} className="settings-content" aria-label="Settings controls">
            <div className="settings-workspace">
            <div className="settings-content-inner">

        <Section
          id="account-storage"
          title="Account storage"
        >
          <SettingRow referenceId="credential_manager" infoCards={infoCards}>
            <Toggle
              checked={draft.useCredentialManager}
              label="Use Windows Credential Manager (instead of encrypted file)"
              onChange={(useCredentialManager) => updateDraft({ useCredentialManager })}
            />
          </SettingRow>
        </Section>

        <Section
          id="launching"
          title="Launching"
        >
          <SubsectionHeading id="app-startup">App startup</SubsectionHeading>
          <SettingRow referenceId="startup_with_windows" infoCards={infoCards}>
            <Toggle
              checked={draft.startupWithWindows}
              label="Start RM with Windows"
              onChange={(startupWithWindows) => updateDraft({ startupWithWindows })}
            />
          </SettingRow>
          <SettingRow referenceId="refresh_on_startup" infoCards={infoCards}>
            <Toggle
              checked={draft.refreshOnStartup}
              label="Revalidate accounts on startup"
              onChange={(refreshOnStartup) => updateDraft({ refreshOnStartup })}
            />
          </SettingRow>
          <SettingRow referenceId="auto_launch_on_startup" infoCards={infoCards}>
            <div className="settings-stack">
              <Toggle
                checked={draft.autoLaunchOnStartup}
                label="Auto-launch on startup"
                onChange={(autoLaunchOnStartup) => updateDraft({ autoLaunchOnStartup })}
              />
              {draft.autoLaunchOnStartup && (
                <label className="settings-inline-field">
                  <span>Account ID:</span>
                  <input
                    type="number"
                    min="1"
                    value={draft.autoLaunchAccountId ?? ""}
                    onChange={(event) =>
                      updateDraft({
                        autoLaunchAccountId: event.target.value
                          ? Number(event.target.value)
                          : null,
                      })
                    }
                  />
                </label>
              )}
            </div>
          </SettingRow>

          <SubsectionHeading id="launch-safeguards">Launch safeguards</SubsectionHeading>
          <SettingRow referenceId="multi_instance" infoCards={infoCards}>
            <Toggle
              checked={draft.multiInstanceEnabled}
              label="Enable multi-instance"
              onChange={(multiInstanceEnabled) => updateDraft({ multiInstanceEnabled })}
            />
          </SettingRow>
          {draft.multiInstanceEnabled && (
            <WarningText>
              Warning: This interacts with Hyperion anti-cheat and may carry ban risk.
            </WarningText>
          )}
          {!draft.multiInstanceEnabled && snapshot.robloxRunning && (
            <p className="settings-muted">
              Close all Roblox processes (including tray) before enabling.
            </p>
          )}
          <SettingRow referenceId="kill_background_roblox" infoCards={infoCards}>
            <Toggle
              checked={draft.killBackgroundRoblox}
              label="Kill Roblox tray/background processes automatically"
              onChange={(killBackgroundRoblox) => updateDraft({ killBackgroundRoblox })}
            />
          </SettingRow>
          <SettingRow referenceId="confirm_kill_all" infoCards={infoCards}>
            <Toggle
              checked={draft.confirmKillAll}
              label="Confirm before killing all Roblox instances"
              onChange={(confirmKillAll) => updateDraft({ confirmKillAll })}
            />
          </SettingRow>
          {draft.multiInstanceEnabled && !draft.killBackgroundRoblox && (
            <WarningText>
              Warning: recommended when multi-instance is enabled. Tray processes stack up.
            </WarningText>
          )}

          <div className="settings-divider" />
          <SubsectionHeading id="window-layout">Window layout</SubsectionHeading>
          <SettingRow referenceId="auto_arrange_windows" infoCards={infoCards}>
            <Toggle
              checked={draft.autoArrangeWindows}
              label="Auto-arrange Roblox windows after launch"
              onChange={(autoArrangeWindows) => updateDraft({ autoArrangeWindows })}
            />
          </SettingRow>
          <div className="settings-indent">
            <SettingRow referenceId="target_display" infoCards={infoCards}>
              <div className="settings-field-row">
                <span>Target Display:</span>
                <Select
                  ariaLabel="Target display"
                  animated
                  value={currentTarget}
                  options={[
                    { value: "primary", label: "Primary Monitor" },
                    ...(snapshot.monitors.length > 1
                      ? [{ value: "all", label: "All Monitors (Distribute / Span)" }]
                      : []),
                    ...snapshot.monitors.map((monitor) => ({
                      value: `index:${monitor.index}`,
                      label: monitor.name,
                    })),
                  ]}
                  onChange={(value) => {
                    const tilingTargetMonitor: MonitorTarget =
                      value === "all"
                        ? { type: "All" }
                        : value.startsWith("index:")
                          ? { type: "Index", value: Number(value.slice(6)) }
                          : { type: "Primary" };
                    updateDraft({ tilingTargetMonitor });
                  }}
                />
              </div>
            </SettingRow>
            <SettingRow referenceId="grid_layout" infoCards={infoCards}>
              <div className="settings-field-row">
                <span>Grid Layout:</span>
                <Select
                  ariaLabel="Grid layout"
                  animated
                  value={currentLayout}
                  options={[
                    { value: "auto", label: "Auto Grid (Square-like)" },
                    { value: "columns", label: "Fixed Columns" },
                    { value: "rows", label: "Fixed Rows" },
                    { value: "custom", label: "Custom Grid (Cols × Rows)" },
                    { value: "side-by-side", label: "Side-by-Side (1 Row)" },
                    { value: "stacked", label: "Stacked (1 Column)" },
                  ]}
                  onChange={(value) => {
                    const tilingLayoutMode: TilingLayoutMode =
                      value === "columns"
                        ? { type: "FixedColumns", value: draft.tilingCustomCols }
                        : value === "rows"
                          ? { type: "FixedRows", value: draft.tilingCustomRows }
                          : value === "custom"
                            ? {
                                type: "CustomGrid",
                                value: {
                                  cols: draft.tilingCustomCols,
                                  rows: draft.tilingCustomRows,
                                },
                              }
                            : value === "side-by-side"
                              ? { type: "SideBySide" }
                              : value === "stacked"
                                ? { type: "Stacked" }
                                : { type: "Auto" };
                    updateDraft({ tilingLayoutMode });
                  }}
                />
              </div>
            </SettingRow>
            {(currentLayout === "columns" || currentLayout === "custom") && (
              <SettingRow referenceId="layout_dimensions" infoCards={infoCards}>
                <label className="settings-field-row">
                  <span>Columns:</span>
                  <input
                    type="number"
                    min="1"
                    max="12"
                    value={draft.tilingCustomCols}
                    onChange={(event) => {
                      const value = Math.max(1, Math.min(12, Number(event.target.value) || 1));
                      updateDraft({
                        tilingCustomCols: value,
                        tilingLayoutMode:
                          currentLayout === "custom"
                            ? {
                                type: "CustomGrid",
                                value: { cols: value, rows: draft.tilingCustomRows },
                              }
                            : { type: "FixedColumns", value },
                      });
                    }}
                  />
                  {currentLayout === "custom" && (
                    <>
                      <span>Rows:</span>
                      <input
                        type="number"
                        min="1"
                        max="12"
                        value={draft.tilingCustomRows}
                        onChange={(event) => {
                          const value = Math.max(1, Math.min(12, Number(event.target.value) || 1));
                          updateDraft({
                            tilingCustomRows: value,
                            tilingLayoutMode: {
                              type: "CustomGrid",
                              value: { cols: draft.tilingCustomCols, rows: value },
                            },
                          });
                        }}
                      />
                    </>
                  )}
                </label>
              </SettingRow>
            )}
            {currentLayout === "rows" && (
              <SettingRow referenceId="layout_rows" infoCards={infoCards}>
                <label className="settings-field-row">
                  <span>Rows:</span>
                  <input
                    type="number"
                    min="1"
                    max="12"
                    value={draft.tilingCustomRows}
                    onChange={(event) => {
                      const value = Math.max(1, Math.min(12, Number(event.target.value) || 1));
                      updateDraft({
                        tilingCustomRows: value,
                        tilingLayoutMode: { type: "FixedRows", value },
                      });
                    }}
                  />
                </label>
              </SettingRow>
            )}
            <SettingRow referenceId="window_padding" infoCards={infoCards}>
              <label className="settings-field-row">
                <span>Window Padding:</span>
                <input
                  type="number"
                  min="0"
                  max="50"
                  value={draft.tilingPadding}
                  onChange={(event) =>
                    updateDraft({
                      tilingPadding: Math.max(0, Math.min(50, Number(event.target.value) || 0)),
                    })
                  }
                />
                <span>px</span>
              </label>
            </SettingRow>
            <button
              className="account-button settings-tile-button"
              type="button"
              disabled={busyAction === "tile"}
              onClick={() =>
                void handleAction("tile", () =>
                  arrangeSettingsWindows({
                    target_monitor: draft.tilingTargetMonitor,
                    layout_mode: draft.tilingLayoutMode,
                    custom_cols: draft.tilingCustomCols,
                    custom_rows: draft.tilingCustomRows,
                    padding: draft.tilingPadding,
                  }),
                )
              }
            >
              <Icon name="grid" />
              Tile Windows Now
            </button>
          </div>
          <SettingRow referenceId="rename_windows" infoCards={infoCards}>
            <Toggle
              checked={draft.renameRobloxWindows}
              label="Name Roblox windows after their account"
              onChange={(renameRobloxWindows) => updateDraft({ renameRobloxWindows })}
            />
          </SettingRow>
          {draft.renameRobloxWindows && !draft.anonymizeNames && (
            <p className="settings-muted">
              Window titles are readable by any program, and show up in screenshots and streams.
            </p>
          )}
          <div className="settings-divider" />
          <SubsectionHeading id="launch-pacing">Launch pacing</SubsectionHeading>
          <SettingRow referenceId="launch_delay" infoCards={infoCards}>
            <label className="settings-field-row">
              <span>Launch delay:</span>
              <input
                type="number"
                min="0"
                max="300"
                value={draft.launchDelaySecs}
                onChange={(event) =>
                  updateDraft({
                    launchDelaySecs: Math.max(0, Math.min(300, Number(event.target.value) || 0)),
                  })
                }
              />
              <span>s</span>
              <span className="settings-muted">(Roblox rate-limits some IPs)</span>
            </label>
          </SettingRow>
        </Section>

        <Section
          id="privacy-identity"
          title="Privacy and identity"
        >
          <SubsectionHeading id="privacy-cleanup">Privacy cleanup</SubsectionHeading>
          <SettingRow referenceId="privacy_mode" infoCards={infoCards}>
            <Toggle
              checked={draft.privacyMode}
              label="Clean before launch"
              onChange={(privacyMode) => updateDraft({ privacyMode })}
            />
          </SettingRow>
          <div className="settings-indent">
            <SettingRow referenceId="privacy_cookies" infoCards={infoCards}>
              <Toggle
                checked={draft.privacyCleanCookies}
                label="Clean cookies"
                disabled={!draft.privacyMode}
                onChange={(privacyCleanCookies) => updateDraft({ privacyCleanCookies })}
              />
            </SettingRow>
            <SettingRow referenceId="privacy_local_storage" infoCards={infoCards}>
              <Toggle
                checked={draft.privacyCleanLocalStorage}
                label="Clean cookies and LocalStorage"
                disabled={!draft.privacyMode}
                onChange={(privacyCleanLocalStorage) => updateDraft({ privacyCleanLocalStorage })}
              />
            </SettingRow>
            <SettingRow referenceId="privacy_full_profile" infoCards={infoCards}>
              <Toggle
                checked={draft.privacyCleanFullProfile}
                label="Clean full Roblox cache/profile"
                disabled={!draft.privacyMode}
                onChange={(privacyCleanFullProfile) => updateDraft({ privacyCleanFullProfile })}
              />
            </SettingRow>
            <SettingRow referenceId="privacy_on_exit" infoCards={infoCards}>
              <Toggle
                checked={draft.privacyCleanOnExit}
                label="Clean selected privacy data on exit"
                disabled={!draft.privacyMode}
                onChange={(privacyCleanOnExit) => updateDraft({ privacyCleanOnExit })}
              />
            </SettingRow>
            <SettingRow referenceId="privacy_clear_clipboard" infoCards={infoCards}>
              <Toggle
                checked={draft.privacyClearClipboard}
                label="Clear clipboard after launch"
                disabled={!draft.privacyMode}
                onChange={(privacyClearClipboard) => updateDraft({ privacyClearClipboard })}
              />
            </SettingRow>
          </div>
          <div className="settings-divider" />
          <SubsectionHeading id="network-identity">Network identity</SubsectionHeading>
          <SettingRow referenceId="mac_rotation" infoCards={infoCards}>
            <Toggle
              checked={draft.macRotationEnabled}
              label="Enable MAC address rotation"
              onChange={(macRotationEnabled) => updateDraft({ macRotationEnabled })}
            />
          </SettingRow>
          {draft.macRotationEnabled && (
            <div className="settings-indent">
              <SettingRow referenceId="mac_preserve_oui" infoCards={infoCards}>
                <Toggle
                  checked={draft.macPreserveOui}
                  label="Keep this PC's adapter OUI"
                  onChange={(macPreserveOui) => updateDraft({ macPreserveOui })}
                />
              </SettingRow>
              {!draft.macPreserveOui && (
                <SettingRow referenceId="mac_alternate_oui" infoCards={infoCards}>
                  <div className="settings-field-row">
                    <span>Alternate OUI:</span>
                    <Select
                      ariaLabel="Alternate OUI"
                      animated
                      value={draft.macAlternateOui}
                      options={[
                        { value: "00:1B:21", label: "Intel (00:1B:21)" },
                        { value: "00:E0:4C", label: "Realtek (00:E0:4C)" },
                        { value: "3C:52:82", label: "Microsoft (3C:52:82)" },
                      ]}
                      onChange={(macAlternateOui) => updateDraft({ macAlternateOui })}
                    />
                  </div>
                </SettingRow>
              )}
              <button
                className="account-button"
                type="button"
                disabled={busyAction === "mac"}
                onClick={() =>
                  void handleAction("mac", () =>
                    rotateMacAddress(draft.macPreserveOui, draft.macAlternateOui),
                  )
                }
              >
                Rotate MAC address now
              </button>
              <p className="settings-muted">
                The adapter briefly disconnects and Windows may request administrator permission.
              </p>
            </div>
          )}
          <SubsectionHeading id="displayed-identity">Displayed identity</SubsectionHeading>
          <SettingRow referenceId="anonymize_names" infoCards={infoCards}>
            <Toggle
              checked={draft.anonymizeNames}
              label="Anonymize account names"
              onChange={(anonymizeNames) => updateDraft({ anonymizeNames })}
            />
          </SettingRow>
        </Section>

        <Section
          id="app-data"
          title="App and data"
        >
          {draft.utilityEnabled && draft.developerOptions && (
            <WarningText>
              Uploads are permanent and public. Every asset is moderated under the account that uploaded it.
            </WarningText>
          )}
          <SubsectionHeading id="development">Development</SubsectionHeading>
          <SettingRow referenceId="utility_enabled" infoCards={infoCards}>
            <Toggle
              checked={draft.utilityEnabled}
              label="Show the Utility tab"
              onChange={(utilityEnabled) => updateDraft({ utilityEnabled })}
            />
          </SettingRow>
          <SettingRow referenceId="developer_options" infoCards={infoCards}>
            <Toggle
              checked={draft.developerOptions}
              label="Show the Assets tab in Utility"
              onChange={(developerOptions) => updateDraft({ developerOptions })}
            />
          </SettingRow>
          <SubsectionHeading id="logging">Logging</SubsectionHeading>
          <SettingRow referenceId="log_level" infoCards={infoCards}>
            <div className="settings-field-row">
              <span>Log level</span>
              <Select
                ariaLabel="Log level"
                animated
                value={draft.logLevel}
                options={availableLogLevels.map((level) => ({
                  value: level,
                  label: level,
                }))}
                onChange={(value) => {
                  const level = value as LogLevel;
                  if (level === config.logLevel) {
                    updateDraft({ logLevel: level });
                  } else {
                    setPendingLogLevel(level);
                  }
                }}
              />
            </div>
          </SettingRow>
          <button
            className="account-button"
            type="button"
            disabled={busyAction === "orphaned-data"}
            onClick={() => void handleAction("orphaned-data", cleanOrphanedData)}
          >
            Clean orphaned data
          </button>
          <button
            className="account-button"
            type="button"
            disabled={busyAction === "caches"}
            onClick={() => void handleAction("caches", clearApplicationCaches)}
          >
            Clear application caches
          </button>
          <div className="settings-divider" />
          <SubsectionHeading id="data-location">Data location</SubsectionHeading>
          <SettingRow referenceId="data_folder" infoCards={infoCards}>
            <button
              className="account-button"
              type="button"
              onClick={() => void handleAction("data-folder", openDataFolder)}
            >
              <Icon name="folder" />
              Open RM data folder
            </button>
          </SettingRow>
        </Section>

        <Section
          id="roblox-installation"
          title="Roblox installation"
        >
          <SettingRow referenceId="roblox_player_path" infoCards={infoCards}>
            <label className="settings-field-row settings-field-grow">
              <span>Player path</span>
              <input
                className="settings-wide-input"
                type="text"
                value={draft.robloxPlayerPath ?? ""}
                placeholder="Auto-detect RobloxPlayerBeta.exe"
                onChange={(event) =>
                  updateDraft({ robloxPlayerPath: event.target.value || null })
                }
              />
            </label>
          </SettingRow>
          <p className="settings-muted">Leave empty to detect the latest Roblox installation automatically.</p>
        </Section>

        <Section
          id="advanced"
          title="Advanced"
        >
          <SubsectionHeading id="launch-arguments">Launch arguments</SubsectionHeading>
          <SettingRow referenceId="custom_game_args" infoCards={infoCards}>
            <label className="settings-field-row settings-field-grow">
              <span>Custom Roblox args:</span>
              <input
                type="text"
                value={draft.customGameArgs}
                onChange={(event) => updateDraft({ customGameArgs: event.target.value })}
              />
            </label>
          </SettingRow>
          <p className="settings-muted">Examples: -a 3 -t username=... (passed directly to RobloxPlayerBeta.exe)</p>
          <SubsectionHeading id="fast-flags">Fast flags</SubsectionHeading>
          <SettingRow referenceId="roblox_fast_flags" infoCards={infoCards}>
            {Object.keys(draft.robloxFastFlags).length === 0 ? (
              <span className="settings-code-box">(empty)</span>
            ) : (
              <div className="settings-fast-flags">
                {Object.entries(draft.robloxFastFlags).sort(([first], [second]) => first.localeCompare(second)).map(([key, value]) => (
                  <div className="settings-fast-flag" key={key}>
                    <span>{key} = {value}</span>
                    <button type="button" aria-label={`Remove ${key}`} onClick={() => removeFastFlag(key)}>
                      <Icon name="close" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </SettingRow>
          <p className="settings-muted">Experimental Roblox ClientSettings toggles; written before launch</p>
          {isAddingFlag ? (
            <div className="settings-flag-editor">
              <input aria-label="Flag name" placeholder="Flag name" value={newFlagName} onChange={(event) => setNewFlagName(event.target.value)} />
              <input aria-label="Flag value" placeholder="Value" value={newFlagValue} onChange={(event) => setNewFlagValue(event.target.value)} />
              <button className="account-button" type="button" onClick={addFastFlag}>Add</button>
              <button className="account-button" type="button" onClick={() => setIsAddingFlag(false)}>Cancel</button>
            </div>
          ) : (
            <button className="account-button settings-inline-button" type="button" onClick={() => setIsAddingFlag(true)}>
              <Icon name="add" /> Add Flag
            </button>
          )}
        </Section>

        <Section
          id="integrations"
          title="Integrations"
        >
          <SubsectionHeading id="discord-notifications">Discord notifications</SubsectionHeading>
          <SettingRow referenceId="discord_webhook" infoCards={infoCards}>
            <div className="settings-action-row">
              <span className={snapshot.hasDiscordWebhook ? "settings-success" : "settings-muted"}>
                {snapshot.hasDiscordWebhook ? "Webhook configured" : "No webhook configured"}
              </span>
              <button
                className="account-button"
                type="button"
                onClick={() => {
                  setWebhookUrl("");
                  setWebhookModalOpen(true);
                }}
              >
                {snapshot.hasDiscordWebhook ? "Change" : "Add Discord Webhook"}
              </button>
              {snapshot.hasDiscordWebhook && (
                <button
                  className="account-button"
                  type="button"
                  disabled={busyAction === "webhook"}
                  onClick={() => void handleWebhookRemove()}
                >
                  Remove
                </button>
              )}
            </div>
          </SettingRow>
          <p className="settings-muted">Get notifications for launches and account moderation events.</p>
        </Section>

        <Section
          id="account-encryption"
          title="Account encryption"
        >
          <p>
            {snapshot.hasPassword
              ? "Accounts are encrypted with your master password."
              : "Accounts are encrypted and unlock automatically on this PC."}
          </p>
          <p className="settings-muted">
            {snapshot.hasPassword
              ? "RM asks for it every time it starts. If you forget it, the accounts cannot be recovered."
              : "The key is held in Windows Credential Manager, so the file is useless on its own. Anything running as you can still read it."}
          </p>
          <div className="settings-encryption-heading">
            <span>{snapshot.hasPassword ? "Change your master password:" : "Require a master password at startup:"}</span>
            <InfoButton referenceId="encryption_password" infoCards={infoCards} />
          </div>
          <div className="settings-password-fields">
            <input
              type="password"
              placeholder="New password"
              value={newPassword}
              onChange={(event) => setNewPassword(event.target.value)}
            />
            <input
              type="password"
              placeholder="Confirm password"
              value={confirmPassword}
              onChange={(event) => setConfirmPassword(event.target.value)}
            />
          </div>
          {passwordMismatch && <p className="settings-error-text">Passwords do not match.</p>}
          <div className="settings-action-row">
            <button
              className="account-button"
              type="button"
              disabled={!passwordsMatch || busyAction === "password"}
              onClick={() => void handlePasswordChange()}
            >
              {passwordLabel}
            </button>
            {snapshot.hasPassword && (
              <button
                className="account-button"
                type="button"
                disabled={busyAction === "password"}
                onClick={() => void handleClearPassword()}
              >
                Stop asking for a password
              </button>
            )}
          </div>
        </Section>
        <div className="settings-save-bar">
          <button
            className={`account-button ${isDirty ? "primary" : ""}`}
            type="button"
            disabled={!isDirty || isSaving}
            onClick={() => void handleSave()}
          >
            <Icon name="save" />
            {isSaving ? "Saving..." : "Save Settings"}
          </button>
        </div>
            </div>
            <SettingsContext
              snapshot={snapshot}
              isDirty={isDirty}
              isRefreshingStatus={isRefreshingStatus}
              onRefreshStatus={() => void refreshSystemStatus()}
            />
            </div>
          </section>
        </div>
      </main>

      {isDirty && (
        <div className="settings-unsaved-bar">
          <span role="status">You have unsaved changes!</span>
          <div className="settings-unsaved-actions">
            <button className="account-button" type="button" disabled={isSaving} onClick={handleCancelChanges}>
              Cancel
            </button>
            <button className="account-button primary" type="button" disabled={isSaving} onClick={() => void handleSave()}>
              <Icon name="save" />
              {isSaving ? "Saving..." : "Save"}
            </button>
          </div>
        </div>
      )}

      <div className={`toast-stack settings-toast-stack ${isDirty ? "is-above-unsaved" : ""}`} aria-live="polite" aria-label="Notifications">
        {notices.map((item) => (
          <Toast key={item.id} item={item} onDismiss={() => setNotices((current) => current.filter((noticeItem) => noticeItem.id !== item.id))} />
        ))}
      </div>

      {pendingLogLevel && (
        <ConfirmModal
          title="Change log level?"
          confirmLabel="Proceed"
          confirmIcon="check"
          onCancel={() => setPendingLogLevel(null)}
          onConfirm={() => {
            updateDraft({ logLevel: pendingLogLevel });
            setPendingLogLevel(null);
            notify("info", "Save Settings to apply the new log level.");
          }}
          message={
            <>
              <p>
                Switch log level from {config.logLevel} to {pendingLogLevel}?
              </p>
              <p>
                Lower log levels show less detail, while higher levels can create more log noise.
              </p>
              <strong>
                {import.meta.env.DEV
                  ? "Debug is recommended for development builds."
                  : "Info is recommended for release builds."}
              </strong>
              <p>The app will restart once you save the setting.</p>
            </>
          }
        />
      )}

      {webhookModalOpen && (
        <Popup
          className="confirm-modal settings-webhook-modal"
          backdropClassName="confirm-modal-backdrop"
          onClose={() => setWebhookModalOpen(false)}
          closeOnBackdrop
          labelledBy="settings-webhook-title"
          describedBy="settings-webhook-description"
          busy={webhookBusy !== null}
        >
          <div className="confirm-modal-header">
            <h2 id="settings-webhook-title">Discord Webhook</h2>
            <button
              className="icon-button"
              type="button"
              aria-label="Cancel"
              onClick={() => setWebhookModalOpen(false)}
            >
              <Icon name="close" />
            </button>
          </div>
          <p id="settings-webhook-description">
            The URL is stored in Windows Credential Manager and is never shown in the settings page.
          </p>
          <input
            autoFocus
            type="password"
            placeholder="https://discord.com/api/webhooks/..."
            value={webhookUrl}
            onChange={(event) => setWebhookUrl(event.target.value)}
          />
          <div className="confirm-modal-actions">
            <button
              className="account-button"
              type="button"
              disabled={!webhookUrl.trim() || webhookBusy !== null}
              onClick={() => void handleWebhookTest()}
            >
              Test webhook
            </button>
            <button
              className="account-button primary"
              type="button"
              disabled={!webhookUrl.trim() || webhookBusy !== null}
              onClick={() => void handleWebhookSave()}
            >
              Save webhook
            </button>
          </div>
        </Popup>
      )}
    </>
  );
}
