export function allAccountsSelected(selected: ReadonlySet<number>, eligibleIds: readonly number[]): boolean {
  return eligibleIds.length > 0 && eligibleIds.every(id => selected.has(id));
}

export function toggleAllAccounts(selected: ReadonlySet<number>, eligibleIds: readonly number[]): Set<number> {
  const next = new Set(selected);
  const clear = allAccountsSelected(selected, eligibleIds);
  for (const id of eligibleIds) {
    if (clear) next.delete(id);
    else next.add(id);
  }
  return next;
}
