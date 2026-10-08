import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import Select from "./components/Select";
import { Icon } from "./components/Icon";
import { ConfirmModal } from "./ConfirmModal";
import { clearSessionHistory, getSessionHistory, operationError, setHistoryPersistence, type AccountSummary, type HistorySnapshot } from "./lib/ipc";
import { historyCsv, historyLabels } from "./lib/historyExport";
import "./SessionHistory.css";

export function SessionHistory({ open, accounts, anonymize, onClose }: { open: boolean; accounts: AccountSummary[]; anonymize: boolean; onClose: () => void }) {
  const [snapshot, setSnapshot] = useState<HistorySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("all");
  const [format, setFormat] = useState("csv");
  const [width, setWidth] = useState(340);
  const [confirmClear, setConfirmClear] = useState(false);
  const [visibleCount, setVisibleCount] = useState(200);
  const revision = useRef(0);
  const active = useRef(true);
  const pending = useRef(false);
  const resize = useRef<{ x: number; width: number } | null>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const refresh = useCallback(async () => {
    const before = revision.current;
    try { const next = await getSessionHistory(); if (active.current && before === revision.current) { setSnapshot(next); setError(null); } }
    catch (failure) { if (active.current) setError(operationError(failure, "History could not be loaded. Try again.")); }
  }, []);
  useEffect(() => {
    active.current = true;
    let disposed = false;
    const stops: Array<() => void> = [];
    void Promise.allSettled([
      listen<HistorySnapshot>("session-history-updated", event => { if (!disposed) { revision.current++; setSnapshot(event.payload); } }),
      listen("store-unlocked", () => { if (!disposed) void refresh(); }),
    ]).then(results => {
      results.forEach(result => { if (result.status === "fulfilled") { if (disposed) result.value(); else stops.push(result.value); } else if (!disposed) setError("Live history updates could not be connected. Reopen RM to retry."); });
      if (!disposed) void refresh();
    });
    return () => { disposed = true; active.current = false; stops.forEach(stop => stop()); };
  }, [refresh]);
  useEffect(() => { if (open) closeButton.current?.focus(); }, [open]);
  async function change(action: () => Promise<HistorySnapshot>) {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null);
    try { const next = await action(); if (active.current) { revision.current++; setSnapshot(next); setConfirmClear(false); } }
    catch (failure) { if (active.current) setError(operationError(failure, "History action failed. Try again.")); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  }
  const events = (snapshot?.events ?? []).filter(event => filter === "all" || String(event.userId) === filter);
  const labels = new Map(accounts.map((account, index) => [account.userId, anonymize ? `Account ${index + 1}` : account.label]));
  const options = Array.from(new Set([...(snapshot?.events ?? []).map(event => event.userId), ...accounts.map(account => account.userId)]));
  function exportHistory() {
    try {
      const content = format === "json" ? JSON.stringify(events, null, 2) : historyCsv(events);
      const url = URL.createObjectURL(new Blob([content], { type: format === "json" ? "application/json" : "text/csv;charset=utf-8" }));
      const link = document.createElement("a"); link.href = url; link.download = `rm-session-history-${new Date().toISOString().slice(0, 10)}.${format}`;
      link.click(); window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch { setError("History could not be exported. Try again."); }
  }
  function clamp(next: number) { setWidth(Math.max(280, Math.min(480, window.innerWidth - 696, next))); }
  return <>
    {open && <>
      <div className="session-history-resizer" role="separator" aria-label="Resize session history" aria-orientation="vertical" aria-valuemin={280} aria-valuemax={480} aria-valuenow={width} tabIndex={0}
        onPointerDown={event => { resize.current = { x: event.clientX, width }; event.currentTarget.setPointerCapture(event.pointerId); }}
        onPointerMove={event => { if (resize.current) clamp(resize.current.width + resize.current.x - event.clientX); }}
        onPointerUp={() => { resize.current = null; }} onPointerCancel={() => { resize.current = null; }} onLostPointerCapture={() => { resize.current = null; }}
        onKeyDown={event => { if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) { event.preventDefault(); clamp(event.key === "Home" ? 280 : event.key === "End" ? 480 : width + (event.key === "ArrowLeft" ? 16 : -16)); } }} />
      <aside id="session-history-panel" className="session-history-panel" style={{ width }} aria-labelledby="session-history-title">
        <div className="session-history-heading"><h2 id="session-history-title">Session history</h2><button ref={closeButton} className="icon-button" type="button" aria-label="Close session history" onClick={onClose}><Icon name="close" /></button></div>
        <div className="session-history-controls">
          <label>Accounts<Select ariaLabel="History accounts" value={filter} options={[{ value: "all", label: "All accounts" }, ...options.map(id => ({ value: String(id), label: labels.get(id) ?? `User ${id}` }))]} onChange={value => { setFilter(value); setVisibleCount(200); }} /></label>
          <label>Keep history<Select ariaLabel="History storage" value={snapshot?.persistent ? "file" : "memory"} options={[{ value: "memory", label: "Until app closes" }, { value: "file", label: "Save to encrypted file" }]} disabled={busy || !snapshot} onChange={value => void change(() => setHistoryPersistence(value === "file"))} /></label>
          <p>{snapshot?.persistent ? "Encrypted locally. Retained across restarts." : "Memory only. Starts empty after restarting RM."} Switching to memory leaves earlier saved history on disk; Clear removes it.</p>
          <div className="session-history-export"><Select ariaLabel="History export format" value={format} options={[{ value: "csv", label: "CSV" }, { value: "json", label: "JSON" }]} onChange={setFormat} /><button type="button" className="account-button" disabled={!events.length || busy} onClick={exportHistory}><Icon name="upload" />Export</button><button type="button" className="account-button" disabled={busy || !snapshot} onClick={() => setConfirmClear(true)}>Clear</button></div>
          {(error || snapshot?.error) && <div className="session-history-error" role="alert">{error || snapshot?.error}<button type="button" className="account-button" disabled={busy} onClick={() => void refresh()}>Retry</button></div>}
        </div>
        <div className="session-history-events" aria-busy={!snapshot || busy}>
          {!snapshot ? <p className="session-history-empty" role="status">Loading history…</p> : !events.length ? <p className="session-history-empty">No observed activity yet. History records changes while RM is running and your store is unlocked.</p> : events.slice(-visibleCount).reverse().map(event => <article key={event.id} className={`session-history-event history-${event.kind}`}>
            <div className="session-history-event-heading"><strong>{labels.get(event.userId) ?? `User ${event.userId}`}</strong><time dateTime={event.observedAt} data-tip={new Date(event.observedAt).toLocaleString()}>{new Date(event.observedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time></div>
            <span>{historyLabels[event.kind] ?? "Presence changed"}</span>
            {event.location && <p>{event.location}</p>}
            {event.placeId && <span className="session-history-identifier">Place {event.placeId}</span>}
            {event.jobId && <span className="session-history-identifier">Server {event.jobId}</span>}
          </article>)}
          {events.length > visibleCount && <button className="account-button" type="button" onClick={() => setVisibleCount(count => count + 200)}>Show older activity</button>}
        </div>
        <p className="session-history-footer">Latest 10,000 events · observed times, not exact joins. CSV/JSON exports contain activity metadata and no credentials.</p>
      </aside>
    </>}
    {confirmClear && <ConfirmModal title="Clear session history?" message="Remove recorded activity from memory and the encrypted history file and its backup. This cannot be undone." confirmLabel={busy ? "Clearing…" : "Clear history"} confirmDisabled={busy} onConfirm={() => void change(clearSessionHistory)} onCancel={() => { if (!busy) setConfirmClear(false); }} />}
  </>;
}
