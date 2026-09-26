import { useEffect, useId, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent, type PointerEvent } from "react";
import { AccountAvatar } from "./components/AccountAvatar";
import { Icon } from "./components/Icon";
import Select from "./components/Select";
import { buildInventoryAccountGroups, mergeAccountInventories, matchesInventoryComparison, sortInventoryItems, type OwnedInventoryItem, type InventoryComparison, type InventorySort, type InventorySortDirection } from "./lib/inventoryBrowsing";
import { getSelectionRectangle, selectInventoryAsset, selectInventoryAssets, type SelectionRectangle } from "./lib/inventorySelection";
import { useSelectAllShortcut } from "./hooks/useSelectAllShortcut";
import {
  fetchAccountInventory,
  listAccounts,
  listAccountGroups,
  openInventoryAssets,
  type AccountSummary,
} from "./lib/ipc";

type Category = "All" | "Hats" | "Hair" | "Face" | "Neck" | "Shoulder" | "Front" | "Back" | "Waist" | "Shirts" | "Pants" | "T-Shirts" | "Animations" | "Gear" | "Other";
type InventoryViewSize = "small" | "medium" | "large";

const categories: Exclude<Category, "All">[] = ["Hats", "Hair", "Face", "Neck", "Shoulder", "Front", "Back", "Waist", "Shirts", "Pants", "T-Shirts", "Animations", "Gear", "Other"];

type SelectionDrag = {
  pointerId: number;
  startX: number;
  startY: number;
  clientX: number;
  clientY: number;
  originalIds: Set<number>;
  baseIds: Set<number>;
  isDragging: boolean;
  tiles: { assetId: number; left: number; top: number; right: number; bottom: number }[];
};

function getCategory(assetType: string): Exclude<Category, "All"> {
  const categoriesByAssetType: Record<string, Exclude<Category, "All">> = {
    Hat: "Hats",
    HairAccessory: "Hair",
    FaceAccessory: "Face",
    NeckAccessory: "Neck",
    ShoulderAccessory: "Shoulder",
    FrontAccessory: "Front",
    BackAccessory: "Back",
    WaistAccessory: "Waist",
    Shirt: "Shirts",
    Pants: "Pants",
    TShirt: "T-Shirts",
    EmoteAnimation: "Animations",
    Gear: "Gear",
  };
  return categoriesByAssetType[assetType] ?? "Other";
}

export function InventoriesPage({ initialSelectedIds }: { initialSelectedIds: Set<number> }) {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [groupOrder, setGroupOrder] = useState<string[]>([]);
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const [isGroupOrderUnavailable, setIsGroupOrderUnavailable] = useState(false);
  const groupIdPrefix = useId();
  const [accountIds, setAccountIds] = useState<Set<number>>(() => new Set(initialSelectedIds));
  const [accountsLoading, setAccountsLoading] = useState(true);
  const [items, setItems] = useState<OwnedInventoryItem[]>([]);
  const [loadedAccountCount, setLoadedAccountCount] = useState(0);
  const [comparison, setComparison] = useState<InventoryComparison>("all");
  const [sort, setSort] = useState<InventorySort>("name");
  const [sortDirection, setSortDirection] = useState<InventorySortDirection>("ascending");
  const [itemsLoading, setItemsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshCount, setRefreshCount] = useState(0);
  const [selectedCategories, setSelectedCategories] = useState<Set<Exclude<Category, "All">>>(new Set());
  const [search, setSearch] = useState("");
  const [selectedItemIds, setSelectedItemIds] = useState<Set<number>>(new Set());
  const [copiedId, setCopiedId] = useState<number | null>(null);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [gridSize, setGridSize] = useState<InventoryViewSize>("medium");
  const [listSize, setListSize] = useState<InventoryViewSize>("medium");
  const [isFilterSidebarCollapsed, setIsFilterSidebarCollapsed] = useState(false);
  const [isFilterSidebarResizing, setIsFilterSidebarResizing] = useState(false);
  const [filterSidebarWidth, setFilterSidebarWidth] = useState(240);
  const [shouldFocusSidebarSearch, setShouldFocusSidebarSearch] = useState(false);
  const [shouldFocusCompactSearch, setShouldFocusCompactSearch] = useState(false);
  const [sidebarSectionToFocus, setSidebarSectionToFocus] = useState<"types" | "comparison" | "sort" | null>(null);
  const [selectedSearch, setSelectedSearch] = useState("");
  const [selectionRectangle, setSelectionRectangle] = useState<SelectionRectangle | null>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const sidebarSearchRef = useRef<HTMLInputElement>(null);
  const compactSearchRef = useRef<HTMLButtonElement>(null);
  const filterOptionsRef = useRef<HTMLDivElement>(null);
  const isSearchAutoExpandedRef = useRef(false);
  const dropdownRef = useRef<HTMLDetailsElement>(null);
  const dragRef = useRef<SelectionDrag | null>(null);
  const animationFrameRef = useRef<number | null>(null);
  const suppressClickRef = useRef(false);
  const copyTimeoutRef = useRef<number | null>(null);

  useEffect(() => {
    if (!isFilterSidebarCollapsed && shouldFocusSidebarSearch) {
      sidebarSearchRef.current?.focus();
      setShouldFocusSidebarSearch(false);
    }
    if (isFilterSidebarCollapsed && shouldFocusCompactSearch) {
      compactSearchRef.current?.focus();
      setShouldFocusCompactSearch(false);
    }
    if (!isFilterSidebarCollapsed && sidebarSectionToFocus) {
      filterOptionsRef.current?.querySelector<HTMLButtonElement>(`[data-filter-section="${sidebarSectionToFocus}"] button`)?.focus();
      setSidebarSectionToFocus(null);
    }
  }, [isFilterSidebarCollapsed, shouldFocusSidebarSearch, shouldFocusCompactSearch, sidebarSectionToFocus]);

  useEffect(() => {
    function onDocumentPointerDown(event: globalThis.PointerEvent) {
      if (event.target instanceof Node && !dropdownRef.current?.contains(event.target)) {
        if (dropdownRef.current) dropdownRef.current.open = false;
      }
    }
    function onDocumentKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      if (dropdownRef.current?.open) {
        dropdownRef.current.open = false;
        dropdownRef.current.querySelector("summary")?.focus();
      }
      const drag = dragRef.current;
      if (drag) {
        setSelectedItemIds(drag.originalIds);
        finishDrag();
      }
    }
    document.addEventListener("pointerdown", onDocumentPointerDown);
    document.addEventListener("pointerup", finishDrag);
    document.addEventListener("keydown", onDocumentKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onDocumentPointerDown);
      document.removeEventListener("pointerup", finishDrag);
      document.removeEventListener("keydown", onDocumentKeyDown);
      if (animationFrameRef.current !== null) window.cancelAnimationFrame(animationFrameRef.current);
      if (copyTimeoutRef.current !== null) window.clearTimeout(copyTimeoutRef.current);
    };
  }, []);

  useEffect(() => {
    let isCurrent = true;
    void Promise.allSettled([listAccounts(), listAccountGroups()])
      .then(([accountResult, groupResult]) => {
        if (!isCurrent) return;
        if (accountResult.status === "rejected") {
          setAccountIds(new Set());
          setError("Accounts could not be loaded. Reopen this page to retry.");
          return;
        }
        if (groupResult.status === "fulfilled") setGroupOrder(groupResult.value.map((group) => group.name));
        else setIsGroupOrderUnavailable(true);
        const loaded = accountResult.value;
        setAccounts(loaded);
        setAccountIds((current) => {
          const available = new Set(loaded.map((account) => account.userId));
          const retained = new Set([...current].filter((id) => available.has(id)));
          if (retained.size === 0 && loaded[0]) retained.add(loaded[0].userId);
          return retained;
        });
      })
      .catch(() => {
        if (isCurrent) {
          setAccountIds(new Set());
          setError("Accounts could not be loaded. Reopen this page to retry.");
        }
      })
      .finally(() => {
        if (isCurrent) setAccountsLoading(false);
      });
    return () => { isCurrent = false; };
  }, []);

  useEffect(() => {
    if (accountsLoading || accountIds.size === 0) {
      setItems([]);
      setLoadedAccountCount(0);
      setItemsLoading(false);
      setSelectedItemIds(new Set());
      return;
    }

    let isCurrent = true;
    setItemsLoading(true);
    setLoadedAccountCount(0);
    setError(null);
    setSelectedItemIds(new Set());
    void Promise.allSettled([...accountIds].map(async (userId) => ({ userId, items: await fetchAccountInventory(userId) })))
      .then((results) => {
        if (!isCurrent) return;
        const loaded = results.flatMap((result) => result.status === "fulfilled" ? [result.value] : []);
        setItems(mergeAccountInventories(loaded));
        setLoadedAccountCount(loaded.length);
        const failures = results.filter((result) => result.status === "rejected").length;
        if (failures) {
          setError(`${failures} ${failures === 1 ? "account inventory" : "account inventories"} could not be loaded. Refresh selected to retry.`);
        }
      })
      .finally(() => {
        if (isCurrent) setItemsLoading(false);
      });
    return () => { isCurrent = false; };
  }, [accountIds, accountsLoading, refreshCount]);

  const isComparisonAvailable = accountIds.size > 1 && loadedAccountCount === accountIds.size && !itemsLoading;
  const effectiveComparison = isComparisonAvailable ? comparison : "all";
  const categoryCounts = useMemo(() => {
    const counts = new Map<Category, number>([["All", items.length]]);
    for (const item of items) {
      const itemCategory = getCategory(item.assetType);
      counts.set(itemCategory, (counts.get(itemCategory) ?? 0) + 1);
    }
    return counts;
  }, [items]);
  const accountsById = useMemo(() => new Map(accounts.map((account) => [account.userId, account])), [accounts]);

  const visibleItems = useMemo(() => {
    const query = search.trim().toLowerCase();
    const filtered = items.filter((item) =>
      (selectedCategories.size === 0 || selectedCategories.has(getCategory(item.assetType))) &&
      matchesInventoryComparison(item, effectiveComparison, accountIds.size) &&
      (!query || `${item.name} ${item.assetId}`.toLowerCase().includes(query)),
    );
    return sortInventoryItems(filtered, { field: sort, direction: sortDirection });
  }, [items, selectedCategories, search, effectiveComparison, accountIds.size, sort, sortDirection]);
  const filterAnimationKey = `${[...selectedCategories].sort().join(",")}:${effectiveComparison}:${search}`;

  useSelectAllShortcut(() => {
    setSelectedItemIds((current) => new Set([...current, ...visibleItems.map((item) => item.assetId)]));
  }, { isEnabled: !itemsLoading && visibleItems.length > 0 });

  const accountGroups = useMemo(() => buildInventoryAccountGroups(accounts, groupOrder), [accounts, groupOrder]);

  function onGroupToggle(group: string) {
    setCollapsedGroups((current) => {
      const next = new Set(current);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }

  function toggleCategory(option: Category) {
    if (option === "All") {
      setSelectedCategories(new Set());
      return;
    }
    setSelectedCategories((current) => {
      const next = new Set(current);
      if (next.has(option)) next.delete(option);
      else next.add(option);
      return next;
    });
  }

  function resizeFilterSidebar(nextWidth: number) {
    setFilterSidebarWidth(Math.max(180, Math.min(360, nextWidth)));
  }

  function toggleFilterSidebar() {
    isSearchAutoExpandedRef.current = false;
    setIsFilterSidebarCollapsed((collapsed) => !collapsed);
  }

  function openSidebarSearch() {
    isSearchAutoExpandedRef.current = isFilterSidebarCollapsed;
    setIsFilterSidebarCollapsed(false);
    setShouldFocusSidebarSearch(true);
  }

  function openFilterSidebar(section: "types" | "comparison" | "sort") {
    isSearchAutoExpandedRef.current = false;
    setIsFilterSidebarCollapsed(false);
    setSidebarSectionToFocus(section);
  }

  function onSidebarSearchKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (!isSearchAutoExpandedRef.current || !["Enter", "Escape"].includes(event.key) || event.nativeEvent.isComposing) return;
    event.preventDefault();
    isSearchAutoExpandedRef.current = false;
    setIsFilterSidebarCollapsed(true);
    setShouldFocusCompactSearch(true);
  }

  function onFilterResizePointerDown(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    setIsFilterSidebarResizing(true);
  }

  function onFilterResizePointerMove(event: PointerEvent<HTMLDivElement>) {
    if (event.buttons !== 1 || !workspaceRef.current) return;
    resizeFilterSidebar(workspaceRef.current.getBoundingClientRect().right - event.clientX);
  }

  function onFilterResizeKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    if (event.key === "ArrowLeft") resizeFilterSidebar(filterSidebarWidth - 16);
    else if (event.key === "ArrowRight") resizeFilterSidebar(filterSidebarWidth + 16);
    else if (event.key === "Home") resizeFilterSidebar(180);
    else if (event.key === "End") resizeFilterSidebar(360);
    else return;
    event.preventDefault();
  }

  function toggleAccount(event: MouseEvent<HTMLButtonElement>, userId: number) {
    setAccountIds((current) => {
      if (!event.ctrlKey && !event.metaKey) return new Set([userId]);
      const next = new Set(current);
      if (next.has(userId)) next.delete(userId);
      else next.add(userId);
      return next;
    });
  }

  function onItemClick(event: MouseEvent<HTMLButtonElement>, assetId: number) {
    if (suppressClickRef.current) return;
    setSelectedItemIds((current) => selectInventoryAsset(current, {
      assetId,
      mode: event.ctrlKey || event.metaKey || current.has(assetId) ? "toggle" : "replace",
    }));
  }

  async function copyIds(ids: number[], options: { feedback: "individual" | "all" } = { feedback: "individual" }) {
    try {
      await navigator.clipboard.writeText(ids.join(","));
      setCopiedId(options.feedback === "all" ? 0 : ids[0]);
      if (copyTimeoutRef.current !== null) window.clearTimeout(copyTimeoutRef.current);
      copyTimeoutRef.current = window.setTimeout(() => setCopiedId(null), 1600);
    } catch {
      setError("Clipboard access is unavailable.");
    }
  }

  async function openSelected() {
    try {
      await openInventoryAssets([...selectedItemIds]);
    } catch {
      setError("Selected items could not be opened on Roblox.");
    }
  }

  function finishDrag() {
    const drag = dragRef.current;
    if (drag && gridRef.current?.hasPointerCapture(drag.pointerId)) {
      gridRef.current.releasePointerCapture(drag.pointerId);
    }
    dragRef.current = null;
    if (animationFrameRef.current !== null) window.cancelAnimationFrame(animationFrameRef.current);
    animationFrameRef.current = null;
    setSelectionRectangle(null);
    window.requestAnimationFrame(() => { suppressClickRef.current = false; });
  }

  function updateDragSelection() {
    const grid = gridRef.current;
    const drag = dragRef.current;
    if (!grid || !drag?.isDragging) return;
    const bounds = grid.getBoundingClientRect();
    const isNearTop = drag.clientY < bounds.top + 24;
    const isNearBottom = drag.clientY > bounds.bottom - 24;
    if (isNearTop) grid.scrollTop -= 12;
    else if (isNearBottom) grid.scrollTop += 12;
    const currentX = Math.max(0, Math.min(grid.clientWidth, drag.clientX - bounds.left)) + grid.scrollLeft;
    const currentY = Math.max(0, Math.min(grid.clientHeight, drag.clientY - bounds.top)) + grid.scrollTop;
    const rectangle = getSelectionRectangle({ x: drag.startX, y: drag.startY }, { x: currentX, y: currentY });
    setSelectionRectangle(rectangle);
    setSelectedItemIds(selectInventoryAssets(rectangle, drag.tiles, drag.baseIds));
    animationFrameRef.current = window.requestAnimationFrame(updateDragSelection);
  }

  function onSelectionPointerDown(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || event.pointerType === "touch" || itemsLoading) return;
    if (event.target instanceof Element && event.target.closest(".inventories-id")) return;
    const grid = event.currentTarget;
    const bounds = grid.getBoundingClientRect();
    if (event.clientX >= bounds.left + grid.clientWidth) return;
    suppressClickRef.current = false;
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX - bounds.left + grid.scrollLeft,
      startY: event.clientY - bounds.top + grid.scrollTop,
      clientX: event.clientX,
      clientY: event.clientY,
      originalIds: new Set(selectedItemIds),
      baseIds: event.ctrlKey || event.metaKey ? new Set(selectedItemIds) : new Set(),
      isDragging: false,
      tiles: [...grid.querySelectorAll<HTMLElement>(".inventories-tile")].map((tile) => {
        const tileBounds = tile.getBoundingClientRect();
        return {
          assetId: Number(tile.dataset.assetId),
          left: tileBounds.left - bounds.left + grid.scrollLeft,
          top: tileBounds.top - bounds.top + grid.scrollTop,
          right: tileBounds.right - bounds.left + grid.scrollLeft,
          bottom: tileBounds.bottom - bounds.top + grid.scrollTop,
        };
      }),
    };
  }

  function onSelectionPointerMove(event: PointerEvent<HTMLDivElement>) {
    const drag = dragRef.current;
    if (!drag || event.pointerId !== drag.pointerId) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const distance = Math.hypot(
      event.clientX - bounds.left + event.currentTarget.scrollLeft - drag.startX,
      event.clientY - bounds.top + event.currentTarget.scrollTop - drag.startY,
    );
    drag.clientX = event.clientX;
    drag.clientY = event.clientY;
    if (!drag.isDragging && distance > 5) {
      drag.isDragging = true;
      suppressClickRef.current = true;
      event.currentTarget.setPointerCapture(event.pointerId);
      updateDragSelection();
    }
  }

  const selectedItems = items.filter((item) => selectedItemIds.has(item.assetId));
  const matchingSelectedItems = selectedItems.filter((item) =>
    `${item.name} ${item.assetId}`.toLowerCase().includes(selectedSearch.trim().toLowerCase()),
  );

  return (
    <>
      <div className="header-row inventories-header"><h1 className="header-title">Inventories</h1></div>
      <main className="inventories-page">
        <aside className="inventories-accounts" aria-label="Inventory accounts">
          <h2>Accounts</h2>
          <p>Ctrl-click to select multiple accounts</p>
          {isGroupOrderUnavailable && <p role="alert">Saved group order could not be loaded. Reopen this page to retry.</p>}
          {accountsLoading ? <p>Loading accounts...</p> : accounts.length === 0 ? <p>{error ? "Accounts are unavailable." : "No accounts yet. Add one in the Accounts workspace to browse its inventory."}</p> : accountGroups.map(([group, members], index) => {
            const isCollapsed = collapsedGroups.has(group);
            const groupId = `${groupIdPrefix}-${index}`;
            return (
            <div className={`inventories-account-group ${isCollapsed ? "is-collapsed" : ""}`} key={group}>
              <h3>
                <button className="inventories-group-toggle" type="button" aria-expanded={!isCollapsed} aria-controls={groupId} onClick={() => onGroupToggle(group)}>
                  <span className={`inventories-group-chevron ${isCollapsed ? "is-collapsed" : ""}`}><Icon name="chevron-down" /></span>
                  <span>{group}</span><span className="inventories-group-count">{members.length}</span>
                </button>
              </h3>
              <div id={groupId} hidden={isCollapsed}>
              {members.map((account) => (
                <button
                  className={`inventories-account ${accountIds.has(account.userId) ? "is-selected" : ""}`}
                  type="button"
                  key={account.userId}
                  aria-pressed={accountIds.has(account.userId)}
                  onClick={(event) => toggleAccount(event, account.userId)}
                >
                  <AccountAvatar account={account} className="inventories-avatar" />
                  <span>{account.alias || account.username}</span>
                </button>
              ))}
              </div>
            </div>
            );
          })}
          <button className="account-button inventories-refresh" type="button" disabled={accountIds.size === 0 || itemsLoading} onClick={() => setRefreshCount((count) => count + 1)}>
            <Icon name="refresh" />{itemsLoading ? "Refreshing..." : "Refresh selected"}
          </button>
        </aside>

        <section className="inventories-main" aria-labelledby="inventory-title">
          <div className={`inventories-workspace ${isFilterSidebarCollapsed ? "is-filter-collapsed" : ""} ${isFilterSidebarResizing ? "is-filter-resizing" : ""}`} ref={workspaceRef} style={{ "--inventory-filter-width": `${filterSidebarWidth}px` } as CSSProperties}>
            <aside className="inventories-filter-sidebar" aria-label="Inventory filters" onBlur={(event) => {
              if (isSearchAutoExpandedRef.current && !event.currentTarget.contains(event.relatedTarget)) {
                isSearchAutoExpandedRef.current = false;
                setIsFilterSidebarCollapsed(true);
              }
            }}>
              <div className="inventories-filter-heading">
                {!isFilterSidebarCollapsed && <h2>Filters</h2>}
                <button className="inventories-filter-toggle" type="button" aria-label={isFilterSidebarCollapsed ? "Expand inventory filters" : "Collapse inventory filters"} aria-expanded={!isFilterSidebarCollapsed} aria-controls="inventory-filter-options" title={isFilterSidebarCollapsed ? "Expand filters" : "Collapse filters"} onClick={toggleFilterSidebar}>
                  <Icon name="chevron-down" />
                </button>
              </div>
              {isFilterSidebarCollapsed && <div className="inventories-filter-rail" aria-label="Collapsed inventory controls">
                <button ref={compactSearchRef} type="button" aria-label="Search inventory" title="Search inventory" aria-pressed={search.trim().length > 0} onClick={openSidebarSearch}><Icon name="search" /></button>
                <button type="button" aria-label="Item type filters" title="Item type filters" aria-pressed={selectedCategories.size > 0} onClick={() => openFilterSidebar("types")}><Icon name="inventory" /></button>
                <button type="button" aria-label="Compare inventories" title="Compare inventories" aria-pressed={effectiveComparison !== "all"} onClick={() => openFilterSidebar("comparison")}><Icon name="users" /></button>
                <button type="button" aria-label="Sort inventory" title="Sort inventory" onClick={() => openFilterSidebar("sort")}><Icon name="list" tone="current-color" /></button>
              </div>}
              <div id="inventory-filter-options" ref={filterOptionsRef} hidden={isFilterSidebarCollapsed}>
          <div className="inventories-control-panel">
            <div className="inventories-control-group">
              <span className="inventories-control-label">Search</span>
              <label className="inventories-search"><Icon name="search" /><input ref={sidebarSearchRef} value={search} onChange={(event) => setSearch(event.target.value)} onKeyDown={onSidebarSearchKeyDown} placeholder="Search items or ids" aria-label="Search inventory" /></label>
            </div>
            <div className="inventories-control-row">
              <div className="inventories-control-group">
                <span className="inventories-control-label">Filter by type</span>
                <div className="inventories-filters" role="group" aria-label="Item categories" data-filter-section="types">
                  {["All" as const, ...categories].filter((option) => option === "All" || (categoryCounts.get(option) ?? 0) > 0).map((option) => {
                    const isActive = option === "All" ? selectedCategories.size === 0 : selectedCategories.has(option);
                    return <button className={isActive ? "is-active" : ""} type="button" key={option} aria-pressed={isActive} onClick={() => toggleCategory(option)}>
                      {option}<span>{categoryCounts.get(option) ?? 0}</span>
                    </button>
                  })}
                </div>
              </div>
              <div className="inventories-control-group">
                <span className="inventories-control-label">Compare</span>
                <div className="inventories-filters inventories-comparison" role="group" aria-label="Compare inventories" data-filter-section="comparison">
              {([
                { value: "all", label: "All items" },
                { value: "shared", label: "Shared by all" },
                { value: "unique", label: "Unique to one" },
              ] as const).map((option) => (
                <button type="button" key={option.value} className={effectiveComparison === option.value ? "is-active" : ""} aria-pressed={effectiveComparison === option.value} disabled={option.value !== "all" && !isComparisonAvailable} onClick={() => setComparison(option.value)}>
                  {option.label}
                  {(option.value === "all" || isComparisonAvailable) && <span>{items.filter((item) => matchesInventoryComparison(item, option.value, accountIds.size)).length}</span>}
                </button>
              ))}
                </div>
              </div>
            </div>
            <div className="inventories-control-row inventories-control-row-secondary">
              <span className="inventories-control-label">Sort items</span>
              <div className="inventories-sort" data-filter-section="sort">
                <Select value={sort} onChange={(value) => setSort(value as InventorySort)} ariaLabel="Sort inventory by" options={[
                  { value: "name", label: "Name" },
                  { value: "assetId", label: "Asset ID" },
                  { value: "assetType", label: "Type" },
                  { value: "priceRobux", label: "Price" },
                ]} />
                <Select value={sortDirection} onChange={(value) => setSortDirection(value as InventorySortDirection)} ariaLabel="Inventory sort direction" options={[
                  { value: "ascending", label: "Ascending" },
                  { value: "descending", label: "Descending" },
                ]} />
              </div>
            </div>
          </div>
          {!itemsLoading && !isComparisonAvailable && <p className="inventories-selection-hint">{accountIds.size < 2 ? "Select multiple accounts to compare inventories." : "Comparison requires every selected inventory to load."}</p>}
              </div>
            </aside>
            {!isFilterSidebarCollapsed && <div className="inventories-filter-resizer" role="separator" aria-label="Resize inventory filters" aria-orientation="vertical" aria-valuemin={180} aria-valuemax={360} aria-valuenow={filterSidebarWidth} tabIndex={0} onPointerDown={onFilterResizePointerDown} onPointerMove={onFilterResizePointerMove} onLostPointerCapture={() => setIsFilterSidebarResizing(false)} onKeyDown={onFilterResizeKeyDown} />}
            <div className="inventories-content">
          <div className="inventories-heading">
            <div className="inventories-title"><h2 id="inventory-title">Roblox inventory</h2><span>{accountIds.size} {accountIds.size === 1 ? "account" : "accounts"}</span></div>
          </div>
          <div className="inventories-toolbar">
            <button className="account-button" type="button" disabled={visibleItems.length === 0 || itemsLoading} onClick={() => setSelectedItemIds((current) => new Set([...current, ...visibleItems.map((item) => item.assetId)]))}>Select all</button>
            <button className="account-button" type="button" disabled={selectedItemIds.size === 0} onClick={() => setSelectedItemIds(new Set())}>Clear selection</button>
            <span>{visibleItems.length} {visibleItems.length === 1 ? "item" : "items"}</span>
            <div className="inventories-view-controls">
              <div className="inventories-view-switch" role="group" aria-label="Inventory view">
                <button type="button" aria-pressed={view === "grid"} onClick={() => setView("grid")}><Icon name="grid" />Grid</button>
                <button type="button" aria-pressed={view === "list"} onClick={() => setView("list")}><Icon name="list" tone="current-color" />List</button>
              </div>
              <div className="inventories-size">
                <span>{view === "grid" ? "Grid size:" : "List size:"}</span>
                <Select value={view === "grid" ? gridSize : listSize} onChange={(value) => {
                  if (view === "grid") setGridSize(value as InventoryViewSize);
                  else setListSize(value as InventoryViewSize);
                }} ariaLabel={view === "grid" ? "Grid item size" : "List row size"} options={[
                  { value: "small", label: "Small" },
                  { value: "medium", label: "Medium" },
                  { value: "large", label: "Large" },
                ]} />
              </div>
            </div>
          </div>
          <p className="inventories-selection-hint">Click to select one item. Ctrl-click to toggle items, or drag to select several. Hold Ctrl while dragging to add to your selection.</p>
          {error && <p className="inventories-error" role="alert">{error}</p>}
          <div
            className={`inventories-grid ${view === "list" ? "is-list" : ""} ${selectionRectangle ? "is-dragging" : ""}`}
            data-size={view === "grid" ? gridSize : listSize}
            ref={gridRef}
            aria-label="Inventory items"
            aria-busy={itemsLoading}
            onPointerDown={onSelectionPointerDown}
            onPointerMove={onSelectionPointerMove}
            onPointerCancel={() => {
              if (dragRef.current) setSelectedItemIds(dragRef.current.originalIds);
              finishDrag();
            }}
            onDragStart={(event) => event.preventDefault()}
          >
            {selectionRectangle && <span className="inventories-marquee" aria-hidden="true" style={selectionRectangle} />}
            {itemsLoading ? (
              Array.from({ length: 8 }, (_, index) => (
                <div className="inventories-skeleton" key={index} aria-hidden="true">
                  <span className="inventories-thumbnail" />
                  <span className="inventories-skeleton-label" />
                  <span className="inventories-skeleton-label" />
                </div>
              ))
            ) : accountIds.size === 0 ? (
              <p className="inventories-grid-state">Select an account to view its inventory.</p>
            ) : visibleItems.length === 0 ? (
              <p className="inventories-grid-state">
                {items.length === 0
                  ? "No inventory items found. Refresh selected to try again."
                  : "No items match these filters. Try another category or search."}
              </p>
            ) : visibleItems.map((item, index) => {
              const owners = item.ownerIds.flatMap((id) => {
                const account = accountsById.get(id);
                return account ? [account] : [];
              });
              const ownerNames = owners.map((account) => account.alias || account.username).join(", ");
              const priceLabel = item.priceRobux === null ? "Price unavailable" : item.priceRobux === 0 ? "Free" : `${item.priceRobux.toLocaleString()} Robux`;
              return (
              <article
                className={`inventories-tile ${selectedItemIds.has(item.assetId) ? "is-selected" : ""}`}
                key={`${filterAnimationKey}:${item.assetId}`}
                data-asset-id={item.assetId}
                style={{ "--inventory-index": index } as CSSProperties}
              >
                <button
                  className="inventories-tile-select"
                  type="button"
                  aria-pressed={selectedItemIds.has(item.assetId)}
                  aria-label={`Select ${item.name}`}
                  title={`${item.name} · ${item.assetType}${item.priceRobux === null ? "" : ` · ${item.priceRobux.toLocaleString()} Robux`}`}
                  onClick={(event) => onItemClick(event, item.assetId)}
                >
                  <span className="inventories-thumbnail">
                    {item.iconUrl ? <img src={item.iconUrl} alt="" loading="lazy" /> : <Icon name="inventory" />}
                  </span>
                  <span className="inventories-tile-check" aria-hidden="true">
                    {selectedItemIds.has(item.assetId) && <Icon name="check" />}
                  </span>
                  <span className="inventories-tile-caption">
                    <strong className="inventories-tile-name">{item.name}</strong>
                    {view === "list" && (
                      <span className="inventories-list-details">
                        <span title={item.assetType}>{item.assetType}</span>
                        <span title={priceLabel}>{priceLabel}</span>
                      </span>
                    )}
                    <span className="inventories-owner-avatars" title={`Owned by: ${ownerNames}`} aria-label={`Owned by: ${ownerNames}`}>
                      {owners.slice(0, 3).map((account) => <AccountAvatar key={account.userId} account={account} className="inventories-owner-avatar" />)}
                      {owners.length > 3 && <span className="inventories-owner-overflow">+{owners.length - 3}</span>}
                    </span>
                  </span>
                </button>
                <button
                  className="inventories-id"
                  type="button"
                  aria-label={`Copy ID for ${item.name}`}
                  data-tip={copiedId === item.assetId ? "Copied" : "Copy ID"}
                  onClick={() => void copyIds([item.assetId])}
                >
                  <span>{item.assetId}</span>
                  <Icon name={copiedId === item.assetId ? "check" : "copy"} />
                </button>
              </article>
              );
            })}
          </div>
          {selectedItemIds.size > 0 && (
            <div className="inventories-selection" aria-label="Selected inventory actions">
              <strong aria-live="polite">{selectedItemIds.size} selected</strong>
              <details className="inventories-selected-dropdown" ref={dropdownRef} onToggle={(event) => {
                if (event.currentTarget.open) {
                  setSelectedSearch("");
                  event.currentTarget.querySelector("input")?.focus();
                }
              }}>
                <summary className="account-button">Selected IDs<Icon name="chevron-down" /></summary>
                <div className="popup-menu inventories-selected-menu">
                  <label className="inventories-search">
                    <Icon name="search" />
                    <input value={selectedSearch} onChange={(event) => setSelectedSearch(event.target.value)} placeholder="Find a selected item" aria-label="Search selected IDs" />
                  </label>
                  <div className="inventories-selected-options">
                    {matchingSelectedItems.map((item) => (
                      <button type="button" key={item.assetId} onClick={() => {
                        void copyIds([item.assetId]);
                        if (dropdownRef.current) dropdownRef.current.open = false;
                        dropdownRef.current?.querySelector("summary")?.focus();
                      }}>
                        <span>{item.name}</span><small>{item.assetId}</small><Icon name={copiedId === item.assetId ? "check" : "copy"} />
                      </button>
                    ))}
                    {matchingSelectedItems.length === 0 && <p>No selected items match this search.</p>}
                  </div>
                </div>
              </details>
              <div className="inventories-dock-actions">
                <button className="account-button" type="button" onClick={() => void copyIds([...selectedItemIds], { feedback: "all" })}>
                  <Icon name={copiedId === 0 ? "check" : "copy"} />{copiedId === 0 ? "Copied" : "Copy all (CSV)"}
                </button>
                <button className="account-button" type="button" disabled={selectedItemIds.size > 20} title={selectedItemIds.size > 20 ? "Select up to 20 items to open on Roblox" : undefined} onClick={() => void openSelected()}>
                  <Icon name="square-arrow-out-up-right" />Open on Roblox
                </button>
                <button className="account-button" type="button" onClick={() => setSelectedItemIds(new Set())}>Clear</button>
              </div>
            </div>
          )}
            </div>
          </div>
        </section>
      </main>
    </>
  );
}
