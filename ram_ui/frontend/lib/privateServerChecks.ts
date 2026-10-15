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
    case "rateLimited": return "Owner lookup rate-limited";
    case "unavailable": return "Owner lookup failed";
    case "incomplete": return "Owner lookup incomplete";
    case "notListed": return "Owner not listed for these accounts";
    case "notChecked": return "Owner not checked";
    default: return "Owner profile unavailable";
  }
}

export function privateServerAvailabilityLabel(check: PrivateServerCheck | undefined, checking: boolean): string {
  if (check?.status === "valid") return "Invitation resolves. Roblox checks each selected account's join access when launching.";
  if (check?.status === "invalid") return "Private-server invitation is inactive or invalid.";
  return checking && !check ? "Checking server availability..." : "Server availability could not be verified. Refresh status to retry.";
}

export function privateServerOwnershipLabel(check: PrivateServerCheck | undefined, checking: boolean): string {
  if (check?.ownership === "owned") return `Owned by ${check.ownerUsername ?? "one of your saved accounts"}`;
  if (check?.ownership === "not-owned") return "None of your saved accounts owns this server.";
  return checking && !check ? "Checking whether any saved account owns this server..." : "Ownership could not be verified for every saved account. Refresh accounts and retry.";
}
