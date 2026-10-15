import { Children, createContext, isValidElement, useContext, useRef, useState, type ReactNode } from "react";
import { accountSectionIds, normalizeAccountLayout, moveAccountSection, type AccountSectionId } from "../lib/accountLayout";
import { Icon } from "./Icon";

const storageKey = "rm.accounts.section-order";
const LayoutContext = createContext<{
  order: AccountSectionId[];
  dragging: AccountSectionId | null;
  setDragging: (id: AccountSectionId | null) => void;
  move: (source: AccountSectionId, target: AccountSectionId) => void;
} | null>(null);

export function AccountSections({ children }: { children: ReactNode }) {
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [order, setOrder] = useState<AccountSectionId[]>(() => {
    try { return normalizeAccountLayout(JSON.parse(localStorage.getItem(storageKey) ?? "null")); }
    catch { return [...accountSectionIds]; }
  });
  const [dragging, setDragging] = useState<AccountSectionId | null>(null);
  const container = useRef<HTMLDivElement>(null);
  function move(source: AccountSectionId, target: AccountSectionId) {
    if (source === target) return;
    const next = moveAccountSection(order, source, target);
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
      setOrder(next);
      setError("");
      setNotice("Account layout saved for all accounts.");
      requestAnimationFrame(() => container.current?.querySelector<HTMLButtonElement>(`[data-section-handle="${source}"]`)?.focus());
    } catch { setError("Account layout could not be saved. Your previous layout is still in use. Try moving the section again."); }
    setDragging(null);
  }
  const sections = Children.toArray(children).filter(isValidElement<{ id: AccountSectionId }>);
  return <LayoutContext.Provider value={{ order, dragging, setDragging, move }}>
    <div className="account-sections" ref={container}>
      {error && <p className="accounts-error" role="alert">{error}</p>}
      <span className="sr-only" role="status">{notice}</span>
      {sections.sort((left, right) => order.indexOf(left.props.id) - order.indexOf(right.props.id))}
    </div>
  </LayoutContext.Provider>;
}

export function AccountSection({ id, label, children }: { id: AccountSectionId; label: string; children: ReactNode }) {
  const layout = useContext(LayoutContext)!;
  const [target, setTarget] = useState(false);
  return <div className={`account-section ${target ? "is-drag-target" : ""}`}
    onDragOver={event => { if (layout.dragging && layout.dragging !== id) { event.preventDefault(); event.dataTransfer.dropEffect = "move"; setTarget(true); } }}
    onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setTarget(false); }}
    onDrop={event => { if (layout.dragging) { event.preventDefault(); layout.move(layout.dragging, id); } setTarget(false); }}>
    <button type="button" className="icon-button account-section-handle" draggable data-section-handle={id}
      aria-label={`Move ${label}. Drag or use Alt plus Up or Down arrow.`}
      data-tip={`Move ${label} - drag or Alt + Up/Down`}
      onDragStart={event => { layout.setDragging(id); event.dataTransfer.effectAllowed = "move"; event.dataTransfer.setData("text/plain", id); }}
      onDragEnd={() => { layout.setDragging(null); setTarget(false); }}
      onKeyDown={event => {
        if (!event.altKey || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
        event.preventDefault();
        const destination = layout.order[layout.order.indexOf(id) + (event.key === "ArrowUp" ? -1 : 1)];
        if (destination) layout.move(id, destination);
      }}><Icon name="grip" /></button>
    {children}
  </div>;
}
