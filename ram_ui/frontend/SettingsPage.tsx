import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ConfirmModal } from "./ConfirmModal";
import { Icon } from "./components/Icon";
import { Popup } from "./components/Popup";
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

type Notice = {
  kind: "success" | "error" | "info";
  message: string;
};

const LOG_LEVELS: LogLevel[] = ["Error", "Warn", "Info", "Debug", "Trace"];

function draftFromConfig(config: SettingsConfig): SettingsUpdate {
  const draft = { ...config } as SettingsUpdate;
  delete (draft as Partial<SettingsConfig>).startupWithWindows;
  delete (draft as Partial<SettingsConfig>).robloxFastFlags;
  return draft;
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
  if (!card) return null;
  return (
    <button
      className={`settings-info settings-info-${card.kind}`}
      type="button"
      aria-label={`${settingTitle(card.kind)} information for ${referenceId}`}
      title={card.text}
    >
      <Icon name="shield-question-mark" />
    </button>
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
      <span className="settings-toggle-box" aria-hidden="true">
        <Icon name="check" />
      </span>
      <span>{label}</span>
    </label>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h2>{title}</h2>
      {children}
    </section>
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

function SettingsNotice({ notice }: { notice: Notice | null }) {
  if (!notice) return null;
  return (
    <div className={`settings-notice settings-notice-${notice.kind}`} role="status">
      <Icon name={notice.kind === "error" ? "warning" : "update"} />
      <span>{notice.message}</span>
    </div>
  );
}

export function SettingsPage() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<SettingsUpdate | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [pendingLogLevel, setPendingLogLevel] = useState<LogLevel | null>(null);
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [webhookModalOpen, setWebhookModalOpen] = useState(false);
  const [webhookUrl, setWebhookUrl] = useState("");
  const [webhookBusy, setWebhookBusy] = useState<"save" | "test" | null>(null);

  async function loadSettings() {
    setIsLoading(true);
    try {
      const nextSnapshot = await getSettings();
      setSnapshot(nextSnapshot);
      setDraft(draftFromConfig(nextSnapshot.config));
      setNotice(null);
    } catch (error) {
      setNotice({ kind: "error", message: String(error) });
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void loadSettings();
  }, []);

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

  function updateDraft(change: Partial<SettingsUpdate>) {
    setDraft((current) => (current ? { ...current, ...change } : current));
  }

  function showError(error: unknown) {
    setNotice({ kind: "error", message: String(error) });
  }

  async function handleSave() {
    if (!draft || !snapshot) return;
    const shouldRestart = draft.logLevel !== snapshot.config.logLevel;
    setIsSaving(true);
    try {
      const config = await saveSettings(draft);
      setSnapshot((current) => (current ? { ...current, config } : current));
      setDraft(draftFromConfig(config));
      if (shouldRestart) {
        setNotice({ kind: "info", message: "Restarting RM to apply the log level" });
        await restartApp();
      } else {
        setNotice({ kind: "success", message: "Settings saved" });
      }
    } catch (error) {
      showError(error);
    } finally {
      setIsSaving(false);
    }
  }

  async function handleStartupChange(enabled: boolean) {
    if (!snapshot) return;
    const previous = snapshot.config.startupWithWindows;
    setSnapshot((current) =>
      current
        ? { ...current, config: { ...current.config, startupWithWindows: enabled } }
        : current,
    );
    setBusyAction("startup");
    try {
      await setStartupWithWindows(enabled);
      setNotice({ kind: "success", message: "Windows startup setting updated" });
    } catch (error) {
      setSnapshot((current) =>
        current
          ? { ...current, config: { ...current.config, startupWithWindows: previous } }
          : current,
      );
      showError(error);
    } finally {
      setBusyAction(null);
    }
  }

  async function handleMultiInstanceChange(enabled: boolean) {
    if (!draft) return;
    const nextDraft = { ...draft, multiInstanceEnabled: enabled };
    updateDraft({ multiInstanceEnabled: enabled });
    if (!enabled) {
      setBusyAction("multi-instance");
      try {
        await saveSettings(nextDraft);
        setSnapshot((current) =>
          current
            ? { ...current, config: { ...current.config, multiInstanceEnabled: false } }
            : current,
        );
        setNotice({ kind: "success", message: "Multi-instance disabled" });
      } catch (error) {
        updateDraft({ multiInstanceEnabled: true });
        showError(error);
      } finally {
        setBusyAction(null);
      }
      return;
    }
    setBusyAction("multi-instance");
    try {
      await enableMultiInstance();
      setSnapshot((current) =>
        current
          ? { ...current, config: { ...current.config, multiInstanceEnabled: true } }
          : current,
      );
      setNotice({ kind: "success", message: "Multi-instance enabled" });
    } catch (error) {
      updateDraft({ multiInstanceEnabled: false });
      showError(error);
    } finally {
      setBusyAction(null);
    }
  }

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
      setNotice({ kind: "success", message });
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
      setNewPassword("");
      setConfirmPassword("");
      setNotice({ kind: "success", message: "Master password updated" });
      await loadSettings();
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
      setNewPassword("");
      setConfirmPassword("");
      setNotice({ kind: "success", message: "RM will stop asking for a password" });
      await loadSettings();
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
      setNotice({ kind: "success", message: "Discord webhook saved" });
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
      setNotice({ kind: "success", message: "Discord webhook test sent" });
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
      setNotice({ kind: "success", message: "Discord webhook removed" });
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
            <p>{notice?.message ?? "RM could not load its settings."}</p>
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
      <main className="content settings-page">
        <SettingsNotice notice={notice} />

        <Section title="Account storage">
          <SettingRow referenceId="credential_manager" infoCards={infoCards}>
            <Toggle
              checked={draft.useCredentialManager}
              label="Use Windows Credential Manager (instead of encrypted file)"
              onChange={(useCredentialManager) => updateDraft({ useCredentialManager })}
            />
          </SettingRow>
        </Section>

        <Section title="Launching">
          <h3>App startup</h3>
          <SettingRow referenceId="startup_with_windows" infoCards={infoCards}>
            <Toggle
              checked={config.startupWithWindows}
              label="Start RM with Windows"
              disabled={busyAction === "startup"}
              onChange={(enabled) => void handleStartupChange(enabled)}
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

          <h3>Launch safeguards</h3>
          <SettingRow referenceId="multi_instance" infoCards={infoCards}>
            <Toggle
              checked={draft.multiInstanceEnabled}
              label="Enable multi-instance"
              disabled={busyAction === "multi-instance"}
              onChange={(enabled) => void handleMultiInstanceChange(enabled)}
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
          <h3>Window layout</h3>
          <SettingRow referenceId="auto_arrange_windows" infoCards={infoCards}>
            <Toggle
              checked={draft.autoArrangeWindows}
              label="Auto-arrange Roblox windows after launch"
              onChange={(autoArrangeWindows) => updateDraft({ autoArrangeWindows })}
            />
          </SettingRow>
          <div className="settings-indent">
            <SettingRow referenceId="target_display" infoCards={infoCards}>
              <label className="settings-field-row">
                <span>Target Display:</span>
                <select
                  value={currentTarget}
                  onChange={(event) => {
                    const value = event.target.value;
                    const tilingTargetMonitor: MonitorTarget =
                      value === "all"
                        ? { type: "All" }
                        : value.startsWith("index:")
                          ? { type: "Index", value: Number(value.slice(6)) }
                          : { type: "Primary" };
                    updateDraft({ tilingTargetMonitor });
                  }}
                >
                  <option value="primary">Primary Monitor</option>
                  {snapshot.monitors.length > 1 && (
                    <option value="all">All Monitors (Distribute / Span)</option>
                  )}
                  {snapshot.monitors.map((monitor) => (
                    <option key={monitor.index} value={`index:${monitor.index}`}>
                      {monitor.name}
                    </option>
                  ))}
                </select>
              </label>
            </SettingRow>
            <SettingRow referenceId="grid_layout" infoCards={infoCards}>
              <label className="settings-field-row">
                <span>Grid Layout:</span>
                <select
                  value={currentLayout}
                  onChange={(event) => {
                    const value = event.target.value;
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
                >
                  <option value="auto">Auto Grid (Square-like)</option>
                  <option value="columns">Fixed Columns</option>
                  <option value="rows">Fixed Rows</option>
                  <option value="custom">Custom Grid (Cols × Rows)</option>
                  <option value="side-by-side">Side-by-Side (1 Row)</option>
                  <option value="stacked">Stacked (1 Column)</option>
                </select>
              </label>
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
              className="account-button"
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
          <h3>Launch pacing</h3>
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

        <Section title="Privacy and identity">
          <h3>Privacy cleanup</h3>
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
          <h3>Network identity</h3>
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
                  <label className="settings-field-row">
                    <span>Alternate OUI:</span>
                    <select
                      value={draft.macAlternateOui}
                      onChange={(event) => updateDraft({ macAlternateOui: event.target.value })}
                    >
                      <option value="00:1B:21">Intel (00:1B:21)</option>
                      <option value="00:E0:4C">Realtek (00:E0:4C)</option>
                      <option value="3C:52:82">Microsoft (3C:52:82)</option>
                    </select>
                  </label>
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
          <h3>Displayed identity</h3>
          <SettingRow referenceId="anonymize_names" infoCards={infoCards}>
            <Toggle
              checked={draft.anonymizeNames}
              label="Anonymize account names"
              onChange={(anonymizeNames) => updateDraft({ anonymizeNames })}
            />
          </SettingRow>
        </Section>

        <Section title="App and data">
          <h3>Development</h3>
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
          <h3>Logging</h3>
          <SettingRow referenceId="log_level" infoCards={infoCards}>
            <label className="settings-field-row">
              <span>Log level</span>
              <select
                value={draft.logLevel}
                onChange={(event) => {
                  const level = event.target.value as LogLevel;
                  if (level === config.logLevel) {
                    updateDraft({ logLevel: level });
                  } else {
                    setPendingLogLevel(level);
                  }
                }}
              >
                {availableLogLevels.map((level) => (
                  <option key={level} value={level}>
                    {level}
                  </option>
                ))}
              </select>
            </label>
          </SettingRow>
          <button
            className="account-button"
            type="button"
            disabled={busyAction === "orphaned-data"}
            onClick={() => void handleAction("orphaned-data", cleanOrphanedData)}
          >
            Clean orphaned data
          </button>
          {draft.developerOptions && (
            <>
              <WarningText>
                Warning: Uploads are permanent and public. Every asset is moderated under the account that uploaded it.
              </WarningText>
              <button
                className="account-button"
                type="button"
                disabled={busyAction === "caches"}
                onClick={() => void handleAction("caches", clearApplicationCaches)}
              >
                Clear application caches
              </button>
            </>
          )}
          <div className="settings-divider" />
          <h3>Data location</h3>
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

        <Section title="Roblox installation">
          <p className="settings-muted">Leave empty for auto-detect:</p>
          <SettingRow referenceId="roblox_player_path" infoCards={infoCards}>
            <input
              className="settings-wide-input"
              type="text"
              value={draft.robloxPlayerPath ?? ""}
              placeholder="RobloxPlayerBeta.exe or containing folder"
              onChange={(event) => updateDraft({ robloxPlayerPath: event.target.value || null })}
            />
          </SettingRow>
        </Section>

        <Section title="Advanced">
          <h3>Launch arguments</h3>
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
          <h3>Fast flags</h3>
          <SettingRow referenceId="roblox_fast_flags" infoCards={infoCards}>
            {Object.keys(config.robloxFastFlags).length === 0 ? (
              <span className="settings-muted">(empty)</span>
            ) : (
              <div className="settings-fast-flags">
                {Object.entries(config.robloxFastFlags).map(([key, value]) => (
                  <span key={key}>{key} = {value}</span>
                ))}
              </div>
            )}
          </SettingRow>
          <p className="settings-muted">Experimental Roblox ClientSettings toggles; written before launch</p>
        </Section>

        <Section title="Integrations">
          <h3>Discord Notifications</h3>
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

        <button
          className="account-button primary settings-save-button"
          type="button"
          disabled={isSaving}
          onClick={() => void handleSave()}
        >
          <Icon name="save" />
          {isSaving ? "Saving..." : "Save Settings"}
        </button>

        <div className="settings-separator" />

        <Section title="Account encryption">
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
      </main>

      {pendingLogLevel && (
        <ConfirmModal
          title="Change log level?"
          confirmLabel="Proceed"
          confirmIcon="check"
          onCancel={() => setPendingLogLevel(null)}
          onConfirm={() => {
            updateDraft({ logLevel: pendingLogLevel });
            setPendingLogLevel(null);
            setNotice({ kind: "info", message: "Save Settings to apply the new log level." });
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
