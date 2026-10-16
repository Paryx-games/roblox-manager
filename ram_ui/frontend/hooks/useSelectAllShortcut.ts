import { useWorkspaceActive } from "./useWorkspaceActive";
import { useEffect, type RefObject } from "react";
import { isSelectAllShortcut, isTextSelectionTarget } from "../lib/selectAllShortcut";

export function useSelectAllShortcut(onSelectAll: (event: KeyboardEvent) => void, options: { isEnabled?: boolean; scopeRef?: RefObject<HTMLElement | null> } = {}) {
  const { isEnabled = true, scopeRef } = options;
  const active = useWorkspaceActive();
  useEffect(() => {
    if (!isEnabled || !active) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || !isSelectAllShortcut(event) || isTextSelectionTarget(event.target)) return;
      const modal = Array.from(document.querySelectorAll<HTMLElement>('[aria-modal="true"], [role="dialog"]')).find(element => element.getClientRects().length > 0);
      if (modal && (!scopeRef?.current || !modal.contains(scopeRef.current))) return;
      const popup = Array.from(document.querySelectorAll<HTMLElement>('.popup-menu, .rm-select-menu, .inventories-selected-dropdown[open]')).find(element => element.getClientRects().length > 0);
      if (popup && (!scopeRef?.current || !scopeRef.current.contains(popup))) return;
      event.preventDefault();
      onSelectAll(event);
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [active, isEnabled, onSelectAll, scopeRef]);
}
