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
