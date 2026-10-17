import { useEffect, useRef, useState } from "react";
import { Icon } from "./Icon";
import { Popup } from "./Popup";
import { ConfirmModal } from "../ConfirmModal";
import { getDiscordNotifications, resetDiscordNotifications, saveDiscordNotifications, saveDiscordDestination, removeDiscordDestination, testDiscordDestination, testDiscordWebhook, operationError, type DiscordNotificationSettings, type DiscordEvent, type SettingsInfoCard } from "../lib/ipc";
import { InfoTip } from "./InfoTip";

const events: { key: DiscordEvent; label: string; info: string }[] = [
  { key: "moderationDetected", label: "Moderation detected", info: "discord_moderation" },
  { key: "batchLaunchFinished", label: "Batch launch finished", info: "discord_batch" },
  { key: "homeLaunchRequested", label: "Roblox home launch requested", info: "discord_home" },
  { key: "sessionExpired", label: "Account session expired", info: "discord_expiry" },
  { key: "launchFailed", label: "Launch request failed", info: "discord_launch_failure" },
  { key: "clientExited", label: "Roblox client exited", info: "discord_client_exit" },
  { key: "friendRequestReceived", label: "Friend request received", info: "discord_friend_received" },
  { key: "friendRequestSent", label: "Friend request sent", info: "discord_friend_sent" },
  { key: "friendAdded", label: "Friend added", info: "discord_friend_added" },
  { key: "friendRemoved", label: "Friend removed", info: "discord_friend_removed" },
  { key: "friendRequestDeclined", label: "Friend request declined", info: "discord_friend_declined" },
  { key: "userBlocked", label: "User blocked", info: "discord_friend_blocked" },
];

export function DiscordNotifications({ hasWebhook, onConfiguredChange, onBusyChange, infoCards }: {
  hasWebhook: boolean;
  onConfiguredChange: (configured: boolean) => void;
  onBusyChange: (busy: boolean) => void;
  infoCards: Record<string, SettingsInfoCard>;
}) {
  const [settings, setSettings] = useState<DiscordNotificationSettings | null>(null);
  const [busy, setBusy] = useState<"load" | "save" | "test" | "remove" | "preferences" | null>("load");
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState("");
  const [modalOpen, setModalOpen] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const pending = useRef(false);
  useEffect(() => {
    let active = true;
    getDiscordNotifications().then((value) => { if (active) setSettings(value); }).catch((reason) => { if (active) setError(operationError(reason, "Couldn’t load notification settings. Try again.")); }).finally(() => { if (active) setBusy(null); });
    return () => { active = false; };
  }, []);
  useEffect(() => { onBusyChange(busy !== null); return () => onBusyChange(false); }, [busy, onBusyChange]);

  async function run(action: Exclude<typeof busy, null>, operation: () => Promise<void>) {
    if (busy || pending.current) return;
    pending.current = true;
    setBusy(action);
    setError("");
    setFeedback("");
    try { await operation(); }
    catch (reason) { setError(operationError(reason, "Couldn’t update Discord. Try again or replace the webhook.")); }
    finally { pending.current = false; setBusy(null); }
  }
  function close() { if (!busy) { setModalOpen(false); setUrl(""); } }
  function save() {
    void run("save", async () => {
      const next = await saveDiscordDestination(editingId, name.trim(), url.trim());
      setSettings(next);
      onConfiguredChange(next.destinations.length > 0);
      setModalOpen(false);
      setUrl("");
      setFeedback("Webhook saved. Send a test to check it works.");
    });
  }
  function test(candidate = "") {
    void run("test", async () => { await testDiscordWebhook(candidate); setFeedback("Test sent. Check your Discord channel."); });
  }
  function preference(key: DiscordEvent | "enabled", enabled: boolean) {
    if (!settings) return;
    const next = { ...settings, [key]: enabled };
    void run("preferences", async () => { await saveDiscordNotifications(next); setSettings(next); setFeedback("Notification preference saved."); });
  }
  function update(next: DiscordNotificationSettings) {
    void run("preferences", async () => { await saveDiscordNotifications(next); setSettings(next); setFeedback("Discord preferences saved."); });
  }
  return <div className="discord-notifications" aria-busy={busy !== null}>
    <div className="settings-action-row">
      <label className="settings-toggle"><input type="checkbox" checked={settings?.enabled ?? false} disabled={busy !== null || !settings} onChange={event => preference("enabled", event.target.checked)} /><span>Enable Discord notifications</span></label>
      <span className="settings-muted">{settings ? `${settings.destinations.length} saved destinations` : hasWebhook ? "Webhook configured" : "No webhook configured"}</span>
      <button className="account-button" type="button" disabled={busy !== null || !settings || settings.destinations.length >= 16} onClick={() => { setEditingId(null); setName(""); setUrl(""); setError(""); setFeedback(""); setModalOpen(true); }}><Icon name="add" />Add webhook</button>
    </div>
    <div className="discord-destination-grid">
      {settings?.destinations.map(destination => <section className="account-card discord-destination" key={destination.id}>
        <div className="settings-action-row"><strong>{destination.name}</strong><label className="settings-toggle"><input type="checkbox" aria-label={`Enable ${destination.name}`} checked={destination.enabled} disabled={busy !== null} onChange={event => update({ ...settings, destinations: settings.destinations.map(item => item.id === destination.id ? { ...item, enabled: event.target.checked } : item) })} /><span>{destination.enabled ? "Enabled" : "Paused"}</span></label></div>
        <div className="settings-action-row">
          <button className="account-button" type="button" disabled={busy !== null} onClick={() => void run("test", async () => { await testDiscordDestination(destination.id); setFeedback(`Test sent to ${destination.name}.`); })}>Test</button>
          <button className="account-button" type="button" disabled={busy !== null} onClick={() => { setEditingId(destination.id); setName(destination.name); setUrl(""); setError(""); setFeedback(""); setModalOpen(true); }}>Replace</button>
          <button className="account-button danger" type="button" disabled={busy !== null} onClick={() => void run("remove", async () => { const next = await removeDiscordDestination(destination.id); setSettings(next); onConfiguredChange(next.destinations.length > 0); setFeedback(`${destination.name} removed.`); })}>Remove</button>
        </div>
      </section>)}
    </div>
    <div className="discord-event-options">
      {events.map(option => <fieldset className="discord-event" key={option.key}>
        <legend><label className="settings-toggle"><input type="checkbox" checked={settings?.[option.key] ?? false} disabled={busy !== null || !settings} onChange={event => preference(option.key, event.target.checked)} /><span>{option.label}</span></label>{infoCards[option.info] && <InfoTip text={infoCards[option.info].text} />}</legend>
        <div className="discord-event-routes" aria-label={`Destinations for ${option.label}`}>
          {settings?.destinations.map(destination => <label className="settings-toggle" key={destination.id}><input type="checkbox" aria-label={`${option.label} to ${destination.name}`} checked={destination.events.includes(option.key)} disabled={busy !== null} onChange={event => update({ ...settings, destinations: settings.destinations.map(item => item.id === destination.id ? { ...item, events: event.target.checked ? [...item.events, option.key] : item.events.filter(key => key !== option.key) } : item) })} /><span>{destination.name}{destination.enabled ? "" : " (paused)"}</span></label>)}
          {!settings?.destinations.length && <span className="settings-muted">Add a webhook to choose destinations.</span>}
        </div>
      </fieldset>)}
    </div>
    {busy === "load" && <p role="status">Loading notification preferences...</p>}
    {!modalOpen && error && <p role="alert">{error}</p>}
    {!modalOpen && feedback && <p role="status">{feedback}</p>}
    {!settings && busy === null && <div className="settings-action-row">
      <button className="account-button" type="button" onClick={() => void run("load", async () => { setSettings(await getDiscordNotifications()); })}>Retry loading preferences</button>
      <button className="account-button danger" type="button" onClick={() => setConfirmReset(true)}>Reset notification preferences</button>
    </div>}
    {confirmReset && <ConfirmModal title="Reset notification preferences?" message={<><p>Delivery will be paused and routing reset. Your current preferences will be backed up. The primary webhook is kept if saved; additional destinations must be added again.</p>{error && <p role="alert">{error}</p>}</>} confirmLabel={busy ? "Resetting..." : "Reset preferences"} confirmIcon="refresh" busy={busy !== null} confirmDisabled={busy !== null} onCancel={() => { if (!busy) setConfirmReset(false); }} onConfirm={() => void run("preferences", async () => { const next = await resetDiscordNotifications(); setSettings(next); onConfiguredChange(next.destinations.length > 0); setConfirmReset(false); setFeedback("Notification preferences reset. Delivery is paused. Review routing before enabling notifications."); })} />}
    {modalOpen && <Popup className="confirm-modal settings-webhook-modal" backdropClassName="confirm-modal-backdrop" onClose={close} closeOnBackdrop labelledBy="settings-webhook-title" describedBy="settings-webhook-description" busy={busy !== null}>
      <div className="confirm-modal-header"><h2 id="settings-webhook-title">{editingId ? "Replace Discord webhook" : "Add Discord webhook"}</h2><button className="icon-button" type="button" aria-label="Close webhook dialog" disabled={busy !== null} onClick={close}><Icon name="close" /></button></div>
      <p id="settings-webhook-description">Create a webhook for a Discord text channel and paste its URL here. RM stores the URL in Windows Credential Manager. Replacing a destination preserves its routes and enabled state.</p>
      <label htmlFor="discord-webhook-name">Destination name</label>
      <input id="discord-webhook-name" value={name} maxLength={40} disabled={busy !== null} onChange={event => setName(event.target.value)} placeholder="e.g. Moderation alerts" />
      <label htmlFor="discord-webhook-url">Webhook URL</label>
      <input id="discord-webhook-url" type="password" autoComplete="off" spellCheck={false} placeholder="https://discord.com/api/webhooks/..." value={url} disabled={busy !== null} onChange={(event) => setUrl(event.target.value)} />
      {error && <p role="alert">{error}</p>}
      {feedback && <p role="status">{feedback}</p>}
      <div className="confirm-modal-actions"><button className="account-button" type="button" disabled={!url.trim() || busy !== null} onClick={() => test(url.trim())}>{busy === "test" ? "Sending..." : "Test URL"}</button><button className="account-button" type="button" disabled={!name.trim() || !url.trim() || busy !== null} onClick={save}>{busy === "save" ? "Saving..." : "Save webhook"}</button></div>
    </Popup>}
  </div>;
}
