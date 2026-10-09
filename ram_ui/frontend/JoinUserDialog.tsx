import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Popup } from "./components/Popup";
import { ConfirmModal } from "./ConfirmModal";
import Select from "./components/Select";
import { AccountPicker } from "./components/AccountPicker";
import { allAccountsSelected } from "./lib/accountSelection";
import { cancelUserJoin, getJoinCleanups, joinUserAccounts, operationError, resetJoinCleanupJournal, retryJoinCleanup, type AccountSummary, type JoinMode, type JoinProgress, type JoinResult, type PendingFollow } from "./lib/ipc";

export function JoinUserDialog({ accounts, selectedIds, initialTarget, onClose }: { accounts: AccountSummary[]; selectedIds: Set<number>; initialTarget?: number; onClose: () => void }) {
  const [target, setTarget] = useState(initialTarget ? String(initialTarget) : "");
  const [selection, setSelection] = useState<Set<number> | null>(() => selectedIds.size ? new Set(selectedIds) : null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [mode, setMode] = useState<JoinMode>("visibleServer");
  const [busy, setBusy] = useState(false);
  const [cancelled, setCancelled] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<JoinProgress | null>(null);
  const [results, setResults] = useState<JoinResult[]>([]);
  const [cleanup, setCleanup] = useState<PendingFollow[]>([]);
  const [cleanupUnreadable, setCleanupUnreadable] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const operation = useRef<string | null>(null);
  const active = useRef(true);
  const pending = useRef(false);
  const eligibleIds = accounts.filter(account => account.canLaunch && !account.cookieExpired).map(account => account.userId);
  const disabledIds = new Set(accounts.filter(account => !account.canLaunch || account.cookieExpired).map(account => account.userId));
  const chosenIds = selection ?? new Set(eligibleIds);
  const ids = eligibleIds.filter(id => chosenIds.has(id));

  useEffect(() => {
    active.current = true;
    void getJoinCleanups().then(next => { if (active.current) setCleanup(next); }).catch(failure => { if (active.current) { setError(operationError(failure, "Pending follow cleanup could not be loaded.")); setCleanupUnreadable(true); } });
    return () => { active.current = false; if (operation.current) void cancelUserJoin(operation.current).catch(() => {}); };
  }, []);

  async function start() {
    if (pending.current || !target.trim() || !ids.length) return;
    pending.current = true; setBusy(true); setPickerOpen(false); setError(null); setResults([]); setProgress(null); setCancelled(false);
    const id = crypto.randomUUID(); operation.current = id;
    let stop: (() => void) | undefined;
    try {
      stop = await listen<JoinProgress>("join-user-progress", event => { if (active.current && event.payload.operationId === id) setProgress(event.payload); });
      const next = await joinUserAccounts(id, ids, target, mode);
      if (active.current) setResults(next);
    } catch (failure) { if (active.current) setError(operationError(failure, "Could not join this user. Check the target and account permissions.")); }
    finally {
      stop?.(); operation.current = null; pending.current = false;
      if (active.current) { setBusy(false); void getJoinCleanups().then(next => { if (active.current) setCleanup(next); }).catch(failure => { if (active.current) setError(operationError(failure, "Follow cleanup could not be loaded. Reopen this dialog to retry.")); }); }
    }
  }
  async function cancel() {
    if (!operation.current) return;
    try { await cancelUserJoin(operation.current); if (active.current) setCancelled(true); }
    catch { if (active.current) setError("Cancellation could not be sent. Keep this dialog open and try again."); }
  }
  async function retry(entry: PendingFollow) {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null);
    try { const next = await retryJoinCleanup(entry.userId, entry.targetUserId); if (active.current) setCleanup(next); }
    catch (failure) { if (active.current) setError(operationError(failure, "Unfollow could not be completed. Refresh the account and retry later.")); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  }

  return <><Popup className="confirm-modal join-user-dialog" backdropClassName="confirm-modal-backdrop" labelledBy="join-user-title" busy={busy} onClose={onClose}>
    <div className="confirm-modal-header"><h2 id="join-user-title">Join user’s game</h2></div>
    <label className="join-user-field">Username or user ID<input value={target} disabled={busy} onChange={event => setTarget(event.target.value)} placeholder="Exact username or numeric ID" aria-label="Join target" /></label>
    <div className="join-user-field"><span>Launch with</span><AccountPicker accounts={accounts} mode="multiple" showAllAccounts ariaLabel="Launch accounts" open={pickerOpen} onOpenChange={setPickerOpen} selectedIds={chosenIds} onSelectedIdsChange={next => setSelection(allAccountsSelected(next, eligibleIds) ? null : next)} disabledAccountIds={disabledIds} disabled={busy} /></div>
    <label className="join-user-field">Find server using<Select ariaLabel="Join method" value={mode} disabled={busy} onChange={value => setMode(value as JoinMode)} options={[{ value: "visibleServer", label: "Check selected accounts for a visible server" }, { value: "temporaryFollow", label: "Temporarily follow, join, then unfollow" }]} /></label>
    <p className="settings-muted">{mode === "visibleServer" ? "Check accounts one at a time and reuse the first visible Place ID and Job ID. Roblox still decides whether each account can join." : "Check visible presence first. Follow only if needed, preserve existing follows, and remove temporary follows after the launch request. Following may not grant access."}</p>
    <p className="settings-muted join-user-help">Queued requests respect Roblox rate limits and your launch delay. A launch request does not confirm game entry.</p>
    {error && <p className="session-history-error" role="alert">{error}</p>}
    {cleanupUnreadable && <button className="account-button" type="button" disabled={busy} onClick={() => setConfirmReset(true)}>Reset unreadable cleanup journal</button>}
    {progress && <p className="join-user-progress" role="status">{progress.userId ? `${accounts.find(account => account.userId === progress.userId)?.label ?? `User ${progress.userId}`}: ` : ""}{progress.message}{cancelled ? " - cancelling remaining accounts after this step and follow cleanup" : ""}</p>}
    {!!results.length && <ul className="join-user-results">{results.map(result => <li key={result.userId}><strong>{accounts.find(account => account.userId === result.userId)?.label ?? `User ${result.userId}`}</strong><span>{result.message}</span></li>)}</ul>}
    {!!cleanup.length && <section className="join-user-cleanups" aria-label="Pending temporary follow cleanup"><h3>Temporary follows need attention</h3><p className="settings-muted">A previous request may have completed before RM closed. Review the relationship on Roblox before retrying; retry removes this follow.</p>{cleanup.map(entry => <div className="join-user-cleanup" key={`${entry.userId}-${entry.targetUserId}`}><span>{accounts.find(account => account.userId === entry.userId)?.label ?? `User ${entry.userId}`} → user {entry.targetUserId}</span><button className="account-button" type="button" disabled={busy} onClick={() => void retry(entry)}>Retry cleanup</button></div>)}</section>}
    <div className="confirm-modal-actions"><button className="account-button" type="button" disabled={busy} onClick={onClose}>Close</button>{busy && operation.current && <button className="account-button" type="button" disabled={cancelled} onClick={() => void cancel()}>Cancel remaining</button>}<button className="account-button primary" type="button" disabled={busy || !target.trim() || !ids.length} onClick={() => void start()}>{busy ? "Joining…" : `Queue ${ids.length} account${ids.length === 1 ? "" : "s"}`}</button></div>
  </Popup>{confirmReset && <ConfirmModal title="Reset unreadable cleanup journal?" message="Review your account follows on Roblox and remove any unwanted temporary follows first. Resetting discards the unreadable recovery record; it does not unfollow anyone." confirmLabel={busy ? "Resetting…" : "I reviewed follows — reset"} confirmDisabled={busy} onCancel={() => { if (!busy) setConfirmReset(false); }} onConfirm={() => {
    if (pending.current) return;
    pending.current = true; setBusy(true);
    void resetJoinCleanupJournal().then(() => { if (active.current) { setCleanup([]); setCleanupUnreadable(false); setConfirmReset(false); setError(null); } }).catch(failure => { if (active.current) setError(operationError(failure, "The journal could not be reset.")); }).finally(() => { pending.current = false; if (active.current) setBusy(false); });
  }} />}</>;
}
