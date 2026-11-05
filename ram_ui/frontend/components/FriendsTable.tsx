import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent } from "react";
import { FriendRow } from "./FriendComponents";
import type { Friend, FriendAction, FriendUser } from "../lib/ipc";

type ColumnId = "friend" | "status" | "game" | "jobId" | "actions";
type ColumnVisibility = Record<ColumnId, boolean>;
type ColumnWidths = Record<ColumnId, number>;

const preferenceKey = "rm.friends.table.v1";
const columnDefinitions: { id: ColumnId; label: string; minWidth: number }[] = [
  { id: "friend", label: "Friend", minWidth: 112 },
  { id: "status", label: "Status", minWidth: 76 },
  { id: "game", label: "Game / Place", minWidth: 112 },
  { id: "jobId", label: "Job ID", minWidth: 96 },
  { id: "actions", label: "Actions", minWidth: 76 },
];
const defaultVisibility: ColumnVisibility = { friend: true, status: true, game: true, jobId: true, actions: true };
const defaultWidths: ColumnWidths = { friend: 168, status: 92, game: 160, jobId: 128, actions: 100 };

function readPreferences(): { visibility: ColumnVisibility; widths: ColumnWidths } {
  try {
    const saved = JSON.parse(localStorage.getItem(preferenceKey) ?? "null") as Partial<{ visibility: Partial<ColumnVisibility>; widths: Partial<ColumnWidths> }> | null;
    return {
      visibility: Object.fromEntries(columnDefinitions.map(column => [
        column.id,
        column.id === "friend" || typeof saved?.visibility?.[column.id] !== "boolean"
          ? defaultVisibility[column.id]
          : saved.visibility[column.id],
      ])) as ColumnVisibility,
      widths: Object.fromEntries(columnDefinitions.map(column => {
        const width = saved?.widths?.[column.id];
        return [column.id, typeof width === "number" && Number.isFinite(width) ? Math.min(640, Math.max(column.minWidth, width)) : defaultWidths[column.id]];
      })) as ColumnWidths,
    };
  } catch {
    return { visibility: defaultVisibility, widths: defaultWidths };
  }
}

export function FriendsTable({
  rows,
  selected,
  focused,
  allChecked,
  someChecked,
  busy,
  multiInstance,
  onCheck,
  onSelect,
  onToggleAll,
  onCopy,
  onJoin,
  onManage,
  onProfile,
}: {
  rows: Friend[];
  selected: ReadonlySet<number>;
  focused: number | null;
  allChecked: boolean;
  someChecked: boolean;
  busy: boolean;
  multiInstance: boolean;
  onCheck: (friend: Friend) => void;
  onSelect: (friend: Friend) => void;
  onToggleAll: () => void;
  onCopy: (value: string, label: string) => void;
  onJoin: (friend: Friend, expectedJobId?: string, requireNewInstance?: boolean) => void;
  onManage: (user: FriendUser, action: FriendAction) => void;
  onProfile: (userId: number) => void;
}) {
  const [preferences, setPreferences] = useState(readPreferences);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuTriggerRef = useRef<HTMLElement | null>(null);
  const selectAllRef = useRef<HTMLInputElement>(null);
  const drag = useRef<{ pointerId: number; column: ColumnId; x: number; width: number } | null>(null);
  const visibleColumns = useMemo(() => columnDefinitions.filter(column => preferences.visibility[column.id]), [preferences.visibility]);
  const tableWidth = 28 + visibleColumns.reduce((total, column) => total + preferences.widths[column.id], 0);

  useEffect(() => {
    if (selectAllRef.current) selectAllRef.current.indeterminate = someChecked && !allChecked;
  }, [someChecked, allChecked]);

  useEffect(() => {
    try {
      localStorage.setItem(preferenceKey, JSON.stringify(preferences));
    } catch {
      return;
    }
  }, [preferences]);

  useEffect(() => {
    if (!menu) return;
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Node && !menuRef.current?.contains(event.target)) setMenu(null);
    };
    const escape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenu(null);
        requestAnimationFrame(() => {
          const trigger = menuTriggerRef.current;
          const fallback = document.querySelector<HTMLElement>(".friends-table th[tabindex='0']");
          (trigger?.isConnected ? trigger : fallback)?.focus();
        });
      }
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    menuRef.current?.querySelector<HTMLInputElement>("input")?.focus();
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, [menu]);

  function showMenu(x: number, y: number, trigger: HTMLElement | null) {
    menuTriggerRef.current = trigger;
    setMenu({
      x: Math.max(8, Math.min(x, window.innerWidth - 220)),
      y: Math.max(8, Math.min(y, window.innerHeight - 220)),
    });
  }

  function openMenuFromMouse(event: MouseEvent<HTMLElement>) {
    event.preventDefault();
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>("th") : null;
    showMenu(event.clientX, event.clientY, target);
  }

  function openMenuFromKeyboard(event: KeyboardEvent<HTMLElement>) {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    event.preventDefault();
    const bounds = event.currentTarget.getBoundingClientRect();
    const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    showMenu(bounds.left, bounds.bottom, active);
  }

  function resizeBy(column: ColumnId, delta: number) {
    const definition = columnDefinitions.find(item => item.id === column);
    if (!definition) return;
    setPreferences(current => ({
      ...current,
      widths: { ...current.widths, [column]: Math.min(640, Math.max(definition.minWidth, current.widths[column] + delta)) },
    }));
  }

  function renderHeader(column: typeof columnDefinitions[number]) {
    const width = preferences.widths[column.id];
    return <th
      key={column.id}
      scope="col"
      tabIndex={0}
      aria-haspopup="true"
      aria-label={`${column.label}. Right-click or press the context-menu key to choose columns.`}
      style={{ width } as CSSProperties}
    >
      {column.label}
      <span
        className="friends-column-resizer"
        role="separator"
        aria-label={`Resize ${column.label} column`}
        aria-orientation="vertical"
        aria-valuemin={column.minWidth}
        aria-valuemax={640}
        aria-valuenow={width}
        tabIndex={0}
        onPointerDown={event => {
          if (event.button !== 0) return;
          drag.current = { pointerId: event.pointerId, column: column.id, x: event.clientX, width };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={event => {
          const current = drag.current;
          if (!current || current.pointerId !== event.pointerId) return;
          const definition = columnDefinitions.find(item => item.id === current.column);
          if (!definition) return;
          setPreferences(value => ({
            ...value,
            widths: { ...value.widths, [current.column]: Math.min(640, Math.max(definition.minWidth, current.width + event.clientX - current.x)) },
          }));
        }}
        onPointerUp={() => { drag.current = null; }}
        onPointerCancel={() => { drag.current = null; }}
        onLostPointerCapture={() => { drag.current = null; }}
        onKeyDown={event => {
          if (event.key === "ArrowLeft") { event.preventDefault(); resizeBy(column.id, -16); }
          else if (event.key === "ArrowRight") { event.preventDefault(); resizeBy(column.id, 16); }
          else if (event.key === "Home") { event.preventDefault(); resizeBy(column.id, column.minWidth - width); }
          else if (event.key === "End") { event.preventDefault(); resizeBy(column.id, 640 - width); }
        }}
      />
    </th>;
  }

  const tableStyle = { width: `max(100%, ${tableWidth}px)` } as CSSProperties;
  return <>
    <div className="friends-list-scroll">
      <table className="friends-table" style={tableStyle}>
        <caption className="sr-only">Friends, presence and available server details. Right-click a column heading to choose visible columns.</caption>
        <colgroup>
          <col style={{ width: 28 }} />
          {visibleColumns.map(column => <col key={column.id} style={{ width: preferences.widths[column.id] }} />)}
        </colgroup>
        <thead>
          <tr onContextMenu={openMenuFromMouse} onKeyDown={openMenuFromKeyboard}>
            <th scope="col" aria-label="Select friends"><input ref={selectAllRef} type="checkbox" checked={allChecked} aria-label="Select all visible friends" onChange={onToggleAll} /></th>
            {visibleColumns.map(renderHeader)}
          </tr>
        </thead>
        <tbody>
          {rows.map(friend => <FriendRow
            key={friend.userId}
            friend={friend}
            checked={selected.has(friend.userId)}
            selected={focused === friend.userId}
            busy={busy}
            multiInstance={multiInstance}
            visibleColumns={preferences.visibility}
            onCheck={onCheck}
            onSelect={onSelect}
            onCopy={onCopy}
            onJoin={onJoin}
            onManage={onManage}
            onProfile={onProfile}
          />)}
        </tbody>
      </table>
    </div>
    {menu&&<div ref={menuRef} className="friends-column-menu" role="group" aria-label="Visible friend columns" style={{ left: menu.x, top: menu.y }}>
      {columnDefinitions.map(column=><label key={column.id}>
        <input
          type="checkbox"
          checked={preferences.visibility[column.id]}
          disabled={column.id==="friend"}
          onChange={event=>setPreferences(current=>({...current,visibility:{...current.visibility,[column.id]:event.target.checked}}))}
        />
        <span>{column.label}</span>
      </label>)}
    </div>}
  </>;
}
