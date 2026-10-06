import { useState } from "react";
import { changeDisplayNames, operationError, type AccountSummary, type RobloxSettingResult } from "../lib/ipc";
import { RobloxPrivacySettings } from "./RobloxPrivacySettings";

export function RobloxSettings({ accounts }: { accounts: AccountSummary[] }) {
  const [name, setName] = useState(accounts.length === 1 ? accounts[0].displayName : "");
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<RobloxSettingResult[]>([]);
  const [error, setError] = useState("");
  async function saveName() {
    if (busy) return;
    setBusy(true);
    setError("");
    setResults([]);
    try {
      setResults(await changeDisplayNames(accounts.map((account) => account.userId), name));
    } catch (reason) {
      setError(operationError(reason, "Display names could not be changed. Try again."));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="account-card" aria-busy={busy}>
      <h3>Roblox Settings</h3>
      <p>Changes apply on Roblox to {accounts.length === 1 ? "this account" : `all ${accounts.length} selected accounts`}.</p>
      <form className="account-field-grid" onSubmit={(event) => { event.preventDefault(); void saveName(); }}>
        <div className="account-field">
          <label htmlFor="roblox-display-name">Display name</label>
          <input id="roblox-display-name" value={name} disabled={busy} onChange={(event) => setName(event.target.value)} aria-describedby="roblox-name-help" />
        </div>
        <div className="account-field">
          <span id="roblox-name-help">3–20 characters. Roblox applies filtering and a seven-day cooldown.</span>
          <button className="account-button" type="submit" disabled={busy || [...name.trim()].length < 3 || [...name.trim()].length > 20}>{busy ? "Changing..." : accounts.length === 1 ? "Set display name" : `Set display name (${accounts.length})`}</button>
        </div>
      </form>
      {error && <p role="alert">{error}</p>}
      {results.length > 0 && <ul className="roblox-settings-results" aria-live="polite">{results.map((result) => <li key={result.userId}>{accounts.find((account) => account.userId === result.userId)?.label ?? result.userId}: {result.message}</li>)}</ul>}
      <RobloxPrivacySettings accounts={accounts} />
    </section>
  );
}
