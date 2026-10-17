import { useAccountUpdates } from "./hooks/useAccountUpdates";
import { listen } from "@tauri-apps/api/event";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  addPrivateServer,
  checkPrivateServer,
  cachedPrivateServerChecks,
  type PrivateServerCheckSnapshot,
  type PrivateServerCheck,
  listAccounts,
  launchPrivateServer,
  listPrivateServers,
  removePrivateServer,
  updatePrivateServer,
  type AccountSummary,
  type PrivateServerSummary,
} from "./lib/ipc";
import { ConfirmModal } from "./ConfirmModal";
import { AccountPicker } from "./components/AccountPicker";
import { Icon } from "./components/Icon";
import { Popup } from "./components/Popup";
import { PopupMenu } from "./components/PopupMenu";
import Select from "./components/Select";
import { InfoTip } from "./components/InfoTip";
import { privateServerAvailabilityLabel, privateServerOwnershipLabel, privateServerOwnerLabel } from "./lib/privateServerChecks";

type OwnershipStatus = "owned" | "unknown" | "not-owned";

function OwnershipBadge({
  status,
  accountName,
  description,
}: {
  status: OwnershipStatus;
  accountName?: string;
  description?: string;
}) {
  const content = {
    owned: {
      icon: "shield-check",
      label: `Owned by ${accountName ?? "one of your accounts"}`,
    },
    unknown: { icon: "shield-question-mark", label: "Owner unknown" },
    "not-owned": { icon: "shield-x", label: "Not owned by your accounts" },
  }[status];

  return (
    <span
      className={`private-server-ownership private-server-ownership-${status}`}
      role="img"
      data-tip={status === "owned" ? content.label : description ?? content.label}
      aria-label={status === "owned" ? content.label : description ?? content.label}
    >
      <Icon name={content.icon} tone="current-color" />
    </span>
  );
}

export function PrivateServersPage({
  selectedIds,
  onNavigateAccounts,
}: {
  selectedIds: Set<number>;
  onNavigateAccounts: () => void;
}) {
  const [servers, setServers] = useState<PrivateServerSummary[]>([]);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<"custom" | "name" | "recent" | "status">(
    "custom",
  );
  const [descending, setDescending] = useState(false);
  const [checks, setChecks] = useState<Record<number, PrivateServerCheck>>({});
  const [checking, setChecking] = useState(false);
  const [checkFailed, setCheckFailed] = useState(false);
  const checkVersion = useRef(0);
  const checkEvents = useRef(0);
  const mounted = useRef(true);
  const [showCreate, setShowCreate] = useState(false);
  const [openPicker, setOpenPicker] = useState<number | null>(null);
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [isAccountsLoading, setIsAccountsLoading] = useState(true);
  const [serverAccounts, setServerAccounts] = useState<
    Record<number, Set<number>>
  >({});
  const [deleteTarget, setDeleteTarget] = useState<PrivateServerSummary | null>(
    null,
  );
  const [deleting, setDeleting] = useState(false);
  const [editTarget, setEditTarget] = useState<PrivateServerSummary | null>(
    null,
  );
  const [editName, setEditName] = useState("");
  const [editUrl, setEditUrl] = useState("");
  const [editSaving, setEditSaving] = useState(false);
  const [launchingIndex, setLaunchingIndex] = useState<number | null>(null);
  const launchingIndexRef = useRef<number | null>(null);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [openMenu, setOpenMenu] = useState<number | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuPlacement, setMenuPlacement] = useState("");
  const addSectionRef = useRef<HTMLElement>(null);

  useAccountUpdates(setAccounts, () => setError("Live account updates disconnected. Restart RM to reconnect."));

  useEffect(() => {
    if (openMenu === null) return;

    function dismissMenu(event: PointerEvent) {
      if (!menuRef.current?.contains(event.target as Node)) {
        setOpenMenu(null);
      }
    }
    function dismissMenuOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setOpenMenu(null);
    }

    document.addEventListener("pointerdown", dismissMenu);
    document.addEventListener("keydown", dismissMenuOnEscape);
    return () => {
      document.removeEventListener("pointerdown", dismissMenu);
      document.removeEventListener("keydown", dismissMenuOnEscape);
    };
  }, [openMenu]);

  useLayoutEffect(() => {
    if (openMenu === null) return;
    const anchor = menuRef.current;
    const menu = anchor?.querySelector<HTMLElement>(".private-server-menu");
    if (!anchor || !menu) return;
    const bounds = anchor.getBoundingClientRect();
    const menuBounds = menu.getBoundingClientRect();
    const isOpenRight = bounds.right - menuBounds.width < 8;
    const isOpenUp = bounds.bottom + menuBounds.height > window.innerHeight - 8;
    setMenuPlacement(
      `${isOpenRight ? "is-open-right" : ""} ${isOpenUp ? "is-open-up" : ""}`.trim(),
    );
  }, [openMenu]);

  async function reload() {
    const version = ++checkVersion.current;
    const eventsBeforeLoad = checkEvents.current;
    setLoading(true);
    setChecking(false);
    try {
      const loaded = await listPrivateServers();
      if (!mounted.current || checkVersion.current !== version) return;
      setServers(loaded);
      const cached = await cachedPrivateServerChecks();
      if (!mounted.current || checkVersion.current !== version) return;
      if (checkEvents.current === eventsBeforeLoad) {
        setChecks(cached.checks);
        setChecking(cached.checking); setCheckFailed(cached.failed);
      }
      setError(null);
    } catch {
      if (mounted.current && checkVersion.current === version) setError("Saved private-server bookmarks could not be loaded. Use Refresh status to retry; saved bookmarks were not changed.");
    } finally {
      if (mounted.current && checkVersion.current === version) setLoading(false);
    }
  }
  async function validateServers(targets: PrivateServerSummary[]) {
    setChecking(true);
    setError(null);
    try {
      for (const target of targets) await checkPrivateServer(target.index);
      const cached = await cachedPrivateServerChecks();
      if (mounted.current) { setChecks(cached.checks); setChecking(cached.checking); setCheckFailed(cached.failed); }
    } catch {
      if (mounted.current) { setError("Status refresh could not start. Try again."); setChecking(false); }
    }
  }
  useEffect(() => {
    mounted.current = true;
    let active = true;
    const subscription = listen<PrivateServerCheckSnapshot>("private-server-checks-updated", event => {
      if (active) { checkEvents.current += 1; setChecks(event.payload.checks); setChecking(event.payload.checking); setCheckFailed(event.payload.failed); }
    });
    async function load() {
      let disconnected = false;
      try { await subscription; } catch { disconnected = true; }
      if (!active) return;
      await reload();
      if (active && disconnected) setError(current => [current, "Live server updates disconnected. Restart RM to reconnect."].filter(Boolean).join(" "));
    }
    void load();
    void listAccounts()
      .then(accounts => { if (active) setAccounts(accounts); })
      .catch(() => { if (active) { setAccounts([]); setError("Accounts could not be loaded. Open Accounts, refresh them, then reopen Private Servers."); } })
      .finally(() => { if (active) setIsAccountsLoading(false); });
    return () => { active = false; mounted.current = false; checkVersion.current += 1; void subscription.then(unlisten => unlisten()).catch(() => {}); };
  }, []);
  const groups = useMemo(() => {
    const map = new Map<number, PrivateServerSummary[]>();
    const visible = servers.filter((server) =>
      `${server.name} ${server.placeName}`
        .toLowerCase()
        .includes(search.trim().toLowerCase()),
    );
    visible.forEach((server) =>
      map.set(server.placeId, [...(map.get(server.placeId) ?? []), server]),
    );
    return [...map.entries()].map(
      ([placeId, group]) =>
        [
          placeId,
          [...group].sort((left, right) => {
            let result = left.index - right.index;
            if (sort === "name") result = left.name.localeCompare(right.name);
            if (sort === "recent") result = right.index - left.index;
            if (sort === "status") {
              const rank = { valid: 0, unknown: 1, invalid: 2 };
              result = rank[checks[left.index]?.status ?? "unknown"] - rank[checks[right.index]?.status ?? "unknown"] || left.name.localeCompare(right.name);
            }
            return result * (descending ? -1 : 1);
          }),
        ] as [number, PrivateServerSummary[]],
    );
  }, [search, servers, sort, checks, descending]);

  const orderedGroups = descending ? [...groups].reverse() : groups;
  async function addServer() {
    setSaving(true);
    setError(null);
    try {
      const server = await addPrivateServer(name, url);
      setServers((current) => [...current, server]);
      setName("");
      setUrl("");
      setShowCreate(false);
      void validateServers([server]);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "Couldn’t save this server. Check its name and Roblox link, then try again.",
      );
    } finally {
      setSaving(false);
    }
  }
  async function launch(index: number) {
    if (launchingIndexRef.current !== null) return;
    launchingIndexRef.current = index;
    setLaunchingIndex(index);
    try {
      const selectedAccountIds = serverAccounts[index] ?? selectedIds;
      await launchPrivateServer(index, [...selectedAccountIds]);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "Some accounts couldn’t launch. Refresh them and check server access. Check Instances before retrying; some may already be running.",
      );
    } finally {
      launchingIndexRef.current = null;
      setLaunchingIndex(null);
    }
  }
  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await removePrivateServer(deleteTarget.index);
      setServerAccounts({});
      setDeleteTarget(null);
      await reload();
    } catch {
      setError("Couldn’t remove this bookmark. Refresh the list and try again. Removing a bookmark won’t delete the Roblox server.");
    } finally {
      setDeleting(false);
    }
  }
  async function pasteUrl() {
    try {
      const clipboardText = await navigator.clipboard.readText();
      if (clipboardText) setUrl(clipboardText);
    } catch {
      setError("Couldn’t paste the server link. Check clipboard access and try again.");
    }
  }
  async function copyLink(index: number, url: string) {
    try {
      await navigator.clipboard.writeText(url);
      setError(null);
      setCopiedIndex(index);
      window.setTimeout(() => {
        setCopiedIndex((current) => (current === index ? null : current));
      }, 3000);
    } catch {
      setError("Couldn’t copy the server link. Check clipboard access and try again.");
    }
  }
  function openEdit(server: PrivateServerSummary) {
    setEditTarget(server);
    setEditName(server.name);
    setEditUrl(server.url);
  }
  function isValidPrivateServerUrl(value: string) {
    try {
      const parsed = new URL(value.trim());
      const isRobloxHost =
        parsed.protocol === "https:" &&
        (parsed.hostname === "roblox.com" ||
          parsed.hostname.endsWith(".roblox.com"));
      const hasServerPath =
        parsed.pathname.includes("/games/") ||
        parsed.pathname.includes("/share");
      const hasServerCode =
        parsed.searchParams.has("privateServerLinkCode") ||
        parsed.searchParams.has("code");
      return isRobloxHost && hasServerPath && hasServerCode;
    } catch {
      return false;
    }
  }
  async function saveEdit() {
    if (!editTarget || !editName.trim() || !isValidPrivateServerUrl(editUrl)) {
      return;
    }
    setEditSaving(true);
    try {
      const updated = await updatePrivateServer(
        editTarget.index,
        editName.trim(),
        editUrl.trim(),
      );
      setServers((current) =>
        current.map((server) =>
          server.index === updated.index ? updated : server,
        ),
      );
      setEditTarget(null);
      void validateServers([updated]);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The private-server edits could not be saved. Check the bookmark name and server URL; your draft remains in the editor.",
      );
    } finally {
      setEditSaving(false);
    }
  }
  function addUnderGame(placeName: string) {
    setShowCreate(true);
    setName(`${placeName} Server`);
    requestAnimationFrame(() => addSectionRef.current?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    }));
  }
  return (
    <>
      <div className="header-row saved-workspace-header">
        <h1 className="header-title">Private Servers</h1>
        <button className="account-button" type="button" aria-expanded={showCreate} disabled={saving} onClick={() => setShowCreate(current => !current)}><Icon name="add" />{showCreate ? "Close form" : "Add server"}</button>
      </div>
      <main className="private-servers-page">
        {showCreate && <section
          ref={addSectionRef}
          className="private-server-card"
          aria-labelledby="add-private-server"
        >
          <h2 id="add-private-server">Add Private Server</h2>
          <div className="private-server-fields">
            <label>
              Name
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. Grinding Server"
                maxLength={64}
                disabled={saving}
              />
            </label>
            <label>
              URL
              <div className="private-server-input-action">
                <input
                  value={url}
                  onChange={(event) => setUrl(event.target.value)}
                  placeholder="Paste private server link"
                />
                <button
                  type="button"
                  aria-label="Paste from clipboard"
                  data-tip="Paste from clipboard"
                  onClick={() => void pasteUrl()}
                >
                  <Icon name="clipboard" />
                </button>
              </div>
            </label>
          </div>
          <div className="private-server-form-actions">
            <button
              className="account-button primary"
              type="button"
              disabled={saving || !name.trim() || !isValidPrivateServerUrl(url)}
              onClick={() => void addServer()}
            >
              <Icon name="add" />
              {saving ? "Adding..." : "Add Server"}
            </button>
          </div>
        </section>}
        <section
          className="private-server-card"
          aria-labelledby="saved-private-servers"
        >
          <div className="preset-card-head">
            <h2 id="saved-private-servers">Saved private servers ({servers.length}) <InfoTip text="Left shield: green if any saved account owns the server, red if none do after complete checks, and yellow while checking or if ownership is unknown. Right icon: green if the invitation resolves, red if it is inactive or invalid, and yellow if availability is unknown. Roblox checks each selected account's join access when launching." /></h2>
            <button className="account-button" type="button" disabled={checking || loading || saving || deleting || editSaving} onClick={() => { if (!servers.length) void reload(); else void validateServers(servers); }}><Icon name="refresh" />Refresh status</button>
          </div>
          {checking && <p className="common-inventory-summary" role="status">Refreshing server status...</p>}
          {checkFailed && <p className="private-server-error" role="alert">Some checks failed. Last known results are shown. Refresh to retry.</p>}
          {error && (
            <p className="private-server-error" role="alert">
              {error}
            </p>
          )}
          <div className="private-server-toolbar">
            <label className="private-server-search">
              <Icon name="search" />
              <input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search saved servers"
              />
            </label>
            <div className="private-server-sort">
              <span>Sort:</span>
              <Select
                value={sort}
                onChange={(value) => setSort(value as typeof sort)}
                ariaLabel="Sort private servers"
                options={[
                  { value: "custom", label: "Custom" },
                  { value: "name", label: "Name" },
                  { value: "recent", label: "Recently added" },
                  { value: "status", label: "Status" },
                ]}
              />
              <Select
                value={descending ? "descending" : "ascending"}
                onChange={(value) => setDescending(value === "descending")}
                ariaLabel="Sort direction"
                options={[
                  { value: "ascending", label: "Ascending" },
                  { value: "descending", label: "Descending" },
                ]}
              />
            </div>
          </div>
          {loading ? (
            <LoadingSkeleton layout="servers" label="Loading private servers" />
          ) : orderedGroups.length === 0 ? (
            <div className="account-empty-inline">
              <Icon name="game" />
              <strong>{search.trim() ? "No search results found" : "No private servers saved yet"}</strong>
              <span>{search.trim() ? "Try another search term." : "Choose Add server to save a private-server link."}</span>
            </div>
          ) : (
            orderedGroups.map(([placeId, group]) => (
              <div className="private-server-group" key={placeId}>
                <div className="private-server-group-head">
                  <span className="private-server-game-thumb">
                    {group[0].iconUrl ? (
                      <img src={group[0].iconUrl} alt="" />
                    ) : (
                      <Icon name="game" />
                    )}
                  </span>
                  <div>
                    <strong>{group[0].placeName || `Place ${placeId}`}</strong>
                    <span>
                      {group.length} server{group.length === 1 ? "" : "s"}
                    </span>
                  </div>
                  <button
                    className="icon-button"
                    type="button"
                    aria-label="Add server under this game"
                    data-tip="Add server under this game"
                    onClick={() =>
                      addUnderGame(group[0].placeName || `Place ${placeId}`)
                    }
                  >
                    <Icon name="add" />
                  </button>
                </div>
                {group.map((server) => (
                  <div className="private-server-row" key={server.index}>
                    <OwnershipBadge status={checks[server.index]?.ownership ?? "unknown"} accountName={checks[server.index]?.ownerUsername ?? undefined} description={privateServerOwnershipLabel(checks[server.index], checking)} />
                    <span
                      className={`private-server-status private-server-status-${checks[server.index]?.status ?? "unknown"}`}
                      role="img"
                      data-tip={privateServerAvailabilityLabel(checks[server.index], checking)}
                      aria-label={privateServerAvailabilityLabel(checks[server.index], checking)}
                    >
                      <Icon name={checks[server.index]?.status === "valid" ? "check" : checks[server.index]?.status === "invalid" ? "close" : "warning"} tone="current-color" />
                    </span>
                    <span className="private-server-name-block">
                      <span className="private-server-name">{server.name}</span>
                      {checks[server.index]?.ownerId ? <a
                        className="private-server-owner-link"
                        href={`https://www.roblox.com/users/${checks[server.index].ownerId}/profile`}
                        target="_blank"
                        rel="noreferrer"
                        aria-label="Open server owner's Roblox profile"
                        data-tip="Open owner profile"
                      >
                        @{checks[server.index].ownerUsername ?? checks[server.index].ownerId}
                        <Icon name="square-arrow-out-up-right" />
                      </a> : <span className="private-server-owner-link" data-tip={checks[server.index]?.message}>{privateServerOwnerLabel(checks[server.index], checking)}</span>}
                    </span>
                    <AccountPicker
                      accounts={accounts}
                      isLoading={isAccountsLoading}
                      mode="multiple"
                      showAllAccounts
                      open={openPicker === server.index}
                      onOpenChange={(open) =>
                        setOpenPicker(open ? server.index : null)
                      }
                      selectedIds={serverAccounts[server.index] ?? selectedIds}
                      onSelectedIdsChange={(ids) =>
                        setServerAccounts((current) => {
                          return { ...current, [server.index]: ids };
                        })
                      }
                      showManageAccounts={true}
                      onManageAccounts={onNavigateAccounts}
                      className="private-server-account-picker"
                    />
                    <div className="private-server-actions">
                      <button
                        className={`account-button ${
                          (serverAccounts[server.index] ?? selectedIds).size
                            ? "primary"
                            : ""
                        }`}
                        type="button"
                        disabled={
                          launchingIndex !== null || loading || deleting || editSaving ||
                          !(serverAccounts[server.index] ?? selectedIds).size
                        }
                        onClick={() => void launch(server.index)}
                      >
                        <span className="account-icon private-server-launch-icon" />
                        {launchingIndex === server.index ? "Launching..." : "Launch"}
                      </button>
                      <div
                        className="private-server-menu-anchor"
                        ref={openMenu === server.index ? menuRef : undefined}
                      >
                        <button
                          className="icon-button"
                          type="button"
                          aria-label={`More actions for ${server.name}`}
                          aria-expanded={openMenu === server.index}
                          aria-haspopup="menu"
                          data-tip="More actions"
                          onClick={() =>
                            setOpenMenu((current) =>
                              current === server.index ? null : server.index,
                            )
                          }
                        >
                          <Icon name="more" />
                        </button>
                        {openMenu === server.index && (
                          <PopupMenu
                            className={`private-server-menu ${menuPlacement}`}
                            onClose={() => setOpenMenu(null)}
                          >
                            <button
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                void copyLink(server.index, server.url);
                                setOpenMenu(null);
                              }}
                            >
                              <Icon
                                name={
                                  copiedIndex === server.index
                                    ? "check"
                                    : "copy"
                                }
                              />
                              {copiedIndex === server.index
                                ? "Copied"
                                : "Copy link"}
                            </button>
                            <button
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                openEdit(server);
                                setOpenMenu(null);
                              }}
                            >
                              <Icon name="edit" />
                              Edit
                            </button>
                            <button type="button" role="menuitem" disabled={checking} onClick={() => { void validateServers([server]); setOpenMenu(null); }}><Icon name="refresh" />Check link and ownership</button>
                            <div
                              className="private-server-menu-separator"
                              role="separator"
                            />
                            <button
                              className="private-server-menu-danger"
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                setDeleteTarget(server);
                                setOpenMenu(null);
                              }}
                            >
                              <Icon name="delete-danger" />
                              Delete
                            </button>
                          </PopupMenu>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            ))
          )}
        </section>
      </main>
      {deleteTarget && (
        <ConfirmModal
          title="Delete private server?"
          message={
            <>
              This will permanently remove <strong>{deleteTarget.name}</strong>.
            </>
          }
          confirmLabel={deleting ? "Deleting..." : "Delete"}
          confirmDisabled={deleting}
          busy={deleting}
          onConfirm={() => void confirmDelete()}
          onCancel={() => {
            if (!deleting) setDeleteTarget(null);
          }}
        />
      )}
      {editTarget && (
        <Popup
          className="add-account-modal private-server-edit-modal"
          backdropClassName="add-account-modal-backdrop"
          onClose={() => {
            if (!editSaving) setEditTarget(null);
          }}
          closeOnBackdrop
          labelledBy="edit-private-server-title"
          busy={editSaving}
        >
            <div className="add-account-modal-header">
              <div>
                <h2 id="edit-private-server-title">Edit private server</h2>
                <p>Update the saved name or private server link.</p>
              </div>
              <button
                className="icon-button"
                type="button"
                aria-label="Close edit dialog"
                data-tip="Close"
                disabled={editSaving}
                onClick={() => setEditTarget(null)}
              >
                <Icon name="close" />
              </button>
            </div>
            <div className="add-account-modal-body">
              {error && <p className="private-server-error" role="alert">{error}</p>}
              <label htmlFor="private-server-edit-name">Name</label>
              <input
                id="private-server-edit-name"
                value={editName}
                onChange={(event) => setEditName(event.target.value)}
                maxLength={64}
                autoComplete="off"
              />
              <label htmlFor="private-server-edit-url">
                Private server link
              </label>
              <input
                id="private-server-edit-url"
                value={editUrl}
                onChange={(event) => setEditUrl(event.target.value)}
                autoComplete="off"
                aria-invalid={
                  Boolean(editUrl) && !isValidPrivateServerUrl(editUrl)
                }
              />
              {editUrl && !isValidPrivateServerUrl(editUrl) && (
                <p className="private-server-error">
                  Enter a valid Roblox private server link.
                </p>
              )}
              <button
                className="account-button primary"
                type="button"
                disabled={
                  editSaving ||
                  !editName.trim() ||
                  !isValidPrivateServerUrl(editUrl)
                }
                onClick={() => void saveEdit()}
              >
                <Icon name="save" />
                {editSaving ? "Saving..." : "Save changes"}
              </button>
            </div>
        </Popup>
      )}
    </>
  );
}
