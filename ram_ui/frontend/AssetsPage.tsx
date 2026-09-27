import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AccountAvatar } from "./components/AccountAvatar";
import { Icon } from "./components/Icon";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import Select from "./components/Select";
import { Toast, type ToastItem } from "./Toast";
import { buildInventoryAccountGroups } from "./lib/inventoryBrowsing";
import {
  addAssetFiles,
  changeAssetQueue,
  listAccounts,
  listAccountGroups,
  listAssetUniverses,
  listAssetWorkspace,
  uploadAssets,
  type AccountSummary,
  type AssetRow,
  type AssetUniverse,
  type AssetWorkspace,
} from "./lib/ipc";

const stateLabels: Record<string, string> = {
  queued: "Queued",
  invalid: "Invalid",
  duplicate: "Duplicate",
  uploading: "Uploading",
  pending: "Processing",
  inReview: "In review",
  approved: "Approved",
  rejected: "Rejected",
  failed: "Failed",
  expired: "Expired",
  cancelled: "Cancelled",
};
const terminalStates = new Set([
  "invalid",
  "duplicate",
  "failed",
  "rejected",
  "expired",
  "cancelled",
]);
const assetTypeOptions = [
  { value: "all", label: "All types" },
  { value: "Decal", label: "Images" },
  { value: "Audio", label: "Audio" },
  { value: "Model", label: "Models" },
  { value: "Animation", label: "Animations" },
  { value: "Video", label: "Videos" },
];
const assetTypeIcons: Record<string, string> = {
  Decal: "grid",
  Audio: "megaphone",
  Model: "package",
  Animation: "launch",
  Video: "app-window",
};

function formatFileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function AssetsPage({
  initialSelectedIds,
}: {
  initialSelectedIds: Set<number>;
}) {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [groupOrder, setGroupOrder] = useState<string[]>([]);
  const [isGroupOrderUnavailable, setIsGroupOrderUnavailable] = useState(false);
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const groupIdPrefix = useId();
  const [selectedUserId, setSelectedUserId] = useState<number | null>(
    () => initialSelectedIds.values().next().value ?? null,
  );
  const [workspace, setWorkspace] = useState<AssetWorkspace>({
    rows: [],
    isUploading: false,
    isReadOnly: true,
    notice: null,
  });
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshCount, setRefreshCount] = useState(0);
  const [tab, setTab] = useState<"queue" | "library">("queue");
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState("all");
  const [density, setDensity] = useState("details");
  const [universes, setUniverses] = useState<AssetUniverse[]>([]);
  const [universeId, setUniverseId] = useState("");
  const [isLoadingUniverses, setIsLoadingUniverses] = useState(false);
  const [universeError, setUniverseError] = useState<string | null>(null);
  const [isMutating, setIsMutating] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [toast, setToast] = useState<ToastItem | null>(null);
  const isMounted = useRef(true);
  const hasWorkspaceEvent = useRef(false);
  const mutationPending = useRef(false);
  const selectedAccount = accounts.find(
    (account) => account.userId === selectedUserId,
  );
  const canStage =
    !!selectedAccount &&
    !selectedAccount.cookieExpired &&
    !workspace.isReadOnly &&
    !isLoading &&
    !isMutating;

  const notify = useCallback(
    (title: string, message: string, kind: ToastItem["kind"] = "error") => {
      if (isMounted.current)
        setToast({
          id: Date.now(),
          title,
          message,
          kind,
          duration: "standard",
        });
    },
    [],
  );

  useEffect(() => {
    isMounted.current = true;
    return () => {
      isMounted.current = false;
    };
  }, []);

  useEffect(() => {
    let isActive = true;
    let stopListening: (() => void) | undefined;
    hasWorkspaceEvent.current = false;
    setIsLoading(true);
    setError(null);
    async function load() {
      try {
        stopListening = await listen<AssetWorkspace>(
          "assets-updated",
          (event) => {
            if (!isActive) return;
            hasWorkspaceEvent.current = true;
            setWorkspace(event.payload);
          },
        );
        if (!isActive) {
          stopListening();
          return;
        }
        const [nextAccounts, nextWorkspace, groupResult] = await Promise.all([
          listAccounts(),
          listAssetWorkspace(),
          listAccountGroups().then(
            (groups) => ({ groups, isAvailable: true }),
            () => ({ groups: [], isAvailable: false }),
          ),
        ]);
        if (!isActive) return;
        setAccounts(nextAccounts);
        setGroupOrder(groupResult.groups.map((group) => group.name));
        setIsGroupOrderUnavailable(!groupResult.isAvailable);
        setSelectedUserId((current) =>
          nextAccounts.some((account) => account.userId === current)
            ? current
            : (nextAccounts[0]?.userId ?? null),
        );
        if (!hasWorkspaceEvent.current) setWorkspace(nextWorkspace);
      } catch {
        if (isActive)
          setError("Could not load the asset workspace. Retry to reconnect.");
      } finally {
        if (isActive) setIsLoading(false);
      }
    }
    void load();
    return () => {
      isActive = false;
      stopListening?.();
    };
  }, [refreshCount]);

  useEffect(() => {
    let isActive = true;
    setUniverses([]);
    setUniverseId("");
    setTypeFilter("all");
    setUniverseError(null);
    if (!selectedAccount || selectedAccount.cookieExpired) {
      setIsLoadingUniverses(false);
      return;
    }
    setIsLoadingUniverses(true);
    void listAssetUniverses(selectedAccount.userId)
      .then((nextUniverses) => {
        if (isActive) setUniverses(nextUniverses);
      })
      .catch(() => {
        if (isActive)
          setUniverseError(
            "Experience access is unavailable. Files can still be uploaded without a grant.",
          );
      })
      .finally(() => {
        if (isActive) setIsLoadingUniverses(false);
      });
    return () => {
      isActive = false;
    };
  }, [selectedAccount]);

  const stageFiles = useCallback(
    async (paths: string[] = []) => {
      if (!canStage || !selectedAccount || mutationPending.current) return;
      mutationPending.current = true;
      setIsMutating(true);
      try {
        const nextWorkspace = await addAssetFiles(selectedAccount.userId, {
          paths,
          ...(universeId ? { universeId: Number(universeId) } : {}),
        });
        if (isMounted.current) {
          setWorkspace(nextWorkspace);
          setTab("queue");
        }
      } catch {
        notify(
          "Files could not be added",
          "Check the file types, account session, and file access, then try again.",
        );
      } finally {
        mutationPending.current = false;
        if (isMounted.current) setIsMutating(false);
      }
    },
    [canStage, selectedAccount, universeId, notify],
  );

  useEffect(() => {
    let isActive = true;
    let stopListening: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!isActive) return;
        setIsDragging(
          event.payload.type === "enter" || event.payload.type === "over",
        );
        if (event.payload.type === "drop") {
          if (canStage) void stageFiles(event.payload.paths);
          else
            notify(
              "Files could not be added",
              "Select an account with a valid session and an editable workspace first.",
            );
        }
      })
      .then((stop) => {
        if (isActive) stopListening = stop;
        else stop();
      })
      .catch(() => {});
    return () => {
      isActive = false;
      stopListening?.();
    };
  }, [canStage, stageFiles, notify]);

  async function changeQueue(
    action: "clearFinished" | "retry" | "remove",
    row?: AssetRow,
  ) {
    if (workspace.isReadOnly || mutationPending.current) return;
    mutationPending.current = true;
    setIsMutating(true);
    try {
      const nextWorkspace = await changeAssetQueue(
        action,
        row ? { rowId: row.rowId } : {},
      );
      if (isMounted.current) setWorkspace(nextWorkspace);
    } catch {
      notify(
        "Queue could not be updated",
        "Refresh the workspace and try again.",
      );
    } finally {
      mutationPending.current = false;
      if (isMounted.current) setIsMutating(false);
    }
  }

  async function startUpload() {
    if (
      !canStage ||
      !selectedAccount ||
      workspace.isUploading ||
      mutationPending.current
    )
      return;
    mutationPending.current = true;
    setIsMutating(true);
    try {
      await uploadAssets(selectedAccount.userId);
      if (isMounted.current) setWorkspace(await listAssetWorkspace());
    } catch {
      notify(
        "Upload could not be started",
        "Check the account session and queued files, then try again.",
      );
    } finally {
      mutationPending.current = false;
      if (isMounted.current) setIsMutating(false);
    }
  }

  async function copyAssetId(assetId: number) {
    try {
      await navigator.clipboard.writeText(String(assetId));
      notify("Asset ID copied", String(assetId), "success");
    } catch {
      notify(
        "Asset ID could not be copied",
        "Allow clipboard access and try again.",
      );
    }
  }

  const accountGroups = useMemo(
    () => buildInventoryAccountGroups(accounts, groupOrder),
    [accounts, groupOrder],
  );

  function onGroupToggle(group: string) {
    setCollapsedGroups((current) => {
      const next = new Set(current);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }
  const ownedRows = workspace.rows.filter(
    (row) => row.uploadedBy === selectedUserId,
  );
  const rows = ownedRows.filter((row) => {
    const isVisibleInTab =
      tab === "library"
        ? row.assetId !== null
        : row.state !== "approved" &&
          (row.state !== "duplicate" || row.assetId === null);
    return (
      isVisibleInTab &&
      (typeFilter === "all" || row.kind === typeFilter) &&
      `${row.displayName} ${row.assetId ?? ""} ${row.kind} ${stateLabels[row.state] ?? row.state}`
        .toLowerCase()
        .includes(search.toLowerCase())
    );
  });
  const canUpload =
    canStage &&
    !workspace.isUploading &&
    ownedRows.some((row) => row.state === "queued");
  const hasFinishedRows = workspace.rows.some(
    (row) => terminalStates.has(row.state) && row.state !== "approved",
  );

  return (
    <>
      <div className="header-row assets-header">
        <h1 className="header-title">Asset Manager</h1>
        <div className="assets-upload-context">
          Uploading as{" "}
          {selectedAccount ? (
            <>
              <AccountAvatar
                account={selectedAccount}
                className="inventories-avatar"
              />
              <strong>
                {selectedAccount.alias || selectedAccount.username}
              </strong>
            </>
          ) : (
            <strong>No account selected</strong>
          )}
        </div>
      </div>
      <main className={`assets-page ${isDragging ? "is-dragging" : ""}`}>
        <aside
          className="inventories-accounts assets-accounts"
          aria-label="Upload accounts"
        >
          <h2>Accounts</h2>
          <p>Select an account to browse its assets and upload new files.</p>
          {isGroupOrderUnavailable && <p role="alert">Saved group order could not be loaded. Reopen this page to retry.</p>}
          {isLoading ? (
            <LoadingSkeleton
              layout="accounts"
              label="Loading upload accounts"
            />
          ) : (
            accountGroups.map(([name, groupAccounts], index) => {
              const isCollapsed = collapsedGroups.has(name);
              const groupId = `${groupIdPrefix}-${index}`;
              return (
                <section
                  className={`inventories-account-group ${isCollapsed ? "is-collapsed" : ""}`}
                  key={name}
                >
                  <h3>
                    <button
                      className="inventories-group-toggle"
                      type="button"
                      aria-label={name}
                      data-tip={name}
                      aria-expanded={!isCollapsed}
                      aria-controls={groupId}
                      onClick={() => onGroupToggle(name)}
                    >
                      <span className={`inventories-group-chevron ${isCollapsed ? "is-collapsed" : ""}`}>
                        <Icon name="chevron-down" />
                      </span>
                      <span>{name}</span>
                      <span className="inventories-group-count">{groupAccounts.length}</span>
                    </button>
                  </h3>
                  <div id={groupId} hidden={isCollapsed}>
                    {groupAccounts.map((account) => (
                      <button
                        className={`inventories-account ${selectedUserId === account.userId ? "is-selected" : ""}`}
                        key={account.userId}
                        type="button"
                        aria-label={`Upload as ${account.alias || account.username}`}
                        data-tip={account.alias || account.username}
                        aria-pressed={selectedUserId === account.userId}
                        onClick={() => setSelectedUserId(account.userId)}
                      >
                        <AccountAvatar account={account} className="inventories-avatar" />
                        <span className="assets-account-name">
                          {account.alias || account.username}
                        </span>
                      </button>
                    ))}
                  </div>
                </section>
              );
            })
          )}
          {!isLoading && accounts.length === 0 && (
            <p>Add an account on the Accounts page to begin uploading.</p>
          )}
        </aside>
        <section className="assets-main" aria-label="Asset workspace">
          <div className="assets-tabs-row">
            <div className="assets-tabs" aria-label="Asset views">
              <button
                type="button"
                className={`account-button ${tab === "library" ? "primary" : ""}`}
                aria-pressed={tab === "library"}
                onClick={() => {
                  setTab("library");
                  setTypeFilter("all");
                }}
              >
                Library
              </button>
              <button
                type="button"
                className={`account-button ${tab === "queue" ? "primary" : ""}`}
                aria-pressed={tab === "queue"}
                onClick={() => {
                  setTab("queue");
                  setTypeFilter("all");
                }}
              >
                Import Queue
              </button>
            </div>
            <label className="inventories-search assets-search">
              <Icon name="search" />
              <input
                placeholder="Search"
                aria-label="Search assets"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
            </label>
          </div>
          <div className="assets-toolbar">
            {tab === "library" ? (
              <>
                <label className="assets-inline-field">
                  View
                  <Select
                    ariaLabel="Library view"
                    value={density}
                    onChange={setDensity}
                    options={[
                      { value: "details", label: "Details" },
                      { value: "compact", label: "Compact" },
                      { value: "icons", label: "Large icons" },
                    ]}
                  />
                </label>
                <label className="assets-inline-field">
                  Type
                  <Select
                    ariaLabel="Asset type"
                    value={typeFilter}
                    onChange={setTypeFilter}
                    options={assetTypeOptions}
                  />
                </label>
                <span className="assets-toolbar-spacer" />
                <button
                  type="button"
                  className="account-button"
                  disabled={!canStage}
                  onClick={() => void stageFiles()}
                >
                  <Icon name="add" />
                  Add files…
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  className="account-button"
                  disabled={!canStage}
                  onClick={() => void stageFiles()}
                >
                  <Icon name="add" />
                  {isMutating ? "Working..." : "Add files…"}
                </button>
                <button
                  type="button"
                  className="account-button assets-ghost"
                  disabled={
                    workspace.isReadOnly || isMutating || !hasFinishedRows
                  }
                  onClick={() => void changeQueue("clearFinished")}
                >
                  Clear finished
                </button>
                <span className="assets-toolbar-divider" />
                <label className="assets-inline-field">
                  Grant access to
                  <Select
                    ariaLabel="Experience access for new files"
                    value={universeId}
                    onChange={setUniverseId}
                    disabled={!canStage || isLoadingUniverses}
                    options={[
                      {
                        value: "",
                        label: isLoadingUniverses ? "Loading..." : "None",
                      },
                      ...universes.map((universe) => ({
                        value: String(universe.id),
                        label: universe.name,
                      })),
                    ]}
                  />
                </label>
                <span className="assets-toolbar-spacer" />
                <button
                  type="button"
                  className="account-button primary"
                  disabled={!canUpload}
                  onClick={() => void startUpload()}
                >
                  <Icon name="upload" />
                  {workspace.isUploading ? "Uploading..." : "Upload all"}
                </button>
              </>
            )}
          </div>
          {workspace.notice && (
            <p className="assets-notice" role="status">
              {workspace.notice}
            </p>
          )}
          {selectedAccount?.cookieExpired && (
            <p className="assets-notice" role="status">
              This account's session has expired. Sign in again on the Accounts
              page before adding or uploading files.
            </p>
          )}
          {universeError && tab === "queue" && (
            <p className="assets-notice" role="status">
              {universeError}
            </p>
          )}
          <div className="assets-panel">
            {isLoading ? (
              <LoadingSkeleton
                layout={
                  tab === "library" && density === "icons"
                    ? "inventory-grid"
                    : "results"
                }
                label="Loading assets"
              />
            ) : error ? (
              <div className="assets-empty" role="alert">
                <h2>Asset workspace unavailable</h2>
                <p>{error}</p>
                <button
                  type="button"
                  className="account-button"
                  onClick={() => setRefreshCount((current) => current + 1)}
                >
                  Retry
                </button>
              </div>
            ) : rows.length === 0 ? (
              <div className="assets-empty">
                <h2>
                  {search || typeFilter !== "all"
                    ? "No matching assets"
                    : tab === "queue"
                      ? "The import queue is empty"
                      : "Your library is empty"}
                </h2>
                <p>
                  {search || typeFilter !== "all"
                    ? "Try a different search or type filter."
                    : tab === "queue"
                      ? "Use Add files, or drop files anywhere on this window."
                      : "Assets you upload for this account will appear here."}
                </p>
              </div>
            ) : tab === "library" && density === "icons" ? (
              <div className="assets-library-grid" aria-label="Asset library">
                {rows.map((row) => (
                  <article className="assets-library-item" key={row.rowId}>
                    <div
                      className="assets-file-preview"
                      aria-label={`${row.kind} file type icon`}
                    >
                      <Icon name={assetTypeIcons[row.kind] ?? "folder"} />
                      <span>{row.kind}</span>
                    </div>
                    <strong className="assets-library-name">
                      {row.displayName}
                    </strong>
                    <span className="assets-status" data-state={row.state}>
                      {stateLabels[row.state] ?? row.state}
                    </span>
                    {row.message && <small>{row.message}</small>}
                    {row.assetId !== null && (
                      <button
                        className="assets-copy"
                        type="button"
                        aria-label={`Copy asset ID ${row.assetId}`}
                        onClick={() => {
                          if (row.assetId !== null)
                            void copyAssetId(row.assetId);
                        }}
                      >
                        {row.assetId}
                        <Icon name="copy" />
                      </button>
                    )}
                  </article>
                ))}
              </div>
            ) : (
              <table
                className={`assets-table ${density === "compact" && tab === "library" ? "is-compact" : ""}`}
              >
                <thead>
                  <tr>
                    <th scope="col">Name</th>
                    <th scope="col">Type</th>
                    <th scope="col" className="assets-secondary-column">
                      Size
                    </th>
                    <th scope="col">Status</th>
                    <th scope="col">Asset ID</th>
                    <th scope="col" className="assets-secondary-column">
                      Access
                    </th>
                    <th scope="col">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.rowId}>
                      <td>
                        <strong>{row.displayName}</strong>
                        {row.message && <small>{row.message}</small>}
                      </td>
                      <td>{row.kind}</td>
                      <td className="assets-secondary-column">
                        {formatFileSize(row.fileBytes)}
                      </td>
                      <td>
                        <span className="assets-status" data-state={row.state}>
                          {stateLabels[row.state] ?? row.state}
                        </span>
                      </td>
                      <td>
                        {row.assetId !== null ? (
                          <button
                            className="assets-copy"
                            type="button"
                            aria-label={`Copy asset ID ${row.assetId}`}
                            onClick={() => {
                              if (row.assetId !== null)
                                void copyAssetId(row.assetId);
                            }}
                          >
                            {row.assetId}
                            <Icon name="copy" />
                          </button>
                        ) : (
                          "-"
                        )}
                      </td>
                      <td className="assets-secondary-column">
                        {row.grantedUniverses.length
                          ? row.grantedUniverses.join(", ")
                          : "None"}
                      </td>
                      <td>
                        <div className="assets-row-actions">
                          {row.canRetry && (
                            <button
                              className="account-button"
                              type="button"
                              disabled={
                                workspace.isReadOnly ||
                                isMutating ||
                                selectedAccount?.cookieExpired
                              }
                              onClick={() => void changeQueue("retry", row)}
                            >
                              Retry
                            </button>
                          )}
                          {row.canRemove && (
                            <button
                              className="account-button"
                              type="button"
                              disabled={workspace.isReadOnly || isMutating}
                              onClick={() => void changeQueue("remove", row)}
                            >
                              {row.state === "queued" ? "Cancel" : "Remove"}
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
          {isDragging && (
            <div className="assets-drop-overlay" role="status">
              {canStage
                ? "Drop files to add to the import queue"
                : "Select an account with a valid session first"}
            </div>
          )}
        </section>
      </main>
      {toast && <Toast item={toast} onDismiss={() => setToast(null)} />}
    </>
  );
}
