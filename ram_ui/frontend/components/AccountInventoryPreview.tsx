import { useEffect, useRef, useState, type CSSProperties } from "react";
import { fetchInventorySnapshot, operationError, type AccountSummary, type InventoryItem } from "../lib/ipc";
import { mergeAccountInventories } from "../lib/inventoryBrowsing";
import { LoadingSkeleton } from "./LoadingSkeleton";
import { Icon } from "./Icon";

export function AccountInventoryPreview({ accounts, onOpenInventories, inventoriesAvailable }: {
  accounts: AccountSummary[];
  onOpenInventories: () => void;
  inventoriesAvailable: boolean;
}) {
  const selection = accounts.map(account => `${account.userId}:${account.cookieExpired}`).sort().join(",");
  const [items, setItems] = useState<InventoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [cachedCount, setCachedCount] = useState(0);
  const [refresh, setRefresh] = useState(0);
  const lastRefresh = useRef(0);
  const [columns, setColumns] = useState(4);
  const grid = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const observer = new ResizeObserver(entries => setColumns(Math.max(2, Math.min(6, Math.floor(entries[0].contentRect.width / 112)))));
    if (grid.current) observer.observe(grid.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    let active = true;
    setItems([]);
    setError("");
    setLoading(true);
    const forceRefresh = refresh !== lastRefresh.current;
    const ids = selection.split(",").filter(Boolean).map(value => Number(value.split(":")[0]));
    const timer = window.setTimeout(() => {
      void Promise.all(ids.map(async userId => ({ userId, snapshot: await fetchInventorySnapshot(userId, forceRefresh) })))
        .then(results => {
          if (!active) return;
          const merged = mergeAccountInventories(results.map(result => ({ userId: result.userId, items: result.snapshot.items })));
          setItems(ids.length > 1 ? merged.filter(item => item.ownerIds.length === ids.length) : merged);
          setCachedCount(results.filter(result => result.snapshot.cached).length);
        })
        .catch(reason => { if (active) setError(operationError(reason, "Inventory could not be loaded. Refresh to retry.")); })
        .finally(() => { if (active) { setLoading(false); lastRefresh.current = refresh; } });
    }, 250);
    return () => { active = false; window.clearTimeout(timer); };
  }, [selection, refresh]);
  const limit = columns * 2;
  return <section className="account-card" aria-busy={loading}>
    <div className="account-card-header">
      <h3>{accounts.length > 1 ? "Shared Roblox inventory" : "Roblox inventory"}</h3>
      <div className="account-card-actions">
        <button className="icon-button bordered" type="button" aria-label="Refresh inventory" data-tip="Fetch current ownership from Roblox" disabled={loading} onClick={() => setRefresh(value => value + 1)}><Icon name="refresh" /></button>
        <button className="account-button" type="button" onClick={onOpenInventories}>{inventoriesAvailable ? "Inventories" : "Enable Inventories"}</button>
      </div>
    </div>
    <p className="common-inventory-summary" role="status">{loading ? "Loading inventory..." : error ? "Inventory unavailable" : `${items.length} ${accounts.length > 1 ? "shared " : ""}items - ${cachedCount ? `${cachedCount} cached ${cachedCount === 1 ? "inventory" : "inventories"}` : "updated from Roblox"}`}</p>
    {error && <p className="accounts-error" role="alert">{error}</p>}
    <div ref={grid} className="account-inventory-preview" style={{ "--preview-columns": columns } as CSSProperties}>
      {loading ? <LoadingSkeleton layout="inventory-grid" label="Loading inventory preview" count={limit} /> : !error && !items.length ? <p className="account-inventory-empty">{accounts.length > 1 ? "No items are shared by every selected account." : "No inventory items found."}</p> : items.slice(0, limit).map(item => <div className="account-inventory-tile" key={item.assetId} data-tip={`${item.name} - ${item.assetType} - ID: ${item.assetId}`}>
        <span>{item.iconUrl ? <img src={item.iconUrl} alt="" loading="lazy" /> : <Icon name="inventory" />}</span>
        <strong>{item.name}</strong>
      </div>)}
      {!loading && items.length > limit && <div className="account-inventory-more">
        <div className="account-inventory-placeholders" aria-hidden="true">{Array.from({ length: columns }, (_, index) => <span key={index} />)}</div>
        <button className="account-button" type="button" onClick={onOpenInventories} data-tip={inventoriesAvailable ? "Browse the full inventory" : "Enable Inventories in Settings > App and data > Visible pages"}>Load more in inventories tab</button>
      </div>}
    </div>
  </section>;
}
