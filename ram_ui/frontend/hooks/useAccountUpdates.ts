import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AccountSummary } from "../lib/ipc";

export function useAccountUpdates(onUpdate: (accounts: AccountSummary[]) => void, onError: () => void) {
  const handlers = useRef({ onUpdate, onError });
  handlers.current = { onUpdate, onError };
  useEffect(() => {
    let active = true;
    const subscription = listen<AccountSummary[]>("accounts-updated", event => { if (active) handlers.current.onUpdate(event.payload); });
    void subscription.catch(() => { if (active) handlers.current.onError(); });
    return () => { active = false; void subscription.then(unlisten => unlisten()).catch(() => {}); };
  }, []);
}
