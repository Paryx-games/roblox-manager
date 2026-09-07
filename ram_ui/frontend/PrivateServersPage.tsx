import { useEffect, useMemo, useRef, useState } from "react";
import {
  addPrivateServer,
  listAccounts,
  launchPrivateServer,
  listPrivateServers,
  removePrivateServer,
  updatePrivateServer,
  type AccountSummary,
  type PrivateServerSummary,
} from "./lib/ipc";
import { ConfirmModal } from "./ConfirmModal";

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

function AccountAvatar({ account }: { account: AccountSummary }) {
  return (
    <span className="private-server-avatar">
      {account.avatarUrl ? (
        <img src={account.avatarUrl} alt="" />
      ) : (
        account.username.slice(0, 2).toUpperCase()
      )}
    </span>
  );
}

type OwnershipStatus = "owned" | "unknown" | "not-owned";

const ownershipStatus: OwnershipStatus = "unknown";

function OwnershipBadge({
  status,
  accountName,
}: {
  status: OwnershipStatus;
  accountName?: string;
}) {
  const content = {
    owned: {
      icon: "shield-check",
      label: `Owned by ${accountName ?? "one of your accounts"}`,
    },
    unknown: { icon: "shield-question-mark", label: "Ownership unverified" },
    "not-owned": { icon: "shield-x", label: "Not owned by any account" },
  }[status];

  return (
    <span
      className={`private-server-ownership private-server-ownership-${status}`}
      data-tip={content.label}
      aria-label={content.label}
    >
      <Icon name={content.icon} />
    </span>
  );
}

export function PrivateServersPage({
  selectedIds,
}: {
  selectedIds: Set<number>;
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
  const [showBanner, setShowBanner] = useState(true);
  const [openPicker, setOpenPicker] = useState<number | null>(null);
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [serverAccounts, setServerAccounts] = useState<Record<number, number>>(
    {},
  );
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
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const addSectionRef = useRef<HTMLElement>(null);

  async function reload() {
    setLoading(true);
    try {
      setServers(await listPrivateServers());
      setError(null);
    } catch {
      setError("Private servers could not be loaded.");
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    void reload();
    void listAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
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
            if (sort === "name") return left.name.localeCompare(right.name);
            if (sort === "status") return left.name.localeCompare(right.name);
            return 0;
          }),
        ] as [number, PrivateServerSummary[]],
    );
  }, [search, servers, sort]);

  const orderedGroups = descending ? [...groups].reverse() : groups;
  async function addServer() {
    setSaving(true);
    setError(null);
    try {
      const server = await addPrivateServer(name, url);
      setServers((current) => [...current, server]);
      setName("");
      setUrl("");
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The private server could not be added.",
      );
    } finally {
      setSaving(false);
    }
  }
  async function launch(index: number) {
    try {
      const selectedAccount = serverAccounts[index];
      await launchPrivateServer(
        index,
        selectedAccount === undefined ? [...selectedIds] : [selectedAccount],
      );
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The private server could not be launched.",
      );
    }
  }
  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await removePrivateServer(deleteTarget.index);
      setDeleteTarget(null);
      await reload();
    } catch {
      setError("The private server could not be removed.");
    } finally {
      setDeleting(false);
    }
  }
  async function pasteUrl() {
    try {
      const clipboardText = await navigator.clipboard.readText();
      if (clipboardText) setUrl(clipboardText);
    } catch {
      setError("Clipboard access is unavailable.");
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
      setError("Clipboard access is unavailable.");
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
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The private server could not be updated.",
      );
    } finally {
      setEditSaving(false);
    }
  }
  function addUnderGame(placeName: string) {
    setName(`${placeName} Server`);
    addSectionRef.current?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });
    setError("Paste a private server link for this game to add it.");
  }
  return (
    <>
      <div className="header-row">
        <h1 className="header-title">Private Servers</h1>
      </div>
      {showBanner && (
        <aside className="private-server-banner" role="status">
          <Icon name="warning" />
          <span>
            <strong>Coming soon:</strong> live private-server link and ownership
            validation are reserved for a later update. Other server management
            controls are available now.
          </span>
          <button
            className="private-server-banner-dismiss"
            type="button"
            aria-label="Dismiss"
            onClick={() => setShowBanner(false)}
          >
            <Icon name="close" />
          </button>
        </aside>
      )}
      <main className="private-servers-page">
        <section
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
                  <Icon name="copy" />
                </button>
              </div>
            </label>
          </div>
          <div className="private-server-form-actions">
            <button
              className="account-button primary"
              type="button"
              disabled={saving || !name.trim() || !url.trim()}
              onClick={() => void addServer()}
            >
              <Icon name="add" />
              {saving ? "Adding..." : "Add Server"}
            </button>
          </div>
        </section>
        <section
          className="private-server-card"
          aria-labelledby="saved-private-servers"
        >
          <h2 id="saved-private-servers">Saved Private Servers</h2>
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
              <select
                value={sort}
                onChange={(event) => setSort(event.target.value as typeof sort)}
              >
                <option value="custom">Custom</option>
                <option value="name">Name</option>
                <option value="recent">Recently added</option>
                <option value="status">Status</option>
              </select>
              <select
                value={descending ? "descending" : "ascending"}
                onChange={(event) =>
                  setDescending(event.target.value === "descending")
                }
              >
                <option value="ascending">Ascending</option>
                <option value="descending">Descending</option>
              </select>
            </div>
          </div>
          {loading ? (
            <p className="common-inventory-summary">
              Loading private servers...
            </p>
          ) : orderedGroups.length === 0 ? (
            <div className="account-empty-inline">
              <Icon name="game" />
              <strong>No private servers saved yet</strong>
              <span>Add one above to start launching straight into it.</span>
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
                    <span
                      className="private-server-status"
                      data-tip="Not yet checked"
                      aria-label="Not yet checked"
                    >
                      <Icon name="warning" />
                    </span>
                    <OwnershipBadge status={ownershipStatus} />
                    <strong>{server.name}</strong>
                    <div className="private-server-account-picker">
                      {(() => {
                        const account = accounts.find(
                          (candidate) =>
                            candidate.userId === serverAccounts[server.index],
                        );
                        return (
                          <>
                            <button
                              className="private-server-account-trigger"
                              type="button"
                              onClick={() =>
                                setOpenPicker(
                                  openPicker === server.index
                                    ? null
                                    : server.index,
                                )
                              }
                            >
                              {account ? (
                                <AccountAvatar account={account} />
                              ) : (
                                <span className="private-server-avatar">
                                  --
                                </span>
                              )}
                              <span>
                                {account?.username ??
                                  (selectedIds.size
                                    ? "Selected accounts"
                                    : "Select account")}
                              </span>
                              <Icon name="chevron-down" />
                            </button>
                            {openPicker === server.index && (
                              <div className="private-server-account-menu">
                                {accounts.map((candidate) => (
                                  <button
                                    key={candidate.userId}
                                    type="button"
                                    onClick={() => {
                                      setServerAccounts((current) => ({
                                        ...current,
                                        [server.index]: candidate.userId,
                                      }));
                                      setOpenPicker(null);
                                    }}
                                  >
                                    <AccountAvatar account={candidate} />
                                    {candidate.username}
                                  </button>
                                ))}
                                {!accounts.length && (
                                  <span className="private-server-account-empty">
                                    No accounts available
                                  </span>
                                )}
                                <button
                                  type="button"
                                  onClick={() => {
                                    setServerAccounts((current) => {
                                      const next = { ...current };
                                      delete next[server.index];
                                      return next;
                                    });
                                    setOpenPicker(null);
                                  }}
                                >
                                  <span className="private-server-avatar">
                                    --
                                  </span>
                                  No account
                                </button>
                              </div>
                            )}
                          </>
                        );
                      })()}
                    </div>
                    <div className="private-server-actions">
                      <button
                        className="account-button primary"
                        type="button"
                        disabled={
                          !selectedIds.size &&
                          serverAccounts[server.index] === undefined
                        }
                        onClick={() => void launch(server.index)}
                      >
                        <Icon name="launch" />
                        Launch
                      </button>
                      <button
                        className="icon-button"
                        type="button"
                        aria-label={`Copy ${server.name} link`}
                        data-tip={
                          copiedIndex === server.index ? "Copied" : "Copy link"
                        }
                        onClick={() => void copyLink(server.index, server.url)}
                      >
                        <Icon
                          name={copiedIndex === server.index ? "check" : "copy"}
                        />
                      </button>
                      <button
                        className="icon-button"
                        type="button"
                        aria-label={`Edit ${server.name}`}
                        data-tip="Edit"
                        onClick={() => openEdit(server)}
                      >
                        <Icon name="edit" />
                      </button>
                      <button
                        className="icon-button danger"
                        type="button"
                        aria-label={`Delete ${server.name}`}
                        data-tip="Delete"
                        onClick={() => setDeleteTarget(server)}
                      >
                        <Icon name="delete" />
                      </button>
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
          onConfirm={() => void confirmDelete()}
          onCancel={() => {
            if (!deleting) setDeleteTarget(null);
          }}
        />
      )}
      {editTarget && (
        <div
          className="add-account-modal-backdrop"
          role="presentation"
          onClick={() => {
            if (!editSaving) setEditTarget(null);
          }}
        >
          <section
            className="add-account-modal private-server-edit-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="edit-private-server-title"
            onClick={(event) => event.stopPropagation()}
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
          </section>
        </div>
      )}
    </>
  );
}
