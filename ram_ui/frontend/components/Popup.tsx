import { useEffect, useRef, type ReactNode } from "react";
import { useOverlayDismiss } from "../hooks/useOverlayDismiss";

type PopupProps = {
  children: ReactNode;
  className: string;
  backdropClassName: string;
  onClose?: () => void;
  closeOnBackdrop?: boolean;
  isClosing?: boolean;
  role?: "dialog" | "alertdialog";
  labelledBy?: string;
  describedBy?: string;
  busy?: boolean;
};

export function Popup({
  children,
  className,
  backdropClassName,
  onClose,
  closeOnBackdrop = false,
  isClosing = false,
  role = "dialog",
  labelledBy,
  describedBy,
  busy = false,
}: PopupProps) {
  const dialogRef = useRef<HTMLElement>(null);
  useOverlayDismiss(dialogRef, busy || isClosing ? undefined : onClose);

  useEffect(() => {
    const previousFocus = document.activeElement;
    const dialog = dialogRef.current;
    if (!dialog) return;
    const dialogElement = dialog;
    if (!dialog.contains(document.activeElement)) dialog.focus();

    function keepFocusInDialog(event: KeyboardEvent) {
      if (event.key !== "Tab") return;
      const dialogs = document.querySelectorAll<HTMLElement>('[data-rm-overlay][aria-modal="true"]');
      if (dialogs[dialogs.length - 1] !== dialogElement) return;
      const focusable = Array.from(
        dialogElement.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => element.getClientRects().length > 0);
      if (!focusable.length) {
        event.preventDefault();
        dialogElement.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!dialogElement.contains(document.activeElement)) {
        event.preventDefault();
        first.focus();
      } else if (event.shiftKey && (document.activeElement === first || document.activeElement === dialogElement)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", keepFocusInDialog);
    return () => {
      document.removeEventListener("keydown", keepFocusInDialog);
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) {
        previousFocus.focus();
      }
    };
  }, []);

  return (
    <div
      className={`${backdropClassName} ${isClosing ? "is-closing" : ""}`.trim()}
      role="presentation"
      onPointerDown={closeOnBackdrop && !busy && !isClosing ? () => onClose?.() : undefined}
    >
      <section
        ref={dialogRef}
        data-rm-overlay
        tabIndex={-1}
        className={`${className} ${isClosing ? "is-closing" : ""}`.trim()}
        role={role}
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        aria-busy={busy}
        onPointerDown={(event) => event.stopPropagation()}
      >
        {children}
      </section>
    </div>
  );
}
