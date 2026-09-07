import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
} from "react";
import {
  getStoreStatus,
  fetchAccountInventory,
  launchAccount as launchAccountIpc,
  removeAccount,
  runConnectionAction,
  saveLaunchPreset,
  searchConnectionUsers,
  toggleAccountPin,
  openAccountUrl,
  updateAccountGroup,
  reorderAccounts,
  updatePlayerPath,
  createDeviceStore,
  addAccount,
  addAccountAnyway,
  browseAsAccount,
  loginAndAddAccount,
  listAccounts,
  listAccountGroupColors,
  listAccountGroups,
  reorderAccountGroups,
  refreshAccountPresence,
  revalidateAccounts,
  killAllAccounts,
  joinUserGame,
  arrangeAccountWindows,
  createAccountGroup,
  deleteAccountGroup,
  updateAccountGroupMeta,
  unlockDevice,
  unlockPassword,
  updateAccountAlias,
  type AccountSummary,
  type InventoryItem,
  type UserSearchResult,
  type StoreStatus,
} from "./lib/ipc";
import { ConfirmModal } from "./ConfirmModal";
import {
  Toast,
  type ToastDuration,
  type ToastItem,
  type ToastKind,
} from "./Toast";

type SortMode =
  | "custom"
  | "username"
  | "status"
  | "accountAge"
  | "lastActivity";

type AccountGroup = {
  name: string;
  tone: "primary" | "secondary";
  accounts: AccountSummary[];
  color: string;
};

type AccountNotice = {
  id: number;
  message: string;
};

type BulkImportResult = {
  index: number;
  status: "added" | "failed";
  message?: string;
};

const GROUP_COLOR_PRESETS: ReadonlyArray<{
  label: string;
  color: [number, number, number];
}> = [
  { label: "Red", color: [220, 60, 60] },
  { label: "Green", color: [60, 180, 60] },
  { label: "Blue", color: [60, 120, 220] },
  { label: "Yellow", color: [220, 180, 50] },
  { label: "Purple", color: [160, 60, 220] },
  { label: "Cyan", color: [60, 200, 200] },
  { label: "Orange", color: [220, 130, 50] },
  { label: "Pink", color: [220, 80, 160] },
];

type AccountSelectionEvent = Pick<
  MouseEvent<HTMLDivElement>,
  "ctrlKey" | "metaKey"
>;

type GroupContextMenu = {
  name: string;
  x: number;
  y: number;
};

type GroupEditorState = {
  originalName: string | null;
  name: string;
  color: string;
};

type DropPosition = "before" | "after";

type DropIndicator = {
  kind: "account" | "group";
  id: number | string;
  position: DropPosition;
};

function Icon({ name }: { name: string }) {
  return (
    <img
      className="account-icon"
      src={`/icons/${name}.svg`}
      alt=""
      aria-hidden="true"
    />
  );
}

function initials(account: AccountSummary) {
  const source = account.displayName || account.username || account.label;
  return source
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
}

function rgbToHex(color: [number, number, number]) {
  return `#${color.map((value) => value.toString(16).padStart(2, "0")).join("")}`;
}

function hexToRgb(value: string): [number, number, number] | null {
  const match = /^#([\da-f]{6})$/i.exec(value);
  if (!match) return null;
  return [
    Number.parseInt(match[1].slice(0, 2), 16),
    Number.parseInt(match[1].slice(2, 4), 16),
    Number.parseInt(match[1].slice(4, 6), 16),
  ];
}

function setTransparentDragImage(dataTransfer: DataTransfer) {
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  dataTransfer.setDragImage(canvas, 0, 0);
}

function AccountAvatar({
  account,
  large = false,
}: {
  account: AccountSummary;
  large?: boolean;
}) {
  return (
    <span className={`account-avatar ${large ? "account-avatar-large" : ""}`}>
      <span className="account-avatar-media">
        {account.avatarUrl ? (
          <img src={account.avatarUrl} alt="" />
        ) : (
          initials(account)
        )}
      </span>
      <span
        className={`presence-dot presence-${account.presence}`}
        aria-label={account.presenceText}
      />
    </span>
  );
}

function formatActivity(value: string | null) {
  if (!value) return "Never";
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString();
}

function parseCookies(value: string) {
  return value
    .split(/[\n,;\t]+/)
    .map((cookie) => cookie.trim())
    .filter(Boolean);
}

function noticeKind(message: string): ToastKind {
  if (/could not|failed|error|stopped|unable|not be/i.test(message)) {
    return "error";
  }
  if (/enter |not ready|cannot|warning|expired|restricted/i.test(message)) {
    return "warning";
  }
  if (
    /saved|added|requested|refreshed|completed|opened|exported|closed/i.test(
      message,
    )
  ) {
    return "success";
  }
  return "info";
}

function noticeTitle(kind: ToastKind) {
  return kind[0].toUpperCase() + kind.slice(1);
}

function TimedNotice({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}) {
  const [exiting, setExiting] = useState(false);
  const dismissRef = useRef(onDismiss);
  dismissRef.current = onDismiss;

  useEffect(() => {
    setExiting(false);
    const exitTimeout = window.setTimeout(() => setExiting(true), 5000);
    const dismissTimeout = window.setTimeout(() => dismissRef.current(), 5300);
    return () => {
      window.clearTimeout(exitTimeout);
      window.clearTimeout(dismissTimeout);
    };
  }, [message]);

  return (
    <div className={`account-notice-shell ${exiting ? "is-exiting" : ""}`}>
      <p className="account-notice" role="status">
        <span className="notice-timer" aria-hidden="true" />
        {message}
      </p>
    </div>
  );
}

function AccountRow({
  account,
  selected,
  onSelect,
  onDropAccount,
  canDragAccounts,
  accentColor,
  onTogglePin,
  pinning,
  draggingAccountId,
  onAccountDragStateChange,
  dropIndicator,
  onDropIndicatorChange,
}: {
  account: AccountSummary;
  selected: boolean;
  onSelect: (event: AccountSelectionEvent) => void;
  onDropAccount: (
    sourceId: number,
    targetId: number,
    position: DropPosition,
    targetGroup: string,
  ) => void;
  canDragAccounts: boolean;
  accentColor: string;
  onTogglePin: (userId: number) => void;
  pinning: boolean;
  draggingAccountId: number | null;
  onAccountDragStateChange: (id: number | null) => void;
  dropIndicator: DropIndicator | null;
  onDropIndicatorChange: (indicator: DropIndicator | null) => void;
}) {
  return (
    <div
      className={`account-row ${selected ? "is-selected" : ""}`}
      style={{ "--account-accent": accentColor } as CSSProperties}
      role="button"
      tabIndex={0}
      draggable={canDragAccounts}
      onDragStart={
        canDragAccounts
          ? (event) => {
              event.dataTransfer.effectAllowed = "move";
              setTransparentDragImage(event.dataTransfer);
              // webview2 needs a standard mime type to actually start the drag
              event.dataTransfer.setData("text/plain", String(account.userId));
              onAccountDragStateChange(account.userId);
            }
          : undefined
      }
      onDragOver={
        canDragAccounts
          ? (event) => {
              // gate on our own state, not dataTransfer.types - webview2 hides
              // custom types during dragover so the types check never passes
              if (draggingAccountId === null) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              const bounds = event.currentTarget.getBoundingClientRect();
              onDropIndicatorChange({
                kind: "account",
                id: account.userId,
                position:
                  event.clientY < bounds.top + bounds.height / 2
                    ? "before"
                    : "after",
              });
            }
          : undefined
      }
      onDrop={(event) => {
        if (!canDragAccounts || draggingAccountId === null) return;
        event.preventDefault();
        const sourceId = draggingAccountId;
        onAccountDragStateChange(null);
        const position =
          dropIndicator?.kind === "account" &&
          dropIndicator.id === account.userId
            ? dropIndicator.position
            : "after";
        onDropIndicatorChange(null);
        if (sourceId !== account.userId) {
          onDropAccount(sourceId, account.userId, position, account.group);
        }
      }}
      onDragEnd={() => {
        onAccountDragStateChange(null);
        onDropIndicatorChange(null);
      }}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(event);
        }
      }}
    >
      {dropIndicator?.kind === "account" &&
        dropIndicator.id === account.userId && (
          <span
            className={`account-drop-line ${dropIndicator.position}`}
            aria-hidden="true"
          />
        )}
      <span className="account-row-bar" aria-hidden="true" />
      <AccountAvatar account={account} />
      <span className="account-row-name">{account.label}</span>
      <button
        className={`account-row-pin ${account.isPinned ? "is-pinned" : ""}`}
        type="button"
        disabled={pinning}
        aria-label={account.isPinned ? "Unpin account" : "Pin account"}
        data-tip={account.isPinned ? "Unpin account" : "Pin account"}
        onClick={(event) => {
          event.stopPropagation();
          onTogglePin(account.userId);
        }}
      >
        <Icon name={account.isPinned ? "pin-filled" : "pin"} />
      </button>
    </div>
  );
}

function AccountGroup({
  group,
  collapsed,
  onToggle,
  selectedId,
  onSelect,
  onDropAccount,
  canDragAccounts,
  onDropGroup,
  color,
  onTogglePin,
  pinningIds,
  onContextMenu,
  draggingAccountId,
  onAccountDragStateChange,
  draggingGroupName,
  onGroupDragStateChange,
  dropIndicator,
  onDropIndicatorChange,
  showUngroupedSeparator,
}: {
  group: AccountGroup;
  collapsed: boolean;
  selectedId: number | null;
  onToggle: () => void;
  onSelect: (id: number, event: AccountSelectionEvent) => void;
  onDropAccount: (
    sourceId: number,
    targetId: number,
    position: DropPosition,
    targetGroup: string,
  ) => void;
  canDragAccounts: boolean;
  onDropGroup: (
    sourceName: string,
    targetName: string,
    position: DropPosition,
  ) => void;
  color: string;
  onTogglePin: (userId: number) => void;
  pinningIds: Set<number>;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>, name: string) => void;
  draggingAccountId: number | null;
  onAccountDragStateChange: (id: number | null) => void;
  draggingGroupName: string | null;
  onGroupDragStateChange: (name: string | null) => void;
  dropIndicator: DropIndicator | null;
  onDropIndicatorChange: (indicator: DropIndicator | null) => void;
  showUngroupedSeparator: boolean;
}) {
  const isUngrouped = group.name === "Ungrouped";

  return (
    <section
      className={`account-group group-${group.tone} ${
        isUngrouped && showUngroupedSeparator ? "is-ungrouped" : ""
      }`}
      style={
        {
          "--group-color": isUngrouped ? "var(--text-muted)" : color,
        } as CSSProperties
      }
    >
      {isUngrouped && showUngroupedSeparator && (
        <div className="special-categories-label" aria-hidden="true">
          <span>Special Categories</span>
        </div>
      )}
      <button
        className={`account-group-header ${isUngrouped ? "is-ungrouped" : ""}`}
        type="button"
        draggable={group.name !== "Ungrouped"}
        onDragStart={(event) => {
          if (group.name === "Ungrouped") return;
          event.dataTransfer.effectAllowed = "move";
          setTransparentDragImage(event.dataTransfer);
          event.dataTransfer.setData("text/plain", group.name);
          onGroupDragStateChange(group.name);
        }}
        onDragOver={(event) => {
          if (draggingAccountId !== null) {
            event.preventDefault();
            event.dataTransfer.dropEffect = "move";
            onDropIndicatorChange({
              kind: "account",
              id: group.name,
              position: "after",
            });
            return;
          }
          if (draggingGroupName === null) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
          const bounds = event.currentTarget.getBoundingClientRect();
          onDropIndicatorChange({
            kind: "group",
            id: group.name,
            position:
              event.clientY < bounds.top + bounds.height / 2
                ? "before"
                : "after",
          });
        }}
        onDrop={(event) => {
          if (draggingAccountId !== null) {
            event.preventDefault();
            const sourceId = draggingAccountId;
            onAccountDragStateChange(null);
            onDropIndicatorChange(null);
            onDropAccount(sourceId, -1, "after", group.name);
            return;
          }
          if (draggingGroupName === null) return;
          event.preventDefault();
          const sourceName = draggingGroupName;
          onGroupDragStateChange(null);
          const position =
            dropIndicator?.kind === "group" && dropIndicator.id === group.name
              ? dropIndicator.position
              : "after";
          onDropIndicatorChange(null);
          if (sourceName !== group.name)
            onDropGroup(sourceName, group.name, position);
        }}
        onDragEnd={() => {
          onGroupDragStateChange(null);
          onDropIndicatorChange(null);
        }}
        onClick={onToggle}
        onContextMenu={(event) => onContextMenu(event, group.name)}
        aria-expanded={!collapsed}
        aria-haspopup={group.name !== "Ungrouped" ? "menu" : undefined}
      >
        {dropIndicator?.id === group.name && (
          <span
            className={`group-drop-line ${dropIndicator.position}`}
            aria-hidden="true"
          />
        )}
        <span className={`group-chevron ${collapsed ? "is-collapsed" : ""}`}>
          <Icon name="chevron-down" />
        </span>
        <span className="group-marker" aria-hidden="true" />
        <span>{group.name}</span>
        <span className="group-count">{group.accounts.length}</span>
      </button>
      {!collapsed && (
        <div className="account-group-rows">
          {group.accounts.map((account) => (
            <AccountRow
              account={account}
              key={account.userId}
              selected={selectedId === account.userId}
              onSelect={(event) => onSelect(account.userId, event)}
              onDropAccount={(sourceId, targetId, position, targetGroup) =>
                onDropAccount(sourceId, targetId, position, targetGroup)
              }
              canDragAccounts={canDragAccounts}
              accentColor={isUngrouped ? "var(--text-muted)" : color}
              onTogglePin={onTogglePin}
              pinning={pinningIds.has(account.userId)}
              draggingAccountId={draggingAccountId}
              onAccountDragStateChange={onAccountDragStateChange}
              dropIndicator={dropIndicator}
              onDropIndicatorChange={onDropIndicatorChange}
            />
          ))}
        </div>
      )}
    </section>
  );
}

export function AccountsPage() {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [draggingAccountId, setDraggingAccountId] = useState<number | null>(
    null,
  );
  const [draggingGroupName, setDraggingGroupName] = useState<string | null>(
    null,
  );
  const [dropIndicator, setDropIndicator] = useState<DropIndicator | null>(
    null,
  );
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [search, setSearch] = useState("");
  const [sortMode, setSortMode] = useState<SortMode>("custom");
  const [descending, setDescending] = useState(false);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [placeId, setPlaceId] = useState("");
  const [jobId, setJobId] = useState("");
  const [launchData, setLaunchData] = useState("");
  const [connectionQuery, setConnectionQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notices, setNotices] = useState<ToastItem[]>([]);
  const [accountNotices, setAccountNotices] = useState<AccountNotice[]>([]);
  const nextNoticeId = useRef(0);
  const [reloadKey, setReloadKey] = useState(0);
  const [storeStatus, setStoreStatus] = useState<StoreStatus | null>(null);
  const [password, setPassword] = useState("");
  const [alias, setAlias] = useState("");
  const [mutationLoading, setMutationLoading] = useState(false);
  const [browserLoginLoading, setBrowserLoginLoading] = useState(false);
  const [browserLoginOverlayVisible, setBrowserLoginOverlayVisible] =
    useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [addFormClosing, setAddFormClosing] = useState(false);
  const addFormCloseTimer = useRef<number | null>(null);
  const [cookie, setCookie] = useState("");
  const [bulkCookieInput, setBulkCookieInput] = useState("");
  const [bulkProgress, setBulkProgress] = useState<[number, number] | null>(
    null,
  );
  const [bulkResults, setBulkResults] = useState<BulkImportResult[]>([]);
  const [forceAddUsername, setForceAddUsername] = useState("");
  const [addError, setAddError] = useState<string | null>(null);
  const [inventory, setInventory] = useState<InventoryItem[]>([]);
  const [inventoryLoading, setInventoryLoading] = useState(false);
  const [connectionResults, setConnectionResults] = useState<
    UserSearchResult[]
  >([]);
  const [showAccountMenu, setShowAccountMenu] = useState(false);
  const accountMenuRef = useRef<HTMLDivElement>(null);
  const [presenceLoading, setPresenceLoading] = useState(false);
  const [groupColors, setGroupColors] = useState<
    Record<string, [number, number, number]>
  >({});
  const [groupOrder, setGroupOrder] = useState<string[]>([]);
  const [playerPath, setPlayerPath] = useState("");
  const [pinningIds, setPinningIds] = useState<Set<number>>(new Set());
  const [groupContextMenu, setGroupContextMenu] =
    useState<GroupContextMenu | null>(null);
  const [groupEditor, setGroupEditor] = useState<GroupEditorState | null>(null);
  const [groupDeleteConfirmation, setGroupDeleteConfirmation] = useState<
    string | null
  >(null);
  const [killAllConfirmation, setKillAllConfirmation] = useState(false);
  const groupMenuRef = useRef<HTMLDivElement>(null);

  function setNotice(
    message: string | null,
    kind?: ToastKind,
    duration: ToastDuration = "standard",
    title?: string,
  ) {
    if (message === null) {
      setNotices([]);
      return;
    }
    const id = nextNoticeId.current;
    nextNoticeId.current += 1;
    const resolvedKind = kind ?? noticeKind(message);
    setNotices((current) => [
      ...current,
      {
        id,
        title: title ?? noticeTitle(resolvedKind),
        message,
        kind: resolvedKind,
        duration,
      },
    ]);
  }

  function setAccountNotice(message: string) {
    const id = nextNoticeId.current;
    nextNoticeId.current += 1;
    setAccountNotices((current) => [...current, { id, message }]);
  }

  function dismissNotice(id: number) {
    setNotices((current) => current.filter((notice) => notice.id !== id));
  }

  function dismissAccountNotice(id: number) {
    setAccountNotices((current) =>
      current.filter((notice) => notice.id !== id),
    );
  }

  function openAddForm() {
    if (addFormCloseTimer.current !== null) {
      window.clearTimeout(addFormCloseTimer.current);
      addFormCloseTimer.current = null;
    }
    setAddFormClosing(false);
    setShowAddForm(true);
  }

  function closeAddForm() {
    if (!showAddForm || addFormClosing) return;
    setAddFormClosing(true);
    addFormCloseTimer.current = window.setTimeout(() => {
      setShowAddForm(false);
      setAddFormClosing(false);
      addFormCloseTimer.current = null;
    }, 160);
  }

  useEffect(() => {
    if (!showAccountMenu) return;

    function dismissAccountMenu(event: PointerEvent) {
      if (
        event.target instanceof Node &&
        !accountMenuRef.current?.contains(event.target)
      ) {
        setShowAccountMenu(false);
      }
    }

    document.addEventListener("pointerdown", dismissAccountMenu);
    return () =>
      document.removeEventListener("pointerdown", dismissAccountMenu);
  }, [showAccountMenu]);

  useEffect(() => {
    if (!groupContextMenu) return;

    function dismissGroupMenu(event: PointerEvent) {
      if (
        event.target instanceof Node &&
        !groupMenuRef.current?.contains(event.target)
      ) {
        setGroupContextMenu(null);
      }
    }

    function dismissOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setGroupContextMenu(null);
    }

    document.addEventListener("pointerdown", dismissGroupMenu);
    document.addEventListener("keydown", dismissOnEscape);
    return () => {
      document.removeEventListener("pointerdown", dismissGroupMenu);
      document.removeEventListener("keydown", dismissOnEscape);
    };
  }, [groupContextMenu]);

  useEffect(() => {
    if (!showAddForm) return;

    function dismissOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") closeAddForm();
    }

    document.addEventListener("keydown", dismissOnEscape);
    return () => document.removeEventListener("keydown", dismissOnEscape);
  }, [showAddForm, addFormClosing]);

  useEffect(() => {
    return () => {
      if (addFormCloseTimer.current !== null) {
        window.clearTimeout(addFormCloseTimer.current);
      }
    };
  }, []);

  const [commonInventory, setCommonInventory] = useState<InventoryItem[]>([]);
  const [commonInventoryLoading, setCommonInventoryLoading] = useState(false);

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    Promise.all([
      getStoreStatus(),
      listAccountGroupColors(),
      listAccountGroups(),
    ])
      .then(async ([status, colors, groupSummaries]) => {
        if (!mounted) return [];
        setGroupColors(colors);
        setGroupOrder(groupSummaries.map((group) => group.name));
        setStoreStatus(status);
        if (!status.unlocked) {
          if (status.needsPassword) throw new Error("password-required");
          if (status.exists) await unlockDevice();
          else await createDeviceStore();
        }
        return listAccounts();
      })
      .then((nextAccounts) => {
        if (!mounted) return;
        setAccounts(nextAccounts);
        setSelectedId((current) => current ?? nextAccounts[0]?.userId ?? null);
        setError(null);
      })
      .catch((loadError: Error) => {
        if (mounted)
          setError(
            loadError.message === "password-required"
              ? "This account store needs a password to unlock."
              : "Accounts could not be loaded. Check that the account store is unlocked.",
          );
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, [reloadKey]);

  useEffect(() => {
    if (loading || error || !accounts.length) return;
    const refresh = () => {
      void refreshPresence(accounts.map((account) => account.userId));
    };
    const revalidate = window.setInterval(() => {
      void revalidateAccounts([])
        .then((updated) => {
          setAccounts((current) =>
            current.map(
              (account) =>
                updated.find((item) => item.userId === account.userId) ??
                account,
            ),
          );
        })
        .catch(() => setNotice("Automatic account validation failed."));
    }, 300_000);
    const presence = window.setInterval(refresh, 10_000);
    return () => {
      window.clearInterval(revalidate);
      window.clearInterval(presence);
    };
  }, [accounts.length, error, loading]);

  const visibleAccounts = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    const filtered = accounts.filter((account) => {
      if (!query) return true;
      return [account.label, account.username, account.displayName]
        .join(" ")
        .toLocaleLowerCase()
        .includes(query);
    });
    return [...filtered].sort((left, right) => {
      if (sortMode === "custom" && left.isPinned !== right.isPinned) {
        return left.isPinned ? -1 : 1;
      }
      if (sortMode === "custom" && left.sortOrder !== right.sortOrder) {
        return left.sortOrder - right.sortOrder;
      }
      if (sortMode === "username")
        return (
          left.username.localeCompare(right.username) * (descending ? -1 : 1)
        );
      if (sortMode === "lastActivity") {
        const leftTime = left.lastActivity ? Date.parse(left.lastActivity) : 0;
        const rightTime = right.lastActivity
          ? Date.parse(right.lastActivity)
          : 0;
        return (leftTime - rightTime) * (descending ? -1 : 1);
      }
      if (sortMode === "status") {
        return (
          left.presenceText.localeCompare(right.presenceText) *
          (descending ? -1 : 1)
        );
      }
      if (sortMode === "accountAge") {
        const leftTime = left.createdAt ? Date.parse(left.createdAt) : 0;
        const rightTime = right.createdAt ? Date.parse(right.createdAt) : 0;
        return (leftTime - rightTime) * (descending ? -1 : 1);
      }
      return 0;
    });
  }, [accounts, descending, search, sortMode]);

  const groups = useMemo<AccountGroup[]>(() => {
    const grouped = new Map<string, AccountSummary[]>();
    for (const name of [...groupOrder, ...Object.keys(groupColors)]) {
      if (!grouped.has(name)) grouped.set(name, []);
    }
    for (const account of visibleAccounts) {
      const name = account.group.trim() || "Ungrouped";
      grouped.set(name, [...(grouped.get(name) ?? []), account]);
    }
    return [...grouped.entries()]
      .sort(([left], [right]) => {
        const leftOrder = groupOrder.indexOf(left);
        const rightOrder = groupOrder.indexOf(right);
        return (
          (leftOrder < 0 ? Number.MAX_SAFE_INTEGER : leftOrder) -
          (rightOrder < 0 ? Number.MAX_SAFE_INTEGER : rightOrder)
        );
      })
      .map(([name, groupedAccounts], index) => ({
        name,
        tone: index % 2 === 0 ? "primary" : "secondary",
        accounts: groupedAccounts,
        color: groupColors[name]
          ? `rgb(${groupColors[name][0]}, ${groupColors[name][1]}, ${groupColors[name][2]})`
          : index % 2 === 0
            ? "var(--status-danger)"
            : "var(--status-warning)",
      }));
  }, [groupColors, groupOrder, visibleAccounts]);

  const hasNamedGroups = groups.some((group) => group.name !== "Ungrouped");

  const selectedAccount =
    accounts.find((account) => account.userId === selectedId) ?? null;
  const placeIdValid = /^\d+$/.test(placeId.trim());

  useEffect(() => {
    setAlias(selectedAccount?.alias ?? "");
    setPlayerPath(selectedAccount?.playerPath ?? "");
    setAccountNotices([]);
    setInventory([]);
    setCommonInventory([]);
    setConnectionResults([]);
  }, [selectedAccount]);

  function selectAccount(id: number) {
    setSelectedId(id);
    setSelectedIds((current) => new Set(current).add(id));
    setNotice(null);
  }

  function selectAccountWithModifiers(
    id: number,
    event: AccountSelectionEvent,
  ) {
    if (event.ctrlKey || event.metaKey) {
      setSelectedIds((current) => {
        const next = new Set(current);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
      setSelectedId(id);
      return;
    }
    setSelectedIds(new Set([id]));
    selectAccount(id);
  }

  async function refreshPresence(accountIds?: number[]) {
    const ids =
      accountIds ??
      (selectedIds.size
        ? [...selectedIds]
        : accounts.map((account) => account.userId));
    if (!ids.length) return;
    setPresenceLoading(true);
    try {
      const updates = await refreshAccountPresence(ids);
      setAccounts((current) =>
        current.map((account) => {
          const update = updates.find((item) => item.userId === account.userId);
          return update
            ? {
                ...account,
                presence: update.presence,
                presenceText: update.presenceText,
              }
            : account;
        }),
      );
    } catch {
      setNotice("Presence could not be refreshed.");
    } finally {
      setPresenceLoading(false);
    }
  }

  async function reorderAccount(
    sourceId: number,
    targetId: number,
    position: DropPosition,
    targetGroup: string,
  ) {
    if (sortMode !== "custom") return;
    const ordered = [...accounts].sort((left, right) => {
      if (left.isPinned !== right.isPinned) return left.isPinned ? -1 : 1;
      return left.sortOrder - right.sortOrder;
    });
    const sourceIndex = ordered.findIndex(
      (account) => account.userId === sourceId,
    );
    if (sourceIndex < 0) return;
    const [source] = ordered.splice(sourceIndex, 1);
    const targetIndex =
      targetId > 0
        ? ordered.findIndex((account) => account.userId === targetId)
        : -1;
    let insertionIndex = targetIndex;
    if (targetIndex < 0) {
      const groupIndexes = ordered.reduce<number[]>(
        (indexes, account, index) => {
          if ((account.group.trim() || "Ungrouped") === targetGroup)
            indexes.push(index);
          return indexes;
        },
        [],
      );
      insertionIndex =
        groupIndexes.length > 0
          ? groupIndexes[groupIndexes.length - 1]
          : ordered.length - 1;
      position = "after";
    } else if (position === "after") {
      insertionIndex += 1;
    }
    ordered.splice(Math.max(0, insertionIndex), 0, source);
    try {
      const currentGroup = source.group.trim() || "Ungrouped";
      const nextGroup = targetGroup.trim() || "Ungrouped";
      if (currentGroup !== nextGroup) {
        await updateAccountGroup(
          sourceId,
          nextGroup === "Ungrouped" ? "" : nextGroup,
        );
      }
      setAccounts(
        await reorderAccounts(ordered.map((account) => account.userId)),
      );
      setNotice("Account order saved.");
    } catch {
      setNotice("The account move could not be saved.");
    }
  }

  async function reorderGroup(
    sourceName: string,
    targetName: string,
    position: DropPosition,
  ) {
    const nextOrder = [...groupOrder];
    const sourceIndex = nextOrder.indexOf(sourceName);
    if (sourceIndex < 0) return;
    nextOrder.splice(sourceIndex, 1);
    const targetIndex = nextOrder.indexOf(targetName);
    if (targetIndex < 0) return;
    nextOrder.splice(
      position === "after" ? targetIndex + 1 : targetIndex,
      0,
      sourceName,
    );
    try {
      const groups = await reorderAccountGroups(nextOrder);
      setGroupOrder(groups.map((group) => group.name));
      setNotice("Group order saved.");
    } catch {
      setNotice("The group order could not be saved.");
    }
  }

  async function revalidate() {
    const ids = selectedIds.size ? [...selectedIds] : [];
    setMutationLoading(true);
    try {
      const updated = await revalidateAccounts(ids);
      setAccounts((current) =>
        current.map(
          (account) =>
            updated.find((item) => item.userId === account.userId) ?? account,
        ),
      );
      setNotice("Account credentials revalidated.");
    } catch {
      setNotice("Account validation could not be completed.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function bulkLaunch() {
    if (!placeIdValid) {
      setNotice("Enter a numeric Place ID before bulk launching.");
      return;
    }
    setMutationLoading(true);
    try {
      for (const userId of selectedIds) {
        await launchAccountIpc(userId, Number(placeId), jobId, launchData);
      }
      setNotice(`${selectedIds.size} launches requested.`);
    } catch {
      setNotice("Bulk launch stopped because one account could not launch.");
    } finally {
      setMutationLoading(false);
    }
  }

  function launchAccount() {
    if (!placeIdValid) {
      setAccountNotice("Enter a numeric Place ID before launching.");
      return;
    }
    if (!selectedAccount?.canLaunch) {
      setAccountNotice("This account is not ready to launch.");
      return;
    }
    setMutationLoading(true);
    void launchAccountCommand();
  }

  async function launchAccountCommand() {
    if (!selectedAccount) return;
    try {
      await launchAccountIpc(
        selectedAccount.userId,
        Number(placeId),
        jobId,
        launchData,
      );
      setAccountNotice("Launch requested.");
    } catch {
      setAccountNotice("Roblox could not be launched for this account.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function savePreset() {
    if (!placeIdValid) {
      setAccountNotice("Enter a numeric Place ID before saving a preset.");
      return;
    }
    const name = window.prompt("Preset name");
    if (!name) return;
    try {
      await saveLaunchPreset(name, Number(placeId), jobId, launchData);
      setAccountNotice("Preset saved.");
    } catch {
      setAccountNotice("The preset could not be saved.");
    }
  }

  async function unlockStore() {
    setMutationLoading(true);
    setError(null);
    try {
      await unlockPassword(password);
      setPassword("");
      setReloadKey((current) => current + 1);
    } catch {
      setError(
        "The password was not accepted or the account store could not be opened.",
      );
    } finally {
      setMutationLoading(false);
    }
  }

  async function saveAlias() {
    if (!selectedAccount) return;
    setMutationLoading(true);
    try {
      const updated = await updateAccountAlias(selectedAccount.userId, alias);
      setAccounts((current) =>
        current.map((account) =>
          account.userId === updated.userId ? updated : account,
        ),
      );
      setNotice("Alias saved.");
    } catch {
      setNotice("The alias could not be saved.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function togglePinForAccount(userId: number) {
    setPinningIds((current) => new Set(current).add(userId));
    try {
      const updated = await toggleAccountPin(userId);
      setAccounts((current) =>
        current.map((account) =>
          account.userId === updated.userId ? updated : account,
        ),
      );
    } catch {
      setNotice("The pin state could not be saved.");
    } finally {
      setPinningIds((current) => {
        const next = new Set(current);
        next.delete(userId);
        return next;
      });
    }
  }

  async function togglePin() {
    if (!selectedAccount) return;
    await togglePinForAccount(selectedAccount.userId);
  }

  async function saveGroup(group: string) {
    if (!selectedAccount) return;
    try {
      const updated = await updateAccountGroup(selectedAccount.userId, group);
      setAccounts((current) =>
        current.map((account) =>
          account.userId === updated.userId ? updated : account,
        ),
      );
      setNotice("Group saved.");
    } catch {
      setNotice("The group could not be saved.");
    }
  }

  async function selectGroup(value: string) {
    if (value === "__new__") {
      const name = window.prompt("Group name");
      if (!name) return;
      try {
        await createAccountGroup(name);
        await saveGroup(name);
      } catch {
        setNotice("The group could not be created.");
      }
      return;
    }
    if (value === "__delete__" && selectedAccount?.group) {
      requestGroupDeletion(selectedAccount.group);
      return;
    }
    await saveGroup(value);
  }

  async function createGroup() {
    setGroupEditor({
      originalName: null,
      name: "",
      color: rgbToHex([59, 130, 246]),
    });
  }

  function requestGroupDeletion(name: string) {
    setGroupContextMenu(null);
    setGroupDeleteConfirmation(name);
  }

  function handleGroupContextMenu(
    event: MouseEvent<HTMLButtonElement>,
    name: string,
  ) {
    event.preventDefault();
    if (name === "Ungrouped") return;
    setGroupContextMenu({ name, x: event.clientX, y: event.clientY });
  }

  function openGroupEditor(name: string) {
    const color = groupColors[name] ?? [59, 130, 246];
    setGroupEditor({ originalName: name, name, color: rgbToHex(color) });
    setGroupContextMenu(null);
  }

  async function saveGroupEditor() {
    if (!groupEditor) return;
    const name = groupEditor.name.trim();
    const color = hexToRgb(groupEditor.color);
    if (!name || !color) {
      setNotice("Enter a group name and choose a valid color.");
      return;
    }
    setMutationLoading(true);
    try {
      if (groupEditor.originalName === null) {
        await createAccountGroup(name);
      }
      const groups = await updateAccountGroupMeta(
        groupEditor.originalName ?? name,
        name,
        color,
      );
      const nextColors: Record<string, [number, number, number]> = {};
      for (const group of groups) nextColors[group.name] = group.color;
      setGroupColors(nextColors);
      setGroupOrder(groups.map((group) => group.name));
      if (groupEditor.originalName && name !== groupEditor.originalName) {
        setAccounts((current) =>
          current.map((account) =>
            account.group === groupEditor.originalName
              ? { ...account, group: name }
              : account,
          ),
        );
      }
      setGroupEditor(null);
      setNotice(
        groupEditor.originalName === null ? "Group created." : "Group saved.",
      );
    } catch {
      setNotice("The group could not be saved.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function deleteGroupFromMenu(name: string) {
    requestGroupDeletion(name);
  }

  async function confirmGroupDeletion() {
    if (!groupDeleteConfirmation) return;
    const name = groupDeleteConfirmation;
    setGroupDeleteConfirmation(null);
    setMutationLoading(true);
    try {
      await deleteAccountGroup(name);
      setAccounts((current) =>
        current.map((account) =>
          account.group === name ? { ...account, group: "" } : account,
        ),
      );
      setGroupColors((current) => {
        const next = { ...current };
        delete next[name];
        return next;
      });
      setGroupOrder((current) => current.filter((group) => group !== name));
      setNotice("Group deleted.");
    } catch {
      setNotice("The group could not be deleted.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function savePlayerPath() {
    if (!selectedAccount) return;
    try {
      await updatePlayerPath(selectedAccount.userId, playerPath.trim() || null);
      setNotice("Player path saved.");
    } catch {
      setNotice("The player path could not be saved.");
    }
  }

  async function openAccountPage(inventory: boolean) {
    if (!selectedAccount) return;
    try {
      await openAccountUrl(selectedAccount.userId, inventory);
    } catch {
      setNotice("Roblox could not be opened.");
    }
  }

  async function copyUsername() {
    if (!selectedAccount) return;
    try {
      await navigator.clipboard.writeText(selectedAccount.username);
      setNotice("Username copied.");
    } catch {
      setNotice("Username could not be copied.");
    }
  }

  async function removeSelectedAccount() {
    if (!selectedAccount || !window.confirm(`Remove ${selectedAccount.label}?`))
      return;
    setMutationLoading(true);
    try {
      await removeAccount(selectedAccount.userId);
      const remaining = accounts.filter(
        (account) => account.userId !== selectedAccount.userId,
      );
      setAccounts(remaining);
      setSelectedId(remaining[0]?.userId ?? null);
      setNotice("Account removed.");
    } catch {
      setNotice("The account could not be removed.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function loadInventory() {
    if (!selectedAccount) return;
    setInventoryLoading(true);
    try {
      setInventory(await fetchAccountInventory(selectedAccount.userId));
    } catch {
      setNotice("Inventory could not be loaded for this account.");
    } finally {
      setInventoryLoading(false);
    }
  }

  async function searchConnections() {
    if (!connectionQuery.trim()) {
      setNotice("Enter a username or user ID to search.");
      return;
    }
    try {
      setConnectionResults(await searchConnectionUsers(connectionQuery));
    } catch {
      setNotice("Roblox user search failed.");
    }
  }

  async function applyConnectionAction(targetUserId: number, action: string) {
    if (!selectedAccount) return;
    try {
      await runConnectionAction(selectedAccount.userId, targetUserId, action);
      setNotice("Connection action completed.");
    } catch {
      setNotice("The connection action could not be completed.");
    }
  }

  async function applyBulkConnectionAction(
    targetUserId: number,
    action: string,
  ) {
    const ids = [...selectedIds];
    if (!ids.length) return;
    setMutationLoading(true);
    try {
      for (const userId of ids) {
        await runConnectionAction(userId, targetUserId, action);
      }
      setNotice(`${action} completed for ${ids.length} account(s).`);
    } catch {
      setNotice(`${action} could not be completed for every account.`);
    } finally {
      setMutationLoading(false);
    }
  }

  async function joinTargetGame(targetUserId: number) {
    if (!selectedAccount) return;
    try {
      await joinUserGame(selectedAccount.userId, targetUserId);
      setNotice("Join requested.");
    } catch {
      setNotice("The target user is not currently joinable.");
    }
  }

  async function openSelectedBrowsers() {
    const ids = [...selectedIds];
    if (!ids.length) return;
    setMutationLoading(true);
    try {
      for (const userId of ids) await browseAsAccount(userId);
      setNotice(`Opened ${ids.length} authenticated browser(s).`);
    } catch {
      setNotice("One or more authenticated browsers could not be opened.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function copySelectedIds() {
    try {
      await navigator.clipboard.writeText([...selectedIds].join("\n"));
      setNotice("Account IDs copied.");
    } catch {
      setNotice("Account IDs could not be copied.");
    }
  }

  async function changeSelectedPath() {
    const path = window.prompt("Roblox player path", playerPath);
    if (path === null) return;
    setMutationLoading(true);
    try {
      const updates = await Promise.all(
        [...selectedIds].map((userId) =>
          updatePlayerPath(userId, path.trim() || null),
        ),
      );
      setAccounts((current) =>
        current.map(
          (account) =>
            updates.find((item) => item.userId === account.userId) ?? account,
        ),
      );
      setNotice("Player path updated.");
    } catch {
      setNotice("The player path could not be updated for every account.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function loadCommonInventory() {
    const ids = [...selectedIds];
    if (!ids.length) return;
    setCommonInventoryLoading(true);
    try {
      const inventories = await Promise.all(
        ids.map((userId) => fetchAccountInventory(userId)),
      );
      const counts = new Map<number, { item: InventoryItem; count: number }>();
      for (const item of inventories[0] ?? [])
        counts.set(item.assetId, { item, count: 1 });
      for (const inventoryItems of inventories.slice(1)) {
        const present = new Set(inventoryItems.map((item) => item.assetId));
        for (const [assetId, entry] of counts) {
          if (present.has(assetId)) entry.count += 1;
          else counts.delete(assetId);
        }
      }
      setCommonInventory([...counts.values()].map(({ item }) => item));
      setNotice(`${counts.size} common inventory item(s) found.`);
    } catch {
      setNotice("Common inventory could not be loaded.");
    } finally {
      setCommonInventoryLoading(false);
    }
  }

  function exportAccountsCsv() {
    const escape = (value: string) => `"${value.replace(/"/g, '""')}"`;
    const rows = [
      "username,display_name,user_id,alias,group,created_at,last_activity",
      ...accounts.map((account) =>
        [
          account.username,
          account.displayName,
          String(account.userId),
          account.alias,
          account.group,
          account.createdAt ?? "",
          account.lastActivity ?? "",
        ]
          .map(escape)
          .join(","),
      ),
    ];
    const url = URL.createObjectURL(
      new Blob([rows.join("\n")], { type: "text/csv" }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = "roblox-accounts.csv";
    link.click();
    URL.revokeObjectURL(url);
    setNotice(`Exported ${accounts.length} account(s).`);
  }

  async function addManagedAccount() {
    setMutationLoading(true);
    setAddError(null);
    try {
      const account = await addAccount(cookie);
      setAccounts((current) => [...current, account]);
      setSelectedId(account.userId);
      setCookie("");
      closeAddForm();
      setNotice("Account added.");
    } catch (addError) {
      setAddError(
        addError instanceof Error
          ? addError.message
          : "The account could not be added.",
      );
      setNotice(
        addError instanceof Error
          ? addError.message
          : "The account could not be added.",
      );
    } finally {
      setMutationLoading(false);
    }
  }

  async function addManagedAccountAnyway() {
    setMutationLoading(true);
    try {
      const account = await addAccountAnyway(cookie, forceAddUsername);
      setAccounts((current) => [...current, account]);
      setSelectedId(account.userId);
      setCookie("");
      setForceAddUsername("");
      setAddError(null);
      closeAddForm();
      setNotice("Account added without validation.");
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "The account could not be added.",
      );
    } finally {
      setMutationLoading(false);
    }
  }

  async function importAccounts() {
    const cookies = parseCookies(bulkCookieInput);
    if (!cookies.length) {
      setNotice("Paste at least one Roblox security cookie.");
      return;
    }
    setMutationLoading(true);
    setBulkProgress([0, cookies.length]);
    setBulkResults([]);
    let added = 0;
    const results: BulkImportResult[] = [];
    for (const [index, value] of cookies.entries()) {
      try {
        const account = await addAccount(value);
        setAccounts((current) => [...current, account]);
        setSelectedId(account.userId);
        added += 1;
        results.push({ index: index + 1, status: "added" });
      } catch (error) {
        results.push({
          index: index + 1,
          status: "failed",
          message: error instanceof Error ? error.message : "Validation failed",
        });
      }
      setBulkResults([...results]);
      setBulkProgress([index + 1, cookies.length]);
    }
    setMutationLoading(false);
    setBulkCookieInput("");
    setNotice(`Bulk import finished: ${added} of ${cookies.length} added.`);
    setBulkProgress(null);
  }

  function changeSortMode(nextMode: SortMode) {
    if (
      sortMode === "custom" &&
      nextMode !== "custom" &&
      !window.confirm(
        "Leave custom order? Drag-and-drop reordering is disabled until Custom sorting is selected again.",
      )
    ) {
      return;
    }
    setSortMode(nextMode);
  }

  async function confirmKillAll() {
    setKillAllConfirmation(false);
    setMutationLoading(true);
    try {
      const count = await killAllAccounts();
      setNotice(`Closed ${count} Roblox client(s).`);
    } catch {
      setNotice("Roblox clients could not be closed.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function addAccountFromBrowser() {
    setBrowserLoginOverlayVisible(true);
    setBrowserLoginLoading(true);
    setMutationLoading(true);
    try {
      const account = await loginAndAddAccount();
      if (!account) {
        setNotice("Browser login was canceled.");
        return;
      }
      setAccounts((current) => [...current, account]);
      setSelectedId(account.userId);
      closeAddForm();
      setNotice("Account added.");
    } catch {
      setNotice("Browser login could not add the account.");
    } finally {
      setBrowserLoginLoading(false);
      window.setTimeout(() => setBrowserLoginOverlayVisible(false), 160);
      setMutationLoading(false);
    }
  }

  async function browseAs(inventory = false) {
    if (!selectedAccount) return;
    try {
      await browseAsAccount(selectedAccount.userId, inventory);
      setAccountNotice("Opening authenticated Roblox browser...");
    } catch {
      setAccountNotice("The authenticated Roblox browser could not be opened.");
    }
  }

  return (
    <>
      <div className="header-row accounts-header-row">
        <h1 className="header-title">Accounts</h1>
        <button
          className="accounts-header-export"
          type="button"
          aria-label="Export accounts"
          onClick={exportAccountsCsv}
        >
          <Icon name="copy" />
          Export accounts
        </button>
      </div>
      <main
        className="accounts-page"
        aria-busy={loading}
        onContextMenu={(event) => event.preventDefault()}
      >
        <aside className="accounts-list-panel" aria-label="Managed accounts">
          <div className="accounts-list-head">
            <div className="accounts-search-row">
              <label className="accounts-search-field">
                <Icon name="search" />
                <input
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search accounts"
                />
              </label>
              <button
                className="icon-button bordered"
                type="button"
                aria-label="Add account"
                data-tip="Add account"
                onClick={() => (showAddForm ? closeAddForm() : openAddForm())}
              >
                <Icon name="add" />
              </button>
              <button
                className="icon-button bordered"
                type="button"
                aria-label="Add group"
                data-tip="Add group"
                onClick={() => void createGroup()}
              >
                <Icon name="groups" />
              </button>
            </div>
            <div className="accounts-sort-row">
              <span>Sort:</span>
              <select
                value={sortMode}
                onChange={(event) =>
                  changeSortMode(event.target.value as SortMode)
                }
                aria-label="Sort accounts"
              >
                <option value="custom">Custom</option>
                <option value="username">Username</option>
                <option value="status">Status</option>
                <option value="accountAge">Account age</option>
                <option value="lastActivity">Last used</option>
              </select>
              <select
                value={descending ? "descending" : "ascending"}
                onChange={(event) =>
                  setDescending(event.target.value === "descending")
                }
                aria-label="Sort direction"
              >
                <option value="ascending">Ascending</option>
                <option value="descending">Descending</option>
              </select>
            </div>
            {selectedIds.size > 1 && (
              <div className="bulk-account-actions">
                <span>{selectedIds.size} selected</span>
                <button
                  type="button"
                  onClick={() => void refreshPresence()}
                  disabled={presenceLoading}
                >
                  <Icon name="refresh" />
                  Refresh status
                </button>
                <button
                  type="button"
                  onClick={() => void bulkLaunch()}
                  disabled={mutationLoading}
                >
                  <Icon name="launch" />
                  Bulk launch
                </button>
                <button
                  type="button"
                  onClick={() => void openSelectedBrowsers()}
                  disabled={mutationLoading}
                >
                  <Icon name="browser" />
                  Open browsers
                </button>
                <button type="button" onClick={() => void copySelectedIds()}>
                  Copy IDs
                </button>
                <button
                  type="button"
                  onClick={() => void changeSelectedPath()}
                  disabled={mutationLoading}
                >
                  Change path
                </button>
                <button
                  type="button"
                  onClick={() => void loadCommonInventory()}
                  disabled={commonInventoryLoading}
                >
                  Common inventory
                </button>
              </div>
            )}
            {commonInventory.length > 0 && (
              <p className="common-inventory-summary">
                {commonInventory.length} common inventory item(s)
              </p>
            )}
          </div>
          <div className="accounts-groups">
            {loading && (
              <p className="accounts-list-message">Loading accounts...</p>
            )}
            {!loading && error && (
              <div className="accounts-list-message accounts-error">
                <p>{error}</p>
                {storeStatus?.needsPassword ? (
                  <div className="unlock-form">
                    <input
                      type="password"
                      value={password}
                      onChange={(event) => setPassword(event.target.value)}
                      placeholder="Master password"
                      aria-label="Master password"
                    />
                    <button
                      className="account-button"
                      type="button"
                      disabled={!password || mutationLoading}
                      onClick={() => void unlockStore()}
                    >
                      Unlock accounts
                    </button>
                  </div>
                ) : (
                  <button
                    className="account-button"
                    type="button"
                    onClick={() => setReloadKey((current) => current + 1)}
                  >
                    Retry loading accounts
                  </button>
                )}
              </div>
            )}
            {!loading && !error && groups.length === 0 && (
              <p className="accounts-list-message">
                No accounts match this search.
              </p>
            )}
            {groups.map((group) => (
              <AccountGroup
                group={group}
                key={group.name}
                collapsed={Boolean(collapsed[group.name])}
                onToggle={() =>
                  setCollapsed((current) => ({
                    ...current,
                    [group.name]: !current[group.name],
                  }))
                }
                selectedId={selectedId}
                onSelect={selectAccountWithModifiers}
                onDropAccount={(sourceId, targetId, position, targetGroup) =>
                  void reorderAccount(sourceId, targetId, position, targetGroup)
                }
                canDragAccounts={sortMode === "custom"}
                onDropGroup={(sourceName, targetName, position) =>
                  void reorderGroup(sourceName, targetName, position)
                }
                color={group.color}
                onTogglePin={(userId) => void togglePinForAccount(userId)}
                pinningIds={pinningIds}
                onContextMenu={handleGroupContextMenu}
                draggingAccountId={draggingAccountId}
                onAccountDragStateChange={setDraggingAccountId}
                draggingGroupName={draggingGroupName}
                onGroupDragStateChange={setDraggingGroupName}
                dropIndicator={dropIndicator}
                onDropIndicatorChange={setDropIndicator}
                showUngroupedSeparator={hasNamedGroups}
              />
            ))}
          </div>
          {groupContextMenu && (
            <div
              className="account-menu group-context-menu"
              ref={groupMenuRef}
              role="menu"
              style={{ left: groupContextMenu.x, top: groupContextMenu.y }}
            >
              <button
                type="button"
                role="menuitem"
                onClick={() => openGroupEditor(groupContextMenu.name)}
              >
                <Icon name="edit" />
                Rename and color
              </button>
              <div className="account-menu-separator" role="separator" />
              <button
                className="account-menu-danger"
                type="button"
                role="menuitem"
                onClick={() => void deleteGroupFromMenu(groupContextMenu.name)}
              >
                <Icon name="delete" />
                Delete group
              </button>
            </div>
          )}
        </aside>

        {showAddForm && (
          <div
            className={`add-account-modal-backdrop ${
              addFormClosing ? "is-closing" : ""
            }`}
            role="presentation"
            onPointerDown={(event) => event.stopPropagation()}
          >
            <section
              className={`add-account-modal ${
                browserLoginLoading ? "is-browser-pending" : ""
              }`}
              role="dialog"
              aria-modal="true"
              aria-labelledby="add-account-title"
              aria-busy={browserLoginLoading}
              onClick={(event) => event.stopPropagation()}
            >
              <div className="add-account-modal-header">
                <div>
                  <h2 id="add-account-title">Add account</h2>
                  <p>Choose a secure way to add a Roblox account.</p>
                </div>
                <button
                  className="icon-button"
                  type="button"
                  aria-label="Close add account dialog"
                  data-tip="Close"
                  onClick={closeAddForm}
                >
                  <Icon name="close" />
                </button>
              </div>
              <div className="add-account-modal-body">
                <button
                  className="account-button"
                  type="button"
                  disabled={mutationLoading}
                  onClick={() => void addAccountFromBrowser()}
                >
                  <Icon name="browser" />
                  Log in with browser
                </button>
                <div className="account-form-divider">or paste a cookie</div>
                <label htmlFor="account-cookie">Roblox security cookie</label>
                <input
                  id="account-cookie"
                  type="password"
                  value={cookie}
                  onChange={(event) => setCookie(event.target.value)}
                  placeholder="Paste cookie to validate"
                  autoComplete="off"
                />
                <button
                  className="account-button account-status-button primary"
                  type="button"
                  disabled={!cookie || mutationLoading}
                  onClick={() => void addManagedAccount()}
                >
                  Validate and add account
                </button>
                {addError && (
                  <div className="add-account-recovery">
                    <strong>{addError}</strong>
                    <label htmlFor="force-add-username">
                      Roblox username for unvalidated account
                    </label>
                    <input
                      id="force-add-username"
                      value={forceAddUsername}
                      onChange={(event) =>
                        setForceAddUsername(event.target.value)
                      }
                      placeholder="Username"
                      autoComplete="off"
                    />
                    <button
                      className="account-button"
                      type="button"
                      disabled={!forceAddUsername.trim() || mutationLoading}
                      onClick={() => void addManagedAccountAnyway()}
                    >
                      Add anyway
                    </button>
                  </div>
                )}
                <div className="account-form-divider">bulk import</div>
                <textarea
                  value={bulkCookieInput}
                  onChange={(event) => setBulkCookieInput(event.target.value)}
                  placeholder="Paste one cookie per line, comma-separated, or load a text file"
                  rows={4}
                  disabled={mutationLoading}
                />
                <input
                  type="file"
                  accept=".txt,.csv,.tsv,text/plain,text/csv"
                  disabled={mutationLoading}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) void file.text().then(setBulkCookieInput);
                    event.currentTarget.value = "";
                  }}
                />
                <button
                  className="account-button account-status-button"
                  type="button"
                  disabled={
                    !parseCookies(bulkCookieInput).length || mutationLoading
                  }
                  onClick={() => void importAccounts()}
                >
                  Import {parseCookies(bulkCookieInput).length} account(s)
                </button>
                {bulkProgress && (
                  <span className="account-form-progress">
                    Processed {bulkProgress[0]} of {bulkProgress[1]}...
                  </span>
                )}
                {!bulkProgress && bulkResults.length > 0 && (
                  <ul
                    className="account-form-results"
                    aria-label="Bulk import results"
                  >
                    {bulkResults.map((result) => (
                      <li key={result.index}>
                        <span
                          className={
                            result.status === "added"
                              ? "result-success"
                              : "result-failure"
                          }
                        >
                          Cookie {result.index}: {result.status}
                        </span>
                        {result.message && <span>{result.message}</span>}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              {browserLoginOverlayVisible && (
                <div
                  className={`add-account-browser-overlay ${
                    browserLoginLoading ? "" : "is-fading-out"
                  }`}
                  role="status"
                >
                  <span className="add-account-spinner" aria-hidden="true" />
                  <span>Waiting for browser login...</span>
                </div>
              )}
            </section>
          </div>
        )}

        {groupEditor && (
          <div className="group-editor-backdrop" role="presentation">
            <section
              className="group-editor"
              role="dialog"
              aria-modal="true"
              aria-labelledby="group-editor-title"
            >
              <div className="group-editor-header">
                <h2 id="group-editor-title">
                  {groupEditor.originalName ? "Edit group" : "Create group"}
                </h2>
                <button
                  className="icon-button"
                  type="button"
                  aria-label="Close group editor"
                  data-tip="Close"
                  onClick={() => setGroupEditor(null)}
                >
                  <Icon name="close" />
                </button>
              </div>
              <label className="group-editor-field">
                <span>Group name</span>
                <input
                  value={groupEditor.name}
                  onChange={(event) =>
                    setGroupEditor((current) =>
                      current ? { ...current, name: event.target.value } : null,
                    )
                  }
                  maxLength={64}
                  autoFocus
                />
              </label>
              <div className="group-editor-field">
                <span>Color</span>
                <div className="group-color-controls">
                  <input
                    className="group-color-picker"
                    type="color"
                    value={groupEditor.color}
                    aria-label="Choose group color"
                    onChange={(event) =>
                      setGroupEditor((current) =>
                        current
                          ? { ...current, color: event.target.value }
                          : null,
                      )
                    }
                  />
                  <span className="group-color-value">{groupEditor.color}</span>
                </div>
                <div className="group-color-presets">
                  {GROUP_COLOR_PRESETS.map((preset) => (
                    <button
                      className="group-color-swatch"
                      key={preset.label}
                      type="button"
                      aria-label={`${preset.label} group color`}
                      aria-pressed={
                        groupEditor.color === rgbToHex(preset.color)
                      }
                      style={{
                        backgroundColor: `rgb(${preset.color.join(", ")})`,
                      }}
                      onClick={() =>
                        setGroupEditor((current) =>
                          current
                            ? { ...current, color: rgbToHex(preset.color) }
                            : null,
                        )
                      }
                    />
                  ))}
                </div>
              </div>
              <div className="group-editor-actions">
                <button
                  className="account-button"
                  type="button"
                  onClick={() => setGroupEditor(null)}
                >
                  <Icon name="close" />
                  Cancel
                </button>
                <button
                  className="account-button account-status-button primary"
                  type="button"
                  disabled={mutationLoading}
                  onClick={() => void saveGroupEditor()}
                >
                  <Icon name="save" />
                  {groupEditor.originalName ? "Save group" : "Create group"}
                </button>
              </div>
            </section>
          </div>
        )}

        {groupDeleteConfirmation && (
          <ConfirmModal
            title="Delete group?"
            message={
              <>
                Delete{" "}
                <strong
                  style={{
                    color: groups.find(
                      (group) => group.name === groupDeleteConfirmation,
                    )?.color,
                  }}
                >
                  {groupDeleteConfirmation}
                </strong>
                ? This will not delete the accounts. They will remain managed
                and become ungrouped.
              </>
            }
            confirmLabel="Delete group"
            confirmDisabled={mutationLoading}
            onCancel={() => setGroupDeleteConfirmation(null)}
            onConfirm={() => void confirmGroupDeletion()}
          />
        )}

        {killAllConfirmation && (
          <ConfirmModal
            title="Kill all Roblox clients?"
            message="This closes every Roblox client currently running on this computer."
            confirmLabel="Kill all Roblox"
            confirmIcon="kill"
            confirmDisabled={mutationLoading}
            onCancel={() => setKillAllConfirmation(false)}
            onConfirm={() => void confirmKillAll()}
          />
        )}

        <section className="accounts-detail-panel" aria-label="Account details">
          {!selectedAccount ? (
            <div className="accounts-detail-empty">
              <Icon name="id-card" />
              <h2>Select an account</h2>
              <p>
                Choose a managed account to view launch controls and account
                details.
              </p>
            </div>
          ) : (
            <>
              <section className="account-card account-profile-card">
                <AccountAvatar account={selectedAccount} large />
                <div className="account-profile-copy">
                  <div className="account-identity-name">
                    <h2>
                      {selectedAccount.displayName || selectedAccount.label}
                    </h2>
                    <button
                      className="identity-icon-button"
                      type="button"
                      aria-label="Open account profile in browser"
                      data-tip="Open profile"
                      onClick={() => void openAccountPage(false)}
                    >
                      <Icon name="link" />
                    </button>
                  </div>
                  <div className="account-identity-username">
                    <p>@{selectedAccount.username}</p>
                    <button
                      className="identity-icon-button"
                      type="button"
                      aria-label="Copy username"
                      data-tip="Copy username"
                      onClick={() => void copyUsername()}
                    >
                      <Icon name="copy" />
                    </button>
                  </div>
                  <span className="account-id">
                    ID: {selectedAccount.userId}
                  </span>
                  <span className="status-pill">
                    <span
                      className={`presence-dot presence-${selectedAccount.presence}`}
                    />
                    {selectedAccount.presenceText}
                  </span>
                </div>
                <div className="account-menu-anchor" ref={accountMenuRef}>
                  <button
                    className="icon-button"
                    type="button"
                    aria-label="More account actions"
                    data-tip="More actions"
                    onClick={() => setShowAccountMenu((current) => !current)}
                  >
                    <Icon name="more" />
                  </button>
                  {showAccountMenu && (
                    <div className="account-menu" role="menu">
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          void browseAs();
                          setShowAccountMenu(false);
                        }}
                      >
                        <Icon name="browser" />
                        Browse as account
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          void togglePin();
                          setShowAccountMenu(false);
                        }}
                      >
                        <Icon
                          name={selectedAccount.isPinned ? "pin-off" : "pin"}
                        />
                        {selectedAccount.isPinned
                          ? "Unpin account"
                          : "Pin account"}
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          void removeSelectedAccount();
                          setShowAccountMenu(false);
                        }}
                      >
                        <Icon name="delete" />
                        Remove account
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          void revalidate();
                          setShowAccountMenu(false);
                        }}
                      >
                        <Icon name="refresh" />
                        Revalidate account
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          void arrangeAccountWindows();
                          setShowAccountMenu(false);
                        }}
                      >
                        <Icon name="grid" />
                        Arrange windows
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setKillAllConfirmation(true);
                          setShowAccountMenu(false);
                        }}
                      >
                        <Icon name="kill" />
                        Kill all Roblox
                      </button>
                    </div>
                  )}
                </div>
              </section>

              {(selectedAccount.moderationActive ||
                selectedAccount.cookieExpired) && (
                <section className="account-warning" role="alert">
                  <Icon name="warning" />
                  <div>
                    <strong>
                      {selectedAccount.moderationBanned
                        ? "Warning: Account terminated"
                        : selectedAccount.cookieExpired
                          ? "Account credential expired"
                          : "Warning: Account moderated"}
                    </strong>
                    {selectedAccount.moderationReason && (
                      <span>{selectedAccount.moderationReason}</span>
                    )}
                    {selectedAccount.moderationExpiresAt && (
                      <span>
                        Expires:{" "}
                        {formatActivity(selectedAccount.moderationExpiresAt)}
                      </span>
                    )}
                    <button
                      className="account-button"
                      type="button"
                      onClick={() => void browseAs()}
                    >
                      <Icon name="browser" />
                      Open browser as account
                    </button>
                    <button
                      className="account-button"
                      type="button"
                      disabled={mutationLoading}
                      onClick={() => void revalidate()}
                    >
                      <Icon name="refresh" />
                      Revalidate account
                    </button>
                  </div>
                </section>
              )}

              <section className="account-card launch-card">
                <div className="account-field">
                  <label htmlFor="place-id">Place ID</label>
                  <input
                    id="place-id"
                    className={!placeIdValid && placeId ? "has-error" : ""}
                    value={placeId}
                    onChange={(event) => setPlaceId(event.target.value)}
                    inputMode="numeric"
                  />
                </div>
                <div className="account-field-grid">
                  <div className="account-field">
                    <label htmlFor="job-id">Job ID (optional)</label>
                    <input
                      id="job-id"
                      value={jobId}
                      onChange={(event) => setJobId(event.target.value)}
                      placeholder="Specific server GUID"
                    />
                  </div>
                  <div className="account-field">
                    <label htmlFor="launch-data">Data (optional)</label>
                    <input
                      id="launch-data"
                      value={launchData}
                      onChange={(event) => setLaunchData(event.target.value)}
                      placeholder="Extra launch query data"
                    />
                  </div>
                </div>
                <p className="account-hint">
                  Examples: <code>?linkCode=CODE</code>{" "}
                  <code>?accessCode=CODE</code> <code>?userId=123456789</code>
                </p>
                <div className="account-action-row">
                  <button
                    className="account-button primary"
                    type="button"
                    onClick={launchAccount}
                    disabled={!selectedAccount.canLaunch}
                  >
                    <Icon name="launch" />
                    Launch
                  </button>
                  <button
                    className="account-button"
                    type="button"
                    onClick={() => void browseAs()}
                  >
                    <Icon name="browser" />
                    Open browser
                  </button>
                  <button
                    className="icon-button bordered"
                    type="button"
                    aria-label="Save preset"
                    data-tip="Save preset"
                    onClick={() => void savePreset()}
                  >
                    <Icon name="star" />
                  </button>
                </div>
                {accountNotices.map((notice) => (
                  <TimedNotice
                    key={notice.id}
                    message={notice.message}
                    onDismiss={() => dismissAccountNotice(notice.id)}
                  />
                ))}
              </section>

              <section className="account-card">
                <div className="account-card-header">
                  <h3>Roblox inventory</h3>
                  <div className="account-card-actions">
                    <button
                      className="icon-button bordered"
                      type="button"
                      aria-label="Refresh inventory"
                      data-tip="Refresh"
                      onClick={() => void loadInventory()}
                    >
                      <Icon name="refresh" />
                    </button>
                    <button
                      className="account-button"
                      type="button"
                      onClick={() => void openAccountPage(true)}
                    >
                      <Icon name="inventory" />
                      Open inventory
                    </button>
                  </div>
                </div>
                {inventoryLoading ? (
                  <div className="account-empty-inline">
                    <strong>Loading inventory...</strong>
                  </div>
                ) : inventory.length === 0 ? (
                  <div className="account-empty-inline">
                    <Icon name="inventory" />
                    <strong>No user inventory loaded yet</strong>
                    <span>
                      Refresh to fetch hats, accessories, clothing, gear, and
                      emotes.
                    </span>
                  </div>
                ) : (
                  <div className="inventory-list">
                    {inventory.map((item) => (
                      <div className="inventory-row" key={item.assetId}>
                        <span className="data-value">{item.assetId}</span>
                        <strong>{item.name}</strong>
                        <span>{item.assetType}</span>
                      </div>
                    ))}
                  </div>
                )}
              </section>

              <section className="account-card account-info-grid">
                <div>
                  <label htmlFor="account-alias">Alias</label>
                  <input
                    id="account-alias"
                    value={alias}
                    onChange={(event) => setAlias(event.target.value)}
                    onBlur={() => void saveAlias()}
                    placeholder="No alias set"
                    disabled={mutationLoading}
                  />
                </div>
                <div>
                  <span>Group</span>
                  <select
                    className="account-group-select"
                    value={selectedAccount.group}
                    onChange={(event) => void selectGroup(event.target.value)}
                  >
                    <option value="">Ungrouped</option>
                    {[
                      ...new Set(
                        accounts
                          .map((account) => account.group)
                          .filter(Boolean),
                      ),
                    ].map((group) => (
                      <option value={group} key={group}>
                        {group}
                      </option>
                    ))}
                    <option value="__new__">Create group...</option>
                    {selectedAccount.group && (
                      <option value="__delete__">Delete group...</option>
                    )}
                  </select>
                </div>
                <div>
                  <span>Last activity</span>
                  <strong className="data-value">
                    {formatActivity(selectedAccount.lastActivity)}
                  </strong>
                </div>
                <div>
                  <span>Location</span>
                  <strong>
                    {selectedAccount.presenceLocation || "Website"}
                  </strong>
                </div>
                <div>
                  <label htmlFor="player-path">Player path</label>
                  <input
                    id="player-path"
                    value={playerPath}
                    onChange={(event) => setPlayerPath(event.target.value)}
                    onBlur={() => void savePlayerPath()}
                    placeholder="Default (Auto-detect)"
                    disabled={mutationLoading}
                  />
                </div>
              </section>

              <section className="account-card connections-card">
                <h3>Connections</h3>
                <p>
                  Search a managed username or user ID, then choose an action.
                </p>
                <div className="connections-row">
                  <input
                    value={connectionQuery}
                    onChange={(event) => setConnectionQuery(event.target.value)}
                    placeholder="Username or user ID"
                  />
                  <button
                    className="account-button"
                    type="button"
                    onClick={() => void searchConnections()}
                  >
                    <Icon name="search" />
                    Search Roblox
                  </button>
                </div>
                {connectionResults.length > 0 && (
                  <div className="connection-results">
                    {connectionResults.map((result) => (
                      <div className="connection-result" key={result.userId}>
                        <div>
                          <strong>{result.displayName}</strong>
                          <span>@{result.username}</span>
                        </div>
                        <div className="connection-actions">
                          <button
                            type="button"
                            onClick={() =>
                              void applyConnectionAction(
                                result.userId,
                                "follow",
                              )
                            }
                          >
                            Follow
                          </button>
                          <button
                            type="button"
                            onClick={() =>
                              void applyConnectionAction(
                                result.userId,
                                "friend",
                              )
                            }
                          >
                            Friend
                          </button>
                          <button
                            type="button"
                            onClick={() =>
                              void applyConnectionAction(result.userId, "block")
                            }
                          >
                            Block
                          </button>
                          <button
                            type="button"
                            onClick={() =>
                              void applyConnectionAction(
                                result.userId,
                                "unfollow",
                              )
                            }
                          >
                            Unfollow
                          </button>
                          <button
                            type="button"
                            onClick={() => void joinTargetGame(result.userId)}
                          >
                            Join game
                          </button>
                          {selectedIds.size > 1 && (
                            <>
                              <button
                                type="button"
                                onClick={() =>
                                  void applyBulkConnectionAction(
                                    result.userId,
                                    "friend",
                                  )
                                }
                              >
                                Friend selected
                              </button>
                              <button
                                type="button"
                                onClick={() =>
                                  void applyBulkConnectionAction(
                                    result.userId,
                                    "follow",
                                  )
                                }
                              >
                                Follow selected
                              </button>
                              <button
                                type="button"
                                onClick={() =>
                                  void applyBulkConnectionAction(
                                    result.userId,
                                    "unfollow",
                                  )
                                }
                              >
                                Unfollow selected
                              </button>
                              <button
                                type="button"
                                onClick={() =>
                                  void applyBulkConnectionAction(
                                    result.userId,
                                    "block",
                                  )
                                }
                              >
                                Block selected
                              </button>
                              <button
                                type="button"
                                onClick={() =>
                                  void Promise.all(
                                    [...selectedIds].map((userId) =>
                                      joinUserGame(userId, result.userId),
                                    ),
                                  ).then(
                                    () =>
                                      setNotice(
                                        "Join requested for selected accounts.",
                                      ),
                                    () =>
                                      setNotice(
                                        "The target user could not be joined by every account.",
                                      ),
                                  )
                                }
                              >
                                Join selected
                              </button>
                            </>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            </>
          )}
        </section>
      </main>
      <div
        className="toast-stack"
        aria-live="polite"
        aria-label="Notifications"
      >
        {notices.map((notice) => (
          <Toast
            key={notice.id}
            item={notice}
            onDismiss={() => dismissNotice(notice.id)}
          />
        ))}
      </div>
    </>
  );
}
