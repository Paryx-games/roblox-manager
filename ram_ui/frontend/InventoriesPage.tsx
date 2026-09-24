import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { AccountAvatar } from "./components/AccountAvatar";
import { Icon } from "./components/Icon";
import {
  fetchAccountInventory,
  listAccounts,
  openInventoryAssets,
  type AccountSummary,
  type InventoryItem,
} from "./lib/ipc";

type Category = "All" | "Hair & hats" | "Clothing" | "Animations" | "Gear";

const categories: Category[] = ["All", "Hair & hats", "Clothing", "Animations", "Gear"];

function getCategory(assetType: string): Category {
  if (assetType === "Gear") return "Gear";
  if (assetType === "EmoteAnimation") return "Animations";
  if (["Shirt", "Pants", "TShirt"].includes(assetType)) return "Clothing";
  return "Hair & hats";
}

export function InventoriesPage({ initialSelectedIds }: { initialSelectedIds: Set<number> }) {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [accountIds, setAccountIds] = useState<Set<number>>(() => new Set(initialSelectedIds));
  const [accountsLoading, setAccountsLoading] = useState(true);
  const [items, setItems] = useState<InventoryItem[]>([]);
  const [itemsLoading, setItemsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshCount, setRefreshCount] = useState(0);
  const [category, setCategory] = useState<Category>("All");
  const [search, setSearch] = useState("");
  const [selectedItemIds, setSelectedItemIds] = useState<Set<number>>(new Set());
  const [copiedId, setCopiedId] = useState<number | null>(null);

  useEffect(() => {
    let isCurrent = true;
    void listAccounts()
      .then((loaded) => {
        if (!isCurrent) return;
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
      return;
    }

    let isCurrent = true;
    setItemsLoading(true);
    setError(null);
    setSelectedItemIds(new Set());
    void Promise.allSettled([...accountIds].map((id) => fetchAccountInventory(id)))
      .then((results) => {
        if (!isCurrent) return;
        const merged = new Map<number, InventoryItem>();
        for (const result of results) {
          if (result.status === "fulfilled") {
            for (const item of result.value) merged.set(item.assetId, item);
          }
        }
        setItems([...merged.values()].sort((left, right) => left.name.localeCompare(right.name)));
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

  const visibleItems = useMemo(() => {
    const query = search.trim().toLowerCase();
    return items.filter((item) =>
      (category === "All" || getCategory(item.assetType) === category) &&
      (!query || `${item.name} ${item.assetId}`.toLowerCase().includes(query)),
    );
  }, [items, category, search]);

  const accountGroups = useMemo(() => {
    const grouped = new Map<string, AccountSummary[]>();
    for (const account of accounts) {
      const name = account.group || "Ungrouped";
      grouped.set(name, [...(grouped.get(name) ?? []), account]);
    }
    return grouped;
  }, [accounts]);

  function toggleAccount(event: MouseEvent<HTMLButtonElement>, userId: number) {
    setAccountIds((current) => {
      if (!event.ctrlKey && !event.metaKey) return new Set([userId]);
      const next = new Set(current);
      if (next.has(userId)) next.delete(userId);
      else next.add(userId);
      return next;
    });
  }

  function toggleItem(assetId: number) {
    setSelectedItemIds((current) => {
      const next = new Set(current);
      if (next.has(assetId)) next.delete(assetId);
      else next.add(assetId);
      return next;
    });
  }

  async function copyIds(ids: number[]) {
    try {
      await navigator.clipboard.writeText(ids.join("\n"));
      setCopiedId(ids.length === 1 ? ids[0] : 0);
      window.setTimeout(() => setCopiedId(null), 1600);
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

  return (
    <>
      <div className="header-row"><h1 className="header-title">Inventories</h1></div>
      <main className="inventories-page">
        <aside className="inventories-accounts" aria-label="Inventory accounts">
          <h2>Accounts</h2>
          <p>Ctrl-click to select multiple accounts</p>
          {accountsLoading ? <p>Loading accounts...</p> : accounts.length === 0 ? <p>{error ? "Accounts are unavailable." : "No accounts yet. Add one in the Accounts workspace to browse its inventory."}</p> : [...accountGroups].map(([group, members]) => (
            <div className="inventories-account-group" key={group}>
              <h3>{group}</h3>
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
          ))}
          <button className="account-button inventories-refresh" type="button" disabled={accountIds.size === 0 || itemsLoading} onClick={() => setRefreshCount((count) => count + 1)}>
            <Icon name="refresh" />{itemsLoading ? "Refreshing..." : "Refresh selected"}
          </button>
        </aside>

        <section className="inventories-main" aria-labelledby="inventory-title">
          <div className="inventories-heading">
            <div className="inventories-title"><h2 id="inventory-title">Roblox inventory</h2><span>{accountIds.size} {accountIds.size === 1 ? "account" : "accounts"}</span></div>
            <label className="inventories-search"><Icon name="search" /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search items or ids" aria-label="Search inventory" /></label>
          </div>
          <div className="inventories-filters" aria-label="Item categories">
            {categories.map((option) => (
              <button className={category === option ? "is-active" : ""} type="button" key={option} aria-pressed={category === option} onClick={() => setCategory(option)}>
                {option}<span>{option === "All" ? items.length : items.filter((item) => getCategory(item.assetType) === option).length}</span>
              </button>
            ))}
          </div>
          <div className="inventories-toolbar">
            <button className="account-button" type="button" disabled={visibleItems.length === 0 || itemsLoading} onClick={() => setSelectedItemIds((current) => new Set([...current, ...visibleItems.map((item) => item.assetId)]))}>Select all</button>
            <button className="account-button" type="button" disabled={selectedItemIds.size === 0} onClick={() => setSelectedItemIds(new Set())}>Clear selection</button>
            <span>{visibleItems.length} {visibleItems.length === 1 ? "item" : "items"}</span>
          </div>
          {error && <p className="inventories-error" role="alert">{error}</p>}
          <div className="inventories-table" role="table" aria-label="Inventory items">
            <div className="inventories-table-head" role="row"><span role="columnheader">Item</span><span role="columnheader">Type</span><span role="columnheader">Price</span><span role="columnheader">Id</span></div>
            {itemsLoading ? [0, 1, 2, 3].map((index) => <div className="inventories-skeleton" role="row" key={index} aria-label="Loading inventory"><span role="cell" /><span role="cell" /><span role="cell" /><span role="cell" /></div>) : accountIds.size === 0 ? <div className="inventories-table-state" role="row"><span role="cell" aria-colspan={4}>Select an account to view its inventory.</span></div> : visibleItems.length === 0 ? <div className="inventories-table-state" role="row"><span role="cell" aria-colspan={4}>{items.length === 0 ? "No inventory items found. Refresh selected to try again." : "No items match these filters. Try another category or search."}</span></div> : visibleItems.map((item) => (
              <div className={`inventories-item ${selectedItemIds.has(item.assetId) ? "is-selected" : ""}`} role="row" key={item.assetId}>
                <span className="inventories-item-name" role="cell"><input type="checkbox" aria-label={`Select ${item.name}`} checked={selectedItemIds.has(item.assetId)} onChange={() => toggleItem(item.assetId)} /><span className="inventory-item-icon">{item.iconUrl ? <img src={item.iconUrl} alt="" /> : <Icon name="inventory" />}</span><strong title={item.name}>{item.name}</strong></span>
                <span role="cell">{item.assetType}</span>
                <span role="cell">{item.priceRobux === null ? "Unavailable" : `${item.priceRobux.toLocaleString()} robux`}</span>
                <span className="inventories-id" role="cell"><span>{item.assetId}</span><button type="button" aria-label={`Copy id for ${item.name}`} onClick={() => void copyIds([item.assetId])}><Icon name={copiedId === item.assetId ? "check" : "copy"} /></button></span>
              </div>
            ))}
          </div>
          {selectedItemIds.size > 0 && <div className="inventories-selection" role="status"><strong>{selectedItemIds.size} selected</strong><button className="account-button" type="button" onClick={() => void copyIds([...selectedItemIds])}><Icon name={copiedId === 0 ? "check" : "copy"} />{copiedId === 0 ? "Copied" : "Copy ids"}</button><button className="account-button" type="button" disabled={selectedItemIds.size > 20} title={selectedItemIds.size > 20 ? "Select up to 20 items to open on Roblox" : undefined} onClick={() => void openSelected()}><Icon name="square-arrow-out-up-right" />Open on Roblox</button><button className="account-button" type="button" onClick={() => setSelectedItemIds(new Set())}>Clear</button></div>}
        </section>
      </main>
    </>
  );
}
