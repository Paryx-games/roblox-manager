import {
  useEffect,
  useMemo,
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
  updatePlayerPath,
  createDeviceStore,
  addAccount,
  listAccounts,
  listAccountGroupColors,
  refreshAccountPresence,
  revalidateAccounts,
  killAllAccounts,
  arrangeAccountWindows,
  createAccountGroup,
  deleteAccountGroup,
  unlockDevice,
  unlockPassword,
  updateAccountAlias,
  type AccountSummary,
  type InventoryItem,
  type UserSearchResult,
  type StoreStatus,
} from "./lib/ipc";

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

function AccountAvatar({
  account,
  large = false,
}: {
  account: AccountSummary;
  large?: boolean;
}) {
  return (
    <span className={`account-avatar ${large ? "account-avatar-large" : ""}`}>
      {account.avatarUrl ? <img src={account.avatarUrl} alt="" /> : initials(account)}
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

function AccountRow({
  account,
  selected,
  onSelect,
}: {
  account: AccountSummary;
  selected: boolean;
  onSelect: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      className={`account-row ${selected ? "is-selected" : ""}`}
      type="button"
      onClick={onSelect}
    >
      <span className="account-row-bar" aria-hidden="true" />
      <AccountAvatar account={account} />
      <span className="account-row-name">{account.label}</span>
      <span className="account-row-pin" aria-label="Pinned account">
        <Icon name="pin" />
      </span>
    </button>
  );
}

function AccountGroup({
  group,
  collapsed,
  onToggle,
  selectedId,
  onSelect,
  color,
}: {
  group: AccountGroup;
  collapsed: boolean;
  selectedId: number | null;
  onToggle: () => void;
  onSelect: (id: number, event: MouseEvent<HTMLButtonElement>) => void;
  color: string;
}) {
  return (
    <section
      className={`account-group group-${group.tone}`}
      style={{ "--group-color": color } as CSSProperties}
    >
      <button
        className="account-group-header"
        type="button"
        onClick={onToggle}
        aria-expanded={!collapsed}
      >
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
            />
          ))}
        </div>
      )}
    </section>
  );
}

export function AccountsPage() {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
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
  const [notice, setNotice] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [storeStatus, setStoreStatus] = useState<StoreStatus | null>(null);
  const [password, setPassword] = useState("");
  const [alias, setAlias] = useState("");
  const [mutationLoading, setMutationLoading] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [cookie, setCookie] = useState("");
  const [inventory, setInventory] = useState<InventoryItem[]>([]);
  const [inventoryLoading, setInventoryLoading] = useState(false);
  const [connectionResults, setConnectionResults] = useState<
    UserSearchResult[]
  >([]);
  const [showAccountMenu, setShowAccountMenu] = useState(false);
  const [presenceLoading, setPresenceLoading] = useState(false);
  const [groupColors, setGroupColors] = useState<
    Record<string, [number, number, number]>
  >({});
  const [playerPath, setPlayerPath] = useState("");

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    Promise.all([getStoreStatus(), listAccountGroupColors()])
      .then(async ([status, colors]) => {
        if (!mounted) return [];
        setGroupColors(colors);
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
    for (const account of visibleAccounts) {
      const name = account.group.trim() || "Ungrouped";
      grouped.set(name, [...(grouped.get(name) ?? []), account]);
    }
    return [...grouped.entries()].map(([name, groupedAccounts], index) => ({
      name,
      tone: index % 2 === 0 ? "primary" : "secondary",
      accounts: groupedAccounts,
      color: groupColors[name]
        ? `rgb(${groupColors[name][0]}, ${groupColors[name][1]}, ${groupColors[name][2]})`
        : index % 2 === 0
          ? "var(--status-danger)"
          : "var(--status-warning)",
    }));
  }, [groupColors, visibleAccounts]);

  const selectedAccount =
    accounts.find((account) => account.userId === selectedId) ?? null;
  const placeIdValid = /^\d+$/.test(placeId.trim());

  useEffect(() => {
    setAlias(selectedAccount?.alias ?? "");
    setPlayerPath(selectedAccount?.playerPath ?? "");
    setInventory([]);
    setConnectionResults([]);
  }, [selectedAccount]);

  function selectAccount(id: number) {
    setSelectedId(id);
    setSelectedIds((current) => new Set(current).add(id));
    setNotice(null);
  }

  function selectAccountWithModifiers(
    id: number,
    event: MouseEvent<HTMLButtonElement>,
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

  async function refreshPresence() {
    const ids = selectedIds.size
      ? [...selectedIds]
      : accounts.map((account) => account.userId);
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
      setNotice("Presence refreshed.");
    } catch {
      setNotice("Presence could not be refreshed.");
    } finally {
      setPresenceLoading(false);
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
      setNotice("Enter a numeric Place ID before launching.");
      return;
    }
    if (!selectedAccount?.canLaunch) {
      setNotice("This account is not ready to launch.");
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
      setNotice("Launch requested.");
    } catch {
      setNotice("Roblox could not be launched for this account.");
    } finally {
      setMutationLoading(false);
    }
  }

  async function savePreset() {
    if (!placeIdValid) {
      setNotice("Enter a numeric Place ID before saving a preset.");
      return;
    }
    const name = window.prompt("Preset name");
    if (!name) return;
    try {
      await saveLaunchPreset(name, Number(placeId), jobId, launchData);
      setNotice("Preset saved.");
    } catch {
      setNotice("The preset could not be saved.");
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

  async function togglePin() {
    if (!selectedAccount) return;
    try {
      const updated = await toggleAccountPin(selectedAccount.userId);
      setAccounts((current) =>
        current.map((account) =>
          account.userId === updated.userId ? updated : account,
        ),
      );
    } catch {
      setNotice("The pin state could not be saved.");
    }
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
      if (!window.confirm(`Delete group ${selectedAccount.group}?`)) return;
      try {
        await deleteAccountGroup(selectedAccount.group);
        await saveGroup("");
      } catch {
        setNotice("The group could not be deleted.");
      }
      return;
    }
    await saveGroup(value);
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

  async function addManagedAccount() {
    setMutationLoading(true);
    try {
      const account = await addAccount(cookie);
      setAccounts((current) => [...current, account]);
      setSelectedId(account.userId);
      setCookie("");
      setShowAddForm(false);
      setNotice("Account added.");
    } catch (addError) {
      setNotice(
        addError instanceof Error
          ? addError.message
          : "The account could not be added.",
      );
    } finally {
      setMutationLoading(false);
    }
  }

  return (
    <main className="accounts-page" aria-busy={loading}>
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
              onClick={() => setShowAddForm((current) => !current)}
            >
              <Icon name="add" />
            </button>
          </div>
          {showAddForm && (
            <div className="add-account-form">
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
                className="account-button primary"
                type="button"
                disabled={!cookie || mutationLoading}
                onClick={() => void addManagedAccount()}
              >
                Validate and add account
              </button>
            </div>
          )}
          <div className="accounts-sort-row">
            <span>Sort:</span>
            <select
              value={sortMode}
              onChange={(event) => setSortMode(event.target.value as SortMode)}
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
            </div>
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
              color={group.color}
            />
          ))}
        </div>
      </aside>

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
                <h2>{selectedAccount.displayName || selectedAccount.label}</h2>
                <p>@{selectedAccount.username}</p>
                <span className="account-id">ID: {selectedAccount.userId}</span>
                <span className="status-pill">
                  <span
                    className={`presence-dot presence-${selectedAccount.presence}`}
                  />
                  {selectedAccount.presenceText}
                </span>
              </div>
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
                      void togglePin();
                      setShowAccountMenu(false);
                    }}
                  >
                    <Icon name={selectedAccount.isPinned ? "pin-off" : "pin"} />
                    {selectedAccount.isPinned ? "Unpin account" : "Pin account"}
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
                      void killAllAccounts();
                      setShowAccountMenu(false);
                    }}
                  >
                    <Icon name="kill" />
                    Kill all Roblox
                  </button>
                </div>
              )}
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
                  onClick={() => void openAccountPage(false)}
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
              {notice && (
                <p className="account-notice" role="status">
                  {notice}
                </p>
              )}
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
                    <Icon name="external-link" />
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
                      accounts.map((account) => account.group).filter(Boolean),
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
                <strong>{selectedAccount.presenceLocation || "Website"}</strong>
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
                            void applyConnectionAction(result.userId, "follow")
                          }
                        >
                          Follow
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void applyConnectionAction(result.userId, "friend")
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
  );
}
