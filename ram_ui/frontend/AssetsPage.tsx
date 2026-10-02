import { useWorkspaceState } from "./hooks/useWorkspaceState";
import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AccountAvatar } from "./components/AccountAvatar";
import { Icon } from "./components/Icon";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import Select from "./components/Select";
import { Toast, type ToastItem } from "./Toast";
import { ConfirmModal } from "./ConfirmModal";
import { Popup } from "./components/Popup";
import { buildInventoryAccountGroups } from "./lib/inventoryBrowsing";
import {
  addAssetFiles,
  changeAssetQueue,
  listAccounts,
  listAccountGroups,
  listAssetUniverses,
  listAssetWorkspace,
  uploadAssets,
  listAssetCreators, updateAssetRow, listAssetCreations, grantAssetAccess, revealAssetFile, operationError,
  type AssetCreator, type CreationRow,
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
  const [selectedUserId, setSelectedUserId] = useWorkspaceState<number | null>("assets.selectedUserId",
    () => initialSelectedIds.values().next().value ?? null,
  );
  const [workspace, setWorkspace] = useState<AssetWorkspace>({
    rows: [],
    isUploading: false,
    isReadOnly: true,
    notice: null,
  });
  const [isLoading, setIsLoading] = useState(true);
  const [isAccountsLoading, setIsAccountsLoading] = useState(true);
  const [accountsError, setAccountsError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshCount, setRefreshCount] = useState(0);
  const [tab, setTab] = useWorkspaceState<"queue" | "library" | "creations">("assets.tab", "queue");
  const [creatorId, setCreatorId] = useWorkspaceState("assets.creatorId", "user");
  const [creatorGroups, setCreatorGroups] = useState<AssetUniverse[]>([]);
  const [creatorError, setCreatorError] = useState<string | null>(null);
  const [selectedRows, setSelectedRows] = useState<Set<string>>(new Set());
  const [uploadConfirmation, setUploadConfirmation] = useState<string[] | null>(null);
  const [queueEdit, setQueueEdit] = useState<{ rowId: string; name: string; kind: string; creatorId: string } | null>(null);
  const [manualUniverseId, setManualUniverseId] = useState("");
  const [creationKind, setCreationKind] = useWorkspaceState("assets.creationKind", "Decal");
  const [creations, setCreations] = useState<CreationRow[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [isLoadingCreations, setIsLoadingCreations] = useState(false);
  const [creationsError, setCreationsError] = useState<string | null>(null);
  const [creationRefresh, setCreationRefresh] = useState(0);
  const creationRequest = useRef(0);
  const [search, setSearch] = useWorkspaceState("assets.search", "");
  const [typeFilter, setTypeFilter] = useWorkspaceState("assets.typeFilter", "all");
  const [density, setDensity] = useWorkspaceState("assets.density", "details");
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
  function resolveCreator(value: string): AssetCreator {
    return value === "user" ? { kind: "user", id: selectedUserId ?? 0 } : { kind: "group", id: Number(value) };
  }
  const creatorOptions = [{ value: "user", label: "Selected account" }, ...creatorGroups.map((group) => ({ value: String(group.id), label: `${group.name} (${group.id})` }))];
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
    setCreatorId("user"); setCreatorGroups([]); setCreatorError(null); setSelectedRows(new Set()); setManualUniverseId(""); setQueueEdit(null); setUploadConfirmation(null);
    if (selectedUserId) void listAssetCreators(selectedUserId).then((groups) => { if (isActive) setCreatorGroups(groups); }).catch((error) => { if (isActive) setCreatorError(operationError(error, "Group creators could not be loaded. Account uploads remain available.")); });
    return () => { isActive = false; };
  }, [selectedUserId]);

  useEffect(() => {
    const request = ++creationRequest.current;
    setCreations([]); setNextCursor(null); setCreationsError(null); setSelectedRows(new Set());
    if (tab !== "creations" || !selectedUserId) { setIsLoadingCreations(false); return; }
    setIsLoadingCreations(true);
    const creator: AssetCreator = creatorId === "user" ? { kind: "user", id: selectedUserId } : { kind: "group", id: Number(creatorId) };
    void listAssetCreations(selectedUserId, { creator, kind: creationKind }).then((page) => {
      if (creationRequest.current === request) { setCreations(page.rows); setNextCursor(page.nextCursor); }
    }).catch((error) => { if (creationRequest.current === request) setCreationsError(operationError(error, "Creations could not be loaded. Retry.")); }).finally(() => { if (creationRequest.current === request) setIsLoadingCreations(false); });
    return () => { creationRequest.current += 1; };
  }, [tab, selectedUserId, creatorId, creationKind, creationRefresh]);

  async function loadMoreCreations() {
    if (!selectedUserId || !nextCursor || isLoadingCreations) return;
    const request = creationRequest.current;
    setIsLoadingCreations(true); setCreationsError(null);
    try {
      const page = await listAssetCreations(selectedUserId, { creator: resolveCreator(creatorId), kind: creationKind, cursor: nextCursor });
      if (creationRequest.current === request) { setCreations((current) => [...current, ...page.rows.filter((row) => !current.some((existing) => existing.assetId === row.assetId))]); setNextCursor(page.nextCursor); }
    } catch (error) { if (creationRequest.current === request) setCreationsError(operationError(error, "More creations could not be loaded. Retry.")); }
    finally { if (creationRequest.current === request) setIsLoadingCreations(false); }
  }

  function toggleRow(rowId: string) {
    setSelectedRows((current) => { const next = new Set(current); if (next.has(rowId)) next.delete(rowId); else next.add(rowId); return next; });
  }

  async function runAssetAction(operation: () => Promise<void>) {
    if (mutationPending.current) return;
    mutationPending.current = true; setIsMutating(true);
    try { await operation(); }
    catch (error) { notify("Asset action could not be completed", operationError(error, "Refresh and try again.")); }
    finally { mutationPending.current = false; if (isMounted.current) setIsMutating(false); }
  }

  async function saveQueueEdit() {
    if (!queueEdit || !selectedUserId) return;
    const edit = queueEdit;
    await runAssetAction(async () => {
      const result = await updateAssetRow(selectedUserId, { rowId: edit.rowId, name: edit.name, kind: edit.kind, creator: resolveCreator(edit.creatorId) });
      if (isMounted.current) { setWorkspace(result); setQueueEdit(null); }
    });
  }

  async function copyText(text: string) {
    await runAssetAction(async () => { await navigator.clipboard.writeText(text); notify("Copied", text, "success"); });
  }

  useEffect(() => {
    let isActive = true;
    setIsAccountsLoading(true);
    setAccountsError(null);
    void Promise.allSettled([listAccounts(), listAccountGroups()]).then(
      ([accountResult, groupResult]) => {
        if (!isActive) return;
        if (accountResult.status === "fulfilled") {
          const nextAccounts = accountResult.value;
          setAccounts(nextAccounts);
          setSelectedUserId((current) =>
            nextAccounts.some((account) => account.userId === current)
              ? current
              : (nextAccounts[0]?.userId ?? null),
          );
        } else {
          setAccountsError("Accounts could not be loaded. Retry to reconnect.");
        }
        if (groupResult.status === "fulfilled") {
          setGroupOrder(groupResult.value.map((group) => group.name));
        }
        setIsGroupOrderUnavailable(groupResult.status === "rejected");
        setIsAccountsLoading(false);
      },
    );
    return () => { isActive = false; };
  }, [refreshCount]);

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
        const nextWorkspace = await listAssetWorkspace();
        if (!isActive) return;
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
          creator: resolveCreator(creatorId),
          ...(manualUniverseId || universeId ? { universeId: Number(manualUniverseId || universeId) } : {}),
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
    [canStage, selectedAccount, universeId, creatorId, manualUniverseId, selectedUserId, notify],
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
    if (!selectedUserId || workspace.isReadOnly || mutationPending.current) return;
    mutationPending.current = true;
    setIsMutating(true);
    try {
      const nextWorkspace = await changeAssetQueue(
        selectedUserId,
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

  async function startUpload(rowIds: string[]) {
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
      await uploadAssets(selectedAccount.userId, rowIds);
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
    ownedRows.some((row) => row.state === "queued" && selectedRows.has(row.rowId));
  const selectedAssetIds = tab === "creations" ? creations.filter((row) => selectedRows.has(`creation-${row.assetId}`)).map((row) => row.assetId) : rows.filter((row) => selectedRows.has(row.rowId) && row.assetId !== null).map((row) => row.assetId!);
  const visibleCreationRows = creations.filter((row) => `${row.name} ${row.assetId}`.toLowerCase().includes(search.toLowerCase()));
  const selectableRows = tab === "creations" ? visibleCreationRows.map((row) => `creation-${row.assetId}`) : rows.filter((row) => tab === "library" || row.state === "queued").map((row) => row.rowId);
  async function grantAccess() {
    if (!selectedUserId) return;
    await runAssetAction(async () => {
      const result = await grantAssetAccess(selectedUserId, Number(manualUniverseId || universeId), selectedAssetIds);
      if (result.notice) { notify("Permissions checked; local update failed", result.notice, "warning"); return; }
      notify("Experience access checked", `${result.granted.length} of ${selectedAssetIds.length} assets confirmed. ${result.failures.length ? `Roblox refused ${result.failures.length} permission requests.` : result.granted.length < selectedAssetIds.length ? "Some permissions were not confirmed. Check Creator Dashboard." : ""}`, result.granted.length === selectedAssetIds.length ? "success" : "warning");
    });
  }
  const hasFinishedRows = ownedRows.some(
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
          {isAccountsLoading ? (
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
          {accountsError && (
            <div role="alert">
              <p>{accountsError}</p>
              <button className="account-button" type="button" onClick={() => setRefreshCount((current) => current + 1)}>Retry accounts</button>
            </div>
          )}
          {!isAccountsLoading && !accountsError && accounts.length === 0 && (
            <p>Add an account on the Accounts page to begin uploading.</p>
          )}
        </aside>
        <section className="assets-main" aria-label="Asset workspace">
          <div className="assets-tabs-row">
            <div className="assets-tabs" aria-label="Asset views">
              <button type="button" className={`account-button ${tab === "creations" ? "primary" : ""}`} aria-pressed={tab === "creations"} onClick={() => { setTab("creations"); setSelectedRows(new Set()); }}>Live creations</button>
              <button
                type="button"
                className={`account-button ${tab === "library" ? "primary" : ""}`}
                aria-pressed={tab === "library"}
                onClick={() => {
                  setTab("library");
                  setSelectedRows(new Set());
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
                  setSelectedRows(new Set());
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
            {tab === "creations" ? <>
              <label className="assets-inline-field">Type<Select ariaLabel="Creation type" sizing="content" value={creationKind} onChange={setCreationKind} options={assetTypeOptions.filter((option) => option.value !== "all")} disabled={isMutating} /></label>
              <button className="account-button" type="button" disabled={isLoadingCreations || !selectedAccount} onClick={() => setCreationRefresh((current) => current + 1)}><Icon name="refresh" />Refresh creations</button>
            </> : tab === "library" ? (
              <>
                <label className="assets-inline-field">
                  View
                  <Select
                    ariaLabel="Library view"
                    sizing="content"
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
                    sizing="content"
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
                    sizing="content"
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
                  onClick={() => setUploadConfirmation(ownedRows.filter((row) => row.state === "queued" && selectedRows.has(row.rowId)).map((row) => row.rowId))}
                >
                  <Icon name="upload" />
                  {workspace.isUploading ? "Uploading..." : "Upload selected"}
                </button>
              </>
            )}
          </div>
          <div className="assets-toolbar">
            <label className="assets-inline-field">Creator<Select ariaLabel="Asset creator" sizing="content" value={creatorId} onChange={setCreatorId} options={creatorOptions} disabled={!selectedAccount || isMutating} /></label>
            <button className="account-button" type="button" disabled={!selectableRows.length || isMutating} onClick={() => setSelectedRows((current) => selectableRows.every((id) => current.has(id)) ? new Set() : new Set(selectableRows))}>Select all visible</button>
            <span>{selectedRows.size} selected</span>
            {tab !== "queue" && <><button className="account-button" type="button" disabled={!selectedAssetIds.length || isMutating} onClick={() => void copyText(selectedAssetIds.join(", "))}>Copy selected IDs</button><button className="account-button" type="button" disabled={!selectedAssetIds.length || isMutating || !Number(manualUniverseId || universeId)} onClick={() => void grantAccess()}><Icon name="verification" />Grant selected access</button></>}
          </div>
          <div className="assets-toolbar">
            {tab !== "queue" && <label className="assets-inline-field">Experience<Select ariaLabel="Experience for existing assets" sizing="content" value={universeId} onChange={setUniverseId} disabled={isLoadingUniverses || isMutating} options={[{ value: "", label: "Choose an experience" }, ...universes.map((universe) => ({ value: String(universe.id), label: universe.name }))]} /></label>}
            <label className="assets-inline-field">Manual experience ID<input className="settings-input" inputMode="numeric" aria-label="Manual experience ID" placeholder="Universe ID" value={manualUniverseId} disabled={isMutating} onChange={(event) => setManualUniverseId(event.target.value.replace(/[^0-9]/g, ""))} /></label>
          </div>
          {creatorError && <p role="alert" className="assets-notice">{creatorError}<button className="account-button" type="button" disabled={!selectedUserId || isMutating} onClick={() => { if (selectedUserId) void listAssetCreators(selectedUserId).then((groups) => { setCreatorGroups(groups); setCreatorError(null); }).catch((error) => notify("Group creators unavailable", operationError(error, "Try again later."))); }}>Retry group creators</button></p>}
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
            {tab === "creations" ? <>
              <table className="assets-table"><caption className="instances-caption">Live creations for the selected account or group</caption><thead><tr><th scope="col">Select</th><th scope="col">Asset</th><th scope="col">Type</th><th scope="col" className="assets-secondary-column">Updated</th><th scope="col">Actions</th></tr></thead><tbody>
                {creationsError && <tr><td colSpan={5}><div role="alert"><p className="selectable-text">{creationsError}</p><button className="account-button" type="button" disabled={isLoadingCreations} onClick={() => nextCursor ? void loadMoreCreations() : setCreationRefresh((current) => current + 1)}>Retry creations</button></div></td></tr>}
                {visibleCreationRows.map((row) => <tr key={row.assetId} aria-selected={selectedRows.has(`creation-${row.assetId}`)}><td><input type="checkbox" aria-label={`Select asset ${row.assetId}`} checked={selectedRows.has(`creation-${row.assetId}`)} onChange={() => toggleRow(`creation-${row.assetId}`)} disabled={isMutating} /></td><td><div className="asset-creation-name">{row.thumbnailUrl ? <img className="asset-creation-thumbnail" src={row.thumbnailUrl} alt="" /> : <Icon name={assetTypeIcons[row.kind] ?? "folder"} />}<span>{row.name}<small className="selectable-text">{row.assetId}</small></span></div></td><td>{row.kind}</td><td className="assets-secondary-column">{row.updatedAt ? new Date(row.updatedAt).toLocaleString() : "Unknown"}</td><td><div className="assets-row-actions"><button className="account-button" type="button" disabled={isMutating} onClick={() => void copyText(String(row.assetId))}>Copy ID</button><button className="account-button" type="button" disabled={isMutating} onClick={() => void copyText(`https://www.roblox.com/library/${row.assetId}`)}>Copy link</button><button className="account-button" type="button" disabled={isMutating} onClick={() => void copyText(row.name)}>Copy name</button></div></td></tr>)}
                {isLoadingCreations && <tr><td colSpan={5}><LoadingSkeleton layout="results" label="Loading live creations" /></td></tr>}
                {!isLoadingCreations && !creationsError && visibleCreationRows.length === 0 && <tr><td colSpan={5}>{selectedAccount ? "No matching creations. Choose another type or creator, or change your search." : "Select an account to browse its creations."}</td></tr>}
              </tbody></table>
              {nextCursor && <button className="account-button" type="button" disabled={isLoadingCreations} onClick={() => void loadMoreCreations()}>{isLoadingCreations ? "Loading..." : "Load more creations"}</button>}
            </> : isLoading ? (
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
                    <label><input type="checkbox" aria-label={`Select ${row.displayName}`} checked={selectedRows.has(row.rowId)} onChange={() => toggleRow(row.rowId)} disabled={isMutating} />Select</label>
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
                    {row.message && <small className="selectable-text">{row.message}</small>}
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
                    <div className="assets-row-actions"><button className="account-button" type="button" disabled={isMutating} onClick={() => void runAssetAction(() => revealAssetFile(row.rowId))}>Reveal file</button><button className="account-button" type="button" disabled={isMutating || row.assetId === null} onClick={() => void copyText(`https://www.roblox.com/library/${row.assetId}`)}>Copy link</button><button className="account-button" type="button" disabled={isMutating} onClick={() => void copyText(row.displayName)}>Copy name</button></div>
                  </article>
                ))}
              </div>
            ) : (
              <table
                className={`assets-table ${density === "compact" && tab === "library" ? "is-compact" : ""}`}
              >
                <thead>
                  <tr>
                    <th scope="col">Select</th>
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
                    <tr key={row.rowId} aria-selected={selectedRows.has(row.rowId)}>
                      <td><input type="checkbox" aria-label={`Select ${row.displayName}`} checked={selectedRows.has(row.rowId)} onChange={() => toggleRow(row.rowId)} disabled={isMutating || (tab === "queue" && row.state !== "queued")} /></td>
                      <td>
                        <strong>{row.displayName}</strong>
                        <small>{row.creator.kind === "group" ? "Group" : "Account"} {row.creator.id}</small>
                        {row.message && <small className="selectable-text">{row.message}</small>}
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
                          {(row.state === "queued" || row.state === "duplicate") && <button className="account-button" type="button" disabled={isMutating || workspace.isUploading} onClick={() => setQueueEdit({ rowId: row.rowId, name: row.displayName, kind: row.kind, creatorId: row.creator.kind === "group" ? String(row.creator.id) : "user" })}><Icon name="edit" />Edit</button>}
                          <button className="account-button" type="button" disabled={isMutating} onClick={() => void runAssetAction(() => revealAssetFile(row.rowId))}><Icon name="folder" />Reveal file</button>
                          <button className="account-button" type="button" disabled={isMutating} onClick={() => void copyText(row.displayName)}>Copy name</button>
                          {row.assetId !== null && <button className="account-button" type="button" disabled={isMutating} onClick={() => void copyText(`https://www.roblox.com/library/${row.assetId}`)}>Copy link</button>}
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
      {queueEdit && <Popup className="confirm-modal" backdropClassName="confirm-modal-backdrop" labelledBy="queue-edit-title" onClose={() => { if (!isMutating) setQueueEdit(null); }}><h2 id="queue-edit-title">Edit queued asset</h2><label className="settings-stack">Name<input className="settings-input" aria-label="Queued asset name" value={queueEdit.name} maxLength={100} disabled={isMutating} onChange={(event) => setQueueEdit({ ...queueEdit, name: event.target.value })} /></label><label className="settings-stack">Type<Select ariaLabel="Queued asset type" value={queueEdit.kind} onChange={(kind) => setQueueEdit({ ...queueEdit, kind })} options={assetTypeOptions.filter((option) => option.value !== "all")} disabled={isMutating} /></label><label className="settings-stack">Creator<Select ariaLabel="Queued asset creator" value={queueEdit.creatorId} onChange={(creatorId) => setQueueEdit({ ...queueEdit, creatorId })} options={creatorOptions} disabled={isMutating} /></label><p>Changing type does not convert the file's contents. Roblox must support the selected type and source file.</p><div className="confirm-modal-actions"><button className="account-button" type="button" disabled={isMutating} onClick={() => setQueueEdit(null)}>Cancel</button><button className="account-button" type="button" disabled={isMutating || !queueEdit.name.trim()} onClick={() => void saveQueueEdit()}>{isMutating ? "Saving..." : "Save queue changes"}</button></div></Popup>}
      {uploadConfirmation && <ConfirmModal title="Upload selected assets?" message={<><p>This creates permanent assets under the listed creators and submits them to Roblox moderation. Only these {uploadConfirmation.length} selected rows will be uploaded.</p><ul className="assets-upload-summary">{ownedRows.filter((row) => uploadConfirmation.includes(row.rowId)).map((row) => <li key={row.rowId}>{row.displayName} - {row.kind} - {row.creator.kind === "group" ? "Group" : "Account"} {row.creator.id}</li>)}</ul></>} confirmLabel="Upload selected assets" confirmIcon="upload" onConfirm={() => { const rows = uploadConfirmation; setUploadConfirmation(null); void startUpload(rows); }} onCancel={() => setUploadConfirmation(null)} />}
      {toast && <Toast item={toast} onDismiss={() => setToast(null)} />}
    </>
  );
}
