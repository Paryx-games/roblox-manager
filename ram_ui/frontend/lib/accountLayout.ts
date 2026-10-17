export const accountSectionIds = ["launch", "roblox-settings", "connections", "inventory", "details"] as const;
export type AccountSectionId = typeof accountSectionIds[number];

export function normalizeAccountLayout(value: unknown): AccountSectionId[] {
  const saved = Array.isArray(value) ? value : [];
  return [...new Set([...saved, ...accountSectionIds])].filter(
    (id): id is AccountSectionId => accountSectionIds.includes(id),
  );
}

export function moveAccountSection(order: AccountSectionId[], source: AccountSectionId, target: AccountSectionId) {
  const next = order.filter(id => id !== source);
  next.splice(order.indexOf(target), 0, source);
  return next;
}
