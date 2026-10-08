import type { AccountSummary } from "../lib/ipc";

export type AccountIdentity = Pick<AccountSummary, "userId" | "username" | "displayName" | "label" | "avatarUrl">;

function initials(account: AccountIdentity) {
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
  account: AccountIdentity;
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
