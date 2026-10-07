import { useEffect, useState } from "react";
import { Icon } from "./Icon";
import { Popup } from "./Popup";
import { getDiscordNotifications, saveDiscordNotifications, saveDiscordWebhook, removeDiscordWebhook, testDiscordWebhook, operationError, type DiscordNotificationSettings } from "../lib/ipc";

export function DiscordNotifications({ hasWebhook, onConfiguredChange, onBusyChange }: {
  hasWebhook: boolean;
  onConfiguredChange: (configured: boolean) => void;
  onBusyChange: (busy: boolean) => void;
}) {
  const [settings, setSettings] = useState<DiscordNotificationSettings | null>(null);
  const [busy, setBusy] = useState<"load" | "save" | "test" | "remove" | "preferences" | null>("load");
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState("");
  const [modalOpen, setModalOpen] = useState(false);
  const [url, setUrl] = useState("");
  useEffect(() => {
    let active = true;
    getDiscordNotifications().then((value) => { if (active) setSettings(value); }).catch((reason) => { if (active) setError(operationError(reason, "Couldn’t load notification settings. Try again.")); }).finally(() => { if (active) setBusy(null); });
    return () => { active = false; };
  }, []);
  useEffect(() => { onBusyChange(busy !== null); return () => onBusyChange(false); }, [busy, onBusyChange]);

  async function run(action: Exclude<typeof busy, null>, operation: () => Promise<void>) {
    if (busy) return;
    setBusy(action);
    setError("");
    setFeedback("");
    try { await operation(); }
    catch (reason) { setError(operationError(reason, "Couldn’t update Discord. Try again or replace the webhook.")); }
    finally { setBusy(null); }
  }
  function close() { if (!busy) { setModalOpen(false); setUrl(""); } }
  function save() {
    void run("save", async () => {
      await saveDiscordWebhook(url.trim());
      onConfiguredChange(true);
      setModalOpen(false);
      setUrl("");
      setFeedback("Webhook saved. Send a test to check it works.");
    });
  }
  function test(candidate = "") {
    void run("test", async () => { await testDiscordWebhook(candidate); setFeedback("Test sent. Check your Discord channel."); });
  }
  function preference(key: keyof DiscordNotificationSettings, enabled: boolean) {
    if (!settings) return;
    const next = { ...settings, [key]: enabled };
    void run("preferences", async () => { await saveDiscordNotifications(next); setSettings(next); setFeedback("Notification preference saved."); });
  }
  return <div className="discord-notifications" aria-busy={busy !== null}>
    <div className="settings-action-row">
      <span className="settings-muted">{hasWebhook ? "Webhook configured" : "No webhook configured"}</span>
      <button className="account-button" type="button" disabled={busy !== null} onClick={() => { setUrl(""); setError(""); setFeedback(""); setModalOpen(true); }}>{hasWebhook ? "Replace webhook" : "Add webhook"}</button>
      {hasWebhook && <>
        <button className="account-button" type="button" disabled={busy !== null} onClick={() => test()}>{busy === "test" ? "Sending..." : "Test saved webhook"}</button>
        <button className="account-button danger" type="button" disabled={busy !== null} onClick={() => void run("remove", async () => { await removeDiscordWebhook(); onConfiguredChange(false); setFeedback("Saved webhook removed. Discord notifications are disconnected."); })}>{busy === "remove" ? "Removing..." : "Remove"}</button>
      </>}
    </div>
    <p className="settings-muted">Choose which events reach Discord. Preferences save immediately. RM must be running; moderation is detected during account refreshes.</p>
    <div className="discord-event-options">
      <label className={`settings-toggle ${busy !== null || !settings ? "is-disabled" : ""}`}><input type="checkbox" checked={settings?.moderationDetected ?? false} disabled={busy !== null || !settings} onChange={(event) => preference("moderationDetected", event.target.checked)} /><span>Moderation detected</span></label>
      <p className="settings-muted">New or changed account restrictions; repeated checks do not repeat the same alert.</p>
      <label className={`settings-toggle ${busy !== null || !settings ? "is-disabled" : ""}`}><input type="checkbox" checked={settings?.batchLaunchFinished ?? false} disabled={busy !== null || !settings} onChange={(event) => preference("batchLaunchFinished", event.target.checked)} /><span>Batch launch finished</span></label>
      <p className="settings-muted">Completed or stopped batches from Accounts, Presets and Private Servers. Reports launch requests, while clients may still be connecting.</p>
    </div>
    {!hasWebhook && <p className="settings-muted">Add a webhook to start receiving these events.</p>}
    {busy === "load" && <p role="status">Loading notification preferences...</p>}
    {!modalOpen && error && <p role="alert">{error}</p>}
    {!modalOpen && feedback && <p role="status">{feedback}</p>}
    {!settings && busy === null && <div className="settings-action-row">
      <button className="account-button" type="button" onClick={() => void run("load", async () => { setSettings(await getDiscordNotifications()); })}>Retry loading preferences</button>
      <button className="account-button" type="button" onClick={() => void run("preferences", async () => { const defaults = { moderationDetected: true, batchLaunchFinished: true }; await saveDiscordNotifications(defaults); setSettings(defaults); setFeedback("Default event preferences restored."); })}>Restore default event preferences</button>
    </div>}
    {modalOpen && <Popup className="confirm-modal settings-webhook-modal" backdropClassName="confirm-modal-backdrop" onClose={close} closeOnBackdrop labelledBy="settings-webhook-title" describedBy="settings-webhook-description" busy={busy !== null}>
      <div className="confirm-modal-header"><h2 id="settings-webhook-title">{hasWebhook ? "Replace Discord webhook" : "Add Discord webhook"}</h2><button className="icon-button" type="button" aria-label="Close webhook dialog" disabled={busy !== null} onClick={close}><Icon name="close" /></button></div>
      <p id="settings-webhook-description">Create a webhook for a Discord text channel and paste its URL here. RM stores the URL in Windows Credential Manager. Saving replaces the current destination.</p>
      <label htmlFor="discord-webhook-url">Webhook URL</label>
      <input id="discord-webhook-url" type="password" autoComplete="off" spellCheck={false} placeholder="https://discord.com/api/webhooks/..." value={url} disabled={busy !== null} onChange={(event) => setUrl(event.target.value)} />
      {error && <p role="alert">{error}</p>}
      {feedback && <p role="status">{feedback}</p>}
      <div className="confirm-modal-actions"><button className="account-button" type="button" disabled={!url.trim() || busy !== null} onClick={() => test(url.trim())}>{busy === "test" ? "Sending..." : "Test URL"}</button><button className="account-button" type="button" disabled={!url.trim() || busy !== null} onClick={save}>{busy === "save" ? "Saving..." : "Save webhook"}</button></div>
    </Popup>}
  </div>;
}
