import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getRobloxPrivacy, changeRobloxPrivacy, operationError, type AccountSummary, type AccountPrivacy, type RobloxPrivacyField, type RobloxSettingResult } from "../lib/ipc";

const fields: { field: RobloxPrivacyField; label: string; description: string }[] = [
  { field: "whoCanSeeMyOnlineStatus", label: "Online", description: "Who can see me online?" },
  { field: "whoCanJoinMeInExperiences", label: "In-game", description: "Who can see me in-game and join me?" },
];
const audiences = [
  { label: "Everyone", tone: "everyone", values: ["All", "Everyone"] },
  { label: "Friends, followers & people I follow", tone: "followers", values: ["FriendsFollowingAndFollowers", "FriendsAndFollowingAndFollowers", "FriendsAndFollowersAndFollowing"] },
  { label: "Friends & people I follow", tone: "following", values: ["FriendsAndFollowing"] },
  { label: "Friends", tone: "friends", values: ["Friends"] },
  { label: "Trusted friends", tone: "trusted", values: ["TrustedFriends"] },
  { label: "No one", tone: "no-one", values: ["NoOne", "Noone"] },
];

export function RobloxPrivacySettings({ accounts }: { accounts: AccountSummary[] }) {
  const [data, setData] = useState<AccountPrivacy[]>([]);
  const [busy, setBusy] = useState<RobloxPrivacyField | "load" | null>(null);
  const [pendingValue, setPendingValue] = useState("");
  const [error, setError] = useState("");
  const [results, setResults] = useState<RobloxSettingResult[]>([]);
  const selectionKey = accounts.map((account) => account.userId).join(",");
  const ids = useMemo(() => selectionKey.split(",").map(Number), [selectionKey]);
  const requestVersion = useRef(0);
  const load = useCallback(async () => {
    const version = ++requestVersion.current;
    setBusy("load");
    setError("");
    setResults([]);
    try {
      const loaded = await getRobloxPrivacy(ids);
      if (requestVersion.current === version) setData(loaded);
    } catch (reason) {
      if (requestVersion.current === version) { setData([]); setError(operationError(reason, "Visibility settings could not be loaded. Retry loading.")); }
    } finally { if (requestVersion.current === version) setBusy(null); }
  }, [ids]);
  useEffect(() => { void load(); return () => { ++requestVersion.current; }; }, [load]);
  async function save(field: RobloxPrivacyField, value: string) {
    if (busy || !value || data.every((account) => account.settings.find((setting) => setting.field === field)?.currentValue === value)) return;
    setBusy(field);
    setPendingValue(value);
    setError("");
    setResults([]);
    try {
      setResults(await changeRobloxPrivacy(ids, field, value));
      setData(await getRobloxPrivacy(ids));
    } catch (reason) { setData([]); setError(operationError(reason, "Visibility changes could not be confirmed. Reload settings before retrying.")); }
    finally { setBusy(null); setPendingValue(""); }
  }
  const ready = data.length === accounts.length && data.every((account) => !account.error);
  return (
    <div className="roblox-privacy-settings" aria-busy={busy !== null}>
      {data.map((account) => account.error && <p key={account.userId} role="alert">{accounts.find((item) => item.userId === account.userId)?.label}: {account.error}</p>)}
      {fields.map(({ field, label, description }) => {
        const settings = data.map((account) => account.settings.find((setting) => setting.field === field));
        const mixed = ready && settings.some((setting) => setting?.currentValue !== settings[0]?.currentValue);
        return <div className="roblox-visibility-row" key={field}>
          <span id={`roblox-${field}-label`} className="roblox-visibility-label">{label}{mixed ? " (mixed)" : ""}</span>
          <div className="roblox-visibility-bar" role="group" aria-label={description}>
            {audiences.map((audience) => {
              const value = audience.values.find((value) => ready && settings.every((setting) => setting?.options.includes(value)));
              const selected = ready && settings.every((setting) => audience.values.includes(setting?.currentValue ?? ""));
              return <button key={audience.tone} type="button" className={`roblox-visibility-choice visibility-${audience.tone}`} aria-pressed={selected} disabled={busy !== null || !value} data-tip={!value && ready ? "Unavailable for the selected accounts" : undefined} onClick={() => { if (value) void save(field, value); }}>
                {busy === field && value === pendingValue ? "Saving..." : audience.label}
              </button>;
            })}
          </div>
        </div>;
      })}
      {busy === "load" && <p role="status">Loading visibility settings...</p>}
      {(error || data.some((account) => account.error)) && <button className="account-button" type="button" disabled={busy !== null} onClick={() => void load()}>Retry loading settings</button>}
      {error && <p role="alert">{error}</p>}
      {results.length > 0 && <ul className="roblox-settings-results" aria-live="polite">{results.map((result) => <li key={result.userId}>{accounts.find((account) => account.userId === result.userId)?.label ?? result.userId}: {result.message}</li>)}</ul>}
    </div>
  );
}
