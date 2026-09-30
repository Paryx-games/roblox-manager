import { useEffect, type RefObject } from "react";
import { isSelectAllShortcut, isTextSelectionTarget } from "../lib/selectAllShortcut";

export function useSelectAllShortcut(onSelectAll: (event: KeyboardEvent) => void, options: { isEnabled?: boolean; scopeRef?: RefObject<HTMLElement> } = {}) {
  const { isEnabled = true, scopeRef } = options;
  useEffect(() => {
    if (!isEnabled) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || !isSelectAllShortcut(event) || isTextSelectionTarget(event.target)) return;
      const modal = document.querySelector('[aria-modal="true"], [role="dialog"]');
      if (modal && (!scopeRef?.current || !modal.contains(scopeRef.current))) return;
      const popup = document.querySelector('.popup-menu, .rm-select-menu, .inventories-selected-dropdown[open]');
      if (popup && (!scopeRef?.current || !scopeRef.current.contains(popup))) return;
      event.preventDefault();
      onSelectAll(event);
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [isEnabled, onSelectAll, scopeRef]);
}
