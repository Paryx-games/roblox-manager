import type { AccountSummary, InventoryItem, InventoryBrowserTarget } from "./ipc";

export function buildInventoryAccountGroups(accounts: readonly AccountSummary[], groupOrder: readonly string[]): [string, AccountSummary[]][] {
  const grouped = new Map<string, AccountSummary[]>(groupOrder.map((name) => [name, []]));
  const sortedAccounts = [...accounts].sort((left, right) => Number(right.isPinned) - Number(left.isPinned) || left.sortOrder - right.sortOrder);
  for (const account of sortedAccounts) {
    const name = account.group.trim() || "Ungrouped";
    const members = grouped.get(name) ?? [];
    members.push(account);
    grouped.set(name, members);
  }
  return [...grouped.entries()];
}

export type OwnedInventoryItem = InventoryItem & { ownerIds: number[] };
export type InventoryComparison = "all" | "shared" | "unique";
export type InventorySort = "name" | "assetId" | "assetType" | "priceRobux";
export type InventorySortDirection = "ascending" | "descending";

export function buildInventoryBrowserTargets(items: readonly OwnedInventoryItem[], accountIds: ReadonlySet<number>, selectedItemIds: ReadonlySet<number>): InventoryBrowserTarget[] {
  const targets = new Map<string, InventoryBrowserTarget>();
  for (const userId of accountIds) {
    for (const item of items) {
      if (!selectedItemIds.has(item.assetId)) continue;
      targets.set(`${userId}:${item.assetId}`, { userId, assetId: item.assetId });
    }
  }
  return [...targets.values()];
}

export function mergeAccountInventories(inventories: readonly { userId: number; items: InventoryItem[] }[]): OwnedInventoryItem[] {
  const merged = new Map<number, OwnedInventoryItem>();
  for (const inventory of inventories) {
    for (const item of inventory.items) {
      const existing = merged.get(item.assetId);
      if (!existing) {
        merged.set(item.assetId, { ...item, ownerIds: [inventory.userId] });
      } else {
        if (!existing.ownerIds.includes(inventory.userId)) existing.ownerIds.push(inventory.userId);
        existing.iconUrl ??= item.iconUrl;
        existing.priceRobux ??= item.priceRobux;
      }
    }
  }
  return [...merged.values()];
}

export function matchesInventoryComparison(item: OwnedInventoryItem, comparison: InventoryComparison, accountCount: number): boolean {
  if (comparison === "shared") return item.ownerIds.length === accountCount;
  if (comparison === "unique") return item.ownerIds.length === 1;
  return true;
}

export function sortInventoryItems(items: readonly OwnedInventoryItem[], options: { field: InventorySort; direction: InventorySortDirection }): OwnedInventoryItem[] {
  return [...items].sort((left, right) => {
    let difference = 0;
    if (options.field === "priceRobux") {
      if (left.priceRobux === null && right.priceRobux !== null) return 1;
      if (right.priceRobux === null && left.priceRobux !== null) return -1;
      difference = (left.priceRobux ?? 0) - (right.priceRobux ?? 0);
    } else if (options.field === "assetId") {
      difference = left.assetId - right.assetId;
    } else {
      difference = left[options.field].localeCompare(right[options.field], undefined, { numeric: true, sensitivity: "base" });
    }
    const order = options.direction === "descending" ? -1 : 1;
    return difference * order || left.name.localeCompare(right.name) || left.assetId - right.assetId;
  });
}
