import type { AccountSummary } from "../lib/ipc";

function initials(account: AccountSummary) {
  const source = account.displayName || account.username || account.label;
  return source
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
}

export function AccountAvatar({
  account,
  className = "shared-account-avatar",
}: {
  account: AccountSummary;
  className?: string;
}) {
  return (
    <span className={className} aria-label={account.username}>
      {account.avatarUrl ? (
        <img src={account.avatarUrl} alt="" />
      ) : (
        initials(account)
      )}
    </span>
  );
}
