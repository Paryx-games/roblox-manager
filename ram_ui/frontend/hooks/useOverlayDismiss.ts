import { useEffect, useRef, type RefObject } from "react";

export function useOverlayDismiss(ref: RefObject<HTMLElement>, onClose?: () => void, enabled = true) {
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    if (!enabled) return;
    function dismiss(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      const overlays = document.querySelectorAll<HTMLElement>("[data-rm-overlay]");
      const dialogs = Array.from(overlays).filter((overlay) => overlay.getAttribute("aria-modal") === "true");
      const dialog = dialogs[dialogs.length - 1];
      const foreground = Array.from(overlays).filter((overlay) => !dialog || dialog.contains(overlay));
      if (foreground[foreground.length - 1] !== ref.current) return;
      // consume escape before page listeners can dismiss the parent as well
      event.preventDefault();
      event.stopImmediatePropagation();
      closeRef.current?.();
    }
    document.addEventListener("keydown", dismiss, true);
    return () => document.removeEventListener("keydown", dismiss, true);
  }, [ref, enabled]);
}
