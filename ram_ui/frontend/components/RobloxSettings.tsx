import { useEffect, useMemo, useState } from "react";
import { changeDisplayNames, checkRobloxDisplayName, operationError, type AccountSummary, type RobloxSettingResult } from "../lib/ipc";
import { RobloxPrivacySettings } from "./RobloxPrivacySettings";

export function RobloxSettings({ accounts }: { accounts: AccountSummary[] }) {
  const [name, setName] = useState(accounts.length === 1 ? accounts[0].displayName : "");
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<RobloxSettingResult[]>([]);
  const [error, setError] = useState("");
  const [validation, setValidation] = useState<{ name: string; error: string }>({ name: "", error: "" });
  const selectionKey = accounts.map((account) => account.userId).join(",");
  const ids = useMemo(() => selectionKey.split(",").map(Number), [selectionKey]);
  const lengthValid = [...name].length >= 3 && [...name].length <= 20 && !/[\p{Cc}]/u.test(name);
  const supported = lengthValid && validation.name === name && !validation.error;
  useEffect(() => {
    if (!lengthValid || busy) return;
    let active = true;
    const timer = window.setTimeout(() => {
      checkRobloxDisplayName(ids, name).then(() => {
        if (active) setValidation({ name, error: "" });
      }).catch((reason) => {
        if (active) setValidation({ name, error: operationError(reason, "Roblox could not validate this name.") });
      });
    }, 450);
    return () => { active = false; window.clearTimeout(timer); };
  }, [ids, name, lengthValid, busy]);
  async function saveName() {
    if (busy || !supported) return;
    setBusy(true);
    setValidation({ name: "", error: "" });
    setError("");
    setResults([]);
    try {
      setResults(await changeDisplayNames(ids, name));
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
      <form className="roblox-display-name-row account-field" onSubmit={(event) => { event.preventDefault(); void saveName(); }}>
        <label htmlFor="roblox-display-name">Display name</label>
        <input id="roblox-display-name" value={name} disabled={busy} onChange={(event) => { setName(event.target.value); setValidation({ name: "", error: "" }); }} aria-describedby="roblox-name-validation" aria-invalid={!!name && (!lengthValid || (validation.name === name && !!validation.error))} />
        <button className="account-button primary" type="submit" disabled={busy || !supported}>{busy ? "Saving..." : "Set display name"}</button>
      </form>
      <p id="roblox-name-validation" role="status">{name && !lengthValid ? "Use 3–20 characters with no control characters." : validation.name === name ? validation.error : lengthValid && !busy ? "Checking name with Roblox..." : ""}</p>
      {error && <p role="alert">{error}</p>}
      {results.length > 0 && <ul className="roblox-settings-results" aria-live="polite">{results.map((result) => <li key={result.userId}>{accounts.find((account) => account.userId === result.userId)?.label ?? result.userId}: {result.message}</li>)}</ul>}
      <RobloxPrivacySettings accounts={accounts} />
    </section>
  );
}
