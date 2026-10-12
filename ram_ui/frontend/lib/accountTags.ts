export function matchesTags(
  accountIds: readonly string[],
  filter: readonly string[],
  all = false,
): boolean {
  return (
    !filter.length ||
    (all
      ? filter.every((id) => accountIds.includes(id))
      : filter.some((id) => accountIds.includes(id)))
  );
}

export function commonTags(
  accounts: readonly { tagIds: string[] }[],
): string[] {
  return accounts.length
    ? accounts[0].tagIds.filter((id) =>
        accounts.every((account) => account.tagIds.includes(id)),
      )
    : [];
}
