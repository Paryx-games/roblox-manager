import { useEffect, useId, useState } from "react";
import { parseLaunchDestination } from "../lib/launchDestination";
import { resolveLaunchDestination } from "../lib/ipc";

const recentGames = new Map<number, string>();

export function LaunchDestinationInput({ value, onChange, id, labelledBy }: {
  value: string;
  onChange: (value: string) => void;
  id?: string;
  labelledBy?: string;
}) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const [input, setInput] = useState(value);
  const [lookup, setLookup] = useState<{ id: number; name?: string; failed?: boolean } | null>(null);
  const placeId = parseLaunchDestination(input);
  useEffect(() => {
    if ((parseLaunchDestination(input)?.toString() ?? "") !== value) setInput(value);
  }, [value]);
  useEffect(() => {
    if (!placeId) return;
    const cached = recentGames.get(placeId);
    if (cached) { setLookup({ id: placeId, name: cached }); return; }
    let active = true;
    setLookup({ id: placeId });
    const timer = window.setTimeout(() => {
      void resolveLaunchDestination(placeId).then((result) => {
        if (!active || result.placeId !== placeId) return;
        recentGames.delete(placeId);
        recentGames.set(placeId, result.name);
        if (recentGames.size > 20) recentGames.delete(recentGames.keys().next().value!);
        setLookup({ id: placeId, name: result.name });
      }).catch(() => { if (active) setLookup({ id: placeId, failed: true }); });
    }, 400);
    return () => { active = false; window.clearTimeout(timer); };
  }, [placeId]);
  const current = lookup?.id === placeId ? lookup : null;
  return <div className="launch-destination">
    <input id={inputId} aria-label={labelledBy ? undefined : "Place ID or game URL"} aria-labelledby={labelledBy}
      aria-invalid={!!input && !placeId} aria-describedby={`${inputId}-hint`} value={input}
      placeholder="Place ID or https://www.roblox.com/games/…" list={`${inputId}-recent`}
      onChange={(event) => { const next = event.target.value; setInput(next); onChange(parseLaunchDestination(next)?.toString() ?? ""); }} />
    <datalist id={`${inputId}-recent`}>{Array.from(recentGames, ([gameId, name]) => <option key={gameId} value={gameId}>{name}</option>)}</datalist>
    <p id={`${inputId}-hint`} className="settings-muted" role="status">{!input ? "Use a place ID or a public game URL. Recent games appear as suggestions."
      : !placeId ? "Enter a positive place ID or a public Roblox game URL without query parameters. Use Private Servers for invite links."
      : current?.name ? `${current.name} · Place ${placeId}`
      : current?.failed ? `Place ${placeId} · Name unavailable; launching is still available.`
      : `Looking up Place ${placeId}…`}</p>
  </div>;
}
