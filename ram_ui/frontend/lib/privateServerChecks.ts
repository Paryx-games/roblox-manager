import type { PrivateServerCheck } from "./ipc";

export async function runPrivateServerChecks(
  targets: readonly { index: number }[],
  check: (index: number) => Promise<PrivateServerCheck>,
  callbacks: {
    isCurrent: () => boolean;
    onResult: (index: number, result: PrivateServerCheck) => void;
    onError: (index: number, reason: unknown) => void;
  },
) {
  for (const target of targets) {
    if (!callbacks.isCurrent()) return;
    try {
      const result = await check(target.index);
      if (!callbacks.isCurrent()) return;
      callbacks.onResult(target.index, result);
    } catch (reason) {
      if (!callbacks.isCurrent()) return;
      callbacks.onError(target.index, reason);
    }
  }
}

export function privateServerOwnerLabel(check: PrivateServerCheck | undefined, checking: boolean): string {
  if (!check) return checking ? "Checking owner..." : "Owner not checked";
  switch (check.ownerLookup) {
    case "rateLimited": return "Owner check limited";
    case "unavailable": return "Owner check failed";
    case "incomplete": return "Owner unknown";
    case "notListed": return "Owner unknown";
    case "notChecked": return "Owner not checked";
    default: return "Owner unknown";
  }
}

export function privateServerAvailabilityLabel(check: PrivateServerCheck | undefined, checking: boolean): string {
  if (check?.status === "valid") return "Link active. Join access is checked at launch.";
  if (check?.status === "invalid") return "Link inactive or invalid.";
  return checking && !check ? "Checking link..." : "Link unknown. Refresh to retry.";
}

export function privateServerOwnershipLabel(check: PrivateServerCheck | undefined, checking: boolean): string {
  if (check?.ownership === "owned") return `Owned by ${check.ownerUsername ?? "one of your saved accounts"}`;
  if (check?.ownership === "not-owned") return "Not owned by your accounts.";
  return checking && !check ? "Checking owner..." : "Owner unknown. Refresh to retry.";
}
