import { useState } from "react";
import { changeDisplayNames, operationError, type AccountSummary, type RobloxSettingResult } from "../lib/ipc";
import { RobloxPrivacySettings } from "./RobloxPrivacySettings";
import { TimedNotice } from "./TimedNotice";

export function RobloxSettings({ accounts }: { accounts: AccountSummary[] }) {
  const [name, setName] = useState(accounts.length === 1 ? accounts[0].displayName : "");
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<RobloxSettingResult[]>([]);
  const [error, setError] = useState("");
  const [isEdited, setIsEdited] = useState(false);
  const pendingAccounts = accounts.filter((account) => account.displayName !== name);
  const changed = isEdited && pendingAccounts.length > 0;
  const failed = results.filter((result) => !result.success);
  const hasError = !!error || failed.length > 0;
  async function saveName() {
    if (busy || !changed) return;
    setBusy(true);
    setResults((current) => current.filter((result) => !result.success));
    try {
      setResults(await changeDisplayNames(pendingAccounts.map((account) => account.userId), name));
      setError("");
    } catch (reason) {
      setResults([]);
      setError(operationError(reason, "Couldn’t change the display name. Try again."));
    } finally {
      setBusy(false);
    }
  }
  if (!accounts.length) return null;
  return (
    <section className="account-card" aria-busy={busy}>
      <h3>Roblox Settings</h3>
      <p>Changes apply on Roblox to {accounts.length === 1 ? "this account" : `all ${accounts.length} selected accounts`}.</p>
      <form className="roblox-display-name-row account-field" onSubmit={(event) => { event.preventDefault(); void saveName(); }}>
        <label htmlFor="roblox-display-name">Display name</label>
        <input id="roblox-display-name" value={name} disabled={busy} onChange={(event) => { setName(event.target.value); setIsEdited(true); }} aria-describedby="roblox-name-validation" aria-invalid={hasError} />
        <button className={`account-button${changed ? " primary" : ""}`} type="submit" disabled={busy || !changed}>{busy ? "Verifying and setting..." : "Verify and set display name"}</button>
      </form>
      <div id="roblox-name-validation" className={hasError ? "roblox-display-name-error" : undefined}>
        {busy && <p role="status">Checking name with Roblox...</p>}
        {error && <p role="alert">{error}</p>}
        {failed.length > 0 && <ul className="roblox-settings-results" role="alert">{failed.map((result) => <li key={result.userId}>{accounts.length > 1 ? `${accounts.find((account) => account.userId === result.userId)?.label ?? result.userId}: ` : ""}{result.message}</li>)}</ul>}
      </div>
      {results.filter((result) => result.success).map((result) => <TimedNotice
        key={result.userId}
        message={accounts.length > 1 ? `${accounts.find((account) => account.userId === result.userId)?.label ?? result.userId}: ${result.message}` : result.message}
        onDismiss={() => setResults((current) => current.filter((item) => item.userId !== result.userId))}
      />)}
      <RobloxPrivacySettings accounts={accounts} />
    </section>
  );
}
