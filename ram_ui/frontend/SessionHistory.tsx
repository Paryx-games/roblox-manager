import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import Select from "./components/Select";
import { AccountPicker } from "./components/AccountPicker";
import { allAccountsSelected } from "./lib/accountSelection";
import { Toast, type ToastItem } from "./Toast";
import { Icon } from "./components/Icon";
import { ConfirmModal } from "./ConfirmModal";
import { clearSessionHistory, getSessionHistory, operationError, setHistoryPersistence, type AccountSummary, type HistorySnapshot } from "./lib/ipc";
import { historyCsv, historyLabels } from "./lib/historyExport";
import "./SessionHistory.css";

export function SessionHistory({ open, accounts, anonymize, onClose }: { open: boolean; accounts: AccountSummary[]; anonymize: boolean; onClose: () => void }) {
  const [snapshot, setSnapshot] = useState<HistorySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState<Set<number> | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [format, setFormat] = useState("csv");
  const [width, setWidth] = useState(360);
  const [resizing, setResizing] = useState(false);
  const [toast, setToast] = useState<ToastItem | null>(null);
  const toastId = useRef(0);
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
  useEffect(() => { if (open) closeButton.current?.focus(); else { setPickerOpen(false); setResizing(false); resize.current = null; } }, [open]);
  async function change(action: () => Promise<HistorySnapshot>) {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null);
    try { const next = await action(); if (active.current) { revision.current++; setSnapshot(next); setConfirmClear(false); } }
    catch (failure) { if (active.current) setError(operationError(failure, "History action failed. Try again.")); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  }
  const events = (snapshot?.events ?? []).filter(event => filter === null || filter.has(event.userId));
  const labels = new Map(accounts.map((account, index) => [account.userId, anonymize ? `Account ${index + 1}` : account.label]));
  const options = Array.from(new Set([...(snapshot?.events ?? []).map(event => event.userId), ...accounts.map(account => account.userId)]));
  const pickerAccounts = options.map(userId => ({ userId, username: labels.get(userId) ?? `User ${userId}`, displayName: labels.get(userId) ?? `User ${userId}`, label: labels.get(userId) ?? `User ${userId}`, avatarUrl: accounts.find(account => account.userId === userId)?.avatarUrl ?? "" }));
  const selectedFilter = filter ?? new Set(options);
  const emptyMessage = filter?.size === 0 ? "Select accounts to view their history." : snapshot?.events.length ? "No recorded activity for the selected accounts." : "No observed activity yet. History records changes while RM is running and your store is unlocked.";
  function exportHistory() {
    try {
      const content = format === "json" ? JSON.stringify(events, null, 2) : historyCsv(events);
      const url = URL.createObjectURL(new Blob([content], { type: format === "json" ? "application/json" : "text/csv;charset=utf-8" }));
      const link = document.createElement("a"); link.href = url; link.download = `rm-session-history-${new Date().toISOString().slice(0, 10)}.${format}`;
      document.body.append(link); link.click(); link.remove(); window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      setToast({ id: ++toastId.current, title: "History exported", message: `${events.length} event${events.length === 1 ? "" : "s"} exported as ${format.toUpperCase()}. Check your downloads.`, kind: "success", duration: "standard" });
    } catch { setError("History could not be exported. Try again."); setToast({ id: ++toastId.current, title: "Export failed", message: "History could not be exported. Try again.", kind: "error", duration: "standard" }); }
  }
  function clamp(next: number) { setWidth(Math.max(280, Math.min(480, window.innerWidth - 696, next))); }
  return <>
    <div className={`session-history-dock ${open ? "is-open" : ""} ${resizing ? "is-resizing" : ""}`} style={{ "--history-width": `${width}px` } as CSSProperties} aria-hidden={!open} ref={element => { if (element) element.inert = !open; }}>
      <div className="session-history-resizer" role="separator" aria-label="Resize session history" aria-orientation="vertical" aria-valuemin={280} aria-valuemax={480} aria-valuenow={width} tabIndex={0}
        onPointerDown={event => { setResizing(true); resize.current = { x: event.clientX, width }; event.currentTarget.setPointerCapture(event.pointerId); }}
        onPointerMove={event => { if (resize.current) clamp(resize.current.width + resize.current.x - event.clientX); }}
        onPointerUp={() => { setResizing(false); resize.current = null; }} onPointerCancel={() => { setResizing(false); resize.current = null; }} onLostPointerCapture={() => { setResizing(false); resize.current = null; }}
        onKeyDown={event => { if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) { event.preventDefault(); clamp(event.key === "Home" ? 280 : event.key === "End" ? 480 : width + (event.key === "ArrowLeft" ? 16 : -16)); } }} />
      <aside id="session-history-panel" className="session-history-panel" aria-labelledby="session-history-title">
        <div className="session-history-heading"><h2 id="session-history-title">Session history</h2><button ref={closeButton} className="icon-button" type="button" aria-label="Close session history" onClick={onClose}><Icon name="close" /></button></div>
        <div className="session-history-controls">
          <div className="session-history-account-filter"><span>Accounts</span><AccountPicker accounts={pickerAccounts} mode="multiple" showAllAccounts ariaLabel="History accounts" open={pickerOpen} onOpenChange={setPickerOpen} selectedIds={selectedFilter} onSelectedIdsChange={next => { setFilter(allAccountsSelected(next, options) ? null : next); setVisibleCount(200); }} /></div>
          <label>Keep history<Select ariaLabel="History storage" value={snapshot?.persistent ? "file" : "memory"} options={[{ value: "memory", label: "Until app closes" }, { value: "file", label: "Save to encrypted file" }]} disabled={busy || !snapshot} onChange={value => void change(() => setHistoryPersistence(value === "file"))} /></label>
          <p>{snapshot?.persistent ? "Saved locally. Kept across restarts." : "Memory only. Starts empty after restarting RM."} Existing files remain until cleared.</p>
          <div className="session-history-export"><Select ariaLabel="History export format" value={format} options={[{ value: "csv", label: "CSV" }, { value: "json", label: "JSON" }]} onChange={setFormat} /><button type="button" className="account-button" disabled={!events.length || busy} onClick={exportHistory}><Icon name="upload" />Export</button><button type="button" className="account-button" disabled={busy || !snapshot} onClick={() => setConfirmClear(true)}>Clear</button></div>
          {(error || snapshot?.error) && <div className="session-history-error" role="alert">{error || snapshot?.error}<button type="button" className="account-button" disabled={busy} onClick={() => void refresh()}>Retry</button></div>}
        </div>
        <div className="session-history-events" aria-busy={!snapshot || busy}>
          {!snapshot ? <p className="session-history-empty" role="status">Loading history…</p> : !events.length ? <p className="session-history-empty">{emptyMessage}</p> : events.slice(-visibleCount).reverse().map(event => <article key={event.id} className={`session-history-event history-${event.kind}`}>
            <div className="session-history-event-heading"><strong>{labels.get(event.userId) ?? `User ${event.userId}`}</strong><time dateTime={event.observedAt} data-tip={new Date(event.observedAt).toLocaleString()}>{new Date(event.observedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time></div>
            <span>{historyLabels[event.kind] ?? "Presence changed"}</span>
            {event.location && <p>{event.location}</p>}
            {event.placeId && <span className="session-history-identifier">Place {event.placeId}</span>}
            {event.jobId && <span className="session-history-identifier">Server {event.jobId}</span>}
          </article>)}
          {events.length > visibleCount && <button className="account-button" type="button" onClick={() => setVisibleCount(count => count + 200)}>Show older activity</button>}
        </div>
      </aside>
    </div>
    {toast && <Toast item={toast} onDismiss={() => setToast(null)} />}
    {confirmClear && <ConfirmModal title="Clear session history?" message="Remove recorded activity from memory and saved history files and backups. This cannot be undone." confirmLabel={busy ? "Clearing…" : "Clear history"} confirmDisabled={busy} onConfirm={() => void change(clearSessionHistory)} onCancel={() => { if (!busy) setConfirmClear(false); }} />}
  </>;
}
