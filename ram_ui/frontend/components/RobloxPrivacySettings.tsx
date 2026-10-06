import { useState } from "react";
import Select from "./Select";
import { getRobloxPrivacy, changeRobloxPrivacy, operationError, type AccountSummary, type AccountPrivacy, type RobloxPrivacyField, type RobloxSettingResult } from "../lib/ipc";

const fields: { field: RobloxPrivacyField; label: string }[] = [
  { field: "whoCanJoinMeInExperiences", label: "Who can see my experience and join me?" },
  { field: "whoCanSeeMyOnlineStatus", label: "Who can see if I’m online?" },
];
function choiceLabel(value: string) {
  const labels: Record<string, string> = { All: "Everyone", Everyone: "Everyone", NoOne: "No one", Noone: "No one", Friends: "Connections", FriendsAndFollowing: "Connections and people I follow", FriendsFollowingAndFollowers: "Connections, followers and people I follow", Following: "People I follow", Followers: "Followers", TrustedFriends: "Trusted connections" };
  return labels[value] ?? value.replace(/([a-z])([A-Z])/g, "$1 $2");
}

export function RobloxPrivacySettings({ accounts }: { accounts: AccountSummary[] }) {
  const [data, setData] = useState<AccountPrivacy[]>([]);
  const [draft, setDraft] = useState<Partial<Record<RobloxPrivacyField, string>>>({});
  const [busy, setBusy] = useState<RobloxPrivacyField | "load" | null>(null);
  const [error, setError] = useState("");
  const [results, setResults] = useState<RobloxSettingResult[]>([]);
  async function load() {
    if (busy) return;
    setBusy("load");
    setError("");
    setResults([]);
    try { setData(await getRobloxPrivacy(accounts.map((account) => account.userId))); setDraft({}); }
    catch (reason) { setData([]); setError(operationError(reason, "Visibility settings could not be loaded. Retry loading.")); }
    finally { setBusy(null); }
  }
  async function save(field: RobloxPrivacyField) {
    const value = draft[field];
    if (busy || !value) return;
    setBusy(field);
    setError("");
    setResults([]);
    try {
      const ids = accounts.map((account) => account.userId);
      setResults(await changeRobloxPrivacy(ids, field, value));
      setData(await getRobloxPrivacy(ids));
      setDraft({});
    } catch (reason) { setData([]); setError(operationError(reason, "Visibility changes could not be confirmed. Reload settings before retrying.")); }
    finally { setBusy(null); }
  }
  const ready = data.length === accounts.length && data.every((account) => !account.error);
  return (
    <div className="roblox-privacy-settings" aria-busy={busy !== null}>
      <div className="account-card-header"><h4>Visibility and joining</h4><button className="account-button" type="button" disabled={busy !== null} onClick={() => void load()}>{busy === "load" ? "Loading..." : data.length ? "Reload settings" : "Load visibility settings"}</button></div>
      <p>Choices depend on each account’s age, region and parental restrictions. Multiple selections show choices available to every selected account.</p>
      {data.map((account) => account.error && <p key={account.userId} role="alert">{accounts.find((item) => item.userId === account.userId)?.label}: {account.error}</p>)}
      {ready && fields.map(({ field, label }) => {
        const settings = data.map((account) => account.settings.find((setting) => setting.field === field));
        const options = settings[0]?.options.filter((value) => settings.every((setting) => setting?.options.includes(value))) ?? [];
        const current = settings.every((setting) => setting?.currentValue === settings[0]?.currentValue) ? choiceLabel(settings[0]?.currentValue ?? "Unknown") : "Mixed values";
        return <div className="account-field-grid" key={field}>
          <div className="account-field"><label>{label}</label><Select ariaLabel={label} value={draft[field] ?? ""} options={[{ value: "", label: `Current: ${current}` }, ...options.map((value) => ({value, label: choiceLabel(value)}))]} disabled={busy !== null || !options.length} onChange={(value) => setDraft((previous) => ({ ...previous, [field]: value }))} /></div>
          <div className="account-field"><span>{options.length ? "Apply this choice to the selected accounts." : "No shared choices are available. Select an individual account."}</span><button className="account-button" type="button" disabled={busy !== null || !draft[field]} onClick={() => void save(field)}>{busy === field ? "Saving..." : "Apply"}</button></div>
        </div>;
      })}
      {error && <p role="alert">{error}</p>}
      {results.length > 0 && <ul className="roblox-settings-results" aria-live="polite">{results.map((result) => <li key={result.userId}>{accounts.find((account) => account.userId === result.userId)?.label ?? result.userId}: {result.message}</li>)}</ul>}
    </div>
  );
}
