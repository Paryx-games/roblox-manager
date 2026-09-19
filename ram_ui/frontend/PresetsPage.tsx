import { useEffect, useMemo, useState } from "react";
import {
  launchLaunchPreset,
  listAccounts,
  listLaunchPresets,
  openDataFolder,
  removeLaunchPreset,
  saveLaunchPreset,
  updateLaunchPreset,
  type AccountSummary,
  type LaunchPresetSummary,
} from "./lib/ipc";
import { ConfirmModal } from "./ConfirmModal";
import { AccountPicker } from "./components/AccountPicker";
import { Icon } from "./components/Icon";
import { Popup } from "./components/Popup";
import { PopupMenu } from "./components/PopupMenu";

type PresetSort = "custom" | "name" | "recent";

function PresetDetails({
  preset,
  copiedValue,
  onCopy,
}: {
  preset: LaunchPresetSummary;
  copiedValue: string | null;
  onCopy: (value: string) => void;
}) {
  function CopyChip({ value, label }: { value: string; label: string }) {
    return (
      <button
        className="preset-copy-chip"
        type="button"
        aria-label={`Copy ${label}`}
        data-tip={copiedValue === value ? "Copied" : "Copy"}
        onClick={() => onCopy(value)}
      >
        {value}
      </button>
    );
  }

  return (
    <div className="preset-details">
      <span>Place ID</span>
      <CopyChip value={String(preset.placeId)} label="place ID" />
      {preset.jobId && (
        <>
          <span className="preset-detail-separator">·</span>
          <span>Job ID</span>
          <CopyChip value={preset.jobId} label="job ID" />
        </>
      )}
      {preset.data && (
        <>
          <span className="preset-detail-separator">·</span>
          <CopyChip value={preset.data} label="launch data" />
        </>
      )}
    </div>
  );
}

export function PresetsPage({
  selectedIds,
  onNavigateAccounts,
}: {
  selectedIds: Set<number>;
  onNavigateAccounts: () => void;
}) {
  const [presets, setPresets] = useState<LaunchPresetSummary[]>([]);
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [presetAccounts, setPresetAccounts] = useState<
    Record<number, Set<number>>
  >({});
  const [name, setName] = useState("");
  const [placeId, setPlaceId] = useState("");
  const [jobId, setJobId] = useState("");
  const [data, setData] = useState("");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<PresetSort>("custom");
  const [descending, setDescending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [launchingIndex, setLaunchingIndex] = useState<number | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<LaunchPresetSummary | null>(
    null,
  );
  const [editTarget, setEditTarget] = useState<LaunchPresetSummary | null>(
    null,
  );
  const [editName, setEditName] = useState("");
  const [editPlaceId, setEditPlaceId] = useState("");
  const [editJobId, setEditJobId] = useState("");
  const [editData, setEditData] = useState("");
  const [editSaving, setEditSaving] = useState(false);
  const [openMenu, setOpenMenu] = useState<number | null>(null);
  const [openPicker, setOpenPicker] = useState<number | null>(null);
  const [copiedValue, setCopiedValue] = useState<string | null>(null);

  async function reload() {
    setLoading(true);
    try {
      setPresets(await listLaunchPresets());
      setError(null);
    } catch {
      setError("Presets could not be loaded.");
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

  useEffect(() => {
    if (openMenu === null) return;

    function dismissMenu(event: PointerEvent) {
      if (
        !(event.target instanceof Element) ||
        !event.target.closest(".preset-menu-anchor")
      ) {
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

  useEffect(() => {
    if (editTarget === null) return;

    function dismissEditorOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setEditTarget(null);
    }

    document.addEventListener("keydown", dismissEditorOnEscape);
    return () => document.removeEventListener("keydown", dismissEditorOnEscape);
  }, [editTarget]);

  const visiblePresets = useMemo(() => {
    const query = search.trim().toLowerCase();
    const filtered = presets.filter((preset) =>
      `${preset.name} ${preset.placeId} ${preset.jobId ?? ""} ${preset.data ?? ""}`
        .toLowerCase()
        .includes(query),
    );
    return [...filtered].sort((left, right) => {
      if (sort === "name") return left.name.localeCompare(right.name);
      return left.index - right.index;
    });
  }, [presets, search, sort]);

  const orderedPresets = descending
    ? [...visiblePresets].reverse()
    : visiblePresets;

  function getPlaceId(value: string) {
    const parsed = Number.parseInt(value.trim(), 10);
    return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
  }

  async function addPreset() {
    const parsedPlaceId = getPlaceId(placeId);
    if (!name.trim() || parsedPlaceId === null) return;
    setSaving(true);
    setError(null);
    try {
      await saveLaunchPreset(name.trim(), parsedPlaceId, jobId, data);
      setName("");
      setPlaceId("");
      setJobId("");
      setData("");
      await reload();
    } catch (reason) {
      setError(
        reason instanceof Error ? reason.message : "The preset could not be saved.",
      );
    } finally {
      setSaving(false);
    }
  }

  async function launchPreset(preset: LaunchPresetSummary) {
    const selectedAccountIds = presetAccounts[preset.index] ?? selectedIds;
    if (!selectedAccountIds.size) return;
    setLaunchingIndex(preset.index);
    setError(null);
    try {
      await launchLaunchPreset(preset.index, [...selectedAccountIds]);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The preset could not be launched.",
      );
    } finally {
      setLaunchingIndex(null);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await removeLaunchPreset(deleteTarget.index);
      setDeleteTarget(null);
      await reload();
    } catch {
      setError("The preset could not be removed.");
    } finally {
      setDeleting(false);
    }
  }

  function openEdit(preset: LaunchPresetSummary) {
    setEditTarget(preset);
    setEditName(preset.name);
    setEditPlaceId(String(preset.placeId));
    setEditJobId(preset.jobId ?? "");
    setEditData(preset.data ?? "");
  }

  async function saveEdit() {
    if (!editTarget || !editName.trim()) return;
    const parsedPlaceId = getPlaceId(editPlaceId);
    if (parsedPlaceId === null) return;
    setEditSaving(true);
    try {
      const updated = await updateLaunchPreset(
        editTarget.index,
        editName.trim(),
        parsedPlaceId,
        editJobId,
        editData,
      );
      setPresets((current) =>
        current.map((preset) =>
          preset.index === updated.index ? updated : preset,
        ),
      );
      setEditTarget(null);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The preset could not be updated.",
      );
    } finally {
      setEditSaving(false);
    }
  }

  async function copyValue(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setCopiedValue(value);
      window.setTimeout(() => {
        setCopiedValue((current) => (current === value ? null : current));
      }, 1400);
    } catch {
      setError("Clipboard access is unavailable.");
    }
  }

  return (
    <>
      <div className="header-row">
        <h1 className="header-title">Presets</h1>
      </div>
      <main className="presets-page">
        <section className="preset-card" aria-labelledby="new-preset">
          <div className="preset-card-head">
            <h2 id="new-preset">New preset</h2>
            <button
              className="account-button preset-folder-button"
              type="button"
              aria-label="Open presets folder"
              data-tip="Open presets folder"
              onClick={() => void openDataFolder()}
            >
              <Icon name="folder" />
              <span className="preset-folder-button-label">Open folder</span>
            </button>
          </div>
          <div className="preset-form">
            <label>
              <span>Name</span>
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="e.g. Adopt Me"
                maxLength={80}
              />
            </label>
            <label>
              <span>Place ID</span>
              <input
                value={placeId}
                onChange={(event) => setPlaceId(event.target.value)}
                placeholder="e.g. 920587237"
                inputMode="numeric"
              />
            </label>
            <label>
              <span>Job ID (optional)</span>
              <input
                value={jobId}
                onChange={(event) => setJobId(event.target.value)}
                placeholder="Specific server GUID"
              />
            </label>
            <label>
              <span>Data (optional)</span>
              <input
                value={data}
                onChange={(event) => setData(event.target.value)}
                placeholder="e.g. ?vip=true"
              />
            </label>
          </div>
          <div className="preset-form-actions">
            <button
              className="account-button primary"
              type="button"
              disabled={saving || !name.trim() || getPlaceId(placeId) === null}
              onClick={() => void addPreset()}
            >
              <Icon name="add" />
              {saving ? "Adding..." : "Add preset"}
            </button>
          </div>
        </section>

        <section className="preset-card" aria-labelledby="saved-presets">
          <h2 id="saved-presets">Saved presets</h2>
          {error && (
            <p className="preset-error" role="alert">
              {error}
            </p>
          )}
          <div className="preset-toolbar">
            <label className="preset-search">
              <Icon name="search" />
              <input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search saved presets"
              />
            </label>
            <div className="preset-sort">
              <span>Sort:</span>
              <select
                value={sort}
                onChange={(event) => setSort(event.target.value as PresetSort)}
              >
                <option value="custom">Custom</option>
                <option value="name">Name</option>
                <option value="recent">Recently added</option>
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
            <p className="common-inventory-summary">Loading presets...</p>
          ) : orderedPresets.length === 0 ? (
            <div className="account-empty-inline preset-empty">
              <Icon name="game" />
              <strong>No presets yet</strong>
              <span>Create one above to launch favorite games faster.</span>
            </div>
          ) : (
            <div className="preset-rows">
              {orderedPresets.map((preset) => {
                const selectedAccountIds =
                  presetAccounts[preset.index] ?? selectedIds;
                return (
                  <div className="preset-row" key={preset.index}>
                    <span className="preset-thumb">
                      <Icon name="game" />
                    </span>
                    <div className="preset-info">
                      <strong>{preset.name}</strong>
                      <PresetDetails
                        preset={preset}
                        copiedValue={copiedValue}
                        onCopy={(value) => void copyValue(value)}
                      />
                    </div>
                    <AccountPicker
                      accounts={accounts}
                      mode="multiple"
                      open={openPicker === preset.index}
                      onOpenChange={(open) =>
                        setOpenPicker(open ? preset.index : null)
                      }
                      selectedIds={selectedAccountIds}
                      onSelectedIdsChange={(ids) =>
                        setPresetAccounts((current) => ({
                          ...current,
                          [preset.index]: ids,
                        }))
                      }
                      showManageAccounts
                      onManageAccounts={onNavigateAccounts}
                      className="preset-account-picker"
                    />
                    <div className="preset-actions">
                      <button
                        className="account-button primary"
                        type="button"
                        disabled={
                          !selectedAccountIds.size || launchingIndex === preset.index
                        }
                        onClick={() => void launchPreset(preset)}
                      >
                        <Icon name="launch" />
                        {launchingIndex === preset.index ? "Launching..." : "Launch"}
                      </button>
                      <div className="preset-menu-anchor">
                        <button
                          className="icon-button"
                          type="button"
                          aria-label={`More actions for ${preset.name}`}
                          aria-expanded={openMenu === preset.index}
                          aria-haspopup="menu"
                          data-tip="More actions"
                          onClick={() =>
                            setOpenMenu((current) =>
                              current === preset.index ? null : preset.index,
                            )
                          }
                        >
                          <Icon name="more" />
                        </button>
                        {openMenu === preset.index && (
                          <PopupMenu className="preset-menu">
                            <button
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                void copyValue(String(preset.placeId));
                                setOpenMenu(null);
                              }}
                            >
                              <Icon
                                name={
                                  copiedValue === String(preset.placeId)
                                    ? "check"
                                    : "copy"
                                }
                              />
                              {copiedValue === String(preset.placeId)
                                ? "Copied"
                                : "Copy place ID"}
                            </button>
                            <button
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                openEdit(preset);
                                setOpenMenu(null);
                              }}
                            >
                              <Icon name="edit" />
                              Edit
                            </button>
                            <div className="preset-menu-separator" role="separator" />
                            <button
                              className="preset-menu-danger"
                              type="button"
                              role="menuitem"
                              onClick={() => {
                                setDeleteTarget(preset);
                                setOpenMenu(null);
                              }}
                            >
                              <Icon name="delete" tone="current-color" />
                              Delete
                            </button>
                          </PopupMenu>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </main>

      {deleteTarget && (
        <ConfirmModal
          title="Delete preset?"
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
        <Popup
          className="add-account-modal preset-edit-modal"
          backdropClassName="add-account-modal-backdrop"
          onClose={() => {
            if (!editSaving) setEditTarget(null);
          }}
          closeOnBackdrop
          labelledBy="edit-preset-title"
        >
          <div className="add-account-modal-header">
            <div>
              <h2 id="edit-preset-title">Edit preset</h2>
              <p>Update the saved launch details.</p>
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
          <div className="add-account-modal-body preset-edit-fields">
            <label htmlFor="preset-edit-name">Name</label>
            <input
              id="preset-edit-name"
              value={editName}
              onChange={(event) => setEditName(event.target.value)}
              maxLength={80}
              autoComplete="off"
            />
            <label htmlFor="preset-edit-place-id">Place ID</label>
            <input
              id="preset-edit-place-id"
              value={editPlaceId}
              onChange={(event) => setEditPlaceId(event.target.value)}
              inputMode="numeric"
              autoComplete="off"
            />
            <label htmlFor="preset-edit-job-id">Job ID (optional)</label>
            <input
              id="preset-edit-job-id"
              value={editJobId}
              onChange={(event) => setEditJobId(event.target.value)}
              autoComplete="off"
            />
            <label htmlFor="preset-edit-data">Data (optional)</label>
            <input
              id="preset-edit-data"
              value={editData}
              onChange={(event) => setEditData(event.target.value)}
              autoComplete="off"
            />
            <button
              className="account-button primary"
              type="button"
              disabled={editSaving || !editName.trim() || getPlaceId(editPlaceId) === null}
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
