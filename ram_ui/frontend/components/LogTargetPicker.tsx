import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { loggingTargets, operationError } from "../lib/ipc";
import { PopupMenu } from "./PopupMenu";
import { Icon } from "./Icon";

export function LogTargetPicker({ selected, onChange, disabled = false }: { selected: string[]; onChange: (targets: string[]) => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  const [above, setAbove] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const [search, setSearch] = useState("");
  const [targets, setTargets] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!open || targets.length) return;
    let active = true;
    setLoading(true);
    void loggingTargets().then(result => { if (active) { setTargets(result); setError(null); } }).catch(reason => { if (active) setError(operationError(reason, "Crates could not be loaded. Close and reopen to retry.")); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [open, targets.length]);
  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Node && !container.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [open]);
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const bounds = container.current?.getBoundingClientRect();
      if (bounds) setAbove(bounds.bottom + 324 > window.innerHeight && bounds.top > 324);
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => { window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); };
  }, [open]);
  const visible = [...new Set([...targets, ...selected])].sort().filter(target => target.includes(search.trim().toLowerCase().replace(/-/g, "_")));
  return <div className="log-target-picker" ref={container}>
    <button type="button" className="account-button" disabled={disabled} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(value => !value)}>{selected.length} crates selected <Icon name="chevron-down" /></button>
    {open && <PopupMenu className={`account-picker-menu log-target-menu ${above ? "is-above" : ""}`} onClose={() => setOpen(false)}>
      <input className="account-input" tabIndex={0} aria-label="Search logging crates" placeholder="Search crates..." value={search} onChange={event => setSearch(event.target.value)} />
      {loading && <p role="status">Loading crates...</p>}
      {error && <p role="alert">{error}</p>}
      {!loading && !error && !visible.length && <p>No matching crates</p>}
      {visible.map(target => <button key={target} type="button" className="account-picker-option" disabled={disabled} role="menuitemcheckbox" aria-checked={selected.includes(target)} onClick={() => onChange(selected.includes(target) ? selected.filter(value => value !== target) : [...selected, target])}><span aria-hidden="true">{selected.includes(target) ? <Icon name="check" /> : <span className="account-icon" />}</span>{target}</button>)}
    </PopupMenu>}
  </div>;
}
